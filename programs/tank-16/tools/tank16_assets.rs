//! Generates Tank-16's procedural object sprite atlas.
//! Contains raster drawing helpers, object artwork, output-path handling, and PNG generation.

use image::{Rgba, RgbaImage};
use imageproc::{
    drawing::{
        draw_filled_circle_mut, draw_filled_ellipse_mut, draw_filled_rect_mut,
        draw_hollow_circle_mut, draw_line_segment_mut, draw_polygon_mut,
    },
    point::Point,
    rect::Rect,
};
use std::path::{Path, PathBuf};

pub const TILE_SIZE: u32 = 64;
pub const OBJECT_ATLAS_COLUMNS: u32 = 6;
pub const OBJECT_ATLAS_ROWS: u32 = 8;

fn color(rgb: [u8; 3]) -> Rgba<u8> {
    Rgba([rgb[0], rgb[1], rgb[2], 255])
}

fn circle(image: &mut RgbaImage, cx: i32, cy: i32, radius: i32, value: Rgba<u8>) {
    draw_filled_circle_mut(image, (cx, cy), radius, value);
}

fn circle_outline(
    image: &mut RgbaImage,
    cx: i32,
    cy: i32,
    radius: i32,
    width: i32,
    value: Rgba<u8>,
) {
    for ring in (radius - width + 1).max(0)..=radius {
        draw_hollow_circle_mut(image, (cx, cy), ring, value);
    }
}

fn line(image: &mut RgbaImage, start: (i32, i32), end: (i32, i32), width: i32, value: Rgba<u8>) {
    if start.1 == end.1 {
        let top = start.1 - (width - 1) / 2;
        rect(
            image,
            start.0.min(end.0),
            top,
            (start.0 - end.0).abs() + 1,
            width,
            value,
        );
        return;
    }
    if start.0 == end.0 {
        let left = start.0 - (width - 1) / 2;
        rect(
            image,
            left,
            start.1.min(end.1),
            width,
            (start.1 - end.1).abs() + 1,
            value,
        );
        return;
    }
    let dx = (end.0 - start.0) as f64;
    let dy = (end.1 - start.1) as f64;
    if width <= 1 {
        draw_line_segment_mut(
            image,
            (start.0 as f32, start.1 as f32),
            (end.0 as f32, end.1 as f32),
            value,
        );
        return;
    }
    let steps = dx.abs().max(dy.abs()).ceil() as usize;
    for step in 0..=steps {
        let fraction = if steps == 0 {
            0.0
        } else {
            step as f64 / steps as f64
        };
        circle(
            image,
            (f64::from(start.0) + dx * fraction).round() as i32,
            (f64::from(start.1) + dy * fraction).round() as i32,
            width / 2,
            value,
        );
    }
}

fn arc(
    image: &mut RgbaImage,
    center: (i32, i32),
    radii: (i32, i32),
    start: f64,
    end: f64,
    width: i32,
    value: Rgba<u8>,
) {
    let steps = ((end - start).abs() * radii.0.max(radii.1) as f64).ceil() as usize;
    let point = |step: usize| {
        let angle = start + (end - start) * step as f64 / steps as f64;
        (
            center.0 + (radii.0 as f64 * angle.cos()).round() as i32,
            center.1 + (radii.1 as f64 * angle.sin()).round() as i32,
        )
    };
    for step in 0..steps {
        line(image, point(step), point(step + 1), width, value);
    }
}

fn polygon(image: &mut RgbaImage, points: &[(i32, i32)], value: Rgba<u8>) {
    let points: Vec<_> = points.iter().map(|(x, y)| Point::new(*x, *y)).collect();
    draw_polygon_mut(image, &points, value);
}

fn outline_polygon(image: &mut RgbaImage, points: &[(i32, i32)], fill: Rgba<u8>) {
    polygon(image, points, fill);
}

fn rect(image: &mut RgbaImage, x: i32, y: i32, width: i32, height: i32, fill: Rgba<u8>) {
    draw_filled_rect_mut(
        image,
        Rect::at(x, y).of_size(width as u32, height as u32),
        fill,
    );
}

fn rounded_rect(
    image: &mut RgbaImage,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    radius: i32,
    fill: Rgba<u8>,
) {
    let radius = radius.min(width / 2).min(height / 2).max(0);
    rect(image, x + radius, y, width - 2 * radius, height, fill);
    rect(image, x, y + radius, width, height - 2 * radius, fill);
    for center in [
        (x + radius, y + radius),
        (x + width - radius - 1, y + radius),
        (x + radius, y + height - radius - 1),
        (x + width - radius - 1, y + height - radius - 1),
    ] {
        circle(image, center.0, center.1, radius, fill);
    }
}

