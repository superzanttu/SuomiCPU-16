use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("sc16maps: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let mut maps_dir = suomicpu16::tank_maps::default_maps_dir();
    while let Some(argument) = arguments.next() {
        if argument == "--help" || argument == "-h" {
            println!("Usage: sc16maps [--maps-dir <directory>]");
            println!("Generate the ten deterministic Tank-16 natural maps.");
            return Ok(());
        }
        if argument == "--maps-dir" {
            maps_dir = PathBuf::from(arguments.next().ok_or("--maps-dir requires a directory")?);
        } else {
            return Err(format!("unknown argument: {}", argument.to_string_lossy()).into());
        }
    }
    let paths = suomicpu16::tank_maps::generate_maps(&maps_dir)?;
    println!(
        "{}",
        serde_json::json!({
            "maps": paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
            "count": paths.len()
        })
    );
    Ok(())
}
