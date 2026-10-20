use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const MAP_NAMES: [&str; 10] = [
    "ARCHIPEL", "RIVER-01", "LAKESIDE", "DELTA-01", "COAST-01", "ISLANDS", "FJORD-01", "MARSH-01",
    "HIGHLND", "ESTUARY",
];

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("suomicpu16-map-cli-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).expect("temporary directory should be created");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(binary: &str, arguments: &[&str]) -> Output {
    let executable = match binary {
        "sc16mapedit" => env!("CARGO_BIN_EXE_sc16mapedit"),
        "sc16tank" => env!("CARGO_BIN_EXE_sc16tank"),
        "sc16maps" => env!("CARGO_BIN_EXE_sc16maps"),
        _ => unreachable!("only known test binaries are used"),
    };
    Command::new(executable)
        .args(arguments)
        .output()
        .expect("Rust CLI should start")
}

fn successful_stdout(output: Output) -> String {
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("CLI output should be UTF-8")
}

fn load_map(path: &Path) -> Value {
    let output = run(
        "sc16tank",
        &["load-map", path.to_str().expect("map path should be UTF-8")],
    );
    serde_json::from_str(&successful_stdout(output)).expect("CLI should return map JSON")
}

#[test]
fn editor_help_describes_rust_workflows() {
    let output = successful_stdout(run("sc16mapedit", &["--help"]));
    assert!(output.contains("Paint terrain and object layers"));
}

#[test]
fn map_load_save_round_trip_uses_rust_validation() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("modules")
        .join("Tank-16")
        .join("disks")
        .join("maps")
        .join("FINLAND.json");
    let loaded = load_map(&source);
    assert_eq!(loaded["width"], 100);
    assert_eq!(loaded["height"], 100);
    assert_eq!(loaded["name"], "FINLAND");

    let temporary = TempDir::new();
    let output = run(
        "sc16tank",
        &[
            "save-map",
            source.to_str().expect("map path should be UTF-8"),
            "--output-dir",
            temporary
                .path()
                .to_str()
                .expect("temp path should be UTF-8"),
        ],
    );
    let saved_path = PathBuf::from(successful_stdout(output).trim());
    assert_eq!(load_map(&saved_path), loaded);
}

#[test]
fn generated_maps_are_installed_in_the_runtime_folder() {
    let runtime_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("modules")
        .join("Tank-16")
        .join("disks")
        .join("maps");
    for name in MAP_NAMES {
        let path = runtime_dir.join(format!("{name}.json"));
        assert!(path.is_file(), "missing generated map: {}", path.display());
        assert_eq!(load_map(&path)["name"], name);
    }
}

