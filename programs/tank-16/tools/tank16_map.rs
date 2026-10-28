//! Implements Tank-16 map and runtime-asset processing.
//! Loads, validates, edits, and saves maps; converts tiles/assets and emits
//! guest headers, runtime map data, and executable builds.

use image::imageops::FilterType;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

pub const TILE_SIZE: u32 = 64;
pub const MAX_MAP_DIMENSION: usize = 1024;
pub const RUNTIME_MAP_ENCODING: &str = "grid-hex-v2";
pub const RUNTIME_MAP_HEADER_LIMIT: usize = 4096;
const WATER_TILE_MIN: u16 = 40;
const WATER_TILE_MAX: u16 = 59;
const DEEP_WATER_TILE: u16 = 90;
const REMOVAL_TYPES: [&str; 3] = ["fixed", "explosion", "disappear"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TilesetDimensions {
    pub columns: usize,
    pub rows: usize,
    pub width: u32,
    pub height: u32,
}

pub fn default_game_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("programs")
        .join("tank-16")
}

pub fn tileset_dimensions(path: &Path, pad_edges: bool) -> Result<TilesetDimensions, String> {
    let (width, height) = image::image_dimensions(path)
        .map_err(|error| format!("cannot load tileset {}: {error}", path.display()))?;
    let columns = if pad_edges {
        width.div_ceil(TILE_SIZE)
    } else {
        if !width.is_multiple_of(TILE_SIZE) || !height.is_multiple_of(TILE_SIZE) {
            return Err("tileset dimensions must be multiples of 64 pixels".to_owned());
        }
        width / TILE_SIZE
    } as usize;
    let rows = if pad_edges {
        height.div_ceil(TILE_SIZE)
    } else {
        height / TILE_SIZE
    } as usize;
    if columns == 0 || rows == 0 {
        return Err("tileset must contain at least one 64x64 tile".to_owned());
    }
    if columns * rows > 256 {
        return Err("tileset cannot contain more than 256 tiles".to_owned());
    }
    Ok(TilesetDimensions {
        columns,
        rows,
        width,
        height,
    })
}

pub fn validate_map_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 8 {
        return Err("map name must be 1 to 8 characters".to_owned());
    }
    if !name
        .bytes()
        .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-')
    {
        return Err("map name can contain letters, numbers, _ and -".to_owned());
    }
    Ok(name.to_ascii_uppercase())
}

fn integer(value: &Value, context: &str) -> Result<usize, String> {
    value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("{context} must be a non-negative integer"))
}

fn dimensions(value: &Value) -> Result<(usize, usize), String> {
    let width = integer(&value["width"], "map width")?;
    let height = integer(&value["height"], "map height")?;
    if width == 0 || height == 0 || width > MAX_MAP_DIMENSION || height > MAX_MAP_DIMENSION {
        return Err(format!(
            "map dimensions must be between 1 and {MAX_MAP_DIMENSION} tiles"
        ));
    }
    Ok((width, height))
}

fn tile_pair(
    value: &Value,
    columns: usize,
    rows: usize,
    context: &str,
) -> Result<[usize; 2], String> {
    let pair = value
        .as_array()
        .filter(|pair| pair.len() == 2)
        .ok_or_else(|| format!("{context} must be an [x, y] pair"))?;
    let x = pair[0]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("{context} must contain integer coordinates"))?;
    let y = pair[1]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("{context} must contain integer coordinates"))?;
    if x >= columns || y >= rows {
        return Err(format!("{context} is outside the tileset"));
    }
    Ok([x, y])
}

fn grid(width: usize, height: usize) -> Vec<Vec<Value>> {
    (0..height)
        .map(|_| (0..width).map(|_| Value::Null).collect())
        .collect()
}

pub fn new_map(name: &str, width: usize, height: usize) -> Result<Value, String> {
    let name = validate_map_name(name)?;
    if width == 0 || height == 0 || width > MAX_MAP_DIMENSION || height > MAX_MAP_DIMENSION {
        return Err(format!(
            "map width and height must be between 1 and {MAX_MAP_DIMENSION} tiles"
        ));
    }
    Ok(json!({
        "format_version": 1,
        "name": name,
        "width": width,
        "height": height,
        "tile_size": TILE_SIZE,
        "tileset": "tiles_terrain.png",
        "objects_tileset": "tiles_objects.png",
        "tiles": (0..height).map(|_| (0..width).map(|_| json!([0, 0])).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "obstacles": grid(width, height),
        "objects": grid(width, height)
    }))
}

pub fn set_map_tile(
    map: &mut Value,
    x: usize,
    y: usize,
    tile_x: usize,
    tile_y: usize,
    tileset: TilesetDimensions,
) -> Result<(), String> {
    let width = integer(&map["width"], "map width")?;
    let height = integer(&map["height"], "map height")?;
    if x >= width || y >= height {
        return Err("map coordinates are outside the map".to_owned());
    }
    if tile_x >= tileset.columns || tile_y >= tileset.rows {
        return Err("tile coordinates are outside the tileset".to_owned());
    }
    map["tiles"][y][x] = json!([tile_x, tile_y]);
    Ok(())
}

pub struct LayerTileEdit<'a> {
    pub layer: &'a str,
    pub x: usize,
    pub y: usize,
    pub tile: Option<[usize; 2]>,
    pub hp_change: i64,
    pub removal: &'a str,
}

pub fn set_layer_tile(
    map: &mut Value,
    edit: LayerTileEdit<'_>,
    tileset: TilesetDimensions,
) -> Result<(), String> {
    let LayerTileEdit {
        layer,
        x,
        y,
        tile,
        hp_change,
        removal,
    } = edit;
    if !matches!(layer, "obstacles" | "objects") {
        return Err("select the obstacle or object layer".to_owned());
    }
    let width = integer(&map["width"], "map width")?;
    let height = integer(&map["height"], "map height")?;
    if x >= width || y >= height {
        return Err("map coordinates are outside the map".to_owned());
    }
    if !(-100..=100).contains(&hp_change) {
        return Err("HP change must be between -100 and 100".to_owned());
    }
    if !REMOVAL_TYPES.contains(&removal) {
        return Err("removal must be fixed, explosion or disappear".to_owned());
    }
    let value = if let Some([tile_x, tile_y]) = tile {
        if tile_x >= tileset.columns || tile_y >= tileset.rows {
            return Err("tile coordinates are outside the object tileset".to_owned());
        }
        if layer == "objects" {
            json!({
                "tile": [tile_x, tile_y],
                "hp_change": hp_change,
                "removal": removal
            })
        } else {
            json!([tile_x, tile_y])
        }
    } else {
        Value::Null
    };
    map[layer][y][x] = value;
    Ok(())
}

fn validate_layer(
    source: &Value,
    key: &str,
    width: usize,
    height: usize,
    columns: usize,
    rows: usize,
    object_layer: bool,
) -> Result<Vec<Vec<Value>>, String> {
    let source = source.get(key).unwrap_or(&Value::Null);
    if source.is_null() {
        return Ok(grid(width, height));
    }
    let source_rows = source
        .as_array()
        .filter(|source| source.len() == height)
        .ok_or_else(|| format!("{key} rows do not match map height"))?;
    let mut output = grid(width, height);
    for (y, row) in source_rows.iter().enumerate() {
        let row = row
            .as_array()
            .filter(|row| row.len() == width)
            .ok_or_else(|| format!("{key} columns do not match map width"))?;
        for (x, value) in row.iter().enumerate() {
            if value.is_null() {
                continue;
            }
            let tile_value = if object_layer {
                value
                    .get("tile")
                    .ok_or_else(|| "each object must contain a tile".to_owned())?
            } else {
                value
            };
            let tile = tile_pair(tile_value, columns, rows, "layer tile")?;
            if object_layer {
                let hp_change = value["hp_change"]
                    .as_i64()
                    .ok_or_else(|| "object hp_change must be an integer".to_owned())?;
                if !(-100..=100).contains(&hp_change) {
                    return Err("object hp_change must be between -100 and 100".to_owned());
                }
                let removal = value["removal"]
                    .as_str()
                    .filter(|value| REMOVAL_TYPES.contains(value))
                    .ok_or_else(|| {
                        "object removal must be fixed, explosion or disappear".to_owned()
                    })?;
                output[y][x] = json!({
                    "tile": tile,
                    "hp_change": hp_change,
                    "removal": removal
                });
            } else {
                output[y][x] = json!(tile);
            }
        }
    }
    Ok(output)
}

