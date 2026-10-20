# Python-to-Rust migration

This document inventories the Python files still present in the repository and
orders the remaining work by dependency. The target is to run the SC-16 emulator
and its supported build/runtime workflows with Rust, then remove Python
implementations and their Python-only tests. It does not claim full C90
compatibility as a prerequisite; implement the C needed by the SC-16 modules and
grow support against real module builds.

## Current inventory

There are 32 Python files in `src/`, `tools/`, `modules/`, and `tests/`.
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

| Python file | Status | Why it cannot be deleted yet |
|---|---|---|
| [`src/SuomiCPU.py`](../src/SuomiCPU.py) | Rust exists — Python consumers remain | Rust's CPU, GUI runtime, BIOS selector, and program launcher exist, but Python tests and tools still instantiate the Python emulator. Switch those consumers to Rust and verify module/runtime parity before retiring this compatibility implementation. |
| [`src/bios.py`](../src/bios.py) | Rust exists — Python consumers remain | Rust implements module discovery, BIOS selection, and booting. Python BIOS tests and `src/SuomiCPU.py` still use this module. |
| [`src/console_device.py`](../src/console_device.py) | Blocking — incomplete port | Rust has console MMIO input/output, but not the Python device's complete terminal window, scrollback, bounded buffers, and logging behavior. Port or explicitly retire those behaviors and move tests off Python first. |
| [`src/isa.py`](../src/isa.py) | Rust exists — Python consumers remain | ISA metadata is available in Rust, but the Python assembler and emulator still import Python definitions. Remove it with those consumers after encoding/execution parity is covered. |
| [`src/lib_disk.py`](../src/lib_disk.py) | Rust exists — Python consumers remain | Rust disk behavior and parity tests exist, but the Python emulator, Tank-16 loader, and Python tests still import this implementation. |
| [`src/program_loader.py`](../src/program_loader.py) | Rust exists — Python consumers remain | Tank-16 runtime compilation now routes through `sc16tank build-runtime`; the Python loader still serves the Python emulator and other Python callers. Retire it with those consumers. |
| [`src/sc16_jit.py`](../src/sc16_jit.py) | Rust exists — Python consumer remains | The Rust CPU replaces the need for this Numba execution backend, but `src/SuomiCPU.py` still optionally imports it. Remove it when the Python emulator is retired and the Rust CPU parity suite is sufficient. |
| [`src/__init__.py`](../src/__init__.py) | Package marker | Python test/tool imports still use `src` as a package. Remove after those imports disappear. |

## Build, network, and developer tools

| Python file | Status | Why it cannot be deleted yet |
|---|---|---|
| [`tools/assembler.py`](../tools/assembler.py) | Rust exists — Python consumers remain | The Rust assembler is implemented, but the Python C compiler, emulator, smoke tool, and tests still use this module. Migrate those callers before removal. |
| [`tools/c_compiler.py`](../tools/c_compiler.py) | Blocking — incomplete port | A Rust C compiler exists, but it does not yet build all module programs. The Python loader, Tank-16 asset builder, tests, and developer tools depend on the Python compiler. Port the needed module subset and verify real module builds before switching callers. |
| [`tools/sc16net.py`](../tools/sc16net.py) | Rust exists — Python consumer remains | Rust networking behavior is implemented and tested; the Python emulator and network tests still import this transport. Switch integration tests and retire the Python emulator before deletion. |
| [`tools/__init__.py`](../tools/__init__.py) | Package marker | Python tool imports and tests still use `tools` as a package. Remove after those consumers are migrated. |

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

