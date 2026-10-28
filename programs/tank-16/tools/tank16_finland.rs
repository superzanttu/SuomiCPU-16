//! Builds Tank-16 maps from geographic data for Finland.
//! Processes country, lake, road, railway, and city features into terrain,
//! map objects, and preview output.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const EARTH_RADIUS_KM: f64 = 6371.0088;
const TILE_SIZE: u32 = 64;
const ROAD_TILE_BY_CONNECTIONS: [Option<u16>; 16] = [
    None,
    Some(19),
    Some(9),
    Some(15),
    Some(1),
    Some(1),
    Some(13),
    Some(3),
    Some(2),
    Some(16),
    Some(2),
    Some(6),
    Some(14),
    Some(4),
    Some(5),
    Some(11),
];
const FALLBACK_RAIL_ROUTES: &[&[(f64, f64)]] = &[
    &[
        (24.94, 60.17),
        (23.76, 61.50),
        (22.84, 62.79),
        (25.75, 62.24),
        (27.68, 62.89),
        (27.73, 64.23),
        (25.47, 65.01),
        (25.72, 66.50),
    ],
    &[
        (24.94, 60.17),
        (25.66, 60.98),
        (26.70, 60.87),
        (28.19, 61.05),
        (29.76, 62.60),
    ],
    &[(23.76, 61.50), (21.79, 61.49), (21.62, 63.10)],
];
const FALLBACK_CITIES: &[(&str, f64, f64, u64)] = &[
    ("Helsinki", 24.94, 60.17, 1_000_000),
    ("Tampere", 23.76, 61.50, 300_000),
    ("Turku", 22.27, 60.45, 250_000),
    ("Oulu", 25.47, 65.01, 200_000),
    ("Jyväskylä", 25.75, 62.24, 150_000),
    ("Kuopio", 27.68, 62.89, 120_000),
    ("Lahti", 25.66, 60.98, 120_000),
    ("Joensuu", 29.76, 62.60, 100_000),
    ("Rovaniemi", 25.72, 66.50, 85_000),
    ("Vaasa", 21.62, 63.10, 70_000),
    ("Kouvola", 26.70, 60.87, 70_000),
    ("Lappeenranta", 28.19, 61.05, 70_000),
    ("Seinäjoki", 22.84, 62.79, 65_000),
    ("Kajaani", 27.73, 64.23, 40_000),
    ("Kemi", 24.57, 65.74, 35_000),
];
const BUILDING_TILES: [[usize; 2]; 9] = [
    [0, 0],
    [1, 0],
    [2, 0],
    [0, 1],
    [1, 1],
    [2, 1],
    [0, 2],
    [1, 2],
    [2, 2],
];
const SOURCE_BASE: &str =
    "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/";
type Coordinate = (f64, f64);
type Ring = Vec<Coordinate>;
type Polygon = Vec<Ring>;

