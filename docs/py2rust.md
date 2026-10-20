# Python-to-Rust migration

This document inventories the Python files still present in the repository and
orders the remaining work by dependency. The target is to run the SC-16 emulator
and its supported build/runtime workflows with Rust, then remove Python
implementations and their Python-only tests. It does not claim full C90
compatibility as a prerequisite; implement the C needed by the SC-16 modules and
grow support against real module builds.

## Current inventory

There are 27 Python files in `src/`, `tools/`, `modules/`, and `tests/`.
Generated maps, Tank-16 atlas tools, map loading, and the graphical map editor
now use Rust implementations. Python remains in the emulator and developer
tests while those larger migrations continue.

Status meanings:

- **Blocking — incomplete port:** required behavior does not yet have a
  verified Rust replacement.
- **Rust exists — Python consumers remain:** equivalent Rust functionality is
  present, but Python runtime/build/test code still depends on this file.
- **Python tooling — not ported:** optional or content-authoring functionality
  still runs only in Python.
- **Test/helper — retain for coverage:** test code, not shipped runtime code;
  replace its useful coverage with Rust tests before removal.
- **Package marker:** empty import support; remove after the package's Python
  consumers are gone.

## Runtime and shared source

- [ ] **Migrate [src/SuomiCPU.py](../src/SuomiCPU.py)** — Rust exists; Python consumers remain.
  - Why it remains: Python tests and tools still instantiate the Python emulator. Switch those consumers to Rust and verify module/runtime parity before retiring this compatibility implementation.
- [ ] **Migrate [src/bios.py](../src/bios.py)** — Rust exists; Python consumers remain.
  - Why it remains: Python BIOS tests and `src/SuomiCPU.py` still use the module; Rust already implements module discovery, BIOS selection, and booting.
- [ ] **Migrate [src/console_device.py](../src/console_device.py)** — Rust exists; Python consumers remain.
  - Why it remains: the Python emulator still uses this class for its Tk window, file logging, and host lifecycle controls, even though Rust now covers console MMIO, text-grid, and scrollback state.
- [ ] **Migrate [src/isa.py](../src/isa.py)** — Rust exists; Python consumers remain.
  - Why it remains: the Python assembler and emulator still import Python ISA definitions. Remove them with those consumers after parity is covered.
- [ ] **Migrate [src/lib_disk.py](../src/lib_disk.py)** — Rust exists; Python consumers remain.
  - Why it remains: the Python emulator, Tank-16 loader, and Python tests still import this implementation.
- [ ] **Migrate [src/program_loader.py](../src/program_loader.py)** — Rust exists; Python consumers remain.
  - Why it remains: the Python loader still serves the Python emulator and other Python callers; Tank-16 runtime compilation already routes through `sc16tank build-runtime`.
- [ ] **Remove [src/__init__.py](../src/__init__.py)** — package marker.
  - Why it remains: Python test/tool imports still use `src` as a package.

## Build, network, and developer tools

- [ ] **Migrate [tools/assembler.py](../tools/assembler.py)** — Rust exists; Python consumers remain.
  - Why it remains: the Python C compiler, emulator, smoke tool, and tests still use this module. Migrate those callers before removal.
- [ ] **Port [tools/c_compiler.py](../tools/c_compiler.py)** — blocking; Rust port is incomplete.
  - Why it remains: Rust does not yet build all module programs, and the Python loader, Tank-16 asset builder, tests, and developer tools still depend on it.
- [ ] **Migrate [tools/sc16net.py](../tools/sc16net.py)** — Rust exists; Python consumer remains.
  - Why it remains: the Python emulator and network tests still import this transport. Switch integration tests and retire the emulator before deletion.
- [ ] **Remove [tools/__init__.py](../tools/__init__.py)** — package marker.
  - Why it remains: Python tool imports and tests still use `tools` as a package.

Rust Tank-16 tools currently available:

```powershell
cargo run --bin sc16tank -- validate-map modules\Tank-16\disks\maps\FINLAND.json
cargo run --bin sc16tank -- load-map modules\Tank-16\disks\maps\FINLAND.json
cargo run --bin sc16tank -- save-map modules\Tank-16\disks\maps\FINLAND.json --output-dir modules\Tank-16\disks\maps
cargo run --bin sc16tank -- build-runtime --output tank16.sc16img
cargo run --bin sc16tank -- build-map modules\Tank-16\disks\maps\PERUS-1.json
cargo run --bin sc16tank -- generate-finland-map --countries countries.geojson --lakes lakes.geojson --roads roads.geojson --preview finland-preview.png --output modules\Tank-16\disks\maps\FINLAND.json
cargo run --bin sc16tank -- extend-atlas
cargo run --bin sc16objects
cargo run --bin sc16mapedit -- --map modules\Tank-16\disks\maps\FINLAND.json
cargo run --bin sc16mapedit -- --name ARENA --width 10 --height 8
```

