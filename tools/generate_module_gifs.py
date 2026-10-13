"""Run each C module headlessly and generate an animated GIF screen capture."""

import argparse
import os
import re
import sys
import time
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
os.environ["SDL_VIDEODRIVER"] = "dummy"
os.environ["SDL_AUDIODRIVER"] = "dummy"

import pygame

if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from src.SuomiCPU import (  # noqa: E402
    MEM_SIZE,
    MEM_MASK,
    PALETTE_COLORS,
    SCREEN_HEIGHT,
    SCREEN_WIDTH,
    VRAM_START,
    SuomiCompute16,
    create_memory,
)
from tools.c_compiler import compile_source  # noqa: E402


GALLERY_START = "<!-- module-gif-gallery:start -->"
GALLERY_END = "<!-- module-gif-gallery:end -->"
DEFAULT_RUN_SECONDS = 10.0
DEFAULT_GIF_SECONDS = 15.0
DEFAULT_FPS = 30
DEFAULT_CAPTURE_FPS = 10
CONSOLE_INPUT_DELAY = 1.0


@dataclass(frozen=True)
class Module:
    name: str
    folder: Path
    source: Path | None
    error: str | None = None


def discover_modules(modules_dir: Path) -> list[Module]:
    """Return every module folder, selecting its BIOS-compatible C entry point."""
    modules = []
    for folder in sorted(
        (path for path in modules_dir.iterdir() if path.is_dir()),
        key=lambda path: path.name.casefold(),
    ):
        sources = sorted(
            (path for path in folder.iterdir()
             if path.is_file() and path.suffix.casefold() == ".c"),
            key=lambda path: path.name.casefold(),
        )
        if not sources:
            modules.append(Module(folder.name, folder, None, "No C source file found."))
        elif (folder / "main.c").is_file():
            modules.append(Module(folder.name, folder, folder / "main.c"))
        elif len(sources) == 1:
            modules.append(Module(folder.name, folder, sources[0]))
        else:
            modules.append(Module(
                folder.name,
                folder,
                None,
                "Multiple C source files found; expected main.c.",
            ))
    return modules


def _palette() -> bytes:
    colors = [(value, value, value) for value in range(256)]
    colors[:len(PALETTE_COLORS)] = PALETTE_COLORS
    return bytes(channel for color in colors for channel in color)


def _lzw_encode(pixels: bytes) -> bytes:
    """Encode 8-bit palette indices into GIF's LZW sub-block stream."""
    if not pixels:
        raise ValueError("GIF frames must contain pixels")

    clear_code = 256
    end_code = 257
    code_size = 9
    next_code = 258
    dictionary = {(value,): value for value in range(256)}
    packed = bytearray()
    bit_buffer = 0
    bit_count = 0

    def emit(code: int) -> None:
        nonlocal bit_buffer, bit_count
        bit_buffer |= code << bit_count
        bit_count += code_size
        while bit_count >= 8:
            packed.append(bit_buffer & 0xFF)
            bit_buffer >>= 8
            bit_count -= 8

    emit(clear_code)
    prefix = (pixels[0],)
    for value in pixels[1:]:
        candidate = prefix + (value,)
        if candidate in dictionary:
            prefix = candidate
            continue

        emit(dictionary[prefix])
        if next_code < 4096:
            dictionary[candidate] = next_code
            next_code += 1
            if next_code > (1 << code_size) and code_size < 12:
                code_size += 1
        else:
            emit(clear_code)
            dictionary = {(index,): index for index in range(256)}
            code_size = 9
            next_code = 258
        prefix = (value,)

    emit(dictionary[prefix])
    emit(end_code)
    if bit_count:
        packed.append(bit_buffer & 0xFF)

    blocks = bytearray((8,))
    for offset in range(0, len(packed), 255):
        block = packed[offset:offset + 255]
        blocks.append(len(block))
        blocks.extend(block)
    blocks.append(0)
    return bytes(blocks)


