use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("kiuas16-asm: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let usage = "usage: kiuas16-asm <source.S> <output.bin>";
    let source = arguments.next().ok_or(usage)?;
    if source == "--help" || source == "-h" {
        println!("{usage}");
        println!("Assemble the initial RV64 integer subset to a raw Kiuas16 boot ROM.");
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
        return Err("output path must not overwrite assembly source".into());
    }
    let rom = suomicpu16::assembler::rv64::assemble_boot_rom(&std::fs::read_to_string(&source)?)?;
    std::fs::write(&output, &rom)?;
    eprintln!("Assembled {} ROM bytes -> {}", rom.len(), output.display());
    Ok(())
}