fn outlined_rect(image: &mut RgbaImage, x: i32, y: i32, width: i32, height: i32, fill: Rgba<u8>) {
    rounded_rect(image, x, y, width, height, 4, color([28, 34, 35]));
    rounded_rect(image, x + 2, y + 2, width - 4, height - 4, 3, fill);
}

fn ellipse_rect(image: &mut RgbaImage, x: i32, y: i32, width: i32, height: i32, fill: Rgba<u8>) {
    draw_filled_ellipse_mut(
        image,
        (x + width / 2, y + height / 2),
        width / 2,
        height / 2,
        fill,
    );
}

fn wall(image: &mut RgbaImage, style: usize) {
    let rects: &[(i32, i32, i32, i32)] = match style {
        0 => &[(0, 24, 64, 16)],
        1 => &[(24, 0, 16, 64)],
        2 => &[(0, 0, 64, 18), (0, 0, 18, 64)],
        3 => &[(0, 0, 64, 18), (46, 0, 18, 64)],
        4 => &[(0, 46, 64, 18), (0, 0, 18, 64)],
        _ => &[(0, 46, 64, 18), (46, 0, 18, 64)],
    };
    for &(x, y, width, height) in rects {
        rect(image, x, y, width, height, color([28, 34, 35]));
        rect(
            image,
            x + 2,
            y + 2,
            width - 4,
            height - 4,
            color([104, 111, 108]),
        );
        if width > height {
            line(
                image,
                (x + 2, y + 4),
                (x + width - 2, y + 4),
                2,
                color([160, 164, 151]),
            );
            for divider in (x + 13..x + width).step_by(15) {
                line(
                    image,
                    (divider, y + 5),
                    (divider, y + height - 2),
                    2,
                    color([65, 73, 72]),
                );
            }
            line(
                image,
                (x + 2, y + height - 4),
                (x + width - 2, y + height - 4),
                2,
                color([65, 73, 72]),
            );
        } else {
            line(
                image,
                (x + 4, y + 2),
                (x + 4, y + height - 2),
                2,
                color([160, 164, 151]),
            );
            for divider in (y + 13..y + height).step_by(15) {
                line(
                    image,
                    (x + 5, divider),
                    (x + width - 2, divider),
                    2,
                    color([65, 73, 72]),
                );
            }
            line(
                image,
                (x + width - 4, y + 2),
                (x + width - 4, y + height - 2),
                2,
                color([65, 73, 72]),
            );
        }
    }
}

fn building(image: &mut RgbaImage, style: usize) {
    rect(image, 0, 0, 64, 64, color([28, 34, 35]));
    rect(image, 2, 2, 60, 60, color([103, 117, 117]));
    rect(image, 6, 6, 52, 52, color([133, 150, 146]));
    line(image, (9, 12), (54, 12), 2, color([157, 172, 163]));
    line(image, (9, 53), (54, 53), 2, color([91, 108, 108]));
    line(image, (12, 20), (52, 20), 1, color([111, 130, 128]));
    line(image, (12, 43), (52, 43), 1, color([111, 130, 128]));
    let edges: &[&str] = match style {
        0 => &["top", "left"],
        1 => &["top"],
        2 => &["top", "right"],
        3 => &["left"],
        4 => &[],
        5 => &["right"],
        6 => &["bottom", "left"],
        7 => &["bottom"],
        _ => &["bottom", "right"],
    };
    for edge in edges {
        let (x, y, width, height) = match *edge {
            "top" => (0, 0, 64, 9),
            "bottom" => (0, 55, 64, 9),
            "left" => (0, 0, 9, 64),
            _ => (55, 0, 9, 64),
        };
        rect(image, x, y, width, height, color([28, 34, 35]));
        rect(
            image,
            x + 1,
            y + 1,
            width - 3,
            height - 3,
            color([104, 111, 108]),
        );
        if width > height {
            line(
                image,
                (x + 3, y + 2),
                (x + width - 3, y + 2),
                2,
                color([160, 164, 151]),
            );
            for divider in (x + 15..x + width).step_by(16) {
                line(
                    image,
                    (divider, y + 2),
                    (divider, y + height - 2),
                    1,
                    color([65, 73, 72]),
                );
            }
        } else {
            line(
                image,
                (x + 2, y + 3),
                (x + 2, y + height - 3),
                2,
                color([160, 164, 151]),
            );
            for divider in (y + 15..y + height).step_by(16) {
                line(
                    image,
                    (x + 2, divider),
                    (x + width - 2, divider),
                    1,
                    color([65, 73, 72]),
                );
            }
        }
    }
    if style == 4 || style == 7 {
        if style == 4 {
            outlined_rect(image, 24, 24, 16, 16, color([120, 139, 143]));
            line(image, (28, 28), (36, 28), 2, color([190, 201, 190]));
            line(image, (28, 34), (36, 34), 2, color([190, 201, 190]));
        } else {
            outlined_rect(image, 23, 45, 18, 18, color([113, 72, 42]));
            circle(image, 36, 53, 1, color([238, 178, 44]));
        }
    }
}

