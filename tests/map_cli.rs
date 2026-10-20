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