| Python file | Status | Coverage/reason to retain |
|---|---|---|
| [`tests/__init__.py`](../tests/__init__.py) | Package marker | Needed by current Python test imports; remove with the remaining Python test suite. |
| [`tests/gfx_harness.py`](../tests/gfx_harness.py) | Test/helper — retain for coverage | Shared Pygame/Python-emulator harness used by graphics and game integration tests. Replace with a Rust runtime test harness or captured Rust frames before removal. |
| [`tests/test_assembler.py`](../tests/test_assembler.py) | Test/helper — retain for coverage | Assembler encoding and behavior checks; preserve them in Rust assembler tests before removal. |
| [`tests/test_assembler_coverage.py`](../tests/test_assembler_coverage.py) | Test/helper — retain for coverage | Additional Python assembler cases; port any cases not covered by `rust/src/assembler.rs` tests. |
| [`tests/test_bios.py`](../tests/test_bios.py) | Test/helper — retain for coverage | Python BIOS discovery, selection, and boot behaviors; ensure Rust runtime tests cover the same cases. |
| [`tests/test_c_compiler.py`](../tests/test_c_compiler.py) | Test/helper — retain for coverage | Python compiler behavior; move supported-language and diagnostic cases to Rust tests as the Rust compiler replaces it. |
| [`tests/test_console_device.py`](../tests/test_console_device.py) | Test/helper — retain for coverage | Console buffers, terminal presentation, and device behavior; port or intentionally drop coverage tied only to the Python terminal. |
| [`tests/test_display.py`](../tests/test_display.py) | Test/helper — retain for coverage | Pygame display rendering and emulator integration; replace with Rust runtime/frame tests. |
| [`tests/test_extended_opcodes.py`](../tests/test_extended_opcodes.py) | Test/helper — retain for coverage | Extended ISA execution and graphics behavior; confirm Rust CPU coverage, then move remaining graphics cases to Rust. |
| [`tests/test_finland_map.py`](../tests/test_finland_map.py) | Test/helper — retain for coverage | Finland map/GIS generator and asset output; port generator checks or retain a clearly documented Python authoring-tool test. |
| [`tests/test_gpu_games.py`](../tests/test_gpu_games.py) | Test/helper — retain for coverage | Game-level GPU behavior through the Python emulator; replace with Rust CPU/runtime integration tests. |
| [`tests/test_hertta3d.py`](../tests/test_hertta3d.py) | Test/helper — retain for coverage | Hertta3D game integration through the Python harness; port equivalent execution/render checks to Rust. |
| [`tests/test_isa_opcodes.py`](../tests/test_isa_opcodes.py) | Test/helper — retain for coverage | Exhaustive Python assembler/CPU opcode combinations; migrate uncovered combinations to Rust assembler and CPU tests. |
| [`tests/test_jit_core.py`](../tests/test_jit_core.py) | Test/helper — retain for coverage | Checks Numba JIT equivalence against the Python interpreter. Replace with Rust CPU differential/parity tests; the JIT itself is superseded. |
| [`tests/test_map_editor.py`](../tests/test_map_editor.py) | Test/helper — retain for coverage | Exercises Rust CLI map loading, validation, saving, and the editor entry point. Rust unit tests cover map mutation and serialization contracts. |
| [`tests/test_natural_maps.py`](../tests/test_natural_maps.py) | Test/helper — retain for coverage | Runs the Rust generator and validates generated maps through the Rust CLI. |
| [`tests/test_net.py`](../tests/test_net.py) | Test/helper — retain for coverage | Python transport and emulator networking integration; retain/port end-to-end coverage using the Rust networking path. |
| [`tests/test_spacecave.py`](../tests/test_spacecave.py) | Test/helper — retain for coverage | Spacecave network/game integration through Python CPU and transport; replace with Rust integration coverage. |
| [`tests/test_storage_bios.py`](../tests/test_storage_bios.py) | Test/helper — retain for coverage | Python disk and BIOS integration, including behaviors that need parity checks against Rust storage/runtime. |
| [`tests/test_tank16.py`](../tests/test_tank16.py) | Test/helper — retain for coverage | Tank-16 gameplay integration still uses the Python emulator/compiler, but map and asset operations now run through Rust CLI commands. |

## Ordered migration tasks

Complete tasks in order unless a task is explicitly independent. Do not remove a
Python implementation until the Rust replacement is used by the actual entry
point and its required tests pass.

1. **Finish Rust module compilation and program loading.**
   - Complete the C subset required by the repository's runnable modules in
     `rust/src/c_compiler.rs`; use the Rust assembler rather than
     `tools/assembler.py`.
   - Reproduce the remaining useful `src/program_loader.py` behavior. Its
     Tank-16 dynamic-asset path now delegates to the Rust `sc16tank` CLI.
   - `sc16build smoke-modules` now discovers modules and orchestrates this
     build check; C translation remains delegated to the retained Python
     compiler.
   - Gate deletion of `tools/c_compiler.py`, `tools/assembler.py`, and
     `src/program_loader.py` on builds of selected real module programs and
     migrated regression tests.

2. **Port and route the Tank-16 map/asset workflow through Rust.**
   - Rust now owns map data validation, editing, runtime encoding, asset
     generation, and the graphical editor.
   - Use `sc16objects` for object-atlas generation; small rasterization
     differences from the former Python art are acceptable. Finland generation
     and atlas-extension tests now exercise the Rust CLI.
   - Keep C compilation in Python as an explicit migration-boundary exception;
     Tank-16 map/asset callers and tests now use Rust.

3. **Close Rust emulator/device parity and switch the default workflows.**
   - Compare `src/SuomiCPU.py`, `src/bios.py`, `src/console_device.py`,
     `src/lib_disk.py`, `src/isa.py`, `src/sc16_jit.py`, and
     `tools/sc16net.py` with the Rust CPU/runtime/device implementations.
   - Fill behavior gaps and move BIOS, console, storage, ISA, graphics, and
     networking regressions into Rust tests.
   - Switch launch scripts, build commands, and user documentation to Rust;
     verify supported games and BIOS modules without Python.
   - Retire the Python emulator and then remove its now-unused support modules.

4. **Migrate Python test harnesses and integration coverage.**
   - Replace `tests/gfx_harness.py` with Rust test helpers and convert
     emulator-dependent tests (`test_display`, `test_extended_opcodes`,
     `test_gpu_games`, `test_hertta3d`, `test_jit_core`, `test_net`,
     `test_spacecave`, `test_storage_bios`, and `test_tank16`) to Rust coverage.
   - Move assembler/compiler/BIOS and map validation cases into their relevant
     Rust test modules; keep tests for behaviors until they are ported or
     explicitly retired.
   - Remove the Python test package markers when no Python tests remain.

5. **Resolve optional authoring and gallery utilities.**
   - The README gallery generator now runs the Rust CPU and writes GIFs in
     Rust; it uses the retained Python C compiler only for source translation.
   - Decide and document whether the remaining Python developer utilities are
     required in the Rust-only workflow.

## Completion criteria

- All supported emulator, BIOS, compiler, networking, storage, and module
  workflows use Rust.
- Required Python runtime/build entry points and their Python-only dependencies
  have been removed; optional authoring tools have an explicit disposition.
- Rust tests cover the retained behavior, including module build and Tank-16
  runtime map/assets.
- Documentation and examples no longer direct users to removed Python
  commands.
- The remaining Python-file inventory is empty, or every intentionally retained
  file is explicitly called out as an accepted exception.
