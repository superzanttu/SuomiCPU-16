//! Provides the standalone SC-16 assembler command-line tool.
//! Parses source/output arguments, assembles files, and writes executable images.

use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("sc16asm: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let Some(source) = arguments.next() else {
        return Err("usage: sc16asm <source.asm> [output.img] (or --help)".into());
    };
    if source == "--help" || source == "-h" {
        println!("Usage: sc16asm <source.asm> [output.img]");
        println!("Assemble SC-16 source to a version 2 SC-16 executable image.");
        return Ok(());
    }
    if source.to_string_lossy().starts_with('-') {
        return Err(format!("unknown option: {}", source.to_string_lossy()).into());
    }
    let source = PathBuf::from(source);
    let Some(extension) = source.extension() else {
        return Err(format!(
            "assembly source has no .asm extension: {}",
            source.display()
        )
        .into());
    };
    if !extension.eq_ignore_ascii_case("asm") {
        return Err(format!("expected an .asm source file: {}", source.display()).into());
    }
    let source = source.canonicalize()?;
    let output = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| source.with_extension("sc16img"));
    if arguments.next().is_some() {
        return Err("usage: sc16asm <source.asm> [output.img]".into());
    }
    if output.exists() && source == output.canonicalize()? {
        return Err("output path must not overwrite the assembly source".into());
    }

    let image = suomicpu16::assembler::assemble_file(&source, 0)?;
    suomicpu16::image::write_image(&output, &image)?;
    println!(
        "Assembled {} bytes in {} segment(s), entry point {:#x} -> {}",
        image
            .segments
            .iter()
            .map(|segment| segment.data.len())
            .sum::<usize>(),
        image.segments.len(),
        image.entry_point,
        output.display()
    );
    Ok(())
}