fn parse_flat_runtime(
    source: &Value,
    terrain_columns: usize,
    terrain_rows: usize,
    object_columns: usize,
    object_rows: usize,
) -> Result<Value, String> {
    let (width, height) = dimensions(source)?;
    if width
        .checked_mul(height)
        .is_none_or(|cells| cells > MAX_MAP_DIMENSION.pow(2))
    {
        return Err("invalid compact map dimensions".to_owned());
    }
    let tiles_source = source["tiles"]
        .as_array()
        .filter(|tiles| tiles.len() == width * height)
        .ok_or_else(|| "compact map tile count does not match its dimensions".to_owned())?;
    let mut tiles = grid(width, height);
    for (index, tile) in tiles_source.iter().enumerate() {
        let tile = integer(tile, "compact terrain tile")?;
        if tile >= terrain_columns * terrain_rows {
            return Err("compact map contains an invalid terrain tile".to_owned());
        }
        tiles[index / width][index % width] =
            json!([tile % terrain_columns, tile / terrain_columns]);
    }
    let mut obstacles = grid(width, height);
    let mut objects = grid(width, height);
    for (key, layer, expected_len, object_layer) in [
        ("obstacles", &mut obstacles, 2usize, false),
        ("objects", &mut objects, 4usize, true),
    ] {
        let entries = source.get(key).and_then(Value::as_array);
        let Some(entries) = entries else {
            if source.get(key).is_some() {
                return Err(format!("compact {key} layer must be a list"));
            }
            continue;
        };
        let mut seen = std::collections::HashSet::new();
        for entry in entries {
            let values = entry
                .as_array()
                .filter(|values| values.len() == expected_len)
                .ok_or_else(|| format!("invalid compact {key} entry"))?;
            let index = integer(&values[0], "compact layer index")?;
            let tile = integer(&values[1], "compact layer tile")?;
            if index >= width * height
                || tile >= object_columns * object_rows
                || !seen.insert(index)
            {
                return Err(format!("compact {key} entry is outside the map"));
            }
            let cell = json!([tile % object_columns, tile / object_columns]);
            layer[index / width][index % width] = if object_layer {
                let hp_change = values[2]
                    .as_i64()
                    .ok_or_else(|| "compact object hp_change must be an integer".to_owned())?;
                let removal = integer(&values[3], "compact removal code")?;
                let removal = REMOVAL_TYPES
                    .get(removal)
                    .ok_or_else(|| "invalid compact object removal code".to_owned())?;
                json!({"tile": cell, "hp_change": hp_change, "removal": removal})
            } else {
                cell
            };
        }
    }
    let mut output = source.as_object().cloned().unwrap_or_default();
    output.remove("runtime_encoding");
    output.insert("tiles".to_owned(), json!(tiles));
    output.insert("obstacles".to_owned(), json!(obstacles));
    output.insert("objects".to_owned(), json!(objects));
    output.insert("format_version".to_owned(), json!(1));
    Ok(Value::Object(output))
}

fn parse_grid_runtime(
    source: &Value,
    terrain_columns: usize,
    terrain_rows: usize,
    object_columns: usize,
    object_rows: usize,
) -> Result<Value, String> {
    let (width, height) = dimensions(source)?;
    let rows = source["grid"]
        .as_array()
        .filter(|rows| rows.len() == height)
        .ok_or_else(|| "runtime map grid does not match its dimensions".to_owned())?;
    let mut tiles = grid(width, height);
    let mut obstacles = grid(width, height);
    for (y, row) in rows.iter().enumerate() {
        let row = row
            .as_str()
            .filter(|row| row.len() == width * 4 && row.is_ascii())
            .ok_or_else(|| "runtime map grid does not match its dimensions".to_owned())?;
        for x in 0..width {
            let terrain = u16::from_str_radix(&row[x * 4..x * 4 + 2], 16)
                .map_err(|_| "runtime map grid contains invalid hex".to_owned())?
                as usize;
            let obstacle = u16::from_str_radix(&row[x * 4 + 2..x * 4 + 4], 16)
                .map_err(|_| "runtime map grid contains invalid hex".to_owned())?
                as usize;
            if terrain >= terrain_columns * terrain_rows || obstacle > object_columns * object_rows
            {
                return Err("runtime map grid contains an invalid tile index".to_owned());
            }
            tiles[y][x] = json!([terrain % terrain_columns, terrain / terrain_columns]);
            if obstacle != 0 {
                let tile = obstacle - 1;
                obstacles[y][x] = json!([tile % object_columns, tile / object_columns]);
            }
        }
    }
    let mut output = source.as_object().cloned().unwrap_or_default();
    output.remove("runtime_encoding");
    output.remove("grid");
    output.remove("grid_offset");
    output.remove("objects_offset");
    output.remove("objects_size");
    output.insert("format_version".to_owned(), json!(1));
    output.insert("tiles".to_owned(), json!(tiles));
    output.insert("obstacles".to_owned(), json!(obstacles));
    let object_entries = source["objects"]
        .as_array()
        .ok_or_else(|| "runtime map objects must be a list".to_owned())?;
    let mut objects = grid(width, height);
    let mut seen = std::collections::HashSet::new();
    for entry in object_entries {
        let values = entry
            .as_array()
            .filter(|values| values.len() == 4)
            .ok_or_else(|| "invalid runtime object entry".to_owned())?;
        let index = integer(&values[0], "runtime object index")?;
        let tile = integer(&values[1], "runtime object tile")?;
        if index >= width * height || tile >= object_columns * object_rows || !seen.insert(index) {
            return Err("runtime object entry is outside the map".to_owned());
        }
        let hp_change = values[2]
            .as_i64()
            .ok_or_else(|| "runtime object hp_change must be an integer".to_owned())?;
        let removal = integer(&values[3], "runtime object removal code")?;
        let removal = REMOVAL_TYPES
            .get(removal)
            .ok_or_else(|| "invalid runtime object removal code".to_owned())?;
        objects[index / width][index % width] = json!({
            "tile": [tile % object_columns, tile / object_columns],
            "hp_change": hp_change,
            "removal": removal
        });
    }
    output.insert("objects".to_owned(), json!(objects));
    Ok(Value::Object(output))
}

pub fn validate_map(
    source: &Value,
    terrain: TilesetDimensions,
    objects: TilesetDimensions,
) -> Result<Value, String> {
    let source = source
        .as_object()
        .ok_or_else(|| "map file must contain a JSON object".to_owned())?;
    if source.get("format_version").and_then(Value::as_u64) != Some(1) {
        return Err("unsupported map file format".to_owned());
    }
    let source = Value::Object(source.clone());
    let name = validate_map_name(
        source["name"]
            .as_str()
            .ok_or_else(|| "map name must be text".to_owned())?,
    )?;
    let (width, height) = dimensions(&source)?;
    if source["tile_size"].as_u64() != Some(u64::from(TILE_SIZE)) {
        return Err("map tile size must be 64 pixels".to_owned());
    }
    let tileset = source
        .get("tileset")
        .and_then(Value::as_str)
        .unwrap_or("tiles_terrain.png");
    validate_tileset_filename(tileset)?;
    let terrain_source = source["tiles"]
        .as_array()
        .filter(|rows| rows.len() == height)
        .ok_or_else(|| "map tile rows do not match the map height".to_owned())?;
    let mut tiles = grid(width, height);
    for (y, row) in terrain_source.iter().enumerate() {
        let row = row
            .as_array()
            .filter(|row| row.len() == width)
            .ok_or_else(|| "map tile columns do not match the map width".to_owned())?;
        for (x, tile) in row.iter().enumerate() {
            tiles[y][x] = json!(tile_pair(tile, terrain.columns, terrain.rows, "map tile")?);
        }
    }
    let object_tileset = source
        .get("objects_tileset")
        .and_then(Value::as_str)
        .unwrap_or("tiles_objects.png");
    if object_tileset != "tiles_objects.png" {
        return Err("obstacle and object tileset must be tiles_objects.png".to_owned());
    }
    let obstacles = validate_layer(
        &source,
        "obstacles",
        width,
        height,
        objects.columns,
        objects.rows,
        false,
    )?;
    let object_layer = validate_layer(
        &source,
        "objects",
        width,
        height,
        objects.columns,
        objects.rows,
        true,
    )?;
    let mut result = Map::new();
    result.insert("format_version".to_owned(), json!(1));
    result.insert("name".to_owned(), json!(name));
    result.insert("width".to_owned(), json!(width));
    result.insert("height".to_owned(), json!(height));
    result.insert("tile_size".to_owned(), json!(TILE_SIZE));
    result.insert("tileset".to_owned(), json!(tileset));
    result.insert("objects_tileset".to_owned(), json!(object_tileset));
    result.insert("tiles".to_owned(), json!(tiles));
    result.insert("obstacles".to_owned(), json!(obstacles));
    result.insert("objects".to_owned(), json!(object_layer));
    if let Some(geography) = source.get("geography") {
        if !geography.is_object() {
            return Err("map geography metadata must be an object".to_owned());
        }
        result.insert("geography".to_owned(), geography.clone());
    }
    Ok(Value::Object(result))
}

