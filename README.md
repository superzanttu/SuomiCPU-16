# SuomiCPU-16

SuomiCPU-16 is a Python emulator for the SC-16, a small, byte-addressed machine.

## Features
- Full emulation of the SC-16 architecture.
- Integrated assembler for SC-16 assembly language.
- C compiler for a subset of C89 targeting SC-16.
- Basic graphics and text-mode support.

## Getting Started
Run an example from the repository root:
```console
python main.py examples/factorial.c
```

For a live analog clock with a digital hours, minutes, and seconds readout, run:
```console
python main.py examples/anaclock.c
```

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
