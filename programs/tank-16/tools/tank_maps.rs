use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const MAPS: &[(&str, i32, &str)] = &[
    ("ARCHIPEL", 11, "islands"),
    ("RIVER-01", 23, "river"),
    ("LAKESIDE", 37, "lakes"),
    ("DELTA-01", 41, "delta"),
    ("COAST-01", 53, "coast"),
    ("ISLANDS", 67, "islands"),
    ("FJORD-01", 79, "fjord"),
    ("MARSH-01", 83, "marsh"),
    ("HIGHLND", 97, "highland"),
    ("ESTUARY", 109, "estuary"),
];

const WIDTH: usize = 48;
const HEIGHT: usize = 32;
const BUILDING_CELLS: [(usize, usize); 9] = [
    (0, 0),
    (1, 0),
    (2, 0),
    (0, 1),
    (1, 1),
    (2, 1),
    (0, 2),
    (1, 2),
    (2, 2),
];
const ROAD_TILES: [(u8, u8); 15] = [
    (1, 19),
    (2, 9),
    (3, 15),
    (4, 1),
    (5, 1),
    (6, 13),
    (7, 3),
    (8, 2),
    (9, 16),
    (10, 2),
    (11, 6),
    (12, 14),
    (13, 4),
    (14, 5),
    (15, 11),
];

type Cell = (usize, usize);

fn distance_to_line(
    x: f64,
    y: f64,
    vertical: bool,
    center: f64,
    amplitude: f64,
    phase: f64,
    frequency: f64,
) -> f64 {
    if vertical {
        (x - (center + amplitude * (y * frequency + phase).sin())).abs()
    } else {
        (y - (center + amplitude * (x * frequency + phase).sin())).abs()
    }
}

fn ellipse_contains(
    x: f64,
    y: f64,
    center_x: f64,
    center_y: f64,
    radius_x: f64,
    radius_y: f64,
    noise: f64,
) -> bool {
    let dx = (x - center_x) / radius_x;
    let dy = (y - center_y) / radius_y;
    dx * dx + dy * dy < 1.0 + noise * 0.08
}

fn is_water(kind: &str, x: usize, y: usize, seed: i32) -> bool {
    let (x, y, seed) = (x as f64, y as f64, f64::from(seed));
    let noise = (x * 0.31 + seed).sin() * 0.8
        + (y * 0.27 + seed * 1.7).sin() * 0.7
        + ((x - y) * 0.13 + seed * 0.37).cos() * 0.6;
    match kind {
        "islands" => ![
            (11.0, 13.0, 11.0, 8.0),
            (29.0, 10.0, 8.0, 6.0),
            (35.0, 23.0, 8.0, 7.0),
            (16.0, 26.0, 7.0, 5.0),
        ]
        .into_iter()
        .any(|(cx, cy, rx, ry)| ellipse_contains(x, y, cx, cy, rx, ry, noise)),
        "river" => distance_to_line(x, y, true, 24.0, 5.0, seed * 0.17, 0.21) < 2.4 + noise * 0.3,
        "lakes" => {
            let lakes = [
                (13.0, 10.0, 5.0, 4.0),
                (34.0, 8.0, 6.0, 4.0),
                (32.0, 23.0, 8.0, 5.0),
                (12.0, 24.0, 5.0, 4.0),
            ];
            lakes
                .into_iter()
                .any(|(cx, cy, rx, ry)| ellipse_contains(x, y, cx, cy, rx, ry, noise))
                || distance_to_line(x, y, false, 16.0, 8.0, seed * 0.31, 0.11) < 0.7
        }
        "delta" => {
            distance_to_line(x, y, true, 23.0, 3.0, seed * 0.11, 0.15) < 1.8
                || y > 15.0 && distance_to_line(x, y, false, 21.0, 8.0, seed * 0.23, 0.12) < 1.3
                || y > 27.0 + noise * 0.8
        }
        "coast" => {
            x < 8.0 + 3.0 * (y * 0.19 + seed * 0.2).sin() + noise
                || ellipse_contains(x, y, 20.0, 8.0, 9.0, 5.0, noise)
        }
        "fjord" => {
            distance_to_line(x, y, true, 24.0, 9.0, seed * 0.29, 0.12) < 2.2 + noise * 0.25
                || y < 14.0 && distance_to_line(x, y, true, 12.0, 4.0, seed * 0.13, 0.18) < 1.6
        }
        "marsh" => {
            ellipse_contains(x, y, 10.0, 9.0, 4.0, 3.0, noise)
                || ellipse_contains(x, y, 36.0, 22.0, 5.0, 4.0, noise)
                || ellipse_contains(x, y, 22.0, 25.0, 4.0, 3.0, noise)
                || distance_to_line(x, y, false, 16.0, 5.0, seed * 0.41, 0.17) < 1.0
                || distance_to_line(x, y, true, 29.0, 3.0, seed * 0.33, 0.14) < 0.8
        }
        "highland" => {
            ellipse_contains(x, y, 15.0, 9.0, 3.0, 3.0, noise)
                || ellipse_contains(x, y, 34.0, 22.0, 4.0, 3.0, noise)
                || distance_to_line(x, y, true, 24.0, 4.0, seed * 0.27, 0.2) < 1.2
        }
        _ => {
            x > 38.0 + 2.5 * (y * 0.18 + seed).sin()
                || distance_to_line(x, y, false, 15.0, 5.0, seed * 0.19, 0.13) < 1.8
        }
    }
}

