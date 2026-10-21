# Rust implementation status

Runtime, compiler, tools, and CI use Rust exclusively. Legacy non-Rust
implementations and dependencies have been removed.

The C compiler remains a partial C90 implementation. Sources using unsupported
syntax or semantics fail with a Rust diagnostic; there is no compatibility
compiler fallback. The Rust test suite currently covers 277 cases, and the
module smoke build compiles all 17 bootable BIOS C modules.

Run the Rust validation suite with:

```console
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo check --locked --target wasm32-unknown-unknown
cargo run --locked --bin sc16build -- smoke-modules
```