fn powerup(image: &mut RgbaImage, style: usize) {
    let fills = [
        [197, 57, 48],
        [238, 178, 44],
        [48, 124, 190],
        [48, 124, 190],
        [224, 111, 35],
        [82, 177, 101],
    ];
    circle(image, 32, 33, 23, color([28, 34, 35]));
    circle(image, 32, 31, 19, color(fills[style]));
    circle(image, 25, 23, 5, color([245, 238, 206]));
    circle_outline(image, 32, 31, 15, 2, color([232, 219, 172]));
    match style {
        0 => {
            rect(image, 27, 19, 10, 25, color([248, 248, 225]));
            rect(image, 19, 27, 26, 9, color([248, 248, 225]));
        }
        1 => {
            outlined_rect(image, 17, 18, 30, 29, color([238, 178, 44]));
            line(image, (22, 24), (41, 24), 2, color([232, 219, 172]));
            for x in [24, 31, 38] {
                rect(image, x, 29, 4, 13, color([197, 57, 48]));
            }
        }
        2 => outline_polygon(
            image,
            &[(36, 14), (23, 31), (31, 31), (27, 47), (43, 26), (34, 26)],
            color([232, 219, 172]),
        ),
        3 => {
            arc(
                image,
                (32, 32),
                (14, 16),
                0.2,
                3.0,
                5,
                color([232, 219, 172]),
            );
            outline_polygon(
                image,
                &[(32, 13), (41, 18), (39, 34), (32, 42), (25, 34), (23, 18)],
                color([107, 194, 225]),
            );
        }
        4 => {
            for x in [23, 32, 41] {
                line(image, (x, 42), (x, 22), 3, color([232, 219, 172]));
                polygon(
                    image,
                    &[(x - 4, 25), (x, 17), (x + 4, 25)],
                    color([238, 178, 44]),
                );
            }
        }
        _ => {
            line(image, (22, 39), (41, 21), 5, color([232, 219, 172]));
            line(image, (22, 21), (41, 39), 5, color([232, 219, 172]));
            circle(image, 32, 30, 5, color([197, 57, 48]));
        }
    }
}

fn mine(image: &mut RgbaImage, style: usize) {
    let radii = [7, 10, 14, 18, 23, 20];
    let radius = radii[style];
    if style == 5 {
        for angle in 0..8 {
            let angle = f64::from(angle) * std::f64::consts::PI / 4.0;
            let start = (
                32 + (17.0 * angle.cos()).round() as i32,
                34 + (17.0 * angle.sin()).round() as i32,
            );
            let end = (
                32 + (27.0 * angle.cos()).round() as i32,
                34 + (27.0 * angle.sin()).round() as i32,
            );
            line(image, start, end, 5, color([28, 34, 35]));
            line(image, start, end, 2, color([190, 201, 190]));
        }
    }
    circle(image, 32, 34, radius + 3, color([28, 34, 35]));
    circle(image, 32, 34, radius, color([65, 73, 72]));
    circle(image, 32, 34, (radius - 4).max(3), color([120, 139, 143]));
    circle(
        image,
        32 - radius / 3,
        34 - radius / 3,
        (radius / 3).max(2),
        color([190, 201, 190]),
    );
    circle(image, 32, 34, (radius / 5).max(2), color([197, 57, 48]));
    for x in (32 - radius + 4..32 + radius - 2).step_by(7) {
        line(
            image,
            (x, 34 - radius + 3),
            (x, 34 - radius + 6),
            2,
            color([28, 34, 35]),
        );
    }
}