pub fn validate_tileset_filename(filename: &str) -> Result<(), String> {
    let bytes = filename.as_bytes();
    if !filename.starts_with("tiles_")
        || !filename.ends_with(".png")
        || bytes.len() < 10
        || !bytes[6..bytes.len() - 4]
            .iter()
            .copied()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-')
    {
        return Err("tileset must be a tiles_*.png filename".to_owned());
    }
    Ok(())
}

fn apply_water_depth(tiles: &mut [Vec<Value>]) {
    let height = tiles.len();
    let width = tiles.first().map_or(0, Vec::len);
    let source: Vec<Vec<u16>> = tiles
        .iter()
        .map(|row| {
            row.iter()
                .map(|tile| {
                    let pair = tile.as_array().unwrap();
                    let x = pair[0].as_u64().unwrap() as u16;
                    let y = pair[1].as_u64().unwrap() as u16;
                    let id = y * 10 + x;
                    if id == DEEP_WATER_TILE { 56 } else { id }
                })
                .collect()
        })
        .collect();
    for y in 0..height {
        for x in 0..width {
            let id = source[y][x];
            if !(WATER_TILE_MIN..=WATER_TILE_MAX).contains(&id) {
                continue;
            }
            let min_y = y.saturating_sub(2);
            let max_y = (y + 3).min(height);
            let min_x = x.saturating_sub(2);
            let max_x = (x + 3).min(width);
            let near_land = (min_y..max_y).any(|ny| {
                (min_x..max_x).any(|nx| {
                    let neighbor = source[ny][nx];
                    !((WATER_TILE_MIN..=WATER_TILE_MAX).contains(&neighbor)
                        || (60..=69).contains(&neighbor)
                        || (86..=89).contains(&neighbor))
                })
            });
            let output_id = if near_land { id } else { DEEP_WATER_TILE };
            tiles[y][x] = json!([output_id % 10, output_id / 10]);
        }
    }
    for row in tiles.iter_mut() {
        for tile in row {
            let pair = tile.as_array().unwrap();
            let id = pair[1].as_u64().unwrap() as u16 * 10 + pair[0].as_u64().unwrap() as u16;
            if id == DEEP_WATER_TILE {
                *tile = json!([DEEP_WATER_TILE % 10, DEEP_WATER_TILE / 10]);
            }
        }
    }
}

pub fn load_map(game_dir: &Path, path: &Path) -> Result<Value, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("cannot read map {}: {error}", path.display()))?;
    let document: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("cannot parse map {}: {error}", path.display()))?;
    let filename = document
        .get("tileset")
        .and_then(Value::as_str)
        .unwrap_or("tiles_terrain.png");
    validate_tileset_filename(filename)?;
    let terrain = tileset_dimensions(&game_dir.join("assets/gfx").join(filename), false)?;
    let object_set =
        tileset_dimensions(&game_dir.join("assets/gfx").join("tiles_objects.png"), true)?;
    let expanded = match document.get("runtime_encoding").and_then(Value::as_str) {
        Some(RUNTIME_MAP_ENCODING) => parse_grid_runtime(
            &document,
            terrain.columns,
            terrain.rows,
            object_set.columns,
            object_set.rows,
        )?,
        Some("flat-v1") => parse_flat_runtime(
            &document,
            terrain.columns,
            terrain.rows,
            object_set.columns,
            object_set.rows,
        )?,
        Some(_) => return Err("unsupported runtime map encoding".to_owned()),
        None => document,
    };
    let mut normalized = validate_map(&expanded, terrain, object_set)?;
    let mut tiles = normalized["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row.as_array().unwrap().clone())
        .collect::<Vec<_>>();
    apply_water_depth(&mut tiles);
    normalized["tiles"] = json!(tiles);
    Ok(normalized)
}

