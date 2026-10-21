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
- **Rust port complete — Python consumers remain:** Rust functionality and
  focused Rust regression coverage exist, but Python runtime/build/test code
  still depends on this file. This is not a deletion or default-workflow
  cutover.
- **Python tooling — not ported:** optional or content-authoring functionality
  still runs only in Python.
- **Test/helper — retain for coverage:** test code, not shipped runtime code;
  replace its useful coverage with Rust tests before removal.
- **Package marker:** empty import support; remove after the package's Python
  consumers are gone.

## Runtime and shared source

- [x] **Port [src/SuomiCPU.py](../src/SuomiCPU.py) behavior to Rust** in
  `rust/src/cpu.rs` and `rust/src/runtime.rs`. CPU execution, interrupts,
  devices, headless runtime, and BIOS launch behavior have Rust regression
  coverage. The Python emulator remains until its callers and launch workflow
  are switched and the Tk/host lifecycle is retired.
- [x] **Port [src/bios.py](../src/bios.py) behavior to Rust** in
  `rust/src/runtime.rs`. Module discovery, BIOS selection/input, disk mounting,
  and program launch are covered by Rust runtime tests. Python BIOS tests and
  the Python emulator still import the old implementation.
- [x] **Port [src/console_device.py](../src/console_device.py) device behavior
  to Rust** in `rust/src/console.rs` and `rust/src/cpu.rs`. Rust covers console
  MMIO, FIFO bounds, control bits, text rows, scrollback, and reset semantics.
  The legacy Tk window, host logging, and lifecycle controls are not part of
  the Rust device port and remain a cutover item.
- [x] **Port [src/isa.py](../src/isa.py) definitions to Rust** in
  `rust/src/isa.rs`. Opcode tables, aliases, extensions, and exhaustive
  register-form encodings have Rust tests. Keep the Python definitions until
  the accepted Python assembler no longer imports them.
- [x] **Port [src/lib_disk.py](../src/lib_disk.py) disk behavior to Rust** in
  `rust/src/disk.rs` and `rust/src/cpu.rs`. Persistent blocks, write protection,
  file listing/reads, path validation, and disk MMIO are covered. Python
  emulator, loader, and tests still depend on the Python module.
- [x] **Port [src/program_loader.py](../src/program_loader.py) loading behavior
  to Rust** in `rust/src/image.rs` and `rust/src/runtime.rs`. Rust supports
  binary, segmented image, assembly, and its supported C-source loading paths;
  Tank-16 runtime generation uses `sc16tank build-runtime`. The Python loader
  remains used by legacy Python callers and is not yet removed.
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

Check every bootable BIOS C source against the Rust compiler with the Rust
smoke-build orchestrator:

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
compiler. `sc16build smoke-modules` does not invoke Python; it reports modules
whose C exceeds the current Rust compiler subset as failures. Rust compiler
parity for all modules is still outstanding; the current Rust-only smoke run
builds Netgame Demo and reports the other 16 modules as unsupported.

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
- [x] **Mirror shared [tests/test_assembler_coverage.py](../tests/test_assembler_coverage.py) cases in Rust.**
  - Rust tests cover memory boundaries, entry-point defaults, malformed
    directives, labels, operands, numeric forms, comments, and UTF-8 BOM/
    include error handling. The Python suite remains for the accepted Python
    assembler's own behavioral coverage.
- [x] **Mirror BIOS module discovery from [tests/test_bios.py](../tests/test_bios.py) in Rust.**
  - Rust tests now cover module sorting, C source selection, empty/multiple
    source errors, descriptions, UTF-8 BOM files, and missing directories.
- [ ] **Port the remaining BIOS UI/boot tests from [tests/test_bios.py](../tests/test_bios.py).**
  - [x] Verify keyboard module-selection bounds, scroll reachability, disk
        selection cycling, disk mounting, and module-discovery errors in Rust.
  - [x] Add mouse module selection and disk cycling with fullscreen
        aspect-ratio coordinate mapping in Rust.
  - [x] Launch the selected module on a same-row mouse double-click within the
        legacy 400 ms interval.
  - [x] Scroll the module list with the mouse wheel while keeping the selected
        module visible.
  - [x] Keep module/boot errors visible in the BIOS selector, and show the
        empty-module state instead of exiting during startup.
  - [ ] Port full emulator startup/reset-to-BIOS workflows.
