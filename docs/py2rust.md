# Python-to-Rust migration

This document inventories the Python files still present in the repository and
orders the remaining work by dependency. The target is to run the SC-16 emulator
and its supported build/runtime workflows with Rust, then remove Python
implementations and their Python-only tests. It does not claim full C90
compatibility as a prerequisite; implement the C needed by the SC-16 modules and
grow support against real module builds.

## Current inventory

There are 39 Python files in `src/`, `tools/`, `modules/`, and `tests/`.
Generated natural maps and the Tank-16 sprite viewer have already been moved to
Rust and their Python implementations removed. A Rust equivalent alone is not
enough to delete a Python file while a Python runtime, build path, test, or
documented workflow still imports or invokes it.

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
| [`src/program_loader.py`](../src/program_loader.py) | Blocking — incomplete port | Rust dispatches assembly, C, binary, and data-file loading, but the Python loader also builds Tank-16's dynamic runtime graphics/assets path. Port and route that workflow through Rust before removal. |
| [`src/sc16_jit.py`](../src/sc16_jit.py) | Rust exists — Python consumer remains | The Rust CPU replaces the need for this Numba execution backend, but `src/SuomiCPU.py` still optionally imports it. Remove it when the Python emulator is retired and the Rust CPU parity suite is sufficient. |
| [`src/__init__.py`](../src/__init__.py) | Package marker | Python test/tool imports still use `src` as a package. Remove after those imports disappear. |

## Build, network, and developer tools

| Python file | Status | Why it cannot be deleted yet |
|---|---|---|
| [`tools/assembler.py`](../tools/assembler.py) | Rust exists — Python consumers remain | The Rust assembler is implemented, but the Python C compiler, emulator, smoke tool, and tests still use this module. Migrate those callers before removal. |
| [`tools/c_compiler.py`](../tools/c_compiler.py) | Blocking — incomplete port | A Rust C compiler exists, but it does not yet build all module programs. The Python loader, Tank-16 asset builder, tests, and developer tools depend on the Python compiler. Port the needed module subset and verify real module builds before switching callers. |
| [`tools/sc16net.py`](../tools/sc16net.py) | Rust exists — Python consumer remains | Rust networking behavior is implemented and tested; the Python emulator and network tests still import this transport. Switch integration tests and retire the Python emulator before deletion. |
| [`tools/smoke_compile_modules.py`](../tools/smoke_compile_modules.py) | Blocking — Python build path | It compiles BIOS modules through the Python BIOS/program loader. Replace it with a Rust smoke-build command that reports per-module results before removing this useful validation workflow. |
| [`tools/generate_module_gifs.py`](../tools/generate_module_gifs.py) | Python tooling — not ported | It runs the Python emulator and captures frames for the README gallery. Either port the capture workflow to a Rust runtime/frame-export interface or deliberately replace/remove this optional generator and update its documentation. |
| [`tools/__init__.py`](../tools/__init__.py) | Package marker | Python tool imports and tests still use `tools` as a package. Remove after those consumers are migrated. |

## Tank-16 source and asset tools

| Python file | Status | Why it cannot be deleted yet |
|---|---|---|
| [`modules/Tank-16/tank16_map_loader.py`](../modules/Tank-16/tank16_map_loader.py) | Blocking — incomplete port | Rust now handles map validation, runtime map encoding, asset conversion, and compatible-map discovery. Tank-16's build path still cannot compile its real C source with the Rust C compiler, so the Python build dependency remains. |
| [`modules/Tank-16/map_editor.py`](../modules/Tank-16/map_editor.py) | Rust exists — Python consumers remain | Rust has map validation/load/save/serialization and a basic graphical editor. The Python editor still provides richer editing workflows and its tests remain the active coverage; migrate those consumers and editor features before removal. |
| [`modules/Tank-16/generate_finland_map.py`](../modules/Tank-16/generate_finland_map.py) | Python tooling — not ported | The Finland map and atlas generator performs GIS projection/rasterization and Pygame drawing; it also imports `map_editor.py`. Port the data-processing and atlas-generation workflow, or explicitly classify it as a retained offline Python authoring tool. |
| [`modules/Tank-16/generate_object_tileset.py`](../modules/Tank-16/generate_object_tileset.py) | Rust exists — Python consumer remains | Rust now provides `sc16objects` to generate the procedural 6-by-8 object atlas. Keep the Python implementation until generated-art parity is checked and the documented asset workflow uses the Rust command. |

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
| [`tests/test_map_editor.py`](../tests/test_map_editor.py) | Test/helper — retain for coverage | Map validation, serialization, and editor behaviors; port these contracts to the Rust map/editor implementation. |
| [`tests/test_module_gif_generator.py`](../tests/test_module_gif_generator.py) | Test/helper — retain for coverage | Optional Python gallery capture/GIF output; port with the generator or remove alongside an explicit gallery-workflow decision. |
| [`tests/test_natural_maps.py`](../tests/test_natural_maps.py) | Test/helper — retain for coverage | Runs the Rust generator but still uses the Python map editor to validate output. Move runtime-map validation assertions to Rust before removing this cross-language test. |
| [`tests/test_net.py`](../tests/test_net.py) | Test/helper — retain for coverage | Python transport and emulator networking integration; retain/port end-to-end coverage using the Rust networking path. |
| [`tests/test_spacecave.py`](../tests/test_spacecave.py) | Test/helper — retain for coverage | Spacecave network/game integration through Python CPU and transport; replace with Rust integration coverage. |
| [`tests/test_storage_bios.py`](../tests/test_storage_bios.py) | Test/helper — retain for coverage | Python disk and BIOS integration, including behaviors that need parity checks against Rust storage/runtime. |
| [`tests/test_tank16.py`](../tests/test_tank16.py) | Test/helper — retain for coverage | Tank-16 loader, map, graphics, and gameplay integration through Python; replace after Rust's Tank-16 build/runtime path is complete. |

## Ordered migration tasks

Complete tasks in order unless a task is explicitly independent. Do not remove a
Python implementation until the Rust replacement is used by the actual entry
point and its required tests pass.

1. **Finish Rust module compilation and program loading.**
   - Complete the C subset required by the repository's runnable modules in
     `rust/src/c_compiler.rs`; use the Rust assembler rather than
     `tools/assembler.py`.
   - Reproduce the remaining useful `src/program_loader.py` behavior, including
     Tank-16's specialized dynamic-asset build path from
     `modules/Tank-16/tank16_map_loader.py`.
   - Replace `tools/smoke_compile_modules.py` with a Rust smoke-build workflow.
   - Gate deletion of `tools/c_compiler.py`, `tools/assembler.py`, and
     `src/program_loader.py` on builds of selected real module programs and
     migrated regression tests.

2. **Port and route the Tank-16 map/asset workflow through Rust.**
   - Implement map validation/load/save/serialization to cover the contracts in
     `modules/Tank-16/map_editor.py` and `tests/test_map_editor.py`.
   - Port the object tileset generator and the Finland GIS/map/atlas generator,
     or document a deliberate boundary for retaining offline authoring tools.
   - Remove Python's Tank-16 build and editor dependencies only after generated
     assets/maps are readable by the Rust game path and parity tests pass.

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
   - Port `tools/generate_module_gifs.py` to capture frames from the Rust
     runtime, or remove/replace the optional gallery generation workflow.
   - Decide and document whether all asset-authoring tools must be Rust. If yes,
     port `map_editor.py`, `generate_finland_map.py`, and
     `generate_object_tileset.py`; otherwise keep them as an explicitly scoped
     Python-only content pipeline rather than presenting the migration as
     Python-free.

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
