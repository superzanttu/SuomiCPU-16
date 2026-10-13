# SuomiCPU-16

SuomiCPU-16 is a Python emulator for the SC-16, a small, byte-addressed machine.

## Features
- Full emulation of the SC-16 architecture.
- Integrated assembler for SC-16 assembly language.
- C90-oriented compiler for SC-16 with partial language/preprocessor and
  standard-library support (`setjmp.h` and `signal.h` are not implemented),
  plus a target-specific C99 `<stdint.h>` header
  (not a fully conforming C90 implementation; see the
  [user guide](user_guide.md#9-ansi-c90-compiler)).
- Memory-mapped console, BIOS data-disk selection, JSON-backed virtual disks,
  indexed graphics, and a fixed-point 3D graphics coprocessor.

## Getting Started
Start SC-16 without arguments to show the BIOS module selector. Select a
module folder from `modules/` to compile and load its C program into memory.
Folders with multiple `.c` files use `main.c` as the entry source. Press **D**
to cycle through available data disks or choose no disk; virtual disks store
application data only.
```console
python main.py
```

Use `--disk ID` to preselect a data disk in BIOS, or mount it with a directly
launched program.
The game fills its own window by default. The 80×25 console opens in a separate
window with **REBOOT**, **RESET** (return to BIOS), **COPY**, and
**PAUSE/CONTINUE** controls and emulator status. Select console text and press
Ctrl+C to copy it; **COPY** copies the selection or the visible screen. The
console retains 1,000 rows of scrollback, accessible with the mouse wheel.
Press **F2** in the emulator window to hide/show the console; **F11** toggles
windowed/fullscreen game display. Start directly in fullscreen with
`python main.py --fullscreen` or
`.\run.ps1 -Fullscreen`.

Run an example directly from the repository root:
```console
python main.py examples/factorial.c
```

For five analog clock styles, a Nixie-tube display, and a flip clock, run:
```console
python main.py examples/anaclock.c
```
Press Tab to cycle through the seven clock faces.

For a first-person Elite-style space combat demo, run:
```console
python main.py examples/elitedemo.c
```
Turn with Left/Right, increase or decrease throttle with Up/Down, and fire with
Space. The ship accelerates and drifts with inertia; shields regenerate and the
laser needs time to cool.

Select **Hertta-3D** in the BIOS for a first-person maze game rendered with the
SC-GPU. Use W/S to move, A/D to turn, and Space to throw treats toward Hertta;
feed her enough treats and wait for her to reach you to advance to a harder
maze. Each visible wall cell is rendered as one outlined surface, and a solid-cell
minimap shows the connected maze and marks you and Hertta. Hertta wanders through
the passages and can find a path to you after eating enough treats. Its source is
[`modules/Hertta-3D/hertta3d.c`](modules/Hertta-3D/hertta3d.c).

On Windows, the PowerShell launcher can run the same programs:
```powershell
.\run.ps1 .\examples\factorial.c
```
It changes to the repository directory and starts the emulator through
`main.py`. Make sure Python is available as `python` on `PATH`.

The emulator accepts assembly (`.asm`), C (`.c`), and flat binary (`.bin`)
programs. The C compiler is in [`tools/c_compiler.py`](tools/c_compiler.py);
for example, compile a C file to a binary and run it with:

```console
python tools/c_compiler.py examples/asteroids.c -o asteroids.bin
python main.py asteroids.bin
```

See the [user guide](user_guide.md) for setup details, controls, and the full
example catalog.

<!-- module-gif-gallery:start -->
## Module gallery

Each animation captures a module running in the SC-16 emulator.

| Module | Animation |
| --- | --- |
| AnaClock | <img src="docs/module-gifs/anaclock.gif" alt="AnaClock running in SC-16" width="320"> |
| Asteroids | <img src="docs/module-gifs/asteroids.gif" alt="Asteroids running in SC-16" width="320"> |
| DiskEditor | <img src="docs/module-gifs/diskeditor.gif" alt="DiskEditor running in SC-16" width="320"> |
| Elite | <img src="docs/module-gifs/elite.gif" alt="Elite running in SC-16" width="320"> |
| Hulio | <img src="docs/module-gifs/hulio.gif" alt="Hulio running in SC-16" width="320"> |
| Infinite CIty | <img src="docs/module-gifs/infinite-city.gif" alt="Infinite CIty running in SC-16" width="320"> |
| Lunar Lander | <img src="docs/module-gifs/lunar-lander.gif" alt="Lunar Lander running in SC-16" width="320"> |
| MotoRobots | <img src="docs/module-gifs/motorobots.gif" alt="MotoRobots running in SC-16" width="320"> |
| Netgame Demo | <img src="docs/module-gifs/netgame-demo.gif" alt="Netgame Demo running in SC-16" width="320"> |
| PacMan | <img src="docs/module-gifs/pacman.gif" alt="PacMan running in SC-16" width="320"> |
| Paint | <img src="docs/module-gifs/paint.gif" alt="Paint running in SC-16" width="320"> |
| SC-16 showcase | <img src="docs/module-gifs/sc-16-showcase.gif" alt="SC-16 showcase running in SC-16" width="320"> |
| Space Invaders | <img src="docs/module-gifs/space-invaders.gif" alt="Space Invaders running in SC-16" width="320"> |
| SpaceCave | <img src="docs/module-gifs/spacecave.gif" alt="SpaceCave running in SC-16" width="320"> |
| Text Demo | <img src="docs/module-gifs/text-demo.gif" alt="Text Demo running in SC-16" width="320"> |
<!-- module-gif-gallery:end -->

Regenerate the animations for every module folder with a runnable C entry point:
```console
python tools/generate_module_gifs.py
```
Each generated GIF loops continuously with a 15-second cycle by default.
The source module runs for 10 seconds during capture. Use
`--run-seconds` to change the capture window and `--gif-seconds` to set the
total GIF duration; `--fps` and `--capture-fps` adjust sampling. The script
discovers modules from `modules/` each time it runs and updates this gallery.
