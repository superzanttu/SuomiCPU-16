//! Provides Tank-16's interactive sprite-atlas viewer.
//! Loads object/terrain atlases and supports atlas selection, zoom, and sprite inspection.

use macroquad::prelude::{
    BLACK, Color, Conf, DrawTextureParams, FilterMode, KeyCode, MouseButton, WHITE,
    clear_background, draw_rectangle_lines, draw_text, draw_texture_ex, is_key_pressed,
    is_mouse_button_pressed, load_texture, mouse_position, next_frame, screen_height, screen_width,
    vec2,
};
use std::path::PathBuf;
use suomicpu16::sprite_viewer::{AtlasSize, SpriteBounds};

const HEADER_HEIGHT: f32 = 54.0;
const FOOTER_HEIGHT: f32 = 54.0;
const ATLAS_RELATIVE_PATHS: [&str; 2] = [
    "programs/tank-16/assets/gfx/tiles_objects.png",
    "programs/tank-16/assets/gfx/tiles_terrain.png",
];

fn window_conf() -> Conf {
    Conf {
        window_title: "Tank-16 Sprite Viewer".to_owned(),
        window_width: 1400,
        window_height: 920,
        window_resizable: true,
        ..Conf::default()
    }
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[macroquad::main(window_conf)]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("sc16sprites: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut textures = Vec::with_capacity(ATLAS_RELATIVE_PATHS.len());
    for relative_path in ATLAS_RELATIVE_PATHS {
        let path = repository_root().join(relative_path);
        let path = path
            .to_str()
            .ok_or_else(|| format!("atlas path is not valid text: {}", path.display()))?;
        let texture = load_texture(path)
            .await
            .map_err(|error| format!("cannot load sprite atlas {path}: {error}"))?;
        texture.set_filter(FilterMode::Nearest);
        textures.push(texture);
    }

    let mut atlas_index = 0usize;
    let mut selected_sprite = None;
    let mut scale = 1.0f32;
    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::Tab) {
            atlas_index = 1 - atlas_index;
            selected_sprite = None;
        }
        if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
            scale = (scale + 1.0).min(4.0);
        }
        if is_key_pressed(KeyCode::Minus) {
            scale = (scale - 1.0).max(1.0);
        }

        let texture = &textures[atlas_index];
        let atlas_dimensions = AtlasSize {
            width: texture.width() as u32,
            height: texture.height() as u32,
        };
        let dimensions = atlas_dimensions.padded();
        let image_width = dimensions.width as f32 * scale;
        let image_height = dimensions.height as f32 * scale;
        let image_x = ((screen_width() - image_width) / 2.0).max(0.0);
        let image_y = HEADER_HEIGHT;
        let (mouse_x, mouse_y) = mouse_position();
        let inside_image = mouse_x >= image_x
            && mouse_x < image_x + image_width
            && mouse_y >= image_y
            && mouse_y < image_y + image_height;
        let hovered_sprite = inside_image
            .then(|| {
                dimensions.sprite_id_at(
                    ((mouse_x - image_x) / scale) as u32,
                    ((mouse_y - image_y) / scale) as u32,
                )
            })
            .flatten();

        if is_mouse_button_pressed(MouseButton::Left) && inside_image {
            selected_sprite = hovered_sprite;
            if let Some(sprite_id) = selected_sprite
                && let Some(bounds) = dimensions.sprite_bounds(sprite_id)
            {
                println!(
                    "atlas={} id={} x={} y={} width={} height={}",
                    PathBuf::from(ATLAS_RELATIVE_PATHS[atlas_index])
                        .file_name()
                        .unwrap()
                        .to_string_lossy(),
                    sprite_id,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height
                );
            }
        }

        clear_background(Color::from_rgba(24, 27, 31, 255));
        let atlas_name = PathBuf::from(ATLAS_RELATIVE_PATHS[atlas_index])
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        draw_text(
            format!(
                "{atlas_name}  {}x{}  |  Cell: 64x64  Tab: switch atlas  +/-: zoom  |  Click: select sprite",
                atlas_dimensions.width, atlas_dimensions.height
            ),
            12.0,
            30.0,
            18.0,
            WHITE,
        );
        draw_texture_ex(
            texture,
            image_x,
            image_y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(
                    atlas_dimensions.width as f32 * scale,
                    atlas_dimensions.height as f32 * scale,
                )),
                ..DrawTextureParams::default()
            },
        );

        for row in 0..dimensions.rows() {
            for column in 0..dimensions.columns() {
                let sprite_id = row * dimensions.columns() + column;
                let bounds = dimensions.sprite_bounds(sprite_id).unwrap();
                let x = image_x + bounds.x as f32 * scale;
                let y = image_y + bounds.y as f32 * scale;
                let width = bounds.width as f32 * scale;
                let height = bounds.height as f32 * scale;
                draw_rectangle_lines(x, y, width, height, 1.0, BLACK);
                draw_text(sprite_id.to_string(), x + 3.0, y + 15.0, 16.0, WHITE);
            }
        }

        for (sprite_id, color) in [
            (hovered_sprite, Color::from_rgba(255, 225, 40, 255)),
            (selected_sprite, Color::from_rgba(80, 255, 150, 255)),
        ] {
            if let Some(bounds) = sprite_id.and_then(|id| dimensions.sprite_bounds(id)) {
                draw_sprite_outline(image_x, image_y, scale, bounds, color);
            }
        }

        let active_sprite = selected_sprite.or(hovered_sprite);
        let detail = active_sprite
            .and_then(|id| dimensions.sprite_bounds(id).map(|bounds| (id, bounds)))
            .map(|(id, bounds)| {
                format!(
                    "Sprite ID {id}  x={} y={}  {}x{}",
                    bounds.x, bounds.y, bounds.width, bounds.height
                )
            })
            .unwrap_or_else(|| "Move over a sprite cell".to_owned());
        draw_text(
            &detail,
            12.0,
            screen_height() - FOOTER_HEIGHT + 30.0,
            18.0,
            WHITE,
        );
        next_frame().await;
    }
    Ok(())
}

fn draw_sprite_outline(image_x: f32, image_y: f32, scale: f32, bounds: SpriteBounds, color: Color) {
    draw_rectangle_lines(
        image_x + bounds.x as f32 * scale,
        image_y + bounds.y as f32 * scale,
        bounds.width as f32 * scale,
        bounds.height as f32 * scale,
        2.0,
        color,
    );
}