fn encode_runtime_document(
    map_data: &Value,
    terrain: TilesetDimensions,
    objects: TilesetDimensions,
) -> Result<Value, String> {
    let mut map_data = validate_map(map_data, terrain, objects)?;
    if terrain.columns * terrain.rows > 256 || objects.columns * objects.rows > 255 {
        return Err("tilesets are too large for the runtime grid encoding".to_owned());
    }
    let width = map_data["width"].as_u64().unwrap() as usize;
    let height = map_data["height"].as_u64().unwrap() as usize;
    let mut tiles = map_data["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row.as_array().unwrap().clone())
        .collect::<Vec<_>>();
    apply_water_depth(&mut tiles);
    let mut terrain_usage = BTreeMap::<usize, usize>::new();
    let mut obstacle_usage = BTreeMap::<usize, usize>::new();
    let mut grid_rows = Vec::with_capacity(height);
    let mut runtime_objects = Vec::new();
    for y in 0..height {
        let mut row = String::with_capacity(width * 4);
        for x in 0..width {
            let terrain_pair =
                tile_pair(&tiles[y][x], terrain.columns, terrain.rows, "terrain tile")?;
            let terrain_id = terrain_pair[1] * terrain.columns + terrain_pair[0];
            *terrain_usage.entry(terrain_id).or_default() += 1;
            let obstacle = &map_data["obstacles"][y][x];
            let obstacle_code = if obstacle.is_null() {
                0
            } else {
                let pair = tile_pair(obstacle, objects.columns, objects.rows, "obstacle tile")?;
                let index = pair[1] * objects.columns + pair[0];
                *obstacle_usage.entry(index).or_default() += 1;
                index + 1
            };
            row.push_str(&format!("{terrain_id:02x}{obstacle_code:02x}"));
            let object = &map_data["objects"][y][x];
            if !object.is_null() {
                let pair = tile_pair(
                    &object["tile"],
                    objects.columns,
                    objects.rows,
                    "object tile",
                )?;
                let removal = REMOVAL_TYPES
                    .iter()
                    .position(|value| Some(*value) == object["removal"].as_str())
                    .ok_or_else(|| "unsupported object removal type".to_owned())?;
                runtime_objects.push(json!([
                    y * width + x,
                    pair[1] * objects.columns + pair[0],
                    object["hp_change"],
                    removal
                ]));
            }
        }
        grid_rows.push(Value::String(row));
    }
    let mut result = json!({
        "format_version": 2,
        "runtime_encoding": RUNTIME_MAP_ENCODING,
        "name": map_data["name"],
        "width": width,
        "height": height,
        "tile_size": TILE_SIZE,
        "tileset": map_data["tileset"],
        "objects_tileset": map_data["objects_tileset"],
        "used_tiles": terrain_usage.into_iter().map(|(tile, count)| json!([tile, count])).collect::<Vec<_>>(),
        "used_obstacles": obstacle_usage.into_iter().map(|(tile, count)| json!([tile, count])).collect::<Vec<_>>(),
        "object_count": runtime_objects.len(),
        "grid": grid_rows,
        "objects": runtime_objects
    });
    if let Some(geography) = map_data.get("geography").cloned() {
        result["geography"] = geography;
    }
    map_data = result;
    Ok(map_data)
}

pub fn serialize_runtime_map(document: &Value) -> Result<String, String> {
    let width = integer(&document["width"], "map width")?;
    let height = integer(&document["height"], "map height")?;
    let grid_rows = document["grid"]
        .as_array()
        .filter(|rows| rows.len() == height)
        .ok_or_else(|| "runtime map grid does not match its dimensions".to_owned())?;
    if grid_rows.iter().any(|row| {
        row.as_str()
            .is_none_or(|row| row.len() != width * 4 || !row.is_ascii())
    }) {
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
    let head = keys
        .iter()
        .map(|key| {
            let value = document
                .get(*key)
                .ok_or_else(|| format!("runtime map is missing {key}"))?;
            Ok(format!(
                "{}:{}",
                serde_json::to_string(key).unwrap(),
                serde_json::to_string(value).unwrap()
            ))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join(",");
    let rows = grid_rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            format!(
                "\"{}\"{}\n",
                row.as_str().unwrap(),
                if index + 1 < height { "," } else { " " }
            )
        })
        .collect::<String>();
    let objects = serde_json::to_string(&document["objects"]).unwrap();
    let geography = document
        .get("geography")
        .map(|value| format!(",\n\"geography\":{}", serde_json::to_string(value).unwrap()))
        .unwrap_or_default();
    let mut offsets = (0usize, 0usize, 0usize);
    let (prefix, grid_offset, objects_offset) = loop {
        let prefix = format!(
            "{{{head},\"grid_offset\":{},\"objects_offset\":{},\"objects_size\":{}",
            offsets.0, offsets.1, offsets.2
        );
        let grid_open = ",\n\"grid\":[\n";
        let grid_offset = prefix.len() + grid_open.len();
        let objects_offset = grid_offset + rows.len() + "],\n\"objects\":".len();
        let updated = (grid_offset, objects_offset, objects.len());
        if updated == offsets {
            break (prefix, grid_offset, objects_offset);
        }
        offsets = updated;
    };
    if grid_offset > RUNTIME_MAP_HEADER_LIMIT {
        return Err(format!(
            "runtime map header exceeds {RUNTIME_MAP_HEADER_LIMIT} bytes"
        ));
    }
    let text = format!("{prefix},\n\"grid\":[\n{rows}],\n\"objects\":{objects}{geography}}}\n");
    let bytes = text.as_bytes();
    if bytes.get(grid_offset) != Some(&b'"') || bytes.get(objects_offset) != Some(&b'[') {
        return Err("runtime map offsets are invalid".to_owned());
    }
    Ok(text)
}

pub fn save_map(game_dir: &Path, map_data: &Value, output_dir: &Path) -> Result<PathBuf, String> {
    let terrain = tileset_dimensions(&game_dir.join("assets/gfx/tiles_terrain.png"), false)?;
    let objects = tileset_dimensions(&game_dir.join("assets/gfx/tiles_objects.png"), true)?;
    let document = encode_runtime_document(map_data, terrain, objects)?;
    let name = validate_map_name(
        document["name"]
            .as_str()
            .ok_or_else(|| "map name must be text".to_owned())?,
    )?;
    let path = output_dir.join(format!("{name}.json"));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create map folder {}: {error}", parent.display()))?;
    }
    let text = serialize_runtime_map(&document)?;
    fs::write(&path, text)
        .map_err(|error| format!("cannot write map {}: {error}", path.display()))?;
    Ok(path)
}

pub fn runtime_document(map_data: &Value, game_dir: &Path) -> Result<Value, String> {
    let terrain = tileset_dimensions(&game_dir.join("assets/gfx/tiles_terrain.png"), false)?;
    let objects = tileset_dimensions(&game_dir.join("assets/gfx/tiles_objects.png"), true)?;
    encode_runtime_document(map_data, terrain, objects)
}

#[derive(Debug)]
pub struct RuntimeTileAssets {
    pub metadata: Value,
    pub tiles: Vec<Vec<u8>>,
    pub palette: Vec<[u8; 3]>,
    pub terrain: TilesetDimensions,
    pub objects: TilesetDimensions,
}

fn median_cut_palette(histogram: &BTreeMap<[u8; 3], u64>, limit: usize) -> Vec<[u8; 3]> {
    if histogram.len() <= limit {
        return histogram.keys().copied().collect();
    }
    let mut boxes = vec![
        histogram
            .iter()
            .map(|(color, count)| (*color, *count))
            .collect::<Vec<_>>(),
    ];
    while boxes.len() < limit {
        let mut selected = None;
        let mut best_score = -1i64;
        for (index, colors) in boxes.iter().enumerate() {
            if colors.len() < 2 {
                continue;
            }
            let mut ranges = [0u8; 3];
            for channel in 0..3 {
                let low = colors
                    .iter()
                    .map(|(color, _)| color[channel])
                    .min()
                    .unwrap();
                let high = colors
                    .iter()
                    .map(|(color, _)| color[channel])
                    .max()
                    .unwrap();
                ranges[channel] = high - low;
            }
            let channel = (0..3).max_by_key(|channel| ranges[*channel]).unwrap();
            let weight = colors.iter().map(|(_, count)| *count).sum::<u64>();
            let score = i64::from(ranges[channel]) * weight as i64;
            if score > best_score {
                best_score = score;
                selected = Some((index, channel));
            }
        }
        let Some((index, channel)) = selected else {
            break;
        };
        let mut colors = boxes.remove(index);
        colors.sort_by_key(|(color, _)| color[channel]);
        let midpoint = colors
            .iter()
            .map(|(_, count)| *count)
            .sum::<u64>()
            .div_ceil(2);
        let mut count = 0u64;
        let mut split = 1usize;
        for (index, (_, weight)) in colors.iter().enumerate() {
            count += weight;
            split = index + 1;
            if count >= midpoint {
                break;
            }
        }
        split = split.clamp(1, colors.len() - 1);
        boxes.push(colors[..split].to_vec());
        boxes.push(colors[split..].to_vec());
    }
    boxes
        .iter()
        .map(|colors| {
            let weight = colors.iter().map(|(_, count)| *count).sum::<u64>();
            let mut result = [0u8; 3];
            for channel in 0..3 {
                let total = colors
                    .iter()
                    .map(|(color, count)| u64::from(color[channel]) * count)
                    .sum::<u64>();
                result[channel] = ((total + weight / 2) / weight) as u8;
            }
            result
        })
        .collect()
}

pub fn load_map_assets(
    game_dir: &Path,
    map_path: &Path,
    tile_size: u32,
) -> Result<RuntimeTileAssets, String> {
    if !(1..=64).contains(&tile_size) {
        return Err("map tile pixel size must be between 1 and 64".to_owned());
    }
    let map = load_map(game_dir, map_path)?;
    let terrain_path = game_dir.join("assets/gfx").join(
        map["tileset"]
            .as_str()
            .ok_or_else(|| "map tileset must be text".to_owned())?,
    );
    let object_path = game_dir.join("assets/gfx/tiles_objects.png");
    let terrain_dimensions = tileset_dimensions(&terrain_path, false)?;
    let object_dimensions = tileset_dimensions(&object_path, true)?;
    let terrain_image = image::open(&terrain_path)
        .map_err(|error| {
            format!(
                "cannot decode terrain atlas {}: {error}",
                terrain_path.display()
            )
        })?
        .to_rgba8();
    let mut object_image = image::open(&object_path)
        .map_err(|error| {
            format!(
                "cannot decode object atlas {}: {error}",
                object_path.display()
            )
        })?
        .to_rgba8();
    let padded_object_image = image::RgbaImage::from_fn(
        (object_dimensions.columns as u32) * TILE_SIZE,
        (object_dimensions.rows as u32) * TILE_SIZE,
        |x, y| {
            object_image
                .get_pixel_checked(x, y)
                .copied()
                .unwrap_or(image::Rgba([0, 0, 0, 0]))
        },
    );
    object_image = padded_object_image;
    let mut images = Vec::with_capacity(
        terrain_dimensions.columns * terrain_dimensions.rows
            + object_dimensions.columns * object_dimensions.rows,
    );
    for index in 0..terrain_dimensions.columns * terrain_dimensions.rows {
        let x = (index % terrain_dimensions.columns) as u32 * TILE_SIZE;
        let y = (index / terrain_dimensions.columns) as u32 * TILE_SIZE;
        let tile = image::imageops::crop_imm(&terrain_image, x, y, TILE_SIZE, TILE_SIZE).to_image();
        images.push(image::imageops::resize(
            &tile,
            tile_size,
            tile_size,
            FilterType::Triangle,
        ));
    }
    for index in 0..object_dimensions.columns * object_dimensions.rows {
        let x = (index % object_dimensions.columns) as u32 * TILE_SIZE;
        let y = (index / object_dimensions.columns) as u32 * TILE_SIZE;
        let tile = image::imageops::crop_imm(&object_image, x, y, TILE_SIZE, TILE_SIZE).to_image();
        images.push(image::imageops::resize(
            &tile,
            tile_size,
            tile_size,
            FilterType::Triangle,
        ));
    }
    if images.len() > 256 {
        return Err("terrain and used object tiles exceed 256 GPU tile slots".to_owned());
    }
    let mut histogram = BTreeMap::<[u8; 3], u64>::new();
    for image in &images {
        for pixel in image.pixels().filter(|pixel| pixel[3] >= 128) {
            *histogram.entry([pixel[0], pixel[1], pixel[2]]).or_default() += 1;
        }
    }
    if histogram.is_empty() {
        return Err("tileset contains no opaque pixels".to_owned());
    }
    let atlas_palette = median_cut_palette(&histogram, 224);
    let mut palette = Vec::with_capacity(atlas_palette.len() + 1);
    palette.push([0, 0, 0]);
    palette.extend(atlas_palette.iter().copied());
    let mut indices = HashMap::with_capacity(histogram.len());
    for color in histogram.keys() {
        let closest = atlas_palette
            .iter()
            .enumerate()
            .min_by_key(|(_, candidate)| {
                (0..3)
                    .map(|channel| {
                        let delta = i32::from(color[channel]) - i32::from(candidate[channel]);
                        delta * delta
                    })
                    .sum::<i32>()
            })
            .map(|(index, _)| index)
            .ok_or_else(|| "cannot map colors to empty palette".to_owned())?;
        indices.insert(*color, (closest + 32) as u8);
    }
    let mut tiles = Vec::with_capacity(images.len());
    for image in &images {
        let mut pixels = Vec::with_capacity((tile_size * tile_size) as usize);
        for pixel in image.pixels() {
            if pixel[3] < 128 {
                pixels.push(0);
            } else {
                pixels.push(indices[&[pixel[0], pixel[1], pixel[2]]]);
            }
        }
        tiles.push(pixels);
    }
    let metadata = json!({
        "format": "SC16-TANK16-TILES",
        "version": 1,
        "pixel_encoding": "raw-index-v1",
        "tile_size": tile_size,
        "terrain_columns": terrain_dimensions.columns,
        "terrain_rows": terrain_dimensions.rows,
        "object_columns": object_dimensions.columns,
        "object_rows": object_dimensions.rows,
        "palette": atlas_palette,
        "tile_count": tiles.len()
    });
    Ok(RuntimeTileAssets {
        metadata,
        tiles,
        palette,
        terrain: terrain_dimensions,
        objects: object_dimensions,
    })
}

pub fn convert_tilesets_to_json(
    game_dir: &Path,
    output_path: &Path,
    tile_size: u32,
) -> Result<PathBuf, String> {
    let map_path = game_dir.join("assets/disk/maps/PERUS-1.json");
    let assets = load_map_assets(game_dir, &map_path, tile_size)?;
    let output_path = output_path.to_owned();
    let parent = output_path
        .parent()
        .ok_or_else(|| "runtime tile data path needs a parent directory".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create tile asset folder: {error}"))?;
    let binary_path = output_path.with_extension("bin");
    let tile_dir = parent.join("tiles");
    fs::create_dir_all(&tile_dir).map_err(|error| format!("cannot create tile folder: {error}"))?;
    let mut binary = Vec::new();
    for (index, pixels) in assets.tiles.iter().enumerate() {
        binary.extend_from_slice(pixels);
        fs::write(
            tile_dir.join(format!("tile_{index:03}.json")),
            format!("{{\"pixels\":\"{}\"}}\n", hex_encode(pixels)),
        )
        .map_err(|error| format!("cannot write tile JSON: {error}"))?;
    }
    fs::write(&binary_path, binary).map_err(|error| {
        format!(
            "cannot write tile binary {}: {error}",
            binary_path.display()
        )
    })?;
    let metadata = serde_json::to_vec(&assets.metadata)
        .map_err(|error| format!("cannot serialize tile metadata: {error}"))?;
    fs::write(&output_path, [metadata, vec![b'\n']].concat()).map_err(|error| {
        format!(
            "cannot write tile metadata {}: {error}",
            output_path.display()
        )
    })?;
    Ok(output_path)
}

pub fn map_tile_pixel_size(game_dir: &Path) -> Result<u32, String> {
    let source_path = game_dir.join("src/main.c");
    let source = fs::read_to_string(&source_path)
        .map_err(|error| format!("cannot read {}: {error}", source_path.display()))?;
    let value = source.lines().find_map(|line| {
        let line = line.trim();
        let mut parts = line.split_whitespace();
        (parts.next() == Some("#define") && parts.next() == Some("MAP_TILE_PIXEL_SIZE"))
            .then(|| parts.next()?.parse::<u32>().ok())
            .flatten()
    });
    let value = value.ok_or_else(|| "main.c must define MAP_TILE_PIXEL_SIZE".to_owned())?;
    if !(1..=64).contains(&value) {
        return Err("MAP_TILE_PIXEL_SIZE must be between 1 and 64".to_owned());
    }
    Ok(value)
}

pub fn runtime_assets_header(game_dir: &Path) -> Result<String, String> {
    let tile_json = game_dir.join("assets/disk/assets/TANK16_TILES.json");
    convert_tilesets_to_json(game_dir, &tile_json, map_tile_pixel_size(game_dir)?)?;
    Ok("#define RUNTIME_MAP_LOADING 1\n\
         #define MAP_COLUMNS map_columns\n\
         #define MAP_ROWS map_rows\n\
         #define MAP_OBJECT_COUNT 0\n\
         #define MAP_OBJECT_CAPACITY 1024\n\
         #define OBJECT_FIXED 0\n\
         #define OBJECT_EXPLOSION 1\n\
         #define OBJECT_DISAPPEAR 2\n\
         #define TERRAIN_COLUMNS runtime_terrain_columns\n\
         #define TERRAIN_ROWS runtime_terrain_rows\n\
         #define TERRAIN_TILE_COUNT (runtime_terrain_columns * runtime_terrain_rows)\n\
         #define OBJECT_TILE_BASE TERRAIN_TILE_COUNT\n\
         #define OBJECT_COLUMNS runtime_object_columns\n\
         #define OBJECT_ROWS runtime_object_rows\n\
         #define RUNTIME_TILE_SIZE runtime_tile_size\n\
         #define POWERUP_HEALTH_TILE (OBJECT_TILE_BASE + 2 * OBJECT_COLUMNS + 3)\n\
         #define POWERUP_DAMAGE_TILE (OBJECT_TILE_BASE + 2 * OBJECT_COLUMNS + 4)\n\
         #define POWERUP_SPEED_TILE (OBJECT_TILE_BASE + 2 * OBJECT_COLUMNS + 5)\n\
         #define RUNTIME_TILE_COUNT runtime_tile_count\n\
         #define TILE_PIXEL_COUNT (RUNTIME_TILE_SIZE * RUNTIME_TILE_SIZE)\n\
         #define MAP_PALETTE_COUNT runtime_palette_count\n\
         #define MAP_PALETTE_BASE 32\n\
         #define RUNTIME_TILE_DATA_PATH \"assets/TANK16_TILES.json\"\n\
         #define RUNTIME_TILE_BINARY_PATH \"assets/TANK16_TILES.bin\"\n\
         int map_columns;\n\
         int map_rows;\n\
         int runtime_terrain_columns;\n\
         int runtime_terrain_rows;\n\
         int runtime_object_columns;\n\
         int runtime_object_rows;\n\
         int runtime_tile_size;\n\
         int runtime_tile_count;\n\
         int runtime_palette_count;\n\
         int runtime_tile_data_ready;\n\
         int runtime_tile_binary;\n\
         int runtime_tile_loaded[256];\n\
         unsigned char *map_data;\n\
         unsigned char *map_obstacles;\n\
         volatile unsigned char map_json_chunk[4096];\n\
         volatile char map_path_buffer[129];\n\
         char map_selected_path[129];\n\
         int map_object_x[MAP_OBJECT_CAPACITY];\n\
         int map_object_y[MAP_OBJECT_CAPACITY];\n\
         int map_object_tile[MAP_OBJECT_CAPACITY];\n\
         int map_object_source_tile[MAP_OBJECT_CAPACITY];\n\
         int map_object_hp[MAP_OBJECT_CAPACITY];\n\
         int map_object_removal[MAP_OBJECT_CAPACITY];\n\
         unsigned char runtime_tile_pixels[4096];\n"
        .to_owned())
}

pub fn compile_runtime_tank16(game_dir: &Path) -> Result<crate::image::ProgramImage, String> {
    let source_path = game_dir.join("src/main.c");
    let source = fs::read_to_string(&source_path)
        .map_err(|error| format!("cannot read {}: {error}", source_path.display()))?;
    let include = "#include \"map_assets.h\"";
    if source.matches(include).count() != 1 {
        return Err("Tank-16 main.c must include map_assets.h exactly once".to_owned());
    }
    let header = runtime_assets_header(game_dir)?;
    let source = source.replacen(include, &header, 1);
    compile_with_rust_c_compiler(&source, &source_path)
}

fn compile_with_rust_c_compiler(
    source: &str,
    source_path: &Path,
) -> Result<crate::image::ProgramImage, String> {
    crate::c_compiler::compile_minimal_program(source, &source_path.to_string_lossy())
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            format!(
                "Rust C compiler does not support Tank-16 source {}",
                source_path.display()
            )
        })
}

