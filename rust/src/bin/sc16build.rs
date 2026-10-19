use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("sc16build: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let Some(source) = arguments.next() else {
        return Err(
            "usage: sc16build <source.asm|source.c|source.bin> [output.img] (or --help)".into(),
        );
    };
    if source == "--help" || source == "-h" {
        println!("Usage: sc16build <source.asm|source.c|source.bin> [output.img]");
        println!("Build SC-16 source or wrap a flat binary as a segmented image.");
        return Ok(());
    }
    if source.to_string_lossy().starts_with('-') {
        return Err(format!("unknown option: {}", source.to_string_lossy()).into());
    }
    let source = PathBuf::from(source).canonicalize()?;
    let output = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| source.with_extension("sc16img"));
    if arguments.next().is_some() {
        return Err("usage: sc16build <source.asm|source.c|source.bin> [output.img]".into());
    }
    if output.exists() && source == output.canonicalize()? {
        return Err("output path must not overwrite the source".into());
    }
    let image = if source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bin"))
    {
        suomicpu16::image::load_binary(&source)?
    } else {
        suomicpu16::image::build_source(&source)?
    };
    suomicpu16::image::write_image(&output, &image)?;
    println!(
        "Built {} bytes in {} segment(s), entry point {:#x} -> {}",
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
