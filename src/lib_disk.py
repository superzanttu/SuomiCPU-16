import json
import os
from pathlib import Path

class DiskDevice:
    """
    Simple disk storage device using JSON files.
    Accessible via a memory-mapped interface.
    """
    def __init__(self, storage_dir="disks"):
        self.storage_dir = Path("disks")
        self.storage_dir.mkdir(exist_ok=True)
        self.current_disk = None
        self.disk_data = {} # Map of pointer (int) -> string (255 chars)
        
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
        return True

    def unload_disk(self):
        """Saves the current disk and unloads it."""
        if self.current_disk is None:
            return
        
        file_path = self.storage_dir / f"{self.current_disk}.json"
        try:
            # Convert int keys to strings for JSON
            serializable_data = {str(k): v for k, v in self.disk_data.items()}
            with open(file_path, 'w', encoding='utf-8') as f:
                json.dump(serializable_data, f)
        except Exception as e:
            print(f"Disk save error: {e}")
        
        self.current_disk = None
        self.disk_data = {}

    def read_block(self, pointer: int) -> str:
        """Reads a 255-character block from the disk."""
        if self.current_disk is None:
            return " " * 255
        
        # Ensure pointer is 16-bit
        ptr = pointer & 0xFFFF
        data = self.disk_data.get(ptr, " " * 255)
        # Pad or truncate to exactly 255 characters
        return data.ljust(255)[:255]

    def write_block(self, pointer: int, data: str):
        """Writes a 255-character block to the disk."""
        if self.current_disk is None:
            return
        
        ptr = pointer & 0xFFFF
        # Pad or truncate to exactly 255 characters
        self.disk_data[ptr] = data.ljust(255)[:255]
