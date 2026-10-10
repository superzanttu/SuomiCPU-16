# SuomiCPU-16

SuomiCPU-16 is a Python emulator for the SC-16, a small, byte-addressed machine.

## Features
- Full emulation of the SC-16 architecture.
- Integrated assembler for SC-16 assembly language.
- C90-oriented compiler for SC-16 with partial language/preprocessor support
  and selected C90 headers (`stddef.h`, `limits.h`, `float.h`, `ctype.h`,
  `errno.h`, `assert.h`, `stdlib.h`, `string.h`)
  (not a fully conforming C90 implementation; see the
  [user guide](user_guide.md#9-ansi-c90-compiler)).
- Basic graphics and text-mode support.

## Getting Started
Start SC-16 without arguments to open SC-launcher. It lists the `.c` modules in
the `modules/` folder. Click a module, then click **LOAD MODULE** to run it;
the first source line is shown as its description:
```console
python main.py
```
Press **F11** in the launcher or a running program to toggle windowed/fullscreen
display. Start directly in fullscreen with `python main.py --fullscreen` or
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
