//! Provides Tank-16's asset, map, and runtime build command-line tools.
//! Dispatches map validation/conversion, header and runtime generation,
//! geographic map generation, and terrain-atlas extension.

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
        return Err(
            "usage: sc16tank <validate-map|load-map|save-map|encode-map|round-maps|inspect-map-assets|build-map-header|runtime-assets-header|build-runtime-assets|build-runtime|generate-finland-map|extend-atlas> [options]"
                .into(),
        );
    };
    if command == "--help" || command == "-h" {
        println!("Usage: sc16tank validate-map <map.json>");
        println!("       sc16tank load-map <map.json>");
        println!("       sc16tank save-map <map.json> --output-dir <directory>");
        println!("       sc16tank encode-map <map.json> --output <runtime-map.json>");
        println!("       sc16tank round-maps <map.json>");
        println!("       sc16tank inspect-map-assets <map.json> [--tile-size <1..64>]");
        println!(
            "       sc16tank build-map-header <map.json> [--tile-size <1..64>] [--round-map <map.json> ...]"
        );
        println!("       sc16tank runtime-assets-header");
        println!("       sc16tank build-runtime-assets [--output <file>] [--tile-size <1..64>]");
        println!("       sc16tank build-runtime [--output <image.img>]");
        println!("       sc16tank build-map <map.json> [--rounds] [--output <image.img>]");
        println!(
            "       sc16tank generate-finland-map --countries <file> --lakes <file> [--roads <file>] [--railways <file>] [--cities <file>] [--output <file>] [--preview <file>] [--size <1..100>]"
        );
        println!("       sc16tank extend-atlas [--path <tiles_terrain.png>]");
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
    if command == "load-map" {
        let path = PathBuf::from(arguments.next().ok_or("load-map needs a map path")?);
        if arguments.next().is_some() {
            return Err("usage: sc16tank load-map <map.json>".into());
        }
        let map = suomicpu16::tank16_map::load_map(&game_dir, &path)?;
        println!("{}", serde_json::to_string(&map)?);
        return Ok(());
    }
    if command == "save-map" {
        let path = PathBuf::from(arguments.next().ok_or("save-map needs a map path")?);
        let mut output_dir = None;
        while let Some(option) = arguments.next() {
            if option == "--output-dir" {
                output_dir = Some(PathBuf::from(
                    arguments.next().ok_or("--output-dir needs a directory")?,
                ));
            } else {
                return Err(format!("unknown option: {}", option.to_string_lossy()).into());
            }
        }
        let output_dir = output_dir.ok_or("save-map requires --output-dir <directory>")?;
        let source = suomicpu16::tank16_map::load_map(&game_dir, &path)?;
        let saved = suomicpu16::tank16_map::save_map(&game_dir, &source, &output_dir)?;
        println!("{}", saved.display());
        return Ok(());
    }
    if command == "encode-map" {
        let path = PathBuf::from(arguments.next().ok_or("encode-map needs a map path")?);
        let mut output = None;
        while let Some(option) = arguments.next() {
            if option == "--output" {
                output = Some(PathBuf::from(
                    arguments.next().ok_or("--output needs a path")?,
                ));
            } else {
                return Err(format!("unknown option: {}", option.to_string_lossy()).into());
            }
        }
        let output = output.ok_or("encode-map requires --output <runtime-map.json>")?;
        let map = suomicpu16::tank16_map::load_map(&game_dir, &path)?;
        let document = suomicpu16::tank16_map::runtime_document(&map, &game_dir)?;
        let serialized = suomicpu16::tank16_map::serialize_runtime_map(&document)?;
        std::fs::write(&output, serialized)?;
        println!("{}", output.display());
        return Ok(());
    }
    if command == "round-maps" {
        let path = PathBuf::from(arguments.next().ok_or("round-maps needs a map path")?);
        if arguments.next().is_some() {
            return Err("usage: sc16tank round-maps <map.json>".into());
        }
        let maps = suomicpu16::tank16_map::discover_round_maps(&game_dir, &path)?;
        println!("{}", serde_json::to_string(&maps)?);
        return Ok(());
    }
    if command == "inspect-map-assets" {
        let path = PathBuf::from(
            arguments
                .next()
                .ok_or("inspect-map-assets needs a map path")?,
        );
        let mut tile_size = suomicpu16::tank16_map::map_tile_pixel_size(&game_dir)?;
        while let Some(option) = arguments.next() {
            if option == "--tile-size" {
                tile_size = arguments
                    .next()
                    .ok_or("--tile-size needs a number")?
                    .to_string_lossy()
                    .parse()?;
            } else {
                return Err(format!("unknown option: {}", option.to_string_lossy()).into());
            }
        }
        let map = suomicpu16::tank16_map::load_map(&game_dir, &path)?;
        let assets = suomicpu16::tank16_map::load_map_assets(&game_dir, &path, tile_size)?;
        println!(
            "{}",
            serde_json::json!({
                "map": map,
                "tiles": assets.tiles,
                "palette": assets.palette,
                "dimensions": [
                    assets.terrain.columns,
                    assets.terrain.rows,
                    tile_size
                ]
            })
        );
        return Ok(());
    }
    if command == "build-map-header" {
        let path = PathBuf::from(
            arguments
                .next()
                .ok_or("build-map-header needs a map path")?,
        );
        let mut tile_size = suomicpu16::tank16_map::map_tile_pixel_size(&game_dir)?;
        let mut round_maps = Vec::new();
        while let Some(option) = arguments.next() {
            if option == "--tile-size" {
                tile_size = arguments
                    .next()
                    .ok_or("--tile-size needs a number")?
                    .to_string_lossy()
                    .parse()?;
            } else if option == "--round-map" {
                round_maps.push(PathBuf::from(
                    arguments.next().ok_or("--round-map needs a map path")?,
                ));
            } else {
                return Err(format!("unknown option: {}", option.to_string_lossy()).into());
            }
        }
        let header = suomicpu16::tank16_map::build_map_header_with_tile_size(
            &game_dir,
            &path,
            &round_maps,
            tile_size,
        )?;
        print!("{header}");
        return Ok(());
    }
    if command == "runtime-assets-header" {
        if arguments.next().is_some() {
            return Err("runtime-assets-header accepts no options".into());
        }
        let header = suomicpu16::tank16_map::runtime_assets_header(&game_dir)?;
        print!("{header}");
        return Ok(());
    }
    if command == "build-runtime-assets" {
        let mut output = game_dir.join("assets/disk/assets/TANK16_TILES.json");
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
    if command == "build-runtime" {
        let mut output = None;
        while let Some(option) = arguments.next() {
            if option == "--output" {
                output = Some(PathBuf::from(
                    arguments.next().ok_or("--output needs a path")?,
                ));
            } else {
                return Err(format!("unknown option: {}", option.to_string_lossy()).into());
            }
        }
        let output = output.ok_or("build-runtime requires --output <image.img>")?;
        let image = suomicpu16::tank16_map::compile_runtime_tank16(&game_dir)
            .map_err(std::io::Error::other)?;
        suomicpu16::image::write_image(&output, &image)?;
        println!(
            "Built {} bytes across {} segment(s): {}",
            image
                .segments
                .iter()
                .map(|segment| segment.data.len())
                .sum::<usize>(),
            image.segments.len(),
            output.display()
        );
        return Ok(());
    }
    if command == "build-map" {
        let path = PathBuf::from(arguments.next().ok_or("build-map needs a map path")?);
        let mut randomize_rounds = false;
        let mut output = path.with_extension("sc16img");
        while let Some(option) = arguments.next() {
            if option == "--rounds" {
                randomize_rounds = true;
            } else if option == "--output" {
                output = PathBuf::from(arguments.next().ok_or("--output needs a path")?);
            } else {
                return Err(format!("unknown option: {}", option.to_string_lossy()).into());
            }
        }
        let image = suomicpu16::tank16_map::compile_map_file(&game_dir, &path, randomize_rounds)?;
        suomicpu16::image::write_image(&output, &image)?;
        println!(
            "Built {} bytes across {} segment(s): {}",
            image
                .segments
                .iter()
                .map(|segment| segment.data.len())
                .sum::<usize>(),
            image.segments.len(),
            output.display()
        );
        return Ok(());
    }
    if command == "generate-finland-map" {
        let mut countries = None;
        let mut lakes = None;
        let mut roads = None;
        let mut railways = None;
        let mut cities = None;
        let mut preview = None;
        let mut output = game_dir.join("assets/disk/maps/FINLAND.json");
        let mut size = 100usize;
        while let Some(option) = arguments.next() {
            let value = arguments
                .next()
                .ok_or_else(|| format!("{} needs a value", option.to_string_lossy()))?;
            match option.to_string_lossy().as_ref() {
                "--countries" => countries = Some(PathBuf::from(value)),
                "--lakes" => lakes = Some(PathBuf::from(value)),
                "--roads" => roads = Some(PathBuf::from(value)),
                "--railways" => railways = Some(PathBuf::from(value)),
                "--cities" => cities = Some(PathBuf::from(value)),
                "--preview" => preview = Some(PathBuf::from(value)),
                "--output" => output = PathBuf::from(value),
                "--size" => size = value.to_string_lossy().parse()?,
                _ => return Err(format!("unknown option: {}", option.to_string_lossy()).into()),
            }
        }
        let options = suomicpu16::tank16_finland::FinlandMapOptions {
            countries: countries.ok_or("--countries is required")?,
            lakes: lakes.ok_or("--lakes is required")?,
            roads,
            railways,
            cities,
            output: output.clone(),
            preview,
            size,
        };
        let document = suomicpu16::tank16_finland::generate_finland_map(&options)?;
        println!(
            "{}",
            serde_json::json!({
                "path": output,
                "dimensions": [document["width"], document["height"]],
                "counts": document["geography"]["tile_counts"]
            })
        );
        return Ok(());
    }
    if command == "extend-atlas" {
        let mut atlas = game_dir.join("assets/gfx/tiles_terrain.png");
        while let Some(option) = arguments.next() {
            if option != "--path" {
                return Err(format!("unknown option: {}", option.to_string_lossy()).into());
            }
            atlas = PathBuf::from(arguments.next().ok_or("--path needs a file")?);
        }
        suomicpu16::tank16_finland::extend_terrain_atlas(
            &atlas,
            &game_dir.join("assets/gfx/tiles_terrain.png"),
        )?;
        println!("Extended terrain atlas: {}", atlas.display());
        return Ok(());
    }
    Err(format!("unknown command: {}", command.to_string_lossy()).into())
}