fn trace_route(
    horizontal: bool,
    center: f64,
    amplitude: f64,
    phase: f64,
    frequency: f64,
) -> Vec<Cell> {
    let steps = WIDTH.max(HEIGHT) * 8;
    let mut cells = Vec::new();
    for step in 0..=steps {
        let progress = step as f64 / steps as f64;
        let (x, y) = if horizontal {
            (
                (progress * (WIDTH - 1) as f64).round_ties_even() as isize,
                (center + amplitude * (progress * (WIDTH - 1) as f64 * frequency + phase).sin())
                    .round_ties_even() as isize,
            )
        } else {
            (
                (center + amplitude * (progress * (HEIGHT - 1) as f64 * frequency + phase).sin())
                    .round_ties_even() as isize,
                (progress * (HEIGHT - 1) as f64).round_ties_even() as isize,
            )
        };
        if x >= 0 && x < WIDTH as isize && y >= 0 && y < HEIGHT as isize {
            let cell = (x as usize, y as usize);
            if !cells.contains(&cell) {
                cells.push(cell);
            }
        }
    }
    cells
}

fn contains(cells: &[Cell], cell: Cell) -> bool {
    cells.contains(&cell)
}

fn connections(cells: &[Cell], x: usize, y: usize) -> u8 {
    (u8::from(y > 0 && contains(cells, (x, y - 1))))
        | (u8::from(x + 1 < WIDTH && contains(cells, (x + 1, y))) << 1)
        | (u8::from(y + 1 < HEIGHT && contains(cells, (x, y + 1))) << 2)
        | (u8::from(x > 0 && contains(cells, (x - 1, y))) << 3)
}

fn bridge_tile_id(connection: u8, rail: bool, variant: usize) -> u16 {
    let orientation = if connection & (2 | 8) != 0 { 0 } else { 1 };
    if rail {
        return 86 + orientation + if variant.is_multiple_of(2) { 0 } else { 2 };
    }
    let terminal = connection.count_ones() == 1;
    match (terminal, variant % 3) {
        (true, 0) => 66 + orientation,
        (true, _) => 62 + orientation,
        (false, 0) => 64 + orientation,
        (false, 1) => 68 + orientation,
        _ => 60 + orientation,
    }
}

fn encode_hex(value: &Value) -> String {
    serde_json::to_string(value).expect("JSON value is serializable")
}