- [ ] **Port [tests/test_c_compiler.py](../tests/test_c_compiler.py)** — compiler coverage.
  - Coverage to preserve: supported-language behavior and diagnostics as the Rust compiler replaces Python.
  - [x] Compile and execute fixed-size automatic integer and byte arrays,
        including brace initialization, nested indexing expressions, and
        stack-frame restoration across helper calls.
  - [x] Compile fitting string-literal initializers for global and automatic
        character arrays, including escaped byte values, exact-fit arrays, and
        zero-filled trailing capacity.
  - [x] Infer one-dimensional global and automatic array bounds from brace
        initializers and character string literals; reject incomplete arrays
        whose size cannot be inferred.
  - [x] Compile row-major global and automatic multidimensional arrays with
        nested brace initializers, reads, writes, and inferred total storage.
  - [x] Parse typedef-based `struct` and `union` declarations, including
        primitive/typedef members and fixed-size member arrays. Record layout,
        initialization, and member code generation remain incomplete.
  - [x] Tokenize member-access dots without treating `.field` as a floating
        constant, and concatenate adjacent string-literal tokens.
  - [x] Link and execute calls to native graphics assembly builtins; `gfx_rect`
        is covered end-to-end through the Rust CPU and GPU memory interface.
  - [x] Compile unsigned 16-bit globals, automatic variables, function
        parameters, and return values; bit patterns use the SC-16 word size.
- [x] **Port console-device state/MMIO coverage from [tests/test_console_device.py](../tests/test_console_device.py) to Rust.**
  - Rust tests cover FIFO/echo control, poll yielding, input bounds/overrun,
    fixed text rows, scrollback, wrapping, and history reset.
- [ ] **Retire Python-specific console tests from [tests/test_console_device.py](../tests/test_console_device.py).**
  - Why they remain: Tk window controls/clipboard/scrolling, Python host
    lifecycle messages, and file logging still belong to the legacy emulator.
- [x] **Port shared display logic from [tests/test_display.py](../tests/test_display.py) to Rust.**
  - Rust tests now cover held-key mapping/releases, frame-budget accumulation,
    screen dimensions, palette conversion, and indexed/paletted framebuffer
    rendering.
- [ ] **Port or retire Python GUI-loop tests from [tests/test_display.py](../tests/test_display.py).**
  - [x] Verify fullscreen pointer coordinate mapping and letterbox rejection
        in Rust for the supported logical canvas size.
  - [x] Keep the last frame visible after HALT until a keypress, matching the
        legacy window lifetime.
  - [x] Toggle pause with F10 and fullscreen with F11 without consuming those
        controls as guest keyboard input.
  - [x] Preserve shifted ASCII text input for letters, digits, and supported
        punctuation.
  - [x] Use the window's typed-character queue for console input, preserving
        UTF-8 text and keyboard layouts instead of deriving console text from
        physical key codes.
  - [x] Forward logical mouse coordinates, left/right click and double-click
        events, and signed per-frame wheel deltas through Rust mouse MMIO.
  - Why remaining tests are Python-specific: pause timing, separate console
    window management, and Pygame-specific display pacing.
- [x] **Port [tests/test_extended_opcodes.py](../tests/test_extended_opcodes.py)** — ISA coverage.
  - Rust assembled-program tests cover shifts/rotates, carry-chain arithmetic,
    32-bit operations and pair validation, stack-pointer control, full-width
    addresses, memory wrapping, signed/unsigned branches, register calls/
    jumps, flag-stack operations, and miscellaneous ALU instructions.
- [ ] **Port [tests/test_gpu_games.py](../tests/test_gpu_games.py)** — game/GPU coverage.
  - [x] Verify the MMIO-to-layer-compositor-to-palette-framebuffer path through
        a Rust runtime integration test.
  - Coverage remaining: game-level behavior through Rust CPU/runtime tests.
- [ ] **Port [tests/test_hertta3d.py](../tests/test_hertta3d.py)** — game/render coverage.
  - [x] Verify that a submitted 3D scene produces projected fill and wireframe
        primitives for the Rust window renderer.
  - Coverage remaining: Hertta3D game execution and gameplay-frame rendering
    through Rust.