#[derive(Clone, Debug)]
pub struct FinlandMapOptions {
    pub countries: PathBuf,
    pub lakes: PathBuf,
    pub output: PathBuf,
    pub preview: Option<PathBuf>,
    pub size: usize,
    pub roads: Option<PathBuf>,
    pub railways: Option<PathBuf>,
    pub cities: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

struct City {
    name: String,
    longitude: f64,
    latitude: f64,
    population: u64,
}

fn read_geojson(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read GeoJSON {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("cannot parse GeoJSON {}: {error}", path.display()))
}

pub fn project(longitude: f64, latitude: f64) -> Point {
    let latitude = latitude.to_radians();
    let delta = (longitude - 26.0).to_radians();
    let center = 64.5_f64.to_radians();
    let scale = (2.0
        / (1.0 + center.sin() * latitude.sin() + center.cos() * latitude.cos() * delta.cos()))
    .sqrt();
    Point {
        x: EARTH_RADIUS_KM * scale * latitude.cos() * delta.sin(),
        y: EARTH_RADIUS_KM
            * scale
            * (center.cos() * latitude.sin() - center.sin() * latitude.cos() * delta.cos()),
    }
}

fn point(value: &Value) -> Result<Coordinate, String> {
    let coordinates = value
        .as_array()
        .filter(|coordinates| coordinates.len() >= 2)
        .ok_or_else(|| "GeoJSON coordinate must be a pair".to_owned())?;
    let longitude = coordinates[0]
        .as_f64()
        .ok_or_else(|| "GeoJSON longitude must be numeric".to_owned())?;
    let latitude = coordinates[1]
        .as_f64()
        .ok_or_else(|| "GeoJSON latitude must be numeric".to_owned())?;
    Ok((longitude, latitude))
}

fn polygons(geometry: &Value) -> Result<Vec<Polygon>, String> {
    let coordinates = &geometry["coordinates"];
    match geometry["type"].as_str() {
        Some("Polygon") => parse_polygon(coordinates).map(|polygon| vec![polygon]),
        Some("MultiPolygon") => coordinates
            .as_array()
            .ok_or_else(|| "GeoJSON multipolygon coordinates must be an array".to_owned())?
            .iter()
            .map(parse_polygon)
            .collect(),
        _ => Err("expected Polygon or MultiPolygon geometry".to_owned()),
    }
}

fn parse_polygon(value: &Value) -> Result<Polygon, String> {
    value
        .as_array()
        .ok_or_else(|| "GeoJSON polygon coordinates must be an array".to_owned())?
        .iter()
        .map(|ring| {
            ring.as_array()
                .ok_or_else(|| "GeoJSON polygon ring must be an array".to_owned())?
                .iter()
                .map(point)
                .collect()
        })
        .collect()
}

fn line_geometries(geometry: &Value) -> Result<Vec<Vec<(f64, f64)>>, String> {
    let parse_line = |line: &Value| -> Result<Vec<(f64, f64)>, String> {
        line.as_array()
            .ok_or_else(|| "GeoJSON line coordinates must be an array".to_owned())?
            .iter()
            .map(point)
            .collect()
    };
    match geometry["type"].as_str() {
        Some("LineString") => Ok(vec![parse_line(&geometry["coordinates"])?]),
        Some("MultiLineString") => geometry["coordinates"]
            .as_array()
            .ok_or_else(|| "GeoJSON multiline coordinates must be an array".to_owned())?
            .iter()
            .map(parse_line)
            .collect(),
        _ => Ok(Vec::new()),
    }
}

fn include_in_window(points: &[Coordinate], finland_window: bool) -> bool {
    let (min_lon, max_lon, min_lat, max_lat) = points.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(min_lon, max_lon, min_lat, max_lat), (longitude, latitude)| {
            (
                min_lon.min(*longitude),
                max_lon.max(*longitude),
                min_lat.min(*latitude),
                max_lat.max(*latitude),
            )
        },
    );
    if finland_window {
        !(max_lon < 5.0 || min_lon > 47.0 || max_lat < 55.0 || min_lat > 74.0)
    } else {
        !(max_lon < 18.0 || min_lon > 34.0 || max_lat < 59.0 || min_lat > 72.0)
    }
}

fn raster_polygon(mask: &mut [bool], size: usize, ring: &[(i32, i32)], value: bool) {
    if size == 0 || ring.len() < 3 {
        return;
    }
    let min_y = ring.iter().map(|point| point.1).min().unwrap().max(0);
    let max_y = ring
        .iter()
        .map(|point| point.1)
        .max()
        .unwrap()
        .min(size as i32 - 1);
    for y in min_y..=max_y {
        let mut intersections = Vec::new();
        for index in 0..ring.len() {
            let (x0, y0) = ring[index];
            let (x1, y1) = ring[(index + 1) % ring.len()];
            if (y0 <= y && y1 > y) || (y1 <= y && y0 > y) {
                intersections
                    .push(x0 + ((y - y0) as i64 * (x1 - x0) as i64 / (y1 - y0) as i64) as i32);
            }
        }
        intersections.sort_unstable();
        for pair in intersections.chunks(2).filter(|pair| pair.len() == 2) {
            for x in pair[0].max(0)..=pair[1].min(size as i32 - 1) {
                mask[y as usize * size + x as usize] = value;
            }
        }
    }
}

fn intersects_square(points: &[(i32, i32)], size: usize) -> bool {
    if points.is_empty() || size == 0 {
        return false;
    }
    let min_x = points.iter().map(|point| point.0).min().unwrap();
    let max_x = points.iter().map(|point| point.0).max().unwrap();
    let min_y = points.iter().map(|point| point.1).min().unwrap();
    let max_y = points.iter().map(|point| point.1).max().unwrap();
    max_x >= 0 && max_y >= 0 && min_x < size as i32 && min_y < size as i32
}

fn raster_line(mask: &mut [bool], size: usize, points: &[(i32, i32)]) {
    for pair in points.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for step in 0..=steps {
            let x = x0 + (x1 - x0) * step / steps;
            let y = y0 + (y1 - y0) * step / steps;
            if x >= 0 && y >= 0 && x < size as i32 && y < size as i32 {
                mask[y as usize * size + x as usize] = true;
            }
        }
    }
}