fn rock(image: &mut RgbaImage, style: usize) {
    let shapes: [&[(i32, i32)]; 6] = [
        &[(24, 39), (27, 30), (35, 28), (42, 35), (39, 43), (28, 44)],
        &[
            (14, 40),
            (18, 27),
            (31, 23),
            (43, 30),
            (48, 43),
            (38, 49),
            (22, 47),
        ],
        &[
            (10, 39),
            (16, 22),
            (29, 13),
            (45, 18),
            (54, 34),
            (48, 49),
            (26, 54),
            (14, 49),
        ],
        &[
            (7, 42),
            (12, 24),
            (25, 11),
            (42, 13),
            (56, 29),
            (54, 45),
            (37, 54),
            (17, 52),
        ],
        &[
            (8, 45),
            (14, 25),
            (24, 19),
            (32, 8),
            (44, 17),
            (55, 27),
            (57, 43),
            (43, 53),
            (21, 53),
        ],
        &[
            (6, 44),
            (14, 27),
            (23, 29),
            (30, 12),
            (39, 25),
            (50, 18),
            (58, 37),
            (48, 51),
            (27, 54),
            (13, 51),
        ],
    ];
    outline_polygon(image, shapes[style], color([104, 111, 108]));
    let highlight: Vec<_> = shapes[style]
        .iter()
        .take((shapes[style].len() / 2).max(3))
        .map(|(x, y)| (x - 3, y - 4))
        .collect();
    polygon(image, &highlight, color([160, 164, 151]));
    line(
        image,
        shapes[style][0],
        *shapes[style].last().unwrap(),
        3,
        color([65, 73, 72]),
    );
    for (x, y) in [(25, 37), (37, 42), (31, 28)] {
        rect(image, x, y, 3, 2, color([65, 73, 72]));
    }
    if style == 4 || style == 5 {
        let shadow: Vec<_> = shapes[style]
            .iter()
            .skip(2)
            .take(3)
            .map(|(x, y)| (x + 2, y + 3))
            .collect();
        polygon(image, &shadow, color([65, 73, 72]));
    }
}

fn tree(image: &mut RgbaImage, style: usize) {
    let dark = color([34, 83, 48]);
    if style == 0 {
        ellipse_rect(image, 16, 16, 32, 39, color([28, 34, 35]));
        for (y, half) in [(16, 3), (25, 10), (35, 15), (45, 11), (53, 5)] {
            polygon(
                image,
                &[(32, y - 6), (32 - half, y + 7), (32 + half, y + 7)],
                dark,
            );
            polygon(
                image,
                &[(32, y - 5), (32 - half + 3, y + 5), (32, y + 2)],
                color([45, 104, 57]),
            );
        }
        rect(image, 29, 48, 6, 9, color([113, 72, 42]));
        return;
    }
    let palettes = [
        ([69, 151, 70], [116, 189, 82], [34, 83, 48]),
        ([53, 128, 61], [129, 184, 91], [35, 84, 46]),
        ([71, 145, 71], [174, 198, 119], [52, 97, 56]),
        ([37, 106, 71], [77, 159, 96], [26, 75, 57]),
        ([57, 121, 56], [123, 173, 72], [37, 80, 43]),
    ];
    let (dark, light, shadow) = palettes[(style - 1) % palettes.len()];
    ellipse_rect(image, 21, 18, 23, 31, color(shadow));
    ellipse_rect(image, 17, 13, 26, 35, color(dark));
    ellipse_rect(image, 21, 16, 17, 20, color(light));
    for (dx, dy, radius) in [(-8, -2, 8), (8, -4, 9), (-7, 10, 8), (7, 12, 7)] {
        circle(image, 32 + dx, 32 + dy, radius + 1, color([28, 34, 35]));
        circle(image, 32 + dx, 31 + dy, radius, color(dark));
        circle(image, 29 + dx, 28 + dy, (radius / 3).max(2), color(light));
    }
    if style == 2 {
        line(image, (23, 33), (25, 43), 2, color([235, 230, 198]));
    }
    rect(image, 29, 48, 6, 9, color([113, 72, 42]));
}