pub fn discover_round_maps(game_dir: &Path, selected_path: &Path) -> Result<Vec<PathBuf>, String> {
    let selected = load_map(game_dir, selected_path)?;
    let dimensions = (selected["width"].clone(), selected["height"].clone());
    let selected_tileset = selected["tileset"].clone();
    let selected_objects = object_tile_coordinates(&selected)?;
    let directory = game_dir.join("assets/disk/maps");
    let entries = fs::read_dir(&directory)
        .map_err(|error| format!("cannot list maps in {}: {error}", directory.display()))?;
    let mut candidates = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot inspect map folder: {error}"))?
            .path();
        if path.extension().is_none_or(|extension| extension != "json")
            || path.canonicalize().ok() == selected_path.canonicalize().ok()
        {
            continue;
        }
        let candidate_json = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read map {}: {error}", path.display()))?;
        let candidate_document: Value = serde_json::from_str(&candidate_json)
            .map_err(|error| format!("cannot parse map {}: {error}", path.display()))?;
        if candidate_document["width"].as_u64().is_none()
            || candidate_document["height"].as_u64().is_none()
            || candidate_document["tileset"].as_str().is_none()
        {
            continue;
        }
        let candidate = load_map(game_dir, &path)?;
        if candidate["width"] != dimensions.0
            || candidate["height"] != dimensions.1
            || candidate["tileset"] != selected_tileset
        {
            continue;
        }
        if object_tile_coordinates(&candidate)?
            .iter()
            .all(|coordinate| selected_objects.contains(coordinate))
        {
            candidates.push(path);
        }
    }
    candidates.sort();
    Ok(candidates)
}