fn connection_mask(mask: &[bool], size: usize, x: usize, y: usize) -> u8 {
    let position = y * size + x;
    (if y > 0 && mask[position - size] { 1 } else { 0 })
        | (if x + 1 < size && mask[position + 1] {
            2
        } else {
            0
        })
        | (if y + 1 < size && mask[position + size] {
            4
        } else {
            0
        })
        | (if x > 0 && mask[position - 1] { 8 } else { 0 })
}

fn bridge_tile_id(connections: u8, rail: bool, variant: usize) -> u16 {
    let orientation = if connections & (2 | 8) != 0 { 0 } else { 1 };
    if rail {
        return 86 + orientation + if variant.is_multiple_of(2) { 0 } else { 2 };
    }
    let terminal = connections.count_ones() == 1;
    let stone = variant.is_multiple_of(3);
    if terminal {
        (if stone { 66 } else { 62 }) + orientation
    } else if stone {
        64 + orientation
    } else if variant % 3 == 1 {
        68 + orientation
    } else {
        60 + orientation
    }
}

fn map_pixel(point: (f64, f64), left: f64, top: f64, span: f64, size: usize) -> (i32, i32) {
    let projected = project(point.0, point.1);
    (
        ((projected.x - left) / span * (size - 1) as f64).round() as i32,
        ((top - projected.y) / span * (size - 1) as f64).round() as i32,
    )
}

fn raster_geometry(
    target: &mut [bool],
    geometry: &Value,
    size: usize,
    left: f64,
    top: f64,
    span: f64,
) -> Result<(), String> {
    for polygon in polygons(geometry)? {
        let exterior = &polygon[0];
        if !include_in_window(exterior, true) {
            continue;
        }
        let projected: Vec<_> = exterior
            .iter()
            .map(|point| map_pixel(*point, left, top, span, size))
            .collect();
        if !intersects_square(&projected, size) {
            continue;
        }
        raster_polygon(target, size, &projected, true);
        for hole in polygon.iter().skip(1) {
            let projected: Vec<_> = hole
                .iter()
                .map(|point| map_pixel(*point, left, top, span, size))
                .collect();
            raster_polygon(target, size, &projected, false);
        }
    }
    Ok(())
}

fn draw_features(
    target: &mut [bool],
    features: &[Value],
    size: usize,
    left: f64,
    top: f64,
    span: f64,
) -> Result<usize, String> {
    let mut count = 0;
    for feature in features {
        let mut drawn = false;
        for line in line_geometries(&feature["geometry"])? {
            if line.len() < 2 || !include_in_window(&line, false) {
                continue;
            }
            let projected: Vec<_> = line
                .iter()
                .map(|point| map_pixel(*point, left, top, span, size))
                .collect();
            if !intersects_square(&projected, size) {
                continue;
            }
            raster_line(target, size, &projected);
            drawn = true;
        }
        count += usize::from(drawn);
    }
    Ok(count)
}

fn read_features<'a>(document: &'a Value, context: &str) -> Result<&'a [Value], String> {
    document["features"]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{context} GeoJSON must have a features array"))
}