- [x] **Port [tests/test_isa_opcodes.py](../tests/test_isa_opcodes.py)** — ISA coverage.
  - [x] Verify all 32 base opcode slots, shared-extension aliases, and mnemonic
        lookup tables in Rust.
  - [x] Cover assembled base register operations, stack round-trips, label loops,
        calls, branches, and stack-relative memory in Rust.
  - [x] Cover classic CPU arithmetic/logic, memory boundaries, and float
        operations with Rust unit tests.
  - [x] Cover extended ALU and 32-bit instruction behavior in assembled Rust
        programs (see the extended-opcode task above).
  - [x] Exercise signed stack offsets, short/far branch boundaries, and invalid
        register/immediate/address inputs with Rust assembler tests.
  - [x] Verify interrupt priority, disabled-pending behavior, and RTI state
        restoration for keyboard and RTC sources.
  - [x] Cover IEEE float NaN/signed-zero comparisons, division by zero, and
        integer-conversion range edges in Rust.
  - [x] Cover every valid IEEE float register-pair combination and classic
        half-float overflow, infinity, zero, NaN, division, and comparison edges.
  - [x] Compare all base register-form instruction encodings byte for byte
        across every register combination.
  - Python's extra exhaustive test loops remain useful to its accepted
    assembler/emulator implementations but no longer block Rust ISA parity.
- [ ] **Port [tests/test_net.py](../tests/test_net.py)** — networking coverage.
  - [x] Simulate a two-node join, slot arbitration, title-matched payload
        exchange, and peer departure through the Rust packet handler.
  - [x] Exercise HELLO, DATA, and BYE delivery over loopback UDP sockets, and
        verify stale peers are removed after the timeout.
  - [x] Verify the eight-player capacity, join rejection when full, and bounded
        inbound message queue in Rust.
- [ ] **Port [tests/test_spacecave.py](../tests/test_spacecave.py)** — network/game coverage.
  - Coverage to preserve: Spacecave integration through the Rust CPU and transport.
- [x] **Port [tests/test_storage_bios.py](../tests/test_storage_bios.py)** — storage/BIOS storage behavior.
  - [x] Verify virtual disk block persistence, default blocks, write protection,
        and invalid disk identifiers in Rust.
  - [x] Verify module-file API chunk reads, file-size reporting, sorted listing,
        module-folder isolation, and traversal rejection through disk MMIO.
  - [x] Verify mounting an existing data disk preserves the loaded program and
        rejects missing disk identifiers.
  - Rust reports disk-command errors through the MMIO status register and
    standard error; Python host-console integration remains with the legacy
    emulator until that host is retired.
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
   - [x] Exercise supported Rust C source loading through the headless runtime
     with a compiled program that executes and halts.
   - [x] Add Rust discovery/orchestration in `sc16build smoke-modules`.
     This aggregate smoke command still delegates C translation to the
     retained Python compiler; it is not evidence of Rust compiler parity.
   - [ ] Keep the Rust-only module-build sweep as the parity gate. The latest
     direct Rust compiler sweep still fails all 16 selected non-Tank-16 BIOS
     C modules; Tank-16 uses its specialized Rust runtime builder and is not
     part of that sweep. Unsupported compiler/runtime features remain, including
     record layout/member code generation, standard-library calls, and broader
     game-specific C semantics. Unsigned 16-bit word support now covers globals,
     locals, function parameters, and returns, but does not yet make a selected
     module build pass.
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
   - [x] Port and test the emulator, BIOS, console device, ISA, disk, and
     program-loading behavior from the six `src/` modules listed above in the
     Rust CPU/runtime/device/image modules. The optional Python Numba JIT has
     been removed; the Rust CPU is the primary execution engine.
   - [ ] Port and test the remaining behavior in `tools/sc16net.py`; switch
     all Python consumers to Rust before retiring the corresponding Python
     compatibility modules.
   - [x] Exercise retained assembler programs through the Rust headless
     runtime, including explicit instruction-budget exhaustion reporting.
   - [ ] Retire the Python Tk terminal, console logging, and emulator
     integration alongside the Python emulator. Rust already provides console
     MMIO buffers and text/scrollback state.
   - [ ] Fill remaining behavior gaps and move BIOS, storage, ISA, graphics, and
     networking regressions into Rust tests.
   - [ ] Switch launch scripts, build commands, and user documentation to Rust;
     verify supported games and BIOS modules without Python.
   - [ ] Retire the Python emulator and then remove its now-unused support modules.

- [ ] **Migrate Python test harnesses and integration coverage.**
   - [ ] Replace `tests/gfx_harness.py` with Rust test helpers and convert the
     remaining emulator-dependent tests (`test_display`, `test_gpu_games`,
     `test_hertta3d`, `test_net`, `test_spacecave`, `test_storage_bios`, and
     `test_tank16`) to Rust coverage.
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