#[test]
fn finland_map_has_runtime_ready_features_and_compact_encoding() {
    let map_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("modules")
        .join("Tank-16")
        .join("disks")
        .join("maps")
        .join("FINLAND.json");
    let map = load_map(&map_path);
    assert_eq!(map["name"], "FINLAND");
    assert_eq!(map["width"], 100);
    assert_eq!(map["height"], 100);
    assert_eq!(map["tile_size"], 64);
    assert_eq!(map["tileset"], "tiles_terrain.png");

    let mut terrain_ids = HashSet::new();
    for row in map["tiles"].as_array().expect("terrain should be rows") {
        assert_eq!(row.as_array().map(Vec::len), Some(100));
        for tile in row.as_array().expect("terrain row should be an array") {
            let tile = tile.as_array().expect("terrain tile should be an x/y pair");
            let x = tile[0].as_u64().expect("tile x should be an integer");
            let y = tile[1].as_u64().expect("tile y should be an integer");
            assert!(x < 10 && y < 10);
            terrain_ids.insert(y * 10 + x);
        }
    }
    let geography = &map["geography"];
    let counts = &geography["tile_counts"];
    assert_eq!(
        counts["finland_land"].as_u64().unwrap()
            + counts["neighbor_land"].as_u64().unwrap()
            + counts["water"].as_u64().unwrap(),
        10_000
    );
    for category in ["road_tiles", "rail_tiles", "road_bridges", "rail_bridges"] {
        assert!(counts[category].as_u64().unwrap() > 0);
    }
    assert!(terrain_ids.iter().any(|tile| (40..=59).contains(tile)));
    assert!(terrain_ids.contains(&90));
    assert!(terrain_ids.iter().any(|tile| (60..=69).contains(tile)));
    assert!(terrain_ids.iter().any(|tile| (70..=85).contains(tile)));
    assert!(geography["road_features"].as_u64().unwrap() > 0);
    assert!(
        geography["roads_source"]
            .as_str()
            .unwrap()
            .contains("ne_10m_roads.geojson")
    );
    assert!(geography["railway_features"].as_u64().unwrap() > 0);
    assert!(
        geography["railways_source"]
            .as_str()
            .unwrap()
            .contains("ne_10m_railroads.geojson")
    );
    assert!(!geography["cities"].as_array().unwrap().is_empty());
    for layer in ["objects", "obstacles"] {
        assert!(
            map[layer]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|row| row.as_array().unwrap())
                .any(|cell| !cell.is_null())
        );
    }
    assert!(counts["finland_land"].as_u64().unwrap() > 1_000);
    assert!(counts["water"].as_u64().unwrap() > 1_000);

    let runtime = serde_json::from_slice::<Value>(
        &fs::read(&map_path).expect("runtime map should be readable"),
    )
    .expect("runtime map should be valid JSON");
    assert_eq!(runtime["runtime_encoding"], "grid-hex-v2");
    assert!(fs::metadata(&map_path).unwrap().len() < 65_535);
    assert!(
        !map_path
            .parent()
            .unwrap()
            .join("..")
            .join("..")
            .join("maps")
            .exists()
    );

    let expected_terrain = map["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row.as_array().unwrap())
        .map(|tile| {
            let pair = tile.as_array().unwrap();
            pair[1].as_u64().unwrap() * 10 + pair[0].as_u64().unwrap()
        })
        .collect::<HashSet<_>>();
    let used_terrain = runtime["used_tiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry[0].as_u64().unwrap())
        .collect::<HashSet<_>>();
    assert_eq!(used_terrain, expected_terrain);
    assert_eq!(
        runtime["used_tiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry[1].as_u64().unwrap())
            .sum::<u64>(),
        10_000
    );

    let expected_obstacles = map["obstacles"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row.as_array().unwrap())
        .filter(|cell| !cell.is_null())
        .map(|cell| cell[1].as_u64().unwrap() * 6 + cell[0].as_u64().unwrap())
        .collect::<HashSet<_>>();
    let used_obstacles = runtime["used_obstacles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry[0].as_u64().unwrap())
        .collect::<HashSet<_>>();
    assert_eq!(used_obstacles, expected_obstacles);

    let expected_objects = map["objects"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row.as_array().unwrap())
        .filter(|cell| !cell.is_null())
        .map(|cell| cell["tile"][1].as_u64().unwrap() * 6 + cell["tile"][0].as_u64().unwrap())
        .collect::<HashSet<_>>();
    let used_objects = runtime["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry[1].as_u64().unwrap())
        .collect::<HashSet<_>>();
    assert_eq!(used_objects, expected_objects);

    let raw = fs::read(&map_path).unwrap();
    let offset = runtime["grid_offset"].as_u64().unwrap() as usize;
    assert!(offset <= 4096);
    let stride = 100 * 4 + 4;
    for row in [0, 50, 99] {
        let start = offset + row * stride + 1;
        assert_eq!(
            std::str::from_utf8(&raw[start..start + 400]).unwrap(),
            runtime["grid"][row].as_str().unwrap()
        );
    }
}

#[test]
fn finland_validation_and_atlas_extension_preserve_source_pixels() {
    let game_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("modules")
        .join("Tank-16");
    let map_path = game_dir.join("disks").join("maps").join("FINLAND.json");
    let summary: Value = serde_json::from_str(&successful_stdout(run(
        "sc16tank",
        &[
            "validate-map",
            map_path.to_str().expect("map path should be UTF-8"),
        ],
    )))
    .unwrap();
    assert_eq!(summary["valid"], true);
    assert_eq!(summary["name"], "FINLAND");
    assert_eq!(summary["width"], 100);
    assert_eq!(summary["height"], 100);
    assert_eq!(load_map(&map_path)["geography"]["north_up"], true);

    let source = image::open(game_dir.join("gfx").join("tiles_terrain.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!(source.dimensions(), (640, 640));
    let original = image::imageops::crop_imm(&source, 0, 0, 640, 256).to_image();
    let temporary = TempDir::new();
    let atlas_path = temporary.path().join("tiles_test.png");
    original.save(&atlas_path).unwrap();
    let path = atlas_path.to_str().expect("atlas path should be UTF-8");
    successful_stdout(run("sc16tank", &["extend-atlas", "--path", path]));
    let first = fs::read(&atlas_path).unwrap();
    successful_stdout(run("sc16tank", &["extend-atlas", "--path", path]));
    assert_eq!(fs::read(&atlas_path).unwrap(), first);

    let extended = image::open(&atlas_path).unwrap().to_rgba8();
    assert_eq!(extended.dimensions(), (640, 640));
    assert_eq!(
        image::imageops::crop_imm(&extended, 0, 0, 640, 256).to_image(),
        original
    );
    for tile_id in 40..=90 {
        let tile =
            image::imageops::crop_imm(&extended, (tile_id % 10) * 64, (tile_id / 10) * 64, 64, 64)
                .to_image();
        assert!(tile.pixels().any(|pixel| pixel.0[3] != 0));
    }
}

#[test]
fn generator_creates_ten_distinct_runtime_ready_maps() {
    let temporary = TempDir::new();
    let runtime_dir = temporary.path().join("disks").join("maps");
    let output = run(
        "sc16maps",
        &[
            "--maps-dir",
            runtime_dir.to_str().expect("temp path should be UTF-8"),
        ],
    );
    let report: Value =
        serde_json::from_str(&successful_stdout(output)).expect("generator should return JSON");
    assert_eq!(report["maps"].as_array().map(Vec::len), Some(10));
    assert_eq!(
        fs::read_dir(&runtime_dir)
            .expect("runtime maps should be written")
            .count(),
        10
    );

    let mut terrain_layouts = HashSet::new();
    for name in MAP_NAMES {
        let map = load_map(&runtime_dir.join(format!("{name}.json")));
        assert_eq!(map["name"], name);
        assert_eq!(map["width"], 48);
        assert_eq!(map["height"], 32);
        assert_eq!(map["tileset"], "tiles_terrain.png");
        assert_eq!(map["objects_tileset"], "tiles_objects.png");

        let layout = map["tiles"]
            .as_array()
            .expect("tiles should be rows")
            .iter()
            .flat_map(|row| row.as_array().expect("tile row should be an array"))
            .map(|tile| {
                let pair = tile.as_array().expect("tile should be an x/y pair");
                pair[1].as_u64().expect("tile y should be an integer") * 10
                    + pair[0].as_u64().expect("tile x should be an integer")
            })
            .collect::<Vec<_>>();
        terrain_layouts.insert(layout);

        let counts = &map["geography"]["tile_counts"];
        for category in [
            "water",
            "roads",
            "railways",
            "road_bridges",
            "rail_bridges",
            "trees_and_rocks",
            "city_tiles",
        ] {
            assert!(
                counts[category].as_u64().unwrap_or_default() > 0,
                "generated {name} map should contain {category}"
            );
        }
    }
    assert_eq!(terrain_layouts.len(), MAP_NAMES.len());
}
