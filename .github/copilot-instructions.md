Don't include long explanations in replies.
Don't add comments to code unless the user asks for them.

## Architecture

- The SC-16 emulator, assembler, C compiler, and host runtime are implemented in Rust. `src/c_compiler/mod.rs` emits SC-16 assembly; `src/assembler/mod.rs` assembles it; `src/emulator/cpu.rs` executes the machine code. `src/emulator/image.rs` connects source/image loading, while `src/emulator/runtime.rs` handles BIOS selection and the interactive host runtime.
- C programs under `programs/<slug>/src/` are guest workloads, with their own tools and manifests. Shared guest headers and runtime sources live under `toolchain/`; BIOS firmware and fonts live under `firmware/`. When a program exposes a bug, trace it through generated assembly and CPU execution before changing guest code to work around it.

## Build and Test

- Format check: `cargo fmt --check`
- Library type/build check: `cargo check --lib`
- Run a focused compiler unit test: `cargo test --lib c_compiler::tests::<test_name>`
- Run C90 end-to-end conformance tests: `cargo test --test c90_conformance`
- Run the full suite serially: `cargo test -- --test-threads=1`
- Compiler validation must cover execution, not only parsing or assembly generation. Follow `tests/c90_conformance.rs`: compile C to assembly, assemble it, load it into `Cpu`, run it, and assert observable results. Add focused behavioral regressions for affected compiler, ISA, or runtime behavior; use a real program scenario when it reproduces the reported bug.

## Compiler and Roadmap Conventions

- Keep compiler and emulator implementation paths Rust-native. Unsupported C input should be rejected by the Rust compiler with a diagnostic; do not restore or add a Python compiler/runtime fallback.
- Treat C90 support as a tested subset, not a blanket compatibility claim. Keep documentation aligned with verified behavior and list unsupported requirements explicitly.
- In `docs/sc16_improvements.adoc` and `docs/c90_todo.adoc`, leave a parent task unchecked until every child task is complete and its acceptance tests pass. Update task status only after validating the corresponding behavior.
