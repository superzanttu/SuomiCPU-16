import json
import os
import re
import time
from pathlib import Path


class DiskDeviceError(Exception):
    pass


class DiskDevice:
    """Virtual disks with 256-byte blocks and bounded virtual-file reads."""

    BLOCK_SIZE = 256
    MAX_FILE_READ_SIZE = 4096
    FILE_LIST_CACHE_SECONDS = 1.0
    MAX_BLOCKS = 0x10000
    DISK_ID_PATTERN = re.compile(r"[A-Za-z0-9_-]{8}\Z")
    FILE_COMPONENT_PATTERN = re.compile(r"[A-Za-z0-9_.-]+\Z")

    def __init__(self, storage_dir="disks"):
        root = Path(__file__).resolve().parent.parent
        requested = Path(storage_dir)
        self.storage_dir = requested if requested.is_absolute() else root / requested
        self.storage_dir.mkdir(parents=True, exist_ok=True)
        self.current_disk = None
        self.write_protected = False
        self.disk_data = {}
        self.file_root = None

    @classmethod
    def _validate_id(cls, disk_id):
        if not isinstance(disk_id, str) or not cls.DISK_ID_PATTERN.fullmatch(disk_id):
            raise DiskDeviceError("disk ID must be exactly 8 ASCII letters, digits, '_' or '-'")

    @classmethod
    def _block_text(cls, data):
        if (
            not isinstance(data, str)
            or len(data) != cls.BLOCK_SIZE
            or any(ord(char) > 0xFF for char in data)
        ):
            raise DiskDeviceError("disk blocks must contain exactly 256 bytes")
        return data

    @classmethod
    def _validate_file_path(cls, file_path):
        if (
            not isinstance(file_path, str)
            or not file_path
            or len(file_path) > 128
            or "\\" in file_path
            or file_path.startswith("/")
        ):
            raise DiskDeviceError("invalid virtual file path")
        parts = file_path.split("/")
        if any(
            part in ("", ".", "..")
            or not cls.FILE_COMPONENT_PATTERN.fullmatch(part)
            for part in parts
        ):
            raise DiskDeviceError("invalid virtual file path")
        return Path(*parts)

    def load_disk(self, disk_id):
        self._validate_id(disk_id)
        file_path = self.storage_dir / f"{disk_id}.json"
        protected = False
        data = {}
        if file_path.exists():
            try:
                with file_path.open("r", encoding="utf-8") as disk_file:
                    raw_data = json.load(disk_file)
            except (OSError, UnicodeError, json.JSONDecodeError) as exc:
                raise DiskDeviceError(f"cannot read disk {disk_id}: {exc}") from exc
            if not isinstance(raw_data, dict):
                raise DiskDeviceError(f"disk {disk_id} has an invalid image format")
            protected = raw_data.get("protected", False) is True
            try:
                for key, value in raw_data.items():
                    if key == "protected":
                        continue
                    if not isinstance(key, str) or not key.isdigit() or not isinstance(value, str):
                        raise ValueError("invalid block entry")
                    block = int(key)
                    if not 0 <= block < self.MAX_BLOCKS:
                        raise ValueError("block number is out of range")
                    self._block_text(value)
                    data[block] = value
            except (ValueError, DiskDeviceError) as exc:
                raise DiskDeviceError(f"disk {disk_id} has invalid block data: {exc}") from exc

        previous = (self.current_disk, self.disk_data, self.write_protected)
        if self.current_disk is not None and self.current_disk != disk_id:
            self._save()
        self.current_disk = disk_id
        self.write_protected = protected
        self.disk_data = data
        if not file_path.exists():
            try:
                self._save()
            except DiskDeviceError:
                self.current_disk, self.disk_data, self.write_protected = previous
                raise
        return True

    def list_disks(self):
        try:
            return sorted(
                path.stem
                for path in self.storage_dir.glob("*.json")
                if self.DISK_ID_PATTERN.fullmatch(path.stem)
            )
        except OSError as exc:
            raise DiskDeviceError(f"cannot list virtual disks: {exc}") from exc

    def list_files(self):
        if self.file_root is None:
            raise DiskDeviceError("the running module has no disk folder")
        root = self.file_root
        cached = getattr(self, "_file_list_cache", None)
        now = time.monotonic()
        if (cached is not None and cached[0] == root
                and now - cached[1] < self.FILE_LIST_CACHE_SECONDS):
            return list(cached[2])
        if not root.exists():
            return []
        try:
            resolved_root = root.resolve()
            names = []
            for directory, subdirectories, files in os.walk(root):
                directory_path = Path(directory)
                subdirectories.sort()
                for file_name in files:
                    path = directory_path / file_name
                    if path.is_symlink() and (
                        not path.is_file()
                        or not path.resolve().is_relative_to(resolved_root)
                    ):
                        continue
                    name = path.relative_to(root).as_posix()
                    try:
                        self._validate_file_path(name)
                    except DiskDeviceError:
                        continue
                    names.append(name)
            names.sort()
            self._file_list_cache = (root, now, tuple(names))
            return names
        except OSError as exc:
            raise DiskDeviceError(
                f"cannot list files on disk {self.current_disk}: {exc}"
            ) from exc

    def read_file(self, file_path, offset, length):
        if self.file_root is None:
            raise DiskDeviceError("the running module has no disk folder")
        relative_path = self._validate_file_path(file_path)
        if (
            not isinstance(offset, int)
            or isinstance(offset, bool)
            or not 0 <= offset <= 0xFFFFFFFF
            or not isinstance(length, int)
            or isinstance(length, bool)
            or not 0 <= length <= self.MAX_FILE_READ_SIZE
        ):
            raise DiskDeviceError("invalid virtual file read range")
        root = self.file_root
        safe_paths = getattr(self, "_safe_paths", None)
        if safe_paths is None:
            safe_paths = self._safe_paths = {}
        try:
            path = safe_paths.get((root, file_path))
            if path is None:
                path = root / relative_path
                if not path.resolve().is_relative_to(root.resolve()):
                    raise DiskDeviceError("virtual file path escapes the disk")
                safe_paths[(root, file_path)] = path
            with path.open("rb") as file_handle:
                file_handle.seek(0, os.SEEK_END)
                file_size = file_handle.tell()
                if file_size > 0xFFFFFFFF:
                    raise DiskDeviceError(
                        f"virtual file {file_path!r} exceeds the 32-bit size limit"
                    )
                file_handle.seek(offset)
                return file_handle.read(length), file_size
        except FileNotFoundError as exc:
            raise DiskDeviceError(
                f"virtual file {file_path!r} does not exist"
            ) from exc
        except OSError as exc:
            raise DiskDeviceError(
                f"cannot read virtual file {file_path!r}: {exc}"
            ) from exc

    def set_file_root(self, root):
        try:
            self.file_root = Path(root).resolve()
            self._file_list_cache = None
            self._safe_paths = {}
        except OSError as exc:
            raise DiskDeviceError(f"cannot configure module disk folder: {exc}") from exc

    def is_protected(self, disk_id):
        self._validate_id(disk_id)
        if self.current_disk == disk_id:
            return self.write_protected
        path = self.storage_dir / f"{disk_id}.json"
        try:
            with path.open("r", encoding="utf-8") as disk_file:
                raw_data = json.load(disk_file)
        except FileNotFoundError:
            return False
        except (OSError, UnicodeError, json.JSONDecodeError) as exc:
            raise DiskDeviceError(f"cannot inspect disk {disk_id}: {exc}") from exc
        if not isinstance(raw_data, dict):
            raise DiskDeviceError(f"disk {disk_id} has an invalid image format")
        return raw_data.get("protected", False) is True

    def _save(self):
        if self.current_disk is None:
            raise DiskDeviceError("no virtual disk is mounted")
        self._validate_id(self.current_disk)
        file_path = self.storage_dir / f"{self.current_disk}.json"
        temporary_path = file_path.with_suffix(".json.tmp")
        content = {}
        if self.write_protected:
            content["protected"] = True
        content.update({str(key): value for key, value in sorted(self.disk_data.items())})
        try:
            with temporary_path.open("w", encoding="utf-8") as disk_file:
                json.dump(content, disk_file, ensure_ascii=False, indent=2)
                disk_file.write("\n")
            os.replace(temporary_path, file_path)
        except (OSError, UnicodeError, TypeError, ValueError) as exc:
            raise DiskDeviceError(f"cannot save disk {self.current_disk}: {exc}") from exc

    def set_protected(self, value):
        if self.current_disk is None:
            raise DiskDeviceError("no virtual disk is mounted")
        previous = self.write_protected
        self.write_protected = bool(value)
        try:
            self._save()
        except DiskDeviceError:
            self.write_protected = previous
            raise

    def unload_disk(self):
        if self.current_disk is None:
            return
        self._save()
        self.current_disk = None
        self.disk_data = {}
        self.write_protected = False

    def read_block(self, pointer):
        if self.current_disk is None:
            raise DiskDeviceError("no virtual disk is mounted")
        data = self.disk_data.get(pointer & 0xFFFF, " " * self.BLOCK_SIZE)
        return data.ljust(self.BLOCK_SIZE)[:self.BLOCK_SIZE]

    def write_block(self, pointer, data):
        if self.current_disk is None:
            raise DiskDeviceError("no virtual disk is mounted")
        if self.write_protected:
            raise DiskDeviceError(f"disk {self.current_disk} is write-protected")
        if not isinstance(data, str):
            raise DiskDeviceError("disk block data must be text")
        block = pointer & 0xFFFF
        previous = self.disk_data.get(block)
        block_data = self._block_text(data.ljust(self.BLOCK_SIZE)[:self.BLOCK_SIZE])
        self.disk_data[block] = block_data
        try:
            self._save()
        except DiskDeviceError:
            if previous is None:
                self.disk_data.pop(block, None)
            else:
                self.disk_data[block] = previous
            raise