def write_gif(
    destination: Path,
    frames: list[bytes],
    width: int,
    height: int,
    duration_seconds: float,
) -> None:
    """Write a looping GIF with 256-color frames and the requested total duration."""
    if not frames:
        raise ValueError("Cannot create a GIF without captured frames")
    frame_size = width * height
    if any(len(frame) != frame_size for frame in frames):
        raise ValueError("Every GIF frame must match the configured dimensions")

    total_centiseconds = round(duration_seconds * 100)
    if total_centiseconds < len(frames):
        raise ValueError("GIF duration is too short for the number of captured frames")
    base_delay, extra_delays = divmod(total_centiseconds, len(frames))
    output = bytearray(b"GIF89a")
    output.extend(width.to_bytes(2, "little"))
    output.extend(height.to_bytes(2, "little"))
    output.extend((0xF7, 0, 0))
    output.extend(_palette())
    output.extend(b"!\xFF\x0BNETSCAPE2.0\x03\x01\x00\x00\x00")

    for index, frame in enumerate(frames):
        delay = base_delay + int(index < extra_delays)
        output.extend(b"!\xF9\x04\x00")
        output.extend(delay.to_bytes(2, "little"))
        output.extend(b"\x00\x00")
        output.append(0x2C)
        output.extend(bytes(4))
        output.extend(width.to_bytes(2, "little"))
        output.extend(height.to_bytes(2, "little"))
        output.append(0)
        output.extend(_lzw_encode(frame))
    output.append(0x3B)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(output)


def _make_cpu(source: Path, module_name: str):
    image = compile_source(source.read_text(encoding="utf-8"), str(source))
    cpu = SuomiCompute16.__new__(SuomiCompute16)
    cpu.memory = create_memory()
    cpu.registers = [0] * 8
    cpu.flags = {"Z": 0, "C": 0, "N": 0, "V": 0}
    cpu.running = True
    cpu.sp = MEM_MASK
    cpu.frame_yield = False
    cpu.audio_enabled = False
    cpu.sound_log = []
    cpu._last_click = [(-10**9, 0, 0), (-10**9, 0, 0)]
    cpu.module_name = module_name
    cpu.net_port = 0
    cpu.load_program(image)
    cpu.reset()
    return cpu