fn serialize_runtime_map(document: &Value) -> Result<String, String> {
    let width = document["width"]
        .as_u64()
        .ok_or_else(|| "missing map width".to_owned())? as usize;
    let height = document["height"]
        .as_u64()
        .ok_or_else(|| "missing map height".to_owned())? as usize;
    let grid = document["grid"]
        .as_array()
        .ok_or_else(|| "missing runtime map grid".to_owned())?;
    if grid.len() != height
        || grid
            .iter()
            .any(|row| row.as_str().is_none_or(|row| row.len() != 4 * width))
    {
        return Err("runtime map grid does not match its dimensions".to_owned());
    }

    let keys = [
        "format_version",
        "runtime_encoding",
        "name",
        "width",
        "height",
        "tile_size",
        "tileset",
        "objects_tileset",
        "used_tiles",
        "used_obstacles",
        "object_count",
    ];
    let header = keys
        .iter()
        .map(|key| {
            format!(
                "{}:{}",
                encode_hex(&json!(key)),
                encode_hex(&document[*key])
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let grid_text = grid
        .iter()
        .enumerate()
        .map(|(index, row)| {
            format!(
                "{}{}\n",
                encode_hex(row),
                if index + 1 < height { "," } else { " " }
            )
        })
        .collect::<String>();
    let objects_text = encode_hex(&document["objects"]);
    let geography = document
        .get("geography")
        .map(|value| format!(",\n\"geography\":{}", encode_hex(value)))
        .unwrap_or_default();
    let mut offsets = (0usize, 0usize, 0usize);
    let (prefix, grid_offset, objects_offset) = loop {
        let prefix = format!(
            "{{{header},\"grid_offset\":{},\"objects_offset\":{},\"objects_size\":{}",
            offsets.0, offsets.1, offsets.2
        );
        let grid_open = ",\n\"grid\":[\n";
        let grid_offset = prefix.len() + grid_open.len();
        let objects_offset = grid_offset + grid_text.len() + "],\n\"objects\":".len();
        let updated = (grid_offset, objects_offset, objects_text.len());
        if offsets == updated {
            break (prefix, grid_offset, objects_offset);
        }
        offsets = updated;
    };
    if grid_offset > 4096 {
        return Err("runtime map header exceeds 4096 bytes".to_owned());
    }
    let text =
        format!("{prefix},\n\"grid\":[\n{grid_text}],\n\"objects\":{objects_text}{geography}}}\n");
    if text.as_bytes().get(grid_offset) != Some(&b'"')
        || text.as_bytes().get(objects_offset) != Some(&b'[')
    {
        return Err("runtime map offset encoding is invalid".to_owned());
    }
    Ok(text)
}

pub fn generate_map(name: &str, seed: i32, kind: &str) -> Result<Value, String> {
    let water: Vec<Vec<bool>> = (0..HEIGHT)
        .map(|y| (0..WIDTH).map(|x| is_water(kind, x, y, seed)).collect())
        .collect();
    let dry: Vec<Vec<bool>> = water
        .iter()
        .map(|row| row.iter().map(|is_water| !is_water).collect())
        .collect();
    let roads = trace_route(
        true,
        HEIGHT as f64 * 0.51,
        5.0,
        f64::from(seed) * 0.13,
        0.14,
    );
    let railways = trace_route(false, WIDTH as f64 * 0.5, 7.0, f64::from(seed) * 0.27, 0.18);
    let mut objects = vec![vec![Value::Null; WIDTH]; HEIGHT];
    let mut obstacles = vec![vec![Value::Null; WIDTH]; HEIGHT];
    let mut city_cells = vec![false; WIDTH * HEIGHT];
    let mut cities = Vec::new();

    for (city_x, city_y) in [(WIDTH / 2, HEIGHT / 4), (WIDTH / 2, HEIGHT * 3 / 4)] {
        let mut placement = None;
        for radius in 0..8isize {
            let mut candidates = Vec::new();
            for top in city_y as isize - 1 - radius..city_y as isize + radius {
                for left in city_x as isize - 1 - radius..city_x as isize + radius {
                    let cells: Vec<Cell> = (0..3)
                        .flat_map(|dy| (0..3).map(move |dx| (left + dx, top + dy)))
                        .filter_map(|(x, y)| {
                            (x >= 0 && y >= 0 && x < WIDTH as isize && y < HEIGHT as isize)
                                .then_some((x as usize, y as usize))
                        })
                        .collect();
                    if cells.len() != 9
                        || cells
                            .iter()
                            .any(|(x, y)| !dry[*y][*x] || city_cells[*y * WIDTH + *x])
                    {
                        continue;
                    }
                    let score: i64 = cells
                        .iter()
                        .map(|(x, y)| {
                            let dx = *x as i64 - city_x as i64;
                            let dy = *y as i64 - city_y as i64;
                            dx * dx + dy * dy
                        })
                        .sum();
                    candidates.push((score, left, top, cells));
                }
            }
            candidates.sort_by_key(|(score, left, top, _)| (*score, *left, *top));
            if let Some(candidate) = candidates.into_iter().next() {
                placement = Some(candidate);
                break;
            }
        }
        if let Some((_, left, top, cells)) = placement {
            for (index, (x, y)) in cells.into_iter().enumerate() {
                city_cells[y * WIDTH + x] = true;
                let (tile_x, tile_y) = BUILDING_CELLS[index];
                objects[y][x] = json!({
                    "tile": [tile_x, tile_y],
                    "hp_change": 0,
                    "removal": "fixed"
                });
            }
            cities.push(json!([left + 1, top + 1]));
        }
    }

    let mut tree_count = 0;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if !dry[y][x]
                || city_cells[y * WIDTH + x]
                || contains(&roads, (x, y))
                || contains(&railways, (x, y))
            {
                continue;
            }
            let seed = f64::from(seed);
            let forest = (x as f64 * 0.27 + seed).sin()
                + (y as f64 * 0.31 - seed * 0.3).cos()
                + ((x + y) as f64 * 0.17 + seed * 0.2).sin();
            let noise = (x * 73 + y * 151 + seed as usize * 17) % 43;
            if forest > 0.45 && noise < 2 {
                obstacles[y][x] = json!([(x + seed as usize) % 6, 5]);
                tree_count += 1;
            } else if forest < -0.8 && noise == 2 {
                obstacles[y][x] = json!([2 + (y + seed as usize) % 4, 4]);
                tree_count += 1;
            }
        }
    }

    let mut water_count = 0;
    let mut road_count = 0;
    let mut rail_count = 0;
    let mut road_bridge_count = 0;
    let mut rail_bridge_count = 0;
    let mut terrain_ids = vec![vec![0u16; WIDTH]; HEIGHT];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let tile_id = if water[y][x] {
                water_count += 1;
                if contains(&railways, (x, y)) {
                    rail_bridge_count += 1;
                    bridge_tile_id(connections(&railways, x, y), true, x + y)
                } else if contains(&roads, (x, y)) {
                    road_bridge_count += 1;
                    bridge_tile_id(connections(&roads, x, y), false, x + y)
                } else {
                    let neighbors = u8::from(y > 0 && dry[y - 1][x])
                        | (u8::from(x + 1 < WIDTH && dry[y][x + 1]) << 1)
                        | (u8::from(y + 1 < HEIGHT && dry[y + 1][x]) << 2)
                        | (u8::from(x > 0 && dry[y][x - 1]) << 3);
                    40 + u16::from(neighbors)
                }
            } else {
                let near_water = (y > 0 && water[y - 1][x])
                    || (x + 1 < WIDTH && water[y][x + 1])
                    || (y + 1 < HEIGHT && water[y + 1][x])
                    || (x > 0 && water[y][x - 1]);
                let mut tile_id = if near_water { 20 } else { 0 };
                if contains(&railways, (x, y)) {
                    tile_id = 70 + u16::from(connections(&railways, x, y));
                    rail_count += 1;
                } else if contains(&roads, (x, y))
                    && let Some((_, road_tile)) = ROAD_TILES
                        .iter()
                        .find(|(mask, _)| *mask == connections(&roads, x, y))
                {
                    tile_id = u16::from(*road_tile);
                    road_count += 1;
                }
                tile_id
            };
            terrain_ids[y][x] = tile_id;
        }
    }

    let source_tiles = terrain_ids.clone();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if terrain_ids[y][x] == 90 {
                terrain_ids[y][x] = 56;
            }
            let tile = source_tiles[y][x];
            if !(40..=59).contains(&tile) {
                continue;
            }
            let near_land = (y.saturating_sub(2)..(y + 3).min(HEIGHT)).any(|ny| {
                (x.saturating_sub(2)..(x + 3).min(WIDTH)).any(|nx| {
                    let neighbor = source_tiles[ny][nx];
                    !(40..=69).contains(&neighbor) && !(86..=89).contains(&neighbor)
                })
            });
            if !near_land {
                terrain_ids[y][x] = 90;
            }
        }
    }

    let mut terrain_usage = BTreeMap::<u16, usize>::new();
    let mut obstacle_usage = BTreeMap::<u16, usize>::new();
    let mut grid = Vec::with_capacity(HEIGHT);
    let mut runtime_objects = Vec::new();
    for y in 0..HEIGHT {
        let mut row = String::with_capacity(WIDTH * 4);
        for x in 0..WIDTH {
            let terrain = terrain_ids[y][x];
            *terrain_usage.entry(terrain).or_default() += 1;
            let obstacle = if obstacles[y][x].is_null() {
                0
            } else {
                let cell = obstacles[y][x]
                    .as_array()
                    .expect("generated obstacle is a tile");
                let tile_x = cell[0].as_u64().unwrap() as u16;
                let tile_y = cell[1].as_u64().unwrap() as u16;
                let obstacle_id = tile_y * 6 + tile_x;
                *obstacle_usage.entry(obstacle_id).or_default() += 1;
                obstacle_id + 1
            };
            row.push_str(&format!("{terrain:02x}{obstacle:02x}"));
            if !objects[y][x].is_null() {
                let object = &objects[y][x];
                let tile = object["tile"].as_array().unwrap();
                let tile_id = tile[1].as_u64().unwrap() * 6 + tile[0].as_u64().unwrap();
                runtime_objects.push(json!([y * WIDTH + x, tile_id, object["hp_change"], 0]));
            }
        }
        grid.push(Value::String(row));
    }

    let count_pairs = |counts: BTreeMap<u16, usize>| {
        counts
            .into_iter()
            .map(|(tile, count)| json!([tile, count]))
            .collect::<Vec<_>>()
    };
    let document = json!({
        "format_version": 2,
        "runtime_encoding": "grid-hex-v2",
        "name": name,
        "width": WIDTH,
        "height": HEIGHT,
        "tile_size": 64,
        "tileset": "tiles_terrain.png",
        "objects_tileset": "tiles_objects.png",
        "used_tiles": count_pairs(terrain_usage),
        "used_obstacles": count_pairs(obstacle_usage),
        "object_count": runtime_objects.len(),
        "grid": grid,
        "objects": runtime_objects,
        "geography": {
            "source": "Deterministic natural landscape generator",
            "landscape": kind,
            "seed": seed,
            "tile_counts": {
                "water": water_count,
                "roads": road_count,
                "railways": rail_count,
                "road_bridges": road_bridge_count,
                "rail_bridges": rail_bridge_count,
                "trees_and_rocks": tree_count,
                "city_tiles": cities.len() * BUILDING_CELLS.len()
            },
            "city_centers": cities,
            "rail_connection_bits": {
                "north": 1,
                "east": 2,
                "south": 4,
                "west": 8
            }
        }
    });
    Ok(document)
}

