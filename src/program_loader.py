from collections.abc import Callable
from pathlib import Path
import sys

from tools.assembler import (
    AssemblyError,
    AssemblyImage,
    MemorySegment,
    assemble_file,
)
from tools.c_compiler import CCompilerError


def load_program_file(
    path: str | Path,
    logger: Callable[[str], None] | None,
    memory_capacity: int,
    verbose: bool = False,
) -> AssemblyImage:
    """Load an assembly, C, binary, or supported map program."""
    program_path = Path(path)
    report = logger if logger is not None else (lambda message: None)
    if program_path.suffix.lower() == ".c":
        try:
            tank16_main = (
                Path(__file__).resolve().parent.parent
                / "modules" / "Tank-16" / "main.c"
            )
            if program_path.resolve() == tank16_main:
                tank16_dir = tank16_main.parent
                if str(tank16_dir) not in sys.path:
                    sys.path.insert(0, str(tank16_dir))
                from tank16_map_loader import compile_runtime_tank16

                report(
                    "Compiling Tank-16 with shared graphics assets; "
                    "map layouts are loaded dynamically at runtime"
                )
                if verbose:
                    report(
                        f"[SC-16 program loader] Tank-16 C source: "
                        f"{program_path.resolve()}"
                    )
                    report(
                        "[SC-16 program loader] Building graphics assets and "
                        "compiling runtime image"
                    )
                if verbose:
                    image = compile_runtime_tank16(logger=report)
                else:
                    image = compile_runtime_tank16()
                report("Tank-16 source and graphics assets compiled into SC-16 image")
            else:
                report(f"Compiling C source: {program_path.name}")
                if verbose:
                    report(
                        f"[SC-16 program loader] Passing C source to the compiler: "
                        f"{program_path.resolve()}"
                    )
                from tools.c_compiler import compile_file_with_assembly

                image, assembly = compile_file_with_assembly(
                    program_path, logger=report if verbose else None
                )
                report(f"C source file size: {program_path.stat().st_size} bytes")
                report(f"Generated assembly size: {len(assembly.encode('utf-8'))} bytes")
                report("C compilation complete; SC-16 image ready")
        except (CCompilerError, OSError, ValueError) as exc:
            report(f"ERROR: C compilation failed: {exc}")
            raise
    elif program_path.suffix.lower() == ".json":
        raise ValueError("JSON files are data; load them through the disk API")
    elif program_path.suffix.lower() == ".bin":
        report(f"Loading binary program: {program_path.name}")
        if verbose:
            report(
                f"[SC-16 program loader] Reading raw SC-16 binary: "
                f"{program_path.resolve()}"
            )
        try:
            binary = program_path.read_bytes()
            if len(binary) > memory_capacity:
                raise ValueError("Ohjelma on liian suuri muistiin")
            image = AssemblyImage((MemorySegment(0, binary),), 0)
            report(f"Binary file size: {len(binary)} bytes")
        except (OSError, ValueError) as exc:
            report(f"ERROR: Binary load failed: {exc}")
            raise
    else:
        report(f"Assembling source: {program_path.name}")
        if verbose:
            report(
                f"[SC-16 program loader] Passing assembly source to the assembler: "
                f"{program_path.resolve()}"
            )
        try:
            image = assemble_file(
                program_path, logger=report if verbose else None
            )
            report(f"Assembly source file size: {program_path.stat().st_size} bytes")
        except (AssemblyError, OSError, ValueError) as exc:
            report(f"ERROR: Assembly failed: {exc}")
            raise
        report("Assembly complete; SC-16 image ready")

    image_size = sum(len(segment.data) for segment in image.segments)
    if image.segments:
        image_start = min(segment.address for segment in image.segments)
        image_end = max(
            segment.address + len(segment.data) for segment in image.segments
        )
        image_span = image_end - image_start
    else:
        image_span = 0
    report(
        f"Program image: {image_size} bytes across {len(image.segments)} segments; "
        f"address span {image_span} bytes; memory capacity {memory_capacity} bytes"
    )
    return image
