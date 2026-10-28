//! Provides the Tank-16 object-tileset generation command.
//! Selects the output path and invokes the shared procedural atlas generator.

use std::path::PathBuf;

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(suomicpu16::tank16_assets::default_object_tileset_path);
    if let Err(error) = suomicpu16::tank16_assets::generate_object_tileset(&output) {
        eprintln!("sc16objects: {error}");
        std::process::exit(1);
    }
    println!("Generated object tileset: {}", output.display());
}