def _console_frame(cpu) -> bytes:
    width, height = 640, 400
    surface = pygame.Surface((width, height))
    surface.fill((0, 0, 0))
    font = pygame.font.SysFont("consolas", 14)
    cpu._console_device().render(
        surface,
        font,
        PALETTE_COLORS[3],
        line_height=16,
    )
    surface = pygame.transform.scale(surface, (SCREEN_WIDTH, SCREEN_HEIGHT - 40))
    rgb = pygame.image.tostring(surface, "RGB")
    frame = bytearray(SCREEN_WIDTH * SCREEN_HEIGHT)
    top = 20
    for source_offset in range(0, len(rgb), 3):
        if rgb[source_offset] or rgb[source_offset + 1] or rgb[source_offset + 2]:
            pixel = source_offset // 3
            frame[(pixel // SCREEN_WIDTH + top) * SCREEN_WIDTH + pixel % SCREEN_WIDTH] = 3
    return bytes(frame)


def _capture_frame(cpu, text_mode: bool) -> bytes:
    if text_mode:
        return _console_frame(cpu)
    start = VRAM_START
    return bytes(cpu.memory[start:start + SCREEN_WIDTH * SCREEN_HEIGHT])


def capture_module(
    module: Module,
    destination: Path,
    run_seconds: float,
    gif_seconds: float,
    fps: int,
    capture_fps: int,
    console_input: str,
) -> int:
    if module.source is None:
        raise ValueError(module.error or "No source file found")

    cpu = _make_cpu(module.source, module.name)
    text_mode = "SCREEN_" in module.source.read_text(encoding="utf-8")
    input_bytes = (console_input + "\n").encode("utf-8") if text_mode else b""
    frames = []
    try:
        start_time = time.perf_counter()
        next_frame_time = start_time
        next_capture_time = 0.0
        input_sent = False

        while time.perf_counter() - start_time < run_seconds:
            now = time.perf_counter()
            if now < next_frame_time:
                time.sleep(next_frame_time - now)

            cpu.update_rtc()
            cpu.execute_frame()
            elapsed = time.perf_counter() - start_time
            if input_bytes and not input_sent and elapsed >= CONSOLE_INPUT_DELAY:
                cpu._console_device().receive(input_bytes)
                input_sent = True

            if elapsed >= next_capture_time or not frames:
                frames.append(_capture_frame(cpu, text_mode))
                while next_capture_time <= elapsed:
                    next_capture_time += 1 / capture_fps

            next_frame_time += 1 / fps
            now = time.perf_counter()
            if next_frame_time < now:
                next_frame_time = now

        if not frames:
            frames.append(_capture_frame(cpu, text_mode))
        write_gif(destination, frames, SCREEN_WIDTH, SCREEN_HEIGHT, gif_seconds)
    finally:
        node = getattr(cpu, "net", None)
        if node is not None:
            node.close()
        cpu._console_device().close()
    return len(frames)


def _slug(name: str, used: set[str]) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", name.casefold()).strip("-") or "module"
    candidate = slug
    suffix = 2
    while candidate in used:
        candidate = f"{slug}-{suffix}"
        suffix += 1
    slug = candidate
    used.add(slug)
    return slug


def update_readme(readme: Path, modules: list[tuple[Module, Path | None, str | None]]) -> None:
    rows = [
        "## Module gallery",
        "",
        "Each animation captures a module running in the SC-16 emulator.",
        "",
        "| Module | Animation |",
        "| --- | --- |",
    ]
    for module, gif_path, error in modules:
        name = module.name.replace("|", "\\|")
        if gif_path is not None:
            relative = os.path.relpath(gif_path, readme.parent).replace("\\", "/")
            rows.append(
                f'| {name} | <img src="{relative}" '
                f'alt="{name} running in SC-16" width="320"> |'
            )
        else:
            rows.append(f"| {name} | Capture unavailable: {error or 'not generated'} |")
    gallery = "\n".join((GALLERY_START, *rows, GALLERY_END))
    contents = readme.read_text(encoding="utf-8")
    start = contents.find(GALLERY_START)
    end = contents.find(GALLERY_END)
    if start == -1 and end == -1:
        contents = contents.rstrip() + "\n\n" + gallery + "\n"
    elif start == -1 or end == -1 or end < start:
        raise ValueError(f"README gallery markers are incomplete: {readme}")
    else:
        end += len(GALLERY_END)
        contents = contents[:start] + gallery + contents[end:]
    readme.write_text(contents, encoding="utf-8")


def _parse_args():
    parser = argparse.ArgumentParser(
        description="Run every C module and generate looping screen-capture GIFs."
    )
    parser.add_argument(
        "--modules-dir",
        type=Path,
        default=ROOT / "modules",
        help="folder containing one subfolder per module (default: modules/)",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "docs" / "module-gifs",
        help="GIF output folder (default: docs/module-gifs/)",
    )
    parser.add_argument(
        "--readme",
        type=Path,
        default=ROOT / "README.md",
        help="README gallery to update after capturing modules",
    )
    parser.add_argument(
        "--run-seconds",
        type=float,
        default=DEFAULT_RUN_SECONDS,
        help="real-time run duration per module (default: 10 seconds)",
    )
    parser.add_argument(
        "--gif-seconds",
        type=float,
        default=DEFAULT_GIF_SECONDS,
        help="total playback duration per looping GIF (default: 15 seconds)",
    )
    parser.add_argument(
        "--fps",
        type=int,
        default=DEFAULT_FPS,
        help="emulated frame rate while capturing (default: 30)",
    )
    parser.add_argument(
        "--capture-fps",
        type=int,
        default=DEFAULT_CAPTURE_FPS,
        help="maximum GIF frame sampling rate (default: 10)",
    )
    parser.add_argument(
        "--console-input",
        default="SC-16",
        help="line supplied to text-mode programs that request keyboard input",
    )
    parser.add_argument(
        "--no-readme",
        action="store_true",
        help="generate animations without modifying the README",
    )
    args = parser.parse_args()
    if args.run_seconds <= 0 or args.gif_seconds <= 0:
        parser.error("--run-seconds and --gif-seconds must be positive")
    if args.fps <= 0 or args.capture_fps <= 0 or args.capture_fps > args.fps:
        parser.error("FPS values must be positive and capture-fps cannot exceed fps")
    return args


def main() -> int:
    args = _parse_args()
    if not args.modules_dir.is_dir():
        print(f"Modules folder does not exist: {args.modules_dir}", file=sys.stderr)
        return 1
    modules = discover_modules(args.modules_dir)
    if not modules:
        print(f"No module folders found in {args.modules_dir}", file=sys.stderr)
        return 1

    pygame.init()
    results = []
    failures = 0
    generated = 0
    used_slugs = set()
    try:
        for module in modules:
            slug = _slug(module.name, used_slugs)
            destination = args.output_dir / f"{slug}.gif"
            try:
                count = capture_module(
                    module,
                    destination,
                    args.run_seconds,
                    args.gif_seconds,
                    args.fps,
                    args.capture_fps,
                    args.console_input,
                )
            except Exception as exc:
                failures += 1
                results.append((module, None, str(exc)))
                print(f"{module.name}: ERROR: {exc}", file=sys.stderr)
            else:
                results.append((module, destination, None))
                generated += 1
                print(f"{module.name}: captured {count} frames -> {destination}")
    finally:
        pygame.quit()

    if not args.no_readme:
        try:
            update_readme(args.readme, results)
        except (OSError, ValueError) as exc:
            print(f"README update failed: {exc}", file=sys.stderr)
            failures += 1
        else:
            print(f"Updated module gallery in {args.readme}")

    print(f"Generated {generated} module GIF(s); {failures} failure(s).")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
