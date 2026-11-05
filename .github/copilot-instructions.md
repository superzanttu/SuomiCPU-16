# SuomiCPU-16 Project Guidelines

## Project direction

- Keep user-facing replies concise. Avoid code comments unless they clarify non-obvious behavior or are requested.
- The working product is still the legacy SC-16 emulator and toolchain. The successor target is Kiuas16, an RV64-compatible machine intended to boot xv6 built with this repository's Rust-native toolchain. Do not describe planned firmware, full xv6 support, or other roadmap goals as implemented.
- `docs/sc16nx_todo.adoc` is the sole development checklist. Keep parent milestones unchecked until every child requirement and its acceptance tests pass. Record verified progress, exact commands, target configuration, and observable results there; do not create a competing checklist.
- Preserve SisuOS/SaunaShell as a frozen historical example. Do not extend or port it to Kiuas16.

## Architecture

- The legacy SC-16 emulator, compiler, and assembler are Rust code under `src/emulator/`, `src/c_compiler/`, and `src/assembler/`. Kiuas16 CPU, machine, and devices are under `src/kiuas16/`; the incremental RV64 compiler and assembler paths are `src/c_compiler/rv64.rs` and `src/assembler/rv64.rs`. Rust command-line tools live in `src/tools/`.
- Guest programs live under `programs/`; shared guest headers and runtime sources live under `toolchain/`; firmware assets live under `firmware/`. Follow the owning package's manifest and existing patterns.
- Keep production compilation, assembly, linking, and image construction Rust-native. External compilers, assemblers, linkers, Python, or Perl must not become production fallbacks. QEMU and other external tools may be optional independent verification only.
- When a guest exposes a bug, trace it through compiler output, assembly or machine code, CPU execution, and device behavior before changing guest code to work around it. Preserve guest ownership of device protocols and host/device error reporting.

## Implementation and testing

- Make focused changes, preserve existing behavior, and add regression tests for behavior changes. Reject unsupported input explicitly; do not silently truncate values, invent successful results, or hide failures.
- Treat C90 support as a tested subset, not blanket conformance. Keep supported behavior and known limitations aligned with tests and documentation.
- Validate compiler and machine changes by executing guests and asserting observable results such as UART bytes, registers, memory or disk contents, traps, and step budgets—not just parsing, generated text, or successful assembly.
- Use focused tests while iterating. Relevant integration targets include `kiuas16_compiler`, `kiuas16_assembler`, `kiuas16_runtime`, and `c90_conformance`.
- Before completing a coherent change, run the applicable checks:
  - `cargo fmt --check`
  - `cargo check --lib`
  - Focused integration tests for affected behavior
  - `cargo test --workspace -- --test-threads=1` at integration checkpoints
  - `git diff --check`
- Update directly related documentation and roadmap evidence to match tested behavior. Keep claims of language, ABI, ISA, firmware, and OS support bounded by verified acceptance coverage.
