use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("kiuas16-cc: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let usage = "usage: kiuas16-cc <source.c> <output.bin>";
    let mut arguments = std::env::args_os().skip(1);
    let source = arguments.next().ok_or(usage)?;
    if source == "--help" || source == "-h" {
        println!("{usage}");
        println!("Compile the initial freestanding RV64 C subset to a Kiuas16 boot ROM.");
        return Ok(());
    }
    if source.to_string_lossy().starts_with('-') {
        return Err(format!("unknown option: {}", source.to_string_lossy()).into());
    }
    let source = PathBuf::from(source).canonicalize()?;
    let output = PathBuf::from(arguments.next().ok_or(usage)?);
    if arguments.next().is_some() {
        return Err(usage.into());
    }
    if output.exists() && source == output.canonicalize()? {
        return Err("output path must not overwrite C source".into());
    }
    let assembly = suomicpu16::c_compiler::rv64::compile_to_assembly(
        &std::fs::read_to_string(&source)?,
        &source.to_string_lossy(),
    )?;
    let rom = suomicpu16::assembler::rv64::assemble_boot_rom(&assembly)?;
    std::fs::write(&output, &rom)?;
    eprintln!("Compiled {} ROM bytes -> {}", rom.len(), output.display());
    Ok(())
}