fn forest(image: &mut RgbaImage, style: usize) {
    let layouts: [&[(i32, i32)]; 12] = [
        &[(8, 9), (23, 8), (9, 25), (26, 25), (45, 10)],
        &[(7, 9), (24, 8), (43, 9), (15, 27), (37, 26)],
        &[(55, 9), (40, 8), (54, 25), (36, 25), (19, 10)],
        &[(8, 9), (8, 28), (8, 48), (27, 17), (26, 39)],
        &[
            (10, 10),
            (31, 8),
            (51, 11),
            (15, 31),
            (37, 29),
            (26, 49),
            (50, 48),
        ],
        &[(55, 9), (55, 28), (55, 48), (37, 17), (38, 39)],
        &[(9, 54), (23, 55), (9, 38), (26, 37), (44, 54)],
        &[(8, 55), (24, 55), (43, 55), (15, 38), (37, 38)],
        &[(55, 54), (40, 55), (54, 38), (36, 37), (19, 54)],
        &[(17, 17), (44, 20), (23, 43), (46, 46)],
        &[(9, 30), (29, 10), (50, 32), (30, 52)],
        &[
            (8, 10),
            (27, 8),
            (47, 13),
            (13, 30),
            (36, 29),
            (53, 34),
            (9, 50),
            (29, 49),
            (48, 51),
        ],
    ];
    let palettes = [
        ([46, 129, 58], [100, 177, 76], [28, 76, 42]),
        ([47, 142, 64], [111, 186, 79], [30, 80, 44]),
        ([58, 132, 59], [135, 178, 79], [31, 76, 40]),
        ([34, 124, 65], [89, 169, 88], [25, 72, 47]),
    ];
    let (base, highlight, shade) = palettes[style % palettes.len()];
    ellipse_rect(image, 3, 5, 58, 56, color([28, 58, 36]));
    ellipse_rect(image, 6, 7, 52, 50, color([38, 92, 48]));
    for (index, &(x, y)) in layouts[style].iter().enumerate() {
        let radius = if style == 9 { 9 } else { 13 };
        circle(image, x, y + 2, radius + 2, color([21, 45, 30]));
        circle(image, x, y, radius + 1, color(shade));
        circle(image, x, y - 1, radius, color(base));
        circle(image, x - 4, y - 4, (radius / 3).max(2), color(highlight));
        if index % 3 == 1 {
            circle(image, x + 4, y + 2, 2, color([143, 190, 91]));
        }
    }
    if matches!(style, 4 | 9 | 10) {
        line(image, (18, 46), (45, 20), 5, color([112, 79, 46]));
        line(image, (19, 44), (44, 19), 2, color([179, 132, 75]));
    }
}

fn generate_tile(row: usize, column: usize) -> RgbaImage {
    let mut tile = RgbaImage::from_pixel(TILE_SIZE, TILE_SIZE, Rgba([0, 0, 0, 0]));
    match row {
        0 => wall(&mut tile, column),
        1 => building(&mut tile, column),
        2 if column < 3 => building(&mut tile, column + 6),
        2 => powerup(&mut tile, column - 3),
        3 if column < 3 => powerup(&mut tile, column + 3),
        3 => mine(&mut tile, column - 3),
        4 if column < 2 => mine(&mut tile, column + 3),
        4 => rock(&mut tile, column - 2),
        5 => tree(&mut tile, column),
        6 => forest(&mut tile, column),
        _ => forest(&mut tile, column + 6),
    }
    tile
}

pub fn generate_object_tileset(output: &Path) -> Result<(), String> {
    let mut atlas = RgbaImage::new(
        TILE_SIZE * OBJECT_ATLAS_COLUMNS,
        TILE_SIZE * OBJECT_ATLAS_ROWS,
    );
    for row in 0..OBJECT_ATLAS_ROWS as usize {
        for column in 0..OBJECT_ATLAS_COLUMNS as usize {
            let tile = generate_tile(row, column);
            image::imageops::replace(
                &mut atlas,
                &tile,
                (column as i64) * i64::from(TILE_SIZE),
                (row as i64) * i64::from(TILE_SIZE),
            );
        }
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create atlas folder: {error}"))?;
    }
    atlas
        .save(output)
        .map_err(|error| format!("cannot write object tileset {}: {error}", output.display()))
}

pub fn default_object_tileset_path() -> PathBuf {
    crate::tank16_map::default_game_dir().join("assets/gfx/tiles_objects.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_object_atlas_contains_all_48_visible_tiles() {
        for row in 0..8 {
            for column in 0..6 {
                let tile = generate_tile(row, column);
                assert_eq!(tile.dimensions(), (64, 64));
                assert!(
                    tile.pixels().any(|pixel| pixel[3] != 0),
                    "tile {row},{column}"
                );
            }
        }
    }
}