Compile every bootable BIOS module with the Rust smoke-build orchestrator
(the explicitly retained Python C compiler performs the C translation):

```powershell
cargo run --bin sc16build -- smoke-modules
```

The Finland generator accepts optional `--railways`, `--cities`, and `--size`
arguments. Its tests use local synthetic GeoJSON and do not require network
access. `sc16tank` also provides Rust-backed map loading/saving, runtime
encoding, asset inspection, and C-header generation. The Rust editor opens an
existing map with `--map`, or creates one with `--name`, `--width`, and
`--height`; Ctrl+S saves it. Static builds with round-map variants still
encounter a pre-existing C90 declaration-order diagnostic in the retained C
compiler. The default dynamic Tank-16 build path and the static single-map
build have both compiled successfully.

## Python tests and test helpers

These files do not block launching the Rust emulator directly. They are kept
until their behavioral coverage is represented by Rust tests or an intentional
acceptance-test workflow; deleting tests merely to reach zero `.py` files is not
considered migration.

- [ ] **Remove [tests/__init__.py](../tests/__init__.py)** — package marker.
  - Why it remains: current Python test imports need it until the Python test suite is retired.
- [ ] **Replace [tests/gfx_harness.py](../tests/gfx_harness.py)** — shared test helper.
  - Coverage to preserve: Pygame/Python-emulator graphics and game integration; use a Rust runtime harness or captured Rust frames.
- [x] **Mirror core [tests/test_assembler.py](../tests/test_assembler.py) regressions in Rust.**
  - Rust assembler tests now cover the Python encoding, sparse-address, data,
    control-flow, and stack-instruction regressions. The Python test remains
    intentionally because the Python assembler is an accepted migration
    exception and still needs its own regression coverage.
- [ ] **Port [tests/test_assembler_coverage.py](../tests/test_assembler_coverage.py)** — assembler coverage.
  - Coverage to preserve: Python assembler cases not yet covered by `rust/src/assembler.rs`.
- [ ] **Port [tests/test_bios.py](../tests/test_bios.py)** — BIOS/runtime coverage.
  - Coverage to preserve: BIOS discovery, selection, and boot behaviors in Rust runtime tests.
- [ ] **Port [tests/test_c_compiler.py](../tests/test_c_compiler.py)** — compiler coverage.
  - Coverage to preserve: supported-language behavior and diagnostics as the Rust compiler replaces Python.
- [ ] **Port [tests/test_console_device.py](../tests/test_console_device.py)** — console coverage.
  - Coverage to preserve: Python emulator and Tk console-window integration; Rust already tests MMIO buffering, bounds, text rendering state, and scrollback.
- [ ] **Port [tests/test_display.py](../tests/test_display.py)** — display coverage.
  - Coverage to preserve: Pygame display rendering and emulator integration through Rust runtime/frame tests.
- [ ] **Port [tests/test_extended_opcodes.py](../tests/test_extended_opcodes.py)** — ISA/graphics coverage.
  - Coverage to preserve: confirm Rust CPU coverage and move any remaining graphics cases.
- [ ] **Port [tests/test_gpu_games.py](../tests/test_gpu_games.py)** — game/GPU coverage.
  - Coverage to preserve: game-level GPU behavior through Rust CPU/runtime integration tests.
- [ ] **Port [tests/test_hertta3d.py](../tests/test_hertta3d.py)** — game/render coverage.
  - Coverage to preserve: Hertta3D execution and rendering through Rust.
- [ ] **Port [tests/test_isa_opcodes.py](../tests/test_isa_opcodes.py)** — ISA coverage.
  - Coverage to preserve: migrate uncovered assembler/CPU opcode combinations to Rust.
- [ ] **Port [tests/test_net.py](../tests/test_net.py)** — networking coverage.
  - Coverage to preserve: end-to-end coverage through the Rust networking path.
- [ ] **Port [tests/test_spacecave.py](../tests/test_spacecave.py)** — network/game coverage.
  - Coverage to preserve: Spacecave integration through the Rust CPU and transport.
