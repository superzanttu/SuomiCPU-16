use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("sc16tank: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let Some(command) = arguments.next() else {
        return Err("usage: sc16tank <validate-map|build-runtime-assets> [options]".into());
    };
    if command == "--help" || command == "-h" {
        println!("Usage: sc16tank validate-map <map.json>");
        println!("       sc16tank build-runtime-assets [--output <file>] [--tile-size <1..64>]");
        return Ok(());
    }
    let game_dir = suomicpu16::tank16_map::default_game_dir();
    if command == "validate-map" {
        let path = PathBuf::from(arguments.next().ok_or("validate-map needs a map path")?);
        if arguments.next().is_some() {
            return Err("usage: sc16tank validate-map <map.json>".into());
        }
        let map = suomicpu16::tank16_map::load_map(&game_dir, &path)?;
        println!(
            "{}",
            serde_json::json!({
                "name": map["name"],
                "width": map["width"],
                "height": map["height"],
                "valid": true
            })
        );
        return Ok(());
    }
    if command == "build-runtime-assets" {
        let mut output = game_dir.join("disks/assets/TANK16_TILES.json");
        let mut tile_size = suomicpu16::tank16_map::map_tile_pixel_size(&game_dir)?;
        while let Some(option) = arguments.next() {
            if option == "--output" {
                output = PathBuf::from(arguments.next().ok_or("--output needs a path")?);
            } else if option == "--tile-size" {
                tile_size = arguments
                    .next()
                    .ok_or("--tile-size needs a number")?
                    .to_string_lossy()
                    .parse()?;
            } else {
                return Err(format!("unknown option: {}", option.to_string_lossy()).into());
            }
        }
        let path = suomicpu16::tank16_map::convert_tilesets_to_json(&game_dir, &output, tile_size)?;
        println!("Generated runtime tile assets: {}", path.display());
        return Ok(());
    }
    Err(format!("unknown command: {}", command.to_string_lossy()).into())
}
