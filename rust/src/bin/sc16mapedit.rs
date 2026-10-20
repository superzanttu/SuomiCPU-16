use macroquad::prelude::{
    BLACK, Color, Conf, DrawTextureParams, FilterMode, KeyCode, MouseButton, WHITE,
    clear_background, draw_rectangle, draw_rectangle_lines, draw_text, draw_texture_ex,
    is_key_down, is_key_pressed, is_mouse_button_pressed, load_texture, mouse_position, next_frame,
    screen_height, screen_width, vec2,
};
use serde_json::Value;
use std::path::PathBuf;
use suomicpu16::tank16_map::{
    LayerTileEdit, TilesetDimensions, default_game_dir, load_map, new_map, save_map,
    set_layer_tile, set_map_tile, tileset_dimensions,
};

const SIDEBAR_WIDTH: f32 = 250.0;
const HEADER_HEIGHT: f32 = 48.0;
const FOOTER_HEIGHT: f32 = 36.0;

fn window_conf() -> Conf {
    Conf {
        window_title: "Tank-16 Map Editor (Rust)".to_owned(),
        window_width: 1120,
        window_height: 760,
        window_resizable: true,
        ..Conf::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("sc16mapedit: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let game_dir = default_game_dir();
    let terrain_path = game_dir.join("gfx/tiles_terrain.png");
    let objects_path = game_dir.join("gfx/tiles_objects.png");
    let terrain = load_texture(
        terrain_path
            .to_str()
            .ok_or("terrain atlas path is not valid text")?,
    )
    .await
    .map_err(|error| format!("cannot load terrain atlas: {error}"))?;
    let objects = load_texture(
        objects_path
            .to_str()
            .ok_or("object atlas path is not valid text")?,
    )
    .await
    .map_err(|error| format!("cannot load object atlas: {error}"))?;
    terrain.set_filter(FilterMode::Nearest);
    objects.set_filter(FilterMode::Nearest);
    let terrain_dimensions = tileset_dimensions(&terrain_path, false)?;
    let object_dimensions = tileset_dimensions(&objects_path, true)?;

    let mut args = std::env::args_os().skip(1);
    let mut input_path = None;
    let mut width = 10usize;
    let mut height = 8usize;
    let mut name = "UNTITLED".to_owned();
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!(
                "Usage: sc16mapedit [--map <map.json>] [--name NAME] [--width N] [--height N]"
            );
            println!("Paint terrain and object layers; Ctrl+S saves into Tank-16 disks/maps.");
            return Ok(());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{} needs a value", arg.to_string_lossy()))?;
        if arg == "--map" {
            input_path = Some(PathBuf::from(value));
        } else if arg == "--name" {
            name = value.into_string().map_err(|_| "map name must be text")?;
        } else if arg == "--width" {
            width = value.to_string_lossy().parse()?;
        } else if arg == "--height" {
            height = value.to_string_lossy().parse()?;
        } else {
            return Err(format!("unknown option: {}", arg.to_string_lossy()).into());
        }
    }
    let mut map = if let Some(path) = input_path {
        load_map(&game_dir, &path)?
    } else {
        new_map(&name, width, height)?
    };
    let maps_dir = game_dir.join("disks/maps");
    let mut layer = EditLayer::Terrain;
    let mut selected_terrain = [0usize, 0usize];
    let mut selected_object = [0usize, 0usize];
    let mut palette_offset = 0usize;
    let mut status =
        "G/O/B: select layer | left click: paint | right click: erase | Ctrl+S: save | Esc: quit"
            .to_owned();
    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::G) {
            layer = EditLayer::Terrain;
            palette_offset = 0;
        } else if is_key_pressed(KeyCode::O) {
            layer = EditLayer::Objects;
            palette_offset = 0;
        } else if is_key_pressed(KeyCode::B) {
            layer = EditLayer::Obstacles;
            palette_offset = 0;
        }
        if is_key_pressed(KeyCode::Up) {
            palette_offset = palette_offset.saturating_sub(6);
        }
        if is_key_pressed(KeyCode::Down) {
            palette_offset = (palette_offset + 6).min(
                palette_dimensions(layer, terrain_dimensions, object_dimensions)
                    .columns
                    .saturating_mul(
                        palette_dimensions(layer, terrain_dimensions, object_dimensions).rows,
                    )
                    .saturating_sub(1),
            );
        }
        if is_key_down(KeyCode::LeftControl) && is_key_pressed(KeyCode::S) {
            match save_map(&game_dir, &map, &maps_dir) {
                Ok(path) => status = format!("Saved {}", path.display()),
                Err(error) => status = error,
            }
        }
        let map_width = map["width"].as_u64().unwrap() as usize;
        let map_height = map["height"].as_u64().unwrap() as usize;
        let canvas_x = SIDEBAR_WIDTH;
        let canvas_y = HEADER_HEIGHT;
        let canvas_width = screen_width() - canvas_x - 16.0;
        let canvas_height = screen_height() - canvas_y - FOOTER_HEIGHT - 12.0;
        let cell_size = (canvas_width / map_width as f32)
            .min(canvas_height / map_height as f32)
            .max(1.0);
        let (mouse_x, mouse_y) = mouse_position();
        if mouse_x >= canvas_x
            && mouse_y >= canvas_y
            && mouse_x < canvas_x + map_width as f32 * cell_size
            && mouse_y < canvas_y + map_height as f32 * cell_size
        {
            let x = ((mouse_x - canvas_x) / cell_size) as usize;
            let y = ((mouse_y - canvas_y) / cell_size) as usize;
            if is_mouse_button_pressed(MouseButton::Left) {
                let result = match layer {
                    EditLayer::Terrain => set_map_tile(
                        &mut map,
                        x,
                        y,
                        selected_terrain[0],
                        selected_terrain[1],
                        terrain_dimensions,
                    ),
                    EditLayer::Obstacles => set_layer_tile(
                        &mut map,
                        LayerTileEdit {
                            layer: "obstacles",
                            x,
                            y,
                            tile: Some(selected_object),
                            hp_change: -25,
                            removal: "explosion",
                        },
                        object_dimensions,
                    ),
                    EditLayer::Objects => set_layer_tile(
                        &mut map,
                        LayerTileEdit {
                            layer: "objects",
                            x,
                            y,
                            tile: Some(selected_object),
                            hp_change: -25,
                            removal: "explosion",
                        },
                        object_dimensions,
                    ),
                };
                status = result.map_or_else(|error| error, |_| format!("Painted {x},{y}"));
            } else if is_mouse_button_pressed(MouseButton::Right) && layer != EditLayer::Terrain {
                let key = if layer == EditLayer::Objects {
                    "objects"
                } else {
                    "obstacles"
                };
                map[key][y][x] = Value::Null;
                status = format!("Erased {x},{y}");
            }
        }
        let palette = palette_dimensions(layer, terrain_dimensions, object_dimensions);
        if mouse_x < SIDEBAR_WIDTH && mouse_y >= HEADER_HEIGHT {
            let cell = 42.0;
            let column = ((mouse_x - 8.0) / cell) as usize;
            let row = ((mouse_y - HEADER_HEIGHT - 8.0) / cell) as usize;
            let index = palette_offset + row * 5 + column;
            if column < 5
                && index < palette.columns * palette.rows
                && is_mouse_button_pressed(MouseButton::Left)
            {
                if layer == EditLayer::Terrain {
                    selected_terrain = [index % palette.columns, index / palette.columns];
                } else {
                    selected_object = [index % palette.columns, index / palette.columns];
                }
            }
        }
        draw_editor(
            &map,
            layer,
            &terrain,
            &objects,
            terrain_dimensions,
            object_dimensions,
            selected_terrain,
            selected_object,
            palette_offset,
            cell_size,
            canvas_x,
            canvas_y,
            &status,
        );
        next_frame().await;
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditLayer {
    Terrain,
    Obstacles,
    Objects,
}

fn palette_dimensions(
    layer: EditLayer,
    terrain: TilesetDimensions,
    objects: TilesetDimensions,
) -> TilesetDimensions {
    if layer == EditLayer::Terrain {
        terrain
    } else {
        objects
    }
}

fn draw_atlas_cell(
    texture: &macroquad::texture::Texture2D,
    tile: [usize; 2],
    x: f32,
    y: f32,
    size: f32,
) {
    draw_texture_ex(
        texture,
        x,
        y,
        WHITE,
        DrawTextureParams {
            source: Some(macroquad::prelude::Rect::new(
                (tile[0] * 64) as f32,
                (tile[1] * 64) as f32,
                64.0,
                64.0,
            )),
            dest_size: Some(vec2(size, size)),
            ..DrawTextureParams::default()
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_editor(
    map: &Value,
    layer: EditLayer,
    terrain: &macroquad::texture::Texture2D,
    objects: &macroquad::texture::Texture2D,
    terrain_dimensions: TilesetDimensions,
    object_dimensions: TilesetDimensions,
    selected_terrain: [usize; 2],
    selected_object: [usize; 2],
    palette_offset: usize,
    cell_size: f32,
    canvas_x: f32,
    canvas_y: f32,
    status: &str,
) {
    clear_background(Color::from_rgba(26, 29, 33, 255));
    let width = map["width"].as_u64().unwrap() as usize;
    let height = map["height"].as_u64().unwrap() as usize;
    let rows = map["tiles"].as_array().unwrap();
    for y in 0..height {
        let cells = rows[y].as_array().unwrap();
        for x in 0..width {
            let pair = cells[x].as_array().unwrap();
            let tile = [
                pair[0].as_u64().unwrap() as usize,
                pair[1].as_u64().unwrap() as usize,
            ];
            let screen_x = canvas_x + x as f32 * cell_size;
            let screen_y = canvas_y + y as f32 * cell_size;
            draw_atlas_cell(terrain, tile, screen_x, screen_y, cell_size);
            if layer == EditLayer::Obstacles {
                let obstacle = &map["obstacles"][y][x];
                if let Some(pair) = obstacle.as_array() {
                    draw_atlas_cell(
                        objects,
                        [
                            pair[0].as_u64().unwrap() as usize,
                            pair[1].as_u64().unwrap() as usize,
                        ],
                        screen_x,
                        screen_y,
                        cell_size,
                    );
                }
            } else if layer == EditLayer::Objects {
                let object = &map["objects"][y][x];
                if let Some(pair) = object["tile"].as_array() {
                    draw_atlas_cell(
                        objects,
                        [
                            pair[0].as_u64().unwrap() as usize,
                            pair[1].as_u64().unwrap() as usize,
                        ],
                        screen_x,
                        screen_y,
                        cell_size,
                    );
                }
            }
            draw_rectangle_lines(screen_x, screen_y, cell_size, cell_size, 0.5, BLACK);
        }
    }
    let active = match layer {
        EditLayer::Terrain => "TERRAIN [G]",
        EditLayer::Obstacles => "OBSTACLES [B]",
        EditLayer::Objects => "OBJECTS [O]",
    };
    draw_text(
        format!("{} {}x{} | {active}", map["name"], width, height),
        10.0,
        30.0,
        20.0,
        WHITE,
    );
    draw_rectangle(
        SIDEBAR_WIDTH,
        HEADER_HEIGHT - 4.0,
        2.0,
        screen_height(),
        Color::from_rgba(90, 96, 104, 255),
    );
    let atlas = if layer == EditLayer::Terrain {
        terrain
    } else {
        objects
    };
    let dimensions = palette_dimensions(layer, terrain_dimensions, object_dimensions);
    let cell = 42.0;
    for local in 0..40 {
        let index = palette_offset + local;
        if index >= dimensions.columns * dimensions.rows {
            break;
        }
        let column = local % 5;
        let row = local / 5;
        let x = 8.0 + column as f32 * cell;
        let y = HEADER_HEIGHT + 8.0 + row as f32 * cell;
        draw_atlas_cell(
            atlas,
            [index % dimensions.columns, index / dimensions.columns],
            x,
            y,
            38.0,
        );
        let selected = if layer == EditLayer::Terrain {
            selected_terrain == [index % dimensions.columns, index / dimensions.columns]
        } else {
            selected_object == [index % dimensions.columns, index / dimensions.columns]
        };
        if selected {
            draw_rectangle_lines(x, y, 38.0, 38.0, 2.0, Color::from_rgba(255, 220, 40, 255));
        }
    }
    draw_text(
        "Palette: click | Up/Down scroll",
        8.0,
        screen_height() - 62.0,
        14.0,
        WHITE,
    );
    draw_text(status, 8.0, screen_height() - 16.0, 14.0, WHITE);
}