fn generate_map(options: &FinlandMapOptions, game_dir: &Path) -> Result<Value, String> {
    if !(1..=100).contains(&options.size) {
        return Err("map size must be between 1 and 100 tiles for runtime loading".to_owned());
    }
    let countries = read_geojson(&options.countries)?;
    let lakes = read_geojson(&options.lakes)?;
    let roads = options.roads.as_deref().map(read_geojson).transpose()?;
    let railways = options.railways.as_deref().map(read_geojson).transpose()?;
    let populated_places = options.cities.as_deref().map(read_geojson).transpose()?;

    let country_features = read_features(&countries, "countries")?;
    let finland = country_features
        .iter()
        .find(|feature| feature["properties"]["ADMIN"].as_str() == Some("Finland"))
        .ok_or_else(|| "countries GeoJSON does not contain Finland".to_owned())?;
    let finland_geometry = &finland["geometry"];
    let mut projected_points = Vec::new();
    for polygon in polygons(finland_geometry)? {
        for ring in polygon {
            for (longitude, latitude) in ring {
                let point = project(longitude, latitude);
                projected_points.push((point.x, point.y));
            }
        }
    }
    if projected_points.is_empty() {
        return Err("Finland geometry contains no polygon points".to_owned());
    }
    let min_x = projected_points
        .iter()
        .map(|point| point.0)
        .fold(f64::INFINITY, f64::min);
    let max_x = projected_points
        .iter()
        .map(|point| point.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = projected_points
        .iter()
        .map(|point| point.1)
        .fold(f64::INFINITY, f64::min);
    let max_y = projected_points
        .iter()
        .map(|point| point.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = (max_x - min_x).max(max_y - min_y) * 1.06;
    let left = (min_x + max_x - span) / 2.0;
    let top = (min_y + max_y + span) / 2.0;
    let size = options.size;
    let pixel_count = size * size;
    let mut land = vec![false; pixel_count];
    let mut national = vec![false; pixel_count];
    let mut water = vec![false; pixel_count];
    for feature in country_features {
        raster_geometry(&mut land, &feature["geometry"], size, left, top, span)?;
    }
    raster_geometry(&mut national, finland_geometry, size, left, top, span)?;
    for feature in read_features(&lakes, "lakes")? {
        raster_geometry(&mut water, &feature["geometry"], size, left, top, span)?;
    }
    let road_features = roads
        .as_ref()
        .map(|document| read_features(document, "roads"))
        .transpose()?
        .unwrap_or_default();
    let mut road_cells = vec![false; pixel_count];
    let road_count = draw_features(&mut road_cells, road_features, size, left, top, span)?;
    let rail_features: Vec<Value>;
    if let Some(document) = &railways {
        rail_features = read_features(document, "railways")?.to_vec();
    } else {
        rail_features = FALLBACK_RAIL_ROUTES
            .iter()
            .map(|route| {
                json!({
                    "geometry": {
                        "type": "LineString",
                        "coordinates": route.iter().map(|(x, y)| [x, y]).collect::<Vec<_>>()
                    }
                })
            })
            .collect::<Vec<_>>();
    }
    let mut rail_cells = vec![false; pixel_count];
    let rail_count = draw_features(&mut rail_cells, &rail_features, size, left, top, span)?;
    let mut cities = Vec::<City>::new();
    if let Some(document) = &populated_places {
        for feature in read_features(document, "cities")? {
            let properties = &feature["properties"];
            if feature["geometry"]["type"].as_str() != Some("Point") {
                continue;
            }
            let country = properties
                .get("SOV0NAME")
                .or_else(|| properties.get("ADM0NAME"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if country != "Finland" {
                continue;
            }
            let (longitude, latitude) = point(&feature["geometry"]["coordinates"])?;
            cities.push(City {
                name: properties
                    .get("NAME_EN")
                    .or_else(|| properties.get("NAME"))
                    .and_then(Value::as_str)
                    .unwrap_or("City")
                    .to_owned(),
                longitude,
                latitude,
                population: properties["POP_MAX"].as_u64().unwrap_or(0),
            });
        }
    } else {
        cities = FALLBACK_CITIES
            .iter()
            .map(|(name, longitude, latitude, population)| City {
                name: (*name).to_owned(),
                longitude: *longitude,
                latitude: *latitude,
                population: *population,
            })
            .collect();
    }
    cities.sort_by_key(|city| std::cmp::Reverse(city.population));
    cities.truncate(15);

    let dry: Vec<bool> = land
        .iter()
        .zip(&water)
        .map(|(ground, lake)| *ground && !*lake)
        .collect();
    let finland_roads: Vec<bool> = road_cells
        .iter()
        .zip(&dry)
        .zip(&national)
        .map(|((road, ground), country)| *road && *ground && *country)
        .collect();
    let finland_railways: Vec<bool> = rail_cells
        .iter()
        .zip(&dry)
        .zip(&national)
        .map(|((rail, ground), country)| *rail && *ground && *country)
        .collect();
    let water_roads: Vec<bool> = road_cells
        .iter()
        .zip(&dry)
        .map(|(road, ground)| *road && !*ground)
        .collect();
    let water_railways: Vec<bool> = rail_cells
        .iter()
        .zip(&dry)
        .map(|(rail, ground)| *rail && !*ground)
        .collect();

    let mut object_grid = vec![vec![Value::Null; size]; size];
    let mut obstacle_grid = vec![vec![Value::Null; size]; size];
    let mut city_cells = vec![false; pixel_count];
    let mut city_metadata = Vec::new();
    for city in cities {
        let (city_x, city_y) = map_pixel((city.longitude, city.latitude), left, top, span, size);
        if city_x < 0 || city_y < 0 || city_x >= size as i32 || city_y >= size as i32 {
            continue;
        }
        let (city_x, city_y) = (city_x as usize, city_y as usize);
        let mut placement = None;
        for radius in 0..6 {
            let mut candidates = Vec::new();
            for city_top in (city_y as i32 - 1 - radius)..(city_y as i32 + radius) {
                for left_edge in (city_x as i32 - 1 - radius)..(city_x as i32 + radius) {
                    let cells: Vec<_> = (0..3)
                        .flat_map(|dy| (0..3).map(move |dx| (left_edge + dx, city_top + dy)))
                        .collect();
                    if cells.iter().any(|(x, y)| {
                        *x < 0
                            || *y < 0
                            || *x >= size as i32
                            || *y >= size as i32
                            || !national[*y as usize * size + *x as usize]
                            || city_cells[*y as usize * size + *x as usize]
                    }) {
                        continue;
                    }
                    let distance =
                        (left_edge - city_x as i32).pow(2) + (city_top - city_y as i32).pow(2);
                    candidates.push((distance, left_edge, city_top, cells));
                }
            }
            if !candidates.is_empty() {
                candidates.sort_by_key(|candidate| candidate.0);
                placement = candidates.into_iter().next();
                break;
            }
        }
        let Some((_, left_edge, city_top, cells)) = placement else {
            continue;
        };
        for (index, (x, y)) in cells.iter().enumerate() {
            let position = *y as usize * size + *x as usize;
            city_cells[position] = true;
            object_grid[*y as usize][*x as usize] = json!({
                "tile": BUILDING_TILES[index],
                "hp_change": 0,
                "removal": "fixed"
            });
        }
        city_metadata.push(json!({
            "name": city.name,
            "population": city.population,
            "tile": [left_edge + 1, city_top + 1]
        }));
    }

    let mut forest_obstacles = 0;
    let mut rock_obstacles = 0;
    for y in 0..size {
        for x in 0..size {
            let position = y * size + x;
            if !national[position]
                || !dry[position]
                || city_cells[position]
                || finland_roads[position]
                || finland_railways[position]
            {
                continue;
            }
            let x = x as f64;
            let y = y as f64;
            let forest = (x * 0.17 + (y * 0.09).sin()).sin()
                + (y * 0.19 - x * 0.035).cos()
                + ((x + y) * 0.11).sin();
            let noise = (x as usize * 73 + y as usize * 151) % 29;
            if forest > 0.2 && noise.is_multiple_of(17) {
                obstacle_grid[y as usize][x as usize] = json!([noise % 6, 5]);
                forest_obstacles += 1;
            } else if forest < -0.4 && noise % 17 == 1 {
                obstacle_grid[y as usize][x as usize] = json!([2 + noise % 4, 4]);
                rock_obstacles += 1;
            }
        }
    }

    let mut tile_counts = json!({
        "finland_land": 0,
        "neighbor_land": 0,
        "water": 0,
        "road_tiles": 0,
        "rail_tiles": 0,
        "road_bridges": 0,
        "rail_bridges": 0
    });
    let mut tiles = Vec::with_capacity(size);
    for y in 0..size {
        let mut row = Vec::with_capacity(size);
        for x in 0..size {
            let position = y * size + x;
            let tile_id = if dry[position] {
                let mut base = if national[position] {
                    tile_counts["finland_land"] =
                        json!(tile_counts["finland_land"].as_u64().unwrap() + 1);
                    0
                } else {
                    tile_counts["neighbor_land"] =
                        json!(tile_counts["neighbor_land"].as_u64().unwrap() + 1);
                    20
                };
                if base == 0
                    && [
                        y > 0 && !dry[position - size],
                        x + 1 < size && !dry[position + 1],
                        y + 1 < size && !dry[position + size],
                        x > 0 && !dry[position - 1],
                    ]
                    .iter()
                    .any(|near_water| *near_water)
                {
                    base = 20;
                }
                if finland_railways[position] {
                    tile_counts["rail_tiles"] =
                        json!(tile_counts["rail_tiles"].as_u64().unwrap() + 1);
                    70 + connection_mask(&finland_railways, size, x, y) as u16
                } else if finland_roads[position] {
                    if let Some(road_tile) = ROAD_TILE_BY_CONNECTIONS
                        [connection_mask(&finland_roads, size, x, y) as usize]
                    {
                        tile_counts["road_tiles"] =
                            json!(tile_counts["road_tiles"].as_u64().unwrap() + 1);
                        base + road_tile
                    } else {
                        base
                    }
                } else {
                    base
                }
            } else {
                tile_counts["water"] = json!(tile_counts["water"].as_u64().unwrap() + 1);
                if water_railways[position] {
                    tile_counts["rail_bridges"] =
                        json!(tile_counts["rail_bridges"].as_u64().unwrap() + 1);
                    bridge_tile_id(connection_mask(&water_railways, size, x, y), true, x + y)
                } else if water_roads[position] {
                    tile_counts["road_bridges"] =
                        json!(tile_counts["road_bridges"].as_u64().unwrap() + 1);
                    bridge_tile_id(connection_mask(&water_roads, size, x, y), false, x + y)
                } else {
                    let neighbors = (if y > 0 && dry[position - size] { 1 } else { 0 })
                        | (if x + 1 < size && dry[position + 1] {
                            2
                        } else {
                            0
                        })
                        | (if y + 1 < size && dry[position + size] {
                            4
                        } else {
                            0
                        })
                        | (if x > 0 && dry[position - 1] { 8 } else { 0 });
                    40 + neighbors
                }
            };
            row.push(json!([tile_id % 10, tile_id / 10]));
        }
        tiles.push(row);
    }

    let road_source = roads
        .as_ref()
        .map(|_| format!("{SOURCE_BASE}ne_10m_roads.geojson"));
    let rail_source = if railways.is_some() {
        format!("{SOURCE_BASE}ne_10m_railroads.geojson")
    } else {
        "built-in Finnish routes".to_owned()
    };
    let city_source = if populated_places.is_some() {
        format!("{SOURCE_BASE}ne_10m_populated_places.geojson")
    } else {
        "built-in Finnish city locations".to_owned()
    };
    let source_urls = {
        let mut urls = vec![
            format!("{SOURCE_BASE}ne_10m_admin_0_countries.geojson"),
            format!("{SOURCE_BASE}ne_10m_lakes.geojson"),
        ];
        if roads.is_some() {
            urls.push(format!("{SOURCE_BASE}ne_10m_roads.geojson"));
        }
        if railways.is_some() {
            urls.push(format!("{SOURCE_BASE}ne_10m_railroads.geojson"));
        }
        if populated_places.is_some() {
            urls.push(format!("{SOURCE_BASE}ne_10m_populated_places.geojson"));
        }
        urls
    };
    let geography = json!({
        "source": "Natural Earth 1:10m country boundaries, lakes, roads, railways, and populated places (public domain)",
        "source_urls": source_urls,
        "projection": "Spherical Lambert azimuthal equal-area, center 26E 64.5N",
        "earth_radius_km": EARTH_RADIUS_KM,
        "projected_bounds_km": [left, top - span, left + span, top],
        "north_up": true,
        "kilometers_per_tile": span / (size.saturating_sub(1).max(1)) as f64,
        "legend": {
            "finland_land": [0, 0],
            "neighbor_land": [0, 2],
            "water_tile_ids": (40..60).collect::<Vec<_>>(),
            "bridge_tile_ids": (60..70).collect::<Vec<_>>(),
            "rail_connection_bits": {"north": 1, "east": 2, "south": 4, "west": 8},
            "train_track_tile_ids": (70..86).collect::<Vec<_>>(),
            "rail_bridge_tile_ids": (86..90).collect::<Vec<_>>(),
            "shore_land_neighbor_bits": {"north": 1, "east": 2, "south": 4, "west": 8}
        },
        "tile_counts": tile_counts,
        "roads_source": road_source,
        "road_features": road_count,
        "railways_source": rail_source,
        "railway_features": rail_count,
        "cities_source": city_source,
        "cities": city_metadata,
        "forest_obstacles": forest_obstacles,
        "rock_obstacles": rock_obstacles
    });
    let map = json!({
        "format_version": 1,
        "name": "FINLAND",
        "width": size,
        "height": size,
        "tile_size": TILE_SIZE,
        "tileset": "tiles_terrain.png",
        "objects_tileset": "tiles_objects.png",
        "geography": geography,
        "obstacles": obstacle_grid,
        "objects": object_grid,
        "tiles": tiles
    });
    let document = crate::tank16_map::runtime_document(&map, game_dir)?;
    let text = crate::tank16_map::serialize_runtime_map(&document)?;
    if let Some(preview) = &options.preview {
        render_preview(&map, game_dir, preview)?;
    }
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create map folder {}: {error}", parent.display()))?;
    }
    fs::write(&options.output, text).map_err(|error| {
        format!(
            "cannot write generated map {}: {error}",
            options.output.display()
        )
    })?;
    Ok(document)
}

fn render_preview(map: &Value, game_dir: &Path, output: &Path) -> Result<(), String> {
    let size = map["width"]
        .as_u64()
        .ok_or_else(|| "preview map width is invalid".to_owned())? as usize;
    let scale = (800 / size).clamp(1, 8) as u32;
    let terrain_path = game_dir.join("assets/gfx").join(
        map["tileset"]
            .as_str()
            .ok_or_else(|| "preview terrain tileset is invalid".to_owned())?,
    );
    let terrain_dimensions = crate::tank16_map::tileset_dimensions(&terrain_path, false)?;
    let object_path = game_dir.join("assets/gfx/tiles_objects.png");
    let object_dimensions = crate::tank16_map::tileset_dimensions(&object_path, true)?;
    let terrain = image::open(&terrain_path)
        .map_err(|error| format!("cannot load preview terrain atlas: {error}"))?
        .to_rgba8();
    let object_atlas = image::open(&object_path)
        .map_err(|error| format!("cannot load preview object atlas: {error}"))?
        .to_rgba8();
    let object_atlas = image::RgbaImage::from_fn(
        object_dimensions.columns as u32 * TILE_SIZE,
        object_dimensions.rows as u32 * TILE_SIZE,
        |x, y| {
            object_atlas
                .get_pixel_checked(x, y)
                .copied()
                .unwrap_or(image::Rgba([0, 0, 0, 0]))
        },
    );
    let mut scaled_tiles = HashMap::<(bool, usize), image::RgbaImage>::new();
    let width = map["width"].as_u64().unwrap() as usize;
    let height = map["height"].as_u64().unwrap() as usize;
    let mut preview = image::RgbaImage::new(width as u32 * scale, height as u32 * scale);
    for y in 0..height {
        for x in 0..width {
            let tile = &map["tiles"][y][x];
            let tile_id = tile[1].as_u64().unwrap() as usize * terrain_dimensions.columns
                + tile[0].as_u64().unwrap() as usize;
            let image = scaled_tiles.entry((false, tile_id)).or_insert_with(|| {
                let left = (tile_id % terrain_dimensions.columns) as u32 * TILE_SIZE;
                let top = (tile_id / terrain_dimensions.columns) as u32 * TILE_SIZE;
                let tile =
                    image::imageops::crop_imm(&terrain, left, top, TILE_SIZE, TILE_SIZE).to_image();
                image::imageops::resize(&tile, scale, scale, image::imageops::FilterType::Nearest)
            });
            image::imageops::overlay(
                &mut preview,
                image,
                (x * scale as usize) as i64,
                (y * scale as usize) as i64,
            );
            for layer in ["obstacles", "objects"] {
                let cell = &map[layer][y][x];
                if cell.is_null() {
                    continue;
                }
                let cell_tile = if layer == "objects" {
                    &cell["tile"]
                } else {
                    cell
                };
                let object_id = cell_tile[1].as_u64().unwrap() as usize * object_dimensions.columns
                    + cell_tile[0].as_u64().unwrap() as usize;
                let object = scaled_tiles.entry((true, object_id)).or_insert_with(|| {
                    let left = (object_id % object_dimensions.columns) as u32 * TILE_SIZE;
                    let top = (object_id / object_dimensions.columns) as u32 * TILE_SIZE;
                    let tile =
                        image::imageops::crop_imm(&object_atlas, left, top, TILE_SIZE, TILE_SIZE)
                            .to_image();
                    image::imageops::resize(
                        &tile,
                        scale,
                        scale,
                        image::imageops::FilterType::Nearest,
                    )
                });
                image::imageops::overlay(
                    &mut preview,
                    object,
                    (x * scale as usize) as i64,
                    (y * scale as usize) as i64,
                );
            }
        }
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create preview folder: {error}"))?;
    }
    preview
        .save(output)
        .map_err(|error| format!("cannot write preview {}: {error}", output.display()))
}

pub fn generate_finland_map(options: &FinlandMapOptions) -> Result<Value, String> {
    generate_map(options, &crate::tank16_map::default_game_dir())
}

pub fn extend_terrain_atlas(path: &Path, reference_atlas: &Path) -> Result<(), String> {
    let atlas = image::open(path)
        .map_err(|error| format!("cannot load terrain atlas {}: {error}", path.display()))?
        .to_rgba8();
    let dimensions = atlas.dimensions();
    if !matches!(dimensions, (640, 256 | 384 | 448 | 576 | 640)) {
        return Err(
            "terrain atlas must be 640x256, 640x384, 640x448, 640x576 or 640x640 pixels".to_owned(),
        );
    }
    let reference = image::open(reference_atlas)
        .map_err(|error| {
            format!(
                "cannot load reference terrain atlas {}: {error}",
                reference_atlas.display()
            )
        })?
        .to_rgba8();
    if reference.dimensions() != (640, 640) {
        return Err("reference terrain atlas must be 640x640 pixels".to_owned());
    }
    let mut extended = image::RgbaImage::from_pixel(640, 640, image::Rgba([0, 0, 0, 0]));
    image::imageops::replace(&mut extended, &atlas, 0, 0);
    for tile_id in 40..100 {
        let x = (tile_id % 10) * TILE_SIZE;
        let y = (tile_id / 10) * TILE_SIZE;
        let tile = image::imageops::crop_imm(&reference, x, y, TILE_SIZE, TILE_SIZE).to_image();
        image::imageops::replace(&mut extended, &tile, i64::from(x), i64::from(y));
    }
    extended
        .save(path)
        .map_err(|error| format!("cannot write terrain atlas {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_finland_cities_and_generates_runtime_map_from_geojson() {
        let directory = std::env::temp_dir().join(format!("sc16-finland-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let countries = directory.join("countries.json");
        let lakes = directory.join("lakes.json");
        let roads = directory.join("roads.json");
        let railways = directory.join("railways.json");
        let cities = directory.join("cities.json");
        let output = directory.join("FINLAND.json");
        let preview = directory.join("preview.png");
        fs::write(
            &countries,
            r#"{"features":[{"properties":{"ADMIN":"Finland"},"geometry":{"type":"Polygon","coordinates":[[[22,60],[30,60],[30,69],[22,69],[22,60]],[[25,63],[27,63],[27,65],[25,65],[25,63]]]}}]}"#,
        )
        .unwrap();
        fs::write(
            &lakes,
            r#"{"features":[{"geometry":{"type":"Polygon","coordinates":[[[23,66],[24,66],[24,67],[23,67],[23,66]]]}}]}"#,
        )
        .unwrap();
        fs::write(
            &roads,
            r#"{"features":[{"geometry":{"type":"LineString","coordinates":[[22,62],[30,62]]}}]}"#,
        )
        .unwrap();
        fs::write(
            &railways,
            r#"{"features":[{"geometry":{"type":"LineString","coordinates":[[23,61],[29,68]]}}]}"#,
        )
        .unwrap();
        fs::write(
            &cities,
            r#"{"features":[{"properties":{"NAME_EN":"Test City","SOV0NAME":"Finland","POP_MAX":100000},"geometry":{"type":"Point","coordinates":[24,64]}}]}"#,
        )
        .unwrap();
        let result = generate_finland_map(&FinlandMapOptions {
            countries,
            lakes,
            output,
            size: 64,
            preview: Some(preview.clone()),
            roads: Some(roads),
            railways: Some(railways),
            cities: Some(cities),
        })
        .unwrap();
        assert_eq!(result["runtime_encoding"], "grid-hex-v2");
        assert_eq!(result["width"], 64);
        assert_eq!(result["geography"]["road_features"], 1);
        assert_eq!(result["geography"]["railway_features"], 1);
        assert_eq!(result["geography"]["cities"][0]["name"], "Test City");
        assert_eq!(image::image_dimensions(preview).unwrap(), (512, 512));
        assert_eq!(
            result["geography"]["tile_counts"]["finland_land"]
                .as_u64()
                .unwrap()
                + result["geography"]["tile_counts"]["neighbor_land"]
                    .as_u64()
                    .unwrap()
                + result["geography"]["tile_counts"]["water"]
                    .as_u64()
                    .unwrap(),
            64 * 64
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn atlas_extension_is_idempotent_and_rejects_invalid_dimensions() {
        let game_dir = crate::tank16_map::default_game_dir();
        let reference = game_dir.join("assets/gfx/tiles_terrain.png");
        let source = image::open(&reference).unwrap().to_rgba8();
        let temp_dir =
            std::env::temp_dir().join(format!("sc16-atlas-extension-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let path = temp_dir.join("terrain.png");
        let cropped = image::imageops::crop_imm(&source, 0, 0, 640, 256).to_image();
        cropped.save(&path).unwrap();
        extend_terrain_atlas(&path, &reference).unwrap();
        let first = fs::read(&path).unwrap();
        extend_terrain_atlas(&path, &reference).unwrap();
        assert_eq!(fs::read(&path).unwrap(), first);
        assert_eq!(image::image_dimensions(&path).unwrap(), (640, 640));
        let invalid = temp_dir.join("invalid.png");
        image::RgbaImage::new(128, 128).save(&invalid).unwrap();
        assert!(extend_terrain_atlas(&invalid, &reference).is_err());
        fs::remove_dir_all(temp_dir).unwrap();
    }
}
