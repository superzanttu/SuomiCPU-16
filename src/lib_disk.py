import json
import os
from pathlib import Path

class DiskDevice:
    """
    Simple disk storage device using JSON files.
    Accessible via a memory-mapped interface.
    """
    def __init__(self, storage_dir="disks"):
        self.storage_dir = Path(__file__).resolve().parent.parent / storage_dir
        self.storage_dir.mkdir(parents=True, exist_ok=True)
        self.current_disk = None
        self.disk_data = {} # Map of pointer (int) -> string (256 chars)
        
    def load_disk(self, disk_id: str):
        """Loads a disk by its 8-character identifier."""
        if len(disk_id) != 8:
            return False
        
        file_path = self.storage_dir / f"{disk_id}.json"
        if not file_path.exists():
            # Create a new empty disk if it doesn't exist
            self.disk_data = {}
        else:
            try:
                with open(file_path, 'r', encoding='utf-8') as f:
                    # JSON keys are always strings, we'll convert them back to ints
                    raw_data = json.load(f)
                    self.disk_data = {int(k): v for k, v in raw_data.items()}
            except Exception:
                return False
        
        self.current_disk = disk_id
        self._save()
        return True

    def list_disks(self):
        """Sorted IDs of the 8-character disks stored in the disks folder."""
        return sorted(p.stem for p in self.storage_dir.glob("*.json") if len(p.stem) == 8)

    def _save(self):
        file_path = self.storage_dir / f"{self.current_disk}.json"
        try:
            serializable_data = {str(k): v for k, v in sorted(self.disk_data.items())}
            with open(file_path, 'w', encoding='utf-8') as f:
                f.write("{\n")
                f.write(",\n".join(f"{json.dumps(k)}: {json.dumps(v, ensure_ascii=False)}"
                                   for k, v in serializable_data.items()))
                f.write("\n}\n")
        except Exception as e:
            print(f"Disk save error: {e}")

    def unload_disk(self):
        """Saves the current disk and unloads it."""
        if self.current_disk is None:
            return
        
        self._save()
        self.current_disk = None
        self.disk_data = {}

    def read_block(self, pointer: int) -> str:
        """Reads a 256-character block from the disk."""
        if self.current_disk is None:
            return " " * 256
        
        # Ensure pointer is 16-bit
        ptr = pointer & 0xFFFF
        data = self.disk_data.get(ptr, " " * 256)
        # Pad or truncate to exactly 256 characters
        return data.ljust(256)[:256]

    def write_block(self, pointer: int, data: str):
        """Writes a 256-character block to the disk."""
        if self.current_disk is None:
            return
        
        ptr = pointer & 0xFFFF
        # Pad or truncate to exactly 256 characters
        self.disk_data[ptr] = data.ljust(256)[:256]
        self._save()
