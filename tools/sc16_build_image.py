"""Compile or assemble one SC-16 input into the Rust emulator image format."""

import argparse
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parent.parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from src.program_loader import load_program_file

MAGIC = b"SC16IMG1"
MEMORY_SIZE = 1 << 30


def write_image(source: Path, output: Path) -> None:
    image = load_program_file(source, print, MEMORY_SIZE, verbose=True)
    with output.open("wb") as stream:
        stream.write(MAGIC)
        stream.write(struct.pack(">II", image.entry_point, len(image.segments)))
        for segment in image.segments:
            stream.write(struct.pack(">IQ", segment.address, len(segment.data)))
            stream.write(segment.data)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    arguments = parser.parse_args()
    try:
        write_image(arguments.source, arguments.output)
    except (OSError, UnicodeError, ValueError) as error:
        print(f"SC-16 image build failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