pub fn write_runtime_map(document: &Value, path: &Path) -> Result<(), String> {
    let text = serialize_runtime_map(document)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("cannot create map folder: {error}"))?;
    }
    fs::write(path, text).map_err(|error| format!("cannot write map {}: {error}", path.display()))
}

pub fn generate_maps(maps_dir: &Path) -> Result<Vec<PathBuf>, String> {
    MAPS.iter()
        .map(|(name, seed, kind)| {
            let document = generate_map(name, *seed, kind)?;
            let path = maps_dir.join(format!("{name}.json"));
            write_runtime_map(&document, &path)?;
            Ok(path)
        })
        .collect()
}

pub fn default_maps_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("programs")
        .join("tank-16")
        .join("assets/disk")
        .join("maps")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn generated_maps_are_distinct_runtime_documents_with_all_map_features() {
        let root = std::env::temp_dir().join(format!(
            "sc16-natural-maps-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = generate_maps(&root).unwrap();
        assert_eq!(paths.len(), MAPS.len());
        let mut layouts = Vec::new();
        for ((name, _, _), path) in MAPS.iter().zip(&paths) {
            let raw = fs::read(path).unwrap();
            let document: Value = serde_json::from_slice(&raw).unwrap();
            assert_eq!(document["name"], *name);
            assert_eq!(
                (document["width"].as_u64(), document["height"].as_u64()),
                (Some(48), Some(32))
            );
            assert_eq!(document["runtime_encoding"], "grid-hex-v2");
            assert!(raw.len() < 65_535);
            let counts = &document["geography"]["tile_counts"];
            for key in [
                "water",
                "roads",
                "railways",
                "road_bridges",
                "rail_bridges",
                "trees_and_rocks",
                "city_tiles",
            ] {
                assert!(counts[key].as_u64().unwrap() > 0, "{name}: {key}");
            }
            let grid = document["grid"].as_array().unwrap();
            assert_eq!(grid.len(), 32);
            assert!(grid.iter().all(|row| row.as_str().unwrap().len() == 48 * 4));
            layouts.push(
                grid.iter()
                    .map(|row| row.as_str().unwrap())
                    .collect::<String>(),
            );
        }
        layouts.sort();
        layouts.dedup();
        assert_eq!(layouts.len(), MAPS.len());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_offsets_point_to_json_grid_and_objects() {
        let document = generate_map("RIVER-01", 23, "river").unwrap();
        let text = serialize_runtime_map(&document).unwrap();
        let parsed: Value = serde_json::from_str(&text).unwrap();
        let bytes = text.as_bytes();
        let grid_offset = parsed["grid_offset"].as_u64().unwrap() as usize;
        let objects_offset = parsed["objects_offset"].as_u64().unwrap() as usize;
        assert_eq!(bytes[grid_offset], b'"');
        assert_eq!(bytes[objects_offset], b'[');
        assert_eq!(
            parsed["objects_size"].as_u64().unwrap() as usize,
            encode_hex(&parsed["objects"]).len()
        );
    }
}
