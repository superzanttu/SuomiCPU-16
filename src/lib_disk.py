import json
import os
import re
from pathlib import Path


class DiskDeviceError(Exception):
    pass


class DiskDevice:
    """JSON-backed virtual disks with 256-byte blocks."""

    BLOCK_SIZE = 256
    MAX_BLOCKS = 0x10000
    DISK_ID_PATTERN = re.compile(r"[A-Za-z0-9_-]{8}\Z")

    def __init__(self, storage_dir="disks"):
        root = Path(__file__).resolve().parent.parent
        requested = Path(storage_dir)
        self.storage_dir = requested if requested.is_absolute() else root / requested
        self.storage_dir.mkdir(parents=True, exist_ok=True)
        self.current_disk = None
        self.write_protected = False
        self.disk_data = {}

    @classmethod
    def _validate_id(cls, disk_id):
        if not isinstance(disk_id, str) or not cls.DISK_ID_PATTERN.fullmatch(disk_id):
            raise DiskDeviceError("disk ID must be exactly 8 ASCII letters, digits, '_' or '-'")

    @classmethod
    def _block_text(cls, data):
        if len(data) != cls.BLOCK_SIZE or any(ord(char) > 0xFF for char in data):
            raise DiskDeviceError("disk blocks must contain exactly 256 bytes")
        return data

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
        self.write_protected = bool(value)
        self._save()

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
        self.disk_data[pointer & 0xFFFF] = self._block_text(data.ljust(self.BLOCK_SIZE)[:self.BLOCK_SIZE])
        self._save()