pub fn object_tile_coordinates(map: &Value) -> Result<Vec<[usize; 2]>, String> {
    let mut coordinates = BTreeMap::<(usize, usize), ()>::new();
    for coordinate in [[3, 2], [4, 2], [5, 2]] {
        coordinates.insert((coordinate[1], coordinate[0]), ());
    }
    for layer in ["obstacles", "objects"] {
        for row in map[layer]
            .as_array()
            .ok_or_else(|| format!("map {layer} layer is invalid"))?
        {
            for cell in row
                .as_array()
                .ok_or_else(|| format!("map {layer} row is invalid"))?
            {
                if cell.is_null() {
                    continue;
                }
                let tile = if layer == "objects" {
                    &cell["tile"]
                } else {
                    cell
                };
                let pair = tile
                    .as_array()
                    .filter(|pair| pair.len() == 2)
                    .ok_or_else(|| format!("map {layer} tile is invalid"))?;
                let x = integer(&pair[0], "object tile x")?;
                let y = integer(&pair[1], "object tile y")?;
                coordinates.insert((y, x), ());
            }
        }
    }
    Ok(coordinates.into_keys().map(|(y, x)| [x, y]).collect())
}

fn format_c_values(values: &[i64], width: usize) -> String {
    values
        .chunks(width)
        .map(|row| {
            format!(
                "    {}",
                row.iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(",\n")
}

fn map_tile_ids(map: &Value, terrain_columns: usize) -> Result<Vec<i64>, String> {
    let mut ids = Vec::new();
    for row in map["tiles"]
        .as_array()
        .ok_or_else(|| "map terrain layer is invalid".to_owned())?
    {
        for tile in row
            .as_array()
            .ok_or_else(|| "map terrain row is invalid".to_owned())?
        {
            let pair = tile
                .as_array()
                .filter(|pair| pair.len() == 2)
                .ok_or_else(|| "map terrain tile is invalid".to_owned())?;
            let x = integer(&pair[0], "terrain tile x")?;
            let y = integer(&pair[1], "terrain tile y")?;
            ids.push((y * terrain_columns + x) as i64);
        }
    }
    Ok(ids)
}

fn map_object_cells(map: &Value) -> Result<Vec<(usize, usize, Value)>, String> {
    let mut objects = Vec::new();
    for (y, row) in map["objects"]
        .as_array()
        .ok_or_else(|| "map object layer is invalid".to_owned())?
        .iter()
        .enumerate()
    {
        for (x, cell) in row
            .as_array()
            .ok_or_else(|| "map object row is invalid".to_owned())?
            .iter()
            .enumerate()
        {
            if !cell.is_null() {
                objects.push((x, y, cell.clone()));
            }
        }
    }
    Ok(objects)
}

fn object_tile_id(
    cell: &Value,
    ids: &HashMap<[usize; 2], i64>,
    layer: &str,
) -> Result<i64, String> {
    let tile = if layer == "objects" {
        &cell["tile"]
    } else {
        cell
    };
    let pair = tile
        .as_array()
        .filter(|pair| pair.len() == 2)
        .ok_or_else(|| format!("{layer} tile is invalid"))?;
    let coordinate = [
        integer(&pair[0], "object tile x")?,
        integer(&pair[1], "object tile y")?,
    ];
    ids.get(&coordinate)
        .copied()
        .ok_or_else(|| format!("{layer} tile is not in the selected map atlas"))
}

fn object_field_arrays(
    objects: &[(usize, usize, Value)],
    ids: &HashMap<[usize; 2], i64>,
    object_columns: usize,
) -> Result<BTreeMap<&'static str, Vec<i64>>, String> {
    let mut fields = BTreeMap::from([
        ("x", Vec::new()),
        ("y", Vec::new()),
        ("tile", Vec::new()),
        ("source_tile", Vec::new()),
        ("hp", Vec::new()),
        ("removal", Vec::new()),
    ]);
    for (x, y, cell) in objects {
        let tile = cell["tile"]
            .as_array()
            .filter(|pair| pair.len() == 2)
            .ok_or_else(|| "map object tile is invalid".to_owned())?;
        let tile_x = integer(&tile[0], "object tile x")?;
        let tile_y = integer(&tile[1], "object tile y")?;
        let hp = cell["hp_change"]
            .as_i64()
            .ok_or_else(|| "map object hp_change is invalid".to_owned())?;
        let removal = cell["removal"]
            .as_str()
            .ok_or_else(|| "map object removal is invalid".to_owned())?;
        let removal_code = REMOVAL_TYPES
            .iter()
            .position(|value| *value == removal)
            .ok_or_else(|| "map object removal is unsupported".to_owned())?;
        fields.get_mut("x").unwrap().push(*x as i64);
        fields.get_mut("y").unwrap().push(*y as i64);
        fields
            .get_mut("tile")
            .unwrap()
            .push(object_tile_id(cell, ids, "objects")?);
        fields
            .get_mut("source_tile")
            .unwrap()
            .push((tile_y * object_columns + tile_x) as i64);
        fields.get_mut("hp").unwrap().push(hp);
        fields.get_mut("removal").unwrap().push(removal_code as i64);
    }
    Ok(fields)
}

pub fn build_map_header(
    game_dir: &Path,
    map_path: &Path,
    round_map_paths: &[PathBuf],
) -> Result<String, String> {
    build_map_header_with_tile_size(
        game_dir,
        map_path,
        round_map_paths,
        map_tile_pixel_size(game_dir)?,
    )
}

pub fn build_map_header_with_tile_size(
    game_dir: &Path,
    map_path: &Path,
    round_map_paths: &[PathBuf],
    tile_size: u32,
) -> Result<String, String> {
    let map_data = load_map(game_dir, map_path)?;
    let assets = load_map_assets(game_dir, map_path, tile_size)?;
    let terrain_columns = assets.terrain.columns;
    let terrain_count = terrain_columns * assets.terrain.rows;
    let coordinates = object_tile_coordinates(&map_data)?;
    let mut object_ids = HashMap::new();
    for (index, coordinate) in coordinates.iter().enumerate() {
        object_ids.insert(*coordinate, (terrain_count + index) as i64);
    }
    let mut maps = vec![map_data];
    for round_map_path in round_map_paths {
        let round_map = load_map(game_dir, round_map_path)?;
        if round_map["width"] != maps[0]["width"]
            || round_map["height"] != maps[0]["height"]
            || round_map["tileset"] != maps[0]["tileset"]
        {
            return Err(format!(
                "round map {} must match the selected map dimensions and terrain tileset",
                round_map_path.display()
            ));
        }
        if !object_tile_coordinates(&round_map)?
            .iter()
            .all(|coordinate| object_ids.contains_key(coordinate))
        {
            return Err(format!(
                "round map {} uses object tiles not included by the selected map",
                round_map_path.display()
            ));
        }
        maps.push(round_map);
    }
    let map_values = map_tile_ids(&maps[0], terrain_columns)?;
    let mut map_data_header = String::new();
    if map_values.len() <= 32768 {
        map_data_header.push_str(&format!(
            "unsigned char map_data[MAP_CELLS] = {{\n{}\n}};\n",
            format_c_values(&map_values, 16)
        ));
    } else {
        for (page, values) in map_values.chunks(32768).enumerate() {
            map_data_header.push_str(&format!(
                "unsigned char map_data_{page}[{}] = {{\n{}\n}};\n",
                values.len(),
                format_c_values(values, 16)
            ));
        }
        map_data_header.push_str("#define map_data map_data_0\n");
    }
    let selected_tile_indices = (0..terrain_count)
        .chain(coordinates.iter().map(|coordinate| {
            terrain_count + coordinate[1] * assets.objects.columns + coordinate[0]
        }))
        .collect::<Vec<_>>();
    let tile_arrays = selected_tile_indices
        .iter()
        .map(|index| {
            format!(
                "    {{\n     {}\n    }}",
                format_c_values(
                    &assets.tiles[*index]
                        .iter()
                        .map(|value| i64::from(*value))
                        .collect::<Vec<_>>(),
                    16
                )
                .replace("\n", "\n     ")
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let upload_tiles = (0..selected_tile_indices.len())
        .map(|index| format!("    gfx_tile_upload({index}, terrain_tiles[{index}]);"))
        .collect::<Vec<_>>()
        .join("\n");
    let terrain_count_i64 = terrain_count as i64;
    let terrain_ids = coordinates
        .iter()
        .enumerate()
        .map(|(index, coordinate)| (*coordinate, terrain_count_i64 + index as i64))
        .collect::<HashMap<_, _>>();
    let width = integer(&maps[0]["width"], "map width")?;
    let height = integer(&maps[0]["height"], "map height")?;
    let mut obstacle_tiles = Vec::with_capacity(width * height);
    for row in maps[0]["obstacles"].as_array().unwrap() {
        for cell in row.as_array().unwrap() {
            obstacle_tiles.push(if cell.is_null() {
                0
            } else {
                object_tile_id(cell, &terrain_ids, "obstacle")?
            });
        }
    }
    let objects = map_object_cells(&maps[0])?;
    let variant_objects = maps
        .iter()
        .map(map_object_cells)
        .collect::<Result<Vec<_>, _>>()?;
    let capacity = variant_objects
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut layer_header = format!(
        "#define MAP_VARIANT_COUNT {}\n#define MAP_OBJECT_COUNT {}\n#define MAP_OBJECT_CAPACITY {capacity}\n#define POWERUP_HEALTH_TILE {}\n#define POWERUP_DAMAGE_TILE {}\n#define POWERUP_SPEED_TILE {}\n#define OBJECT_FIXED 0\n#define OBJECT_EXPLOSION 1\n#define OBJECT_DISAPPEAR 2\n",
        maps.len(),
        objects.len(),
        object_ids[&[3, 2]],
        object_ids[&[4, 2]],
        object_ids[&[5, 2]]
    );
    if obstacle_tiles.len() <= 32768 {
        layer_header.push_str(&format!(
            "unsigned char map_obstacles[MAP_CELLS] = {{\n{}\n}};\n",
            format_c_values(&obstacle_tiles, 16)
        ));
    } else if obstacle_tiles.iter().all(|value| *value == 0) {
        layer_header.push_str("#define MAP_OBSTACLES_EMPTY 1\n");
    } else {
        let page_rows = 32;
        let page_capacity = page_rows * width;
        for page in 0..height.div_ceil(page_rows) {
            let start = page * page_capacity;
            let mut values =
                obstacle_tiles[start..(start + page_capacity).min(obstacle_tiles.len())].to_vec();
            values.resize(page_capacity, 0);
            layer_header.push_str(&format!(
                "unsigned char map_obstacles_{page}[{page_capacity}] = {{\n{}\n}};\n",
                format_c_values(&values, 16)
            ));
        }
    }
    let fields_by_map = variant_objects
        .iter()
        .map(|objects| object_field_arrays(objects, &terrain_ids, assets.objects.columns))
        .collect::<Result<Vec<_>, _>>()?;
    let fields = fields_by_map
        .first()
        .ok_or_else(|| "map variants are empty".to_owned())?;
    for (name, values) in fields {
        layer_header.push_str(&format!(
            "int map_object_{name}[MAP_OBJECT_CAPACITY] = {{\n{}\n}};\n",
            format_c_values(if values.is_empty() { &[0] } else { values }, 16)
        ));
    }
    if maps.len() > 1 {
        let variant_map_values = maps
            .iter()
            .map(|map| map_tile_ids(map, terrain_columns))
            .collect::<Result<Vec<_>, _>>()?;
        let variant_obstacles = maps
            .iter()
            .map(|map| {
                map["obstacles"]
                    .as_array()
                    .ok_or_else(|| "map obstacle layer is invalid".to_owned())?
                    .iter()
                    .flat_map(|row| row.as_array().unwrap())
                    .map(|cell| {
                        if cell.is_null() {
                            Ok(0)
                        } else {
                            object_tile_id(cell, &terrain_ids, "obstacle")
                        }
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        layer_header.push_str(&format!(
            "unsigned char map_variant_data[MAP_VARIANT_COUNT][MAP_CELLS] = {{\n{}\n}};\n",
            variant_map_values
                .iter()
                .map(|values| format!(
                    "    {{{}}}",
                    format_c_values(values, 16).replace("\n", "\n     ")
                ))
                .collect::<Vec<_>>()
                .join(",\n")
        ));
        layer_header.push_str(&format!(
            "unsigned char map_variant_obstacles[MAP_VARIANT_COUNT][MAP_CELLS] = {{\n{}\n}};\n",
            variant_obstacles
                .iter()
                .map(|values| format!(
                    "    {{{}}}",
                    format_c_values(values, 16).replace("\n", "\n     ")
                ))
                .collect::<Vec<_>>()
                .join(",\n")
        ));
        layer_header.push_str(&format!(
            "int map_variant_object_count[MAP_VARIANT_COUNT] = {{{}}};\n",
            variant_objects
                .iter()
                .map(Vec::len)
                .map(|count| count.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        for name in fields.keys() {
            let variant_rows = fields_by_map
                .iter()
                .map(|fields| {
                    let mut values = fields[name].clone();
                    values.resize(capacity, 0);
                    format!(
                        "    {{{}}}",
                        format_c_values(&values, 16).replace("\n", "\n     ")
                    )
                })
                .collect::<Vec<_>>()
                .join(",\n");
            layer_header.push_str(&format!(
                "int map_variant_object_{name}[MAP_VARIANT_COUNT][MAP_OBJECT_CAPACITY] = {{\n{variant_rows}\n}};\n"
            ));
        }
    }
    let palette_values = assets
        .palette
        .iter()
        .map(|color| format!("    {{{}, {}, {}}}", color[0], color[1], color[2]))
        .collect::<Vec<_>>()
        .join(",\n");
    Ok(format!(
        "#define MAP_COLUMNS {width}\n#define MAP_ROWS {height}\n#define MAP_CELLS (MAP_COLUMNS * MAP_ROWS)\n#define MAP_TERRAIN_TILE_COUNT {terrain_count}\n#define TILE_COUNT {}\n#define TILE_PIXEL_COUNT {}\n#define MAP_PALETTE_COUNT {}\n#define MAP_PALETTE_BASE 32\n#define MAP_TILESET_FILENAME \"{}\"\nunsigned char map_palette[MAP_PALETTE_COUNT][3] = {{\n{palette_values}\n}};\nunsigned char terrain_tiles[TILE_COUNT][TILE_PIXEL_COUNT] = {{\n{tile_arrays}\n}};\nvoid upload_terrain_tiles(void) {{\n{upload_tiles}\n}}\n{map_data_header}{layer_header}",
        selected_tile_indices.len(),
        tile_size * tile_size,
        assets.palette.len(),
        maps[0]["tileset"].as_str().unwrap()
    ))
}

pub fn compile_map_file(
    game_dir: &Path,
    map_path: &Path,
    randomize_rounds: bool,
) -> Result<crate::image::ProgramImage, String> {
    let source_path = game_dir.join("src/main.c");
    let source = fs::read_to_string(&source_path)
        .map_err(|error| format!("cannot read {}: {error}", source_path.display()))?;
    let include = "#include \"map_assets.h\"";
    if source.matches(include).count() != 1 {
        return Err("Tank-16 main.c must include map_assets.h exactly once".to_owned());
    }
    let round_maps = if randomize_rounds {
        discover_round_maps(game_dir, map_path)?
    } else {
        Vec::new()
    };
    let header = build_map_header(game_dir, map_path, &round_maps)?;
    compile_with_rust_c_compiler(&source.replacen(include, &header, 1), &source_path)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 15) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_map_validates_limits_and_uppercases_names() {
        let map = new_map(" test-1 ", 2, 3).unwrap();
        assert_eq!(map["name"], "TEST-1");
        assert_eq!(map["tiles"].as_array().unwrap().len(), 3);
        assert!(new_map("TOO-LONG-9", 2, 3).is_err());
        assert!(new_map("BAD NAME", 2, 3).is_err());
        assert!(new_map("OK", 1025, 1).is_err());
    }

    #[test]
    fn edits_and_round_trips_terrain_obstacle_and_object_layers() {
        let game_dir = default_game_dir();
        let terrain =
            tileset_dimensions(&game_dir.join("assets/gfx/tiles_terrain.png"), false).unwrap();
        let objects =
            tileset_dimensions(&game_dir.join("assets/gfx/tiles_objects.png"), true).unwrap();
        let mut map = new_map("LAYERS", 3, 2).unwrap();
        set_map_tile(&mut map, 2, 1, 3, 2, terrain).unwrap();
        set_layer_tile(
            &mut map,
            LayerTileEdit {
                layer: "obstacles",
                x: 0,
                y: 1,
                tile: Some([2, 3]),
                hp_change: -25,
                removal: "explosion",
            },
            objects,
        )
        .unwrap();
        set_layer_tile(
            &mut map,
            LayerTileEdit {
                layer: "objects",
                x: 1,
                y: 1,
                tile: Some([1, 2]),
                hp_change: 35,
                removal: "disappear",
            },
            objects,
        )
        .unwrap();
        let output_dir =
            std::env::temp_dir().join(format!("sc16-map-layers-{}", std::process::id()));
        let saved = save_map(&game_dir, &map, &output_dir).unwrap();
        let loaded = load_map(&game_dir, &saved).unwrap();
        assert_eq!(loaded["tiles"][1][2], json!([3, 2]));
        assert_eq!(loaded["obstacles"][1][0], json!([2, 3]));
        assert_eq!(loaded["objects"][1][1]["tile"], json!([1, 2]));
        assert_eq!(loaded["objects"][1][1]["hp_change"], 35);
        assert_eq!(loaded["objects"][1][1]["removal"], "disappear");
        fs::remove_dir_all(output_dir).unwrap();
    }

    #[test]
    fn static_map_header_contains_layer_and_tile_metadata() {
        let game_dir = default_game_dir();
        let map_path = game_dir.join("assets/disk/maps/PERUS-1.json");
        let header = build_map_header(&game_dir, &map_path, &[]).unwrap();
        assert!(header.contains("#define MAP_COLUMNS 10"));
        assert!(header.contains("#define MAP_ROWS 8"));
        assert!(header.contains("#define MAP_VARIANT_COUNT 1"));
        assert!(header.contains("unsigned char map_data[MAP_CELLS]"));
        assert!(header.contains("unsigned char map_obstacles[MAP_CELLS]"));
        assert!(header.contains("int map_object_source_tile[MAP_OBJECT_CAPACITY]"));
        assert!(header.contains("void upload_terrain_tiles(void)"));
    }

    #[test]
    fn round_map_discovery_skips_infinite_world_data() {
        let game_dir = default_game_dir();
        let selected = game_dir.join("assets/disk/maps/PERUS-1.json");
        let rounds = discover_round_maps(&game_dir, &selected).unwrap();
        assert!(!rounds.is_empty());
        assert!(
            rounds
                .iter()
                .all(|path| path.file_name().is_none_or(|name| name != "INFINITE.json"))
        );
        for path in &rounds {
            let candidate = load_map(&game_dir, path).unwrap();
            assert_eq!(candidate["width"], 10);
            assert_eq!(candidate["height"], 8);
        }
        let header = build_map_header(&game_dir, &selected, &rounds[..1]).unwrap();
        assert!(header.contains("#define MAP_VARIANT_COUNT 2"));
        assert!(header.contains("map_variant_data[MAP_VARIANT_COUNT][MAP_CELLS]"));
        assert!(header.contains("map_variant_object_source_tile"));
    }

    #[test]
    fn parses_and_serializes_runtime_map_with_fixed_offsets() {
        let root = default_game_dir();
        let map = new_map("TINY", 2, 2).unwrap();
        let normalized = validate_map(
            &map,
            tileset_dimensions(&root.join("assets/gfx/tiles_terrain.png"), false).unwrap(),
            tileset_dimensions(&root.join("assets/gfx/tiles_objects.png"), true).unwrap(),
        )
        .unwrap();
        let document = runtime_document(&normalized, &root).unwrap();
        let serialized = serialize_runtime_map(&document).unwrap();
        let parsed: Value = serde_json::from_str(&serialized).unwrap();
        let grid_offset = parsed["grid_offset"].as_u64().unwrap() as usize;
        let objects_offset = parsed["objects_offset"].as_u64().unwrap() as usize;
        assert_eq!(serialized.as_bytes()[grid_offset], b'"');
        assert_eq!(serialized.as_bytes()[objects_offset], b'[');
        let path = std::env::temp_dir().join(format!("sc16-map-core-{}.json", std::process::id()));
        fs::write(&path, serialized).unwrap();
        assert_eq!(load_map(&root, &path).unwrap()["width"], 2);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn loads_checked_in_tank_map_and_emits_complete_tile_assets() {
        let root = default_game_dir();
        let map = load_map(&root, &root.join("assets/disk/maps/PERUS-1.json")).unwrap();
        assert_eq!(
            (map["width"].as_u64(), map["height"].as_u64()),
            (Some(10), Some(8))
        );
        let assets =
            load_map_assets(&root, &root.join("assets/disk/maps/PERUS-1.json"), 64).unwrap();
        assert_eq!(assets.terrain.columns, 10);
        assert_eq!(assets.terrain.rows, 10);
        assert_eq!(assets.objects.columns, 6);
        assert_eq!(assets.objects.rows, 8);
        assert_eq!(assets.tiles.len(), 148);
        assert!(assets.palette.len() <= 225);
        assert!(assets.tiles.iter().all(|tile| tile.len() == 64 * 64));
    }

    #[test]
    fn loads_every_checked_in_tank_map() {
        let root = default_game_dir();
        let maps_dir = root.join("assets/disk/maps");
        let entries = fs::read_dir(&maps_dir).unwrap();
        let mut loaded = 0;
        for entry in entries {
            let path = entry.unwrap().path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                let document: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                if document["infinite"].as_bool() == Some(true) {
                    continue;
                }
                load_map(&root, &path)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                loaded += 1;
            }
        }
        assert!(loaded > 0);
    }
}