- [ ] **Port [tests/test_storage_bios.py](../tests/test_storage_bios.py)** — storage/BIOS coverage.
  - Coverage to preserve: parity behaviors through Rust storage and runtime.
- [ ] **Port [tests/test_tank16.py](../tests/test_tank16.py)** — Tank-16 gameplay coverage.
  - Coverage to preserve: gameplay currently uses the Python emulator/compiler; map and asset operations already use Rust CLI commands.

## Ordered migration tasks

Complete tasks in order unless a task is explicitly independent. Do not remove a
Python implementation until the Rust replacement is used by the actual entry
point and its required tests pass.

- [ ] **Finish Rust module compilation and program loading.**
   - [ ] Complete the C subset required by the repository's runnable modules in
     `rust/src/c_compiler.rs`; use the Rust assembler rather than
     `tools/assembler.py`.
   - [ ] Reproduce the remaining useful `src/program_loader.py` behavior. Its
     Tank-16 dynamic-asset path now delegates to the Rust `sc16tank` CLI.
   - [x] Add Rust discovery/orchestration in `sc16build smoke-modules`.
     C translation remains delegated to the retained Python compiler.
   - [ ] Gate deletion of `tools/c_compiler.py`, `tools/assembler.py`, and
     `src/program_loader.py` on builds of selected real module programs and
     migrated regression tests.

- [x] **Port and route the Tank-16 map/asset workflow through Rust.**
   - Rust now owns map data validation, editing, runtime encoding, asset
     generation, and the graphical editor.
   - Use `sc16objects` for object-atlas generation; small rasterization
     differences from the former Python art are acceptable. Finland generation
     and atlas-extension tests now exercise the Rust CLI.
   - Keep C compilation in Python as an explicit migration-boundary exception;
     Tank-16 map/asset callers and tests now use Rust.
   - Map-editor and natural-map CLI integration coverage now runs as Rust
     Cargo integration tests; the redundant Python subprocess tests were
     removed.
   - Finland map metadata/encoding, validation, atlas extension, GeoJSON
     generation, and 100x100 tilemap upload are covered by Rust tests.

- [ ] **Close Rust emulator/device parity and switch the default workflows.**
   - [ ] Compare `src/SuomiCPU.py`, `src/bios.py`, `src/console_device.py`,
     `src/lib_disk.py`, `src/isa.py`, and `tools/sc16net.py` with the Rust
     CPU/runtime/device implementations. The optional Python Numba JIT has
     been removed; the Rust CPU is the primary execution engine.
   - [ ] Retire the Python Tk terminal, console logging, and emulator
     integration alongside the Python emulator. Rust already provides console
     MMIO buffers and text/scrollback state.
   - [ ] Fill remaining behavior gaps and move BIOS, storage, ISA, graphics, and
     networking regressions into Rust tests.
   - [ ] Switch launch scripts, build commands, and user documentation to Rust;
     verify supported games and BIOS modules without Python.
   - [ ] Retire the Python emulator and then remove its now-unused support modules.

- [ ] **Migrate Python test harnesses and integration coverage.**
   - [ ] Replace `tests/gfx_harness.py` with Rust test helpers and convert
     emulator-dependent tests (`test_display`, `test_extended_opcodes`,
     `test_gpu_games`, `test_hertta3d`, `test_net`,
     `test_spacecave`, `test_storage_bios`, and `test_tank16`) to Rust coverage.
   - [ ] Move assembler/compiler/BIOS cases into their relevant
     Rust test modules; keep tests for behaviors until they are ported or
     explicitly retired.
   - [ ] Remove the Python test package markers when no Python tests remain.

- [ ] **Resolve optional authoring and gallery utilities.**
   - [x] Port the README gallery generator to Rust; source translation uses
     the retained Python C compiler.
   - [ ] Decide and document whether the remaining Python developer utilities are
     required in the Rust-only workflow.

## Completion criteria

- [ ] All supported emulator, BIOS, compiler, networking, storage, and module
  workflows use Rust.
- [ ] Required Python runtime/build entry points and their Python-only dependencies
  have been removed; optional authoring tools have an explicit disposition.
- [ ] Rust tests cover the retained behavior, including module build and Tank-16
  runtime map/assets.
- [ ] Documentation and examples no longer direct users to removed Python
  commands.
- [ ] The remaining Python-file inventory is empty, or every intentionally retained
  file is explicitly called out as an accepted exception.
