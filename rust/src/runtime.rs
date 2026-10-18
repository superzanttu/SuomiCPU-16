use crate::cpu::{Cpu, GpuLine, GpuTriangle};
use crate::image::{ImageError, ProgramImage, build_source, load_binary};
use macroquad::prelude::{
    BLACK, Color, Conf, DrawTextureParams, FilterMode, KeyCode, Rect, Texture2D, WHITE,
    clear_background, draw_line, draw_texture_ex, draw_triangle, get_keys_down, get_keys_pressed,
    is_key_down, is_key_pressed, next_frame, screen_height, screen_width, set_fullscreen, vec2,
};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const SPEED_ADDR: usize = 0x43003;
const RTC_START: usize = 0x43010;
const GFX_MODE_ADDR: usize = 0x43014;

#[derive(Debug)]
pub enum RuntimeError {
    Image(ImageError),
    Window(String),
    Io(std::io::Error),
    MissingBuildTools(PathBuf),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Image(error) => write!(formatter, "{error}"),
            Self::Window(error) => write!(formatter, "window error: {error}"),
            Self::Io(error) => write!(formatter, "{error}"),
            Self::MissingBuildTools(path) => {
                write!(
                    formatter,
                    "Python image builder not found: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<ImageError> for RuntimeError {
    fn from(error: ImageError) -> Self {
        Self::Image(error)
    }
}

impl From<std::io::Error> for RuntimeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

struct Canvas {
    texture: Texture2D,
    rgba: Vec<u8>,
}

impl Canvas {
    fn new() -> Self {
        let rgba = vec![0; 640 * 480 * 4];
        let texture = Texture2D::from_rgba8(640, 480, &rgba);
        texture.set_filter(FilterMode::Nearest);
        Self { texture, rgba }
    }

    async fn present(
        &mut self,
        pixels: &[u32],
        width: usize,
        height: usize,
        triangles: &[GpuTriangle],
        lines: &[GpuLine],
        palette: &[[u8; 3]; 256],
    ) {
        self.rgba.fill(0);
        for y in 0..height {
            for x in 0..width {
                let pixel = pixels[y * width + x];
                let index = (y * 640 + x) * 4;
                self.rgba[index] = (pixel >> 16) as u8;
                self.rgba[index + 1] = (pixel >> 8) as u8;
                self.rgba[index + 2] = pixel as u8;
                self.rgba[index + 3] = 255;
            }
        }
        self.texture.update_from_bytes(640, 480, &self.rgba);

        let scale = (screen_width() / width as f32).min(screen_height() / height as f32);
        let destination = vec2(width as f32 * scale, height as f32 * scale);
        let position = vec2(
            (screen_width() - destination.x) / 2.0,
            (screen_height() - destination.y) / 2.0,
        );
        clear_background(BLACK);
        draw_texture_ex(
            &self.texture,
            position.x,
            position.y,
            WHITE,
            DrawTextureParams {
                source: Some(Rect::new(0.0, 0.0, width as f32, height as f32)),
                dest_size: Some(destination),
                ..DrawTextureParams::default()
            },
        );
        let to_screen =
            |(x, y): (i32, i32)| vec2(position.x + x as f32 * scale, position.y + y as f32 * scale);
        for triangle in triangles {
            let color = palette[triangle.color as usize];
            let points = triangle.points.map(to_screen);
            draw_triangle(
                points[0],
                points[1],
                points[2],
                Color::from_rgba(color[0], color[1], color[2], 255),
            );
        }
        for line in lines {
            let color = palette[line.color as usize];
            let (start, end) = (to_screen(line.start), to_screen(line.end));
            draw_line(
                start.x,
                start.y,
                end.x,
                end.y,
                scale,
                Color::from_rgba(color[0], color[1], color[2], 255),
            );
        }
        next_frame().await;
    }
}

#[derive(Clone)]
struct BootModule {
    name: String,
    description: String,
    source: Option<PathBuf>,
    error: Option<String>,
}

pub fn window_conf() -> Conf {
    Conf {
        window_title: "SuomiCPU-16 Emulator (Rust)".to_owned(),
        window_width: 960,
        window_height: 720,
        high_dpi: false,
        ..Conf::default()
    }
}

pub async fn run(
    root: &Path,
    program: Option<&Path>,
    fullscreen: bool,
) -> Result<(), RuntimeError> {
    if !root.join("tools").join("sc16_build_image.py").is_file() {
        return Err(RuntimeError::MissingBuildTools(
            root.join("tools").join("sc16_build_image.py"),
        ));
    }
    set_fullscreen(fullscreen);

    let mut cpu = Cpu::new();
    let font = load_font();
    let mut pixels = vec![0u32; 640 * 480];
    let mut canvas = Canvas::new();
    if let Some(source) = program {
        let image = build_program(root, source)?;
        cpu.load_program(&image.segments, image.entry_point)
            .map_err(|error| RuntimeError::Window(error.to_string()))?;
    } else {
        let modules = discover_modules(&root.join("modules"))?;
        if modules.is_empty() {
            return Err(RuntimeError::Window(
                "no runnable C modules were found in modules/".to_owned(),
            ));
        }
        if !select_and_boot(root, &mut cpu, &mut canvas, &mut pixels, &font, &modules).await? {
            return Ok(());
        }
    }

    let mut old_keys = HashSet::new();
    let mut speed_remainder = 0.0;
    let mut last_frame = Instant::now();
    while !is_key_down(KeyCode::Escape) && cpu.running {
        let keys = get_keys_down();
        update_keys(&mut cpu, &old_keys, &keys);
        old_keys = keys;
        for key in get_keys_pressed() {
            if key == KeyCode::Enter {
                cpu.receive_console_input(b"\n");
                cpu.signal_keyboard_event(13);
            } else if key == KeyCode::Backspace {
                cpu.receive_console_input(b"\x08");
                cpu.signal_keyboard_event(8);
            } else if let Some(byte) = key_to_ascii(key) {
                cpu.receive_console_input(&[byte]);
                cpu.signal_keyboard_event(byte);
            }
        }
        update_rtc(&mut cpu);
        cpu.begin_frame();
        let elapsed = last_frame.elapsed().as_secs_f64();
        last_frame = Instant::now();
        let mode = cpu.memory[SPEED_ADDR];
        let budget = match mode {
            0 => rate_budget(10.0, elapsed, &mut speed_remainder),
            1 => rate_budget(100.0, elapsed, &mut speed_remainder),
            2 => rate_budget(1_000_000.0, elapsed, &mut speed_remainder),
            3 => rate_budget(10_000_000.0, elapsed, &mut speed_remainder),
            _ => 300_000,
        };
        cpu.run(budget)
            .map_err(|error| RuntimeError::Window(error.to_string()))?;
        render_cpu(&cpu, &mut pixels);
        let (width, height) = screen_size(&cpu);
        canvas
            .present(
                &pixels,
                width,
                height,
                &cpu.gpu_triangles,
                &cpu.gpu_lines,
                &cpu.palette,
            )
            .await;
    }
    Ok(())
}

pub fn run_headless(root: &Path, program: &Path, budget: usize) -> Result<usize, RuntimeError> {
    let mut cpu = Cpu::new();
    let image = build_program(root, program)?;
    cpu.load_program(&image.segments, image.entry_point)
        .map_err(|error| RuntimeError::Window(error.to_string()))?;
    let executed = cpu
        .run(budget)
        .map_err(|error| RuntimeError::Window(error.to_string()))?;
    if cpu.running {
        return Err(RuntimeError::Window(format!(
            "instruction budget exhausted after {executed} instructions"
        )));
    }
    Ok(executed)
}

fn build_program(root: &Path, source: &Path) -> Result<ProgramImage, RuntimeError> {
    if source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bin"))
    {
        return load_binary(source).map_err(RuntimeError::from);
    }
    let python = std::env::var("SC16_PYTHON").unwrap_or_else(|_| "python".to_owned());
    build_source(root, source, &python).map_err(RuntimeError::from)
}

fn discover_modules(directory: &Path) -> Result<Vec<BootModule>, RuntimeError> {
    let mut modules = Vec::new();
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());
    for entry in entries {
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let mut sources = fs::read_dir(entry.path())?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|item| {
                item.path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("c"))
            })
            .map(|item| item.path())
            .collect::<Vec<_>>();
        sources.sort_by_key(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
        });
        let name = entry.file_name().to_string_lossy().into_owned();
        let (source, error) = match sources.len() {
            0 => (None, Some("No .c files found in module folder.".to_owned())),
            1 => (sources.pop(), None),
            _ => {
                let main = sources
                    .iter()
                    .find(|path| {
                        path.file_name()
                            .is_some_and(|name| name.eq_ignore_ascii_case("main.c"))
                    })
                    .cloned();
                match main {
                    Some(path) => (Some(path), None),
                    None => (
                        None,
                        Some("Multiple .c files found; expected main.c.".to_owned()),
                    ),
                }
            }
        };
        let description = source
            .as_ref()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|contents| contents.lines().next().map(str::to_owned))
            .map(|line| line.trim_start_matches('/').trim().to_owned())
            .filter(|line| !line.is_empty())
            .unwrap_or_else(|| error.clone().unwrap_or_else(|| "No description".to_owned()));
        modules.push(BootModule {
            name,
            description,
            source,
            error,
        });
    }
    Ok(modules)
}

async fn select_and_boot(
    root: &Path,
    cpu: &mut Cpu,
    canvas: &mut Canvas,
    pixels: &mut [u32],
    font: &[[u8; 8]],
    modules: &[BootModule],
) -> Result<bool, RuntimeError> {
    let mut selected = 0usize;
    loop {
        pixels.fill(rgb(8, 12, 24));
        draw_text(pixels, 640, 32, 26, "SUOMICPU-16 BIOS", 0xffffd000, font);
        draw_text(
            pixels,
            640,
            32,
            58,
            "SELECT A MODULE  -  UP/DOWN + ENTER",
            0xffb8c7df,
            font,
        );
        for (index, module) in modules.iter().enumerate() {
            let y = 94 + index * 24;
            let selected_row = index == selected;
            let color = if selected_row { 0xff101020 } else { 0xff080c18 };
            for row in y..(y + 20).min(470) {
                for column in 28..612 {
                    pixels[row * 640 + column] = color;
                }
            }
            draw_text(
                pixels,
                640,
                38,
                y + 2,
                &module.name,
                if selected_row { 0xffffffff } else { 0xffc2cee0 },
                font,
            );
            draw_text(
                pixels,
                640,
                240,
                y + 2,
                &module.description,
                0xff91a1b8,
                font,
            );
        }
        canvas
            .present(pixels, 640, 480, &[], &[], &cpu.palette)
            .await;
        if is_key_pressed(KeyCode::Escape) {
            return Ok(false);
        }
        if is_key_pressed(KeyCode::Up) {
            selected = selected.saturating_sub(1);
        } else if is_key_pressed(KeyCode::Down) {
            selected = (selected + 1).min(modules.len() - 1);
        } else if is_key_pressed(KeyCode::Enter) {
            let module = &modules[selected];
            if let Some(error) = &module.error {
                draw_text(pixels, 640, 32, 440, error, 0xffff6060, font);
                let error_until = Instant::now() + std::time::Duration::from_secs(2);
                while Instant::now() < error_until {
                    canvas
                        .present(pixels, 640, 480, &[], &[], &cpu.palette)
                        .await;
                }
                continue;
            }
            let source = module
                .source
                .as_ref()
                .expect("valid boot module has a source");
            let image = build_program(root, source)?;
            cpu.load_program(&image.segments, image.entry_point)
                .map_err(|error| RuntimeError::Window(error.to_string()))?;
            return Ok(true);
        }
    }
}

fn update_keys(cpu: &mut Cpu, previous: &HashSet<KeyCode>, current: &HashSet<KeyCode>) {
    for (key, mask) in key_masks() {
        let was_down = previous.contains(&key);
        let is_down = current.contains(&key);
        if was_down != is_down {
            cpu.set_key(mask, is_down);
            if is_down {
                cpu.signal_keyboard_event(key_to_ascii(key).unwrap_or(0));
            }
        }
    }
}

fn key_masks() -> [(KeyCode, u32); 25] {
    [
        (KeyCode::Left, 1),
        (KeyCode::A, 1),
        (KeyCode::Right, 2),
        (KeyCode::D, 2),
        (KeyCode::Up, 4),
        (KeyCode::W, 4),
        (KeyCode::Down, 8),
        (KeyCode::S, 8),
        (KeyCode::Space, 16),
        (KeyCode::Enter, 32),
        (KeyCode::Tab, 64),
        (KeyCode::Q, 0x100),
        (KeyCode::E, 0x200),
        (KeyCode::LeftShift, 0x400),
        (KeyCode::RightShift, 0x400),
        (KeyCode::X, 0x400),
        (KeyCode::LeftControl, 0x800),
        (KeyCode::RightControl, 0x800),
        (KeyCode::Z, 0x800),
        (KeyCode::C, 0x1_0000),
        (KeyCode::J, 0x2_0000),
        (KeyCode::L, 0x4_0000),
        (KeyCode::I, 0x8_0000),
        (KeyCode::K, 0x10_0000),
        (KeyCode::Period, 0x20_0000),
    ]
}

fn key_to_ascii(key: KeyCode) -> Option<u8> {
    match key {
        KeyCode::A => Some(b'a'),
        KeyCode::B => Some(b'b'),
        KeyCode::C => Some(b'c'),
        KeyCode::D => Some(b'd'),
        KeyCode::E => Some(b'e'),
        KeyCode::F => Some(b'f'),
        KeyCode::G => Some(b'g'),
        KeyCode::H => Some(b'h'),
        KeyCode::I => Some(b'i'),
        KeyCode::J => Some(b'j'),
        KeyCode::K => Some(b'k'),
        KeyCode::L => Some(b'l'),
        KeyCode::M => Some(b'm'),
        KeyCode::N => Some(b'n'),
        KeyCode::O => Some(b'o'),
        KeyCode::P => Some(b'p'),
        KeyCode::Q => Some(b'q'),
        KeyCode::R => Some(b'r'),
        KeyCode::S => Some(b's'),
        KeyCode::T => Some(b't'),
        KeyCode::U => Some(b'u'),
        KeyCode::V => Some(b'v'),
        KeyCode::W => Some(b'w'),
        KeyCode::X => Some(b'x'),
        KeyCode::Y => Some(b'y'),
        KeyCode::Z => Some(b'z'),
        KeyCode::Key0 => Some(b'0'),
        KeyCode::Key1 => Some(b'1'),
        KeyCode::Key2 => Some(b'2'),
        KeyCode::Key3 => Some(b'3'),
        KeyCode::Key4 => Some(b'4'),
        KeyCode::Key5 => Some(b'5'),
        KeyCode::Key6 => Some(b'6'),
        KeyCode::Key7 => Some(b'7'),
        KeyCode::Key8 => Some(b'8'),
        KeyCode::Key9 => Some(b'9'),
        KeyCode::Space => Some(b' '),
        KeyCode::Comma => Some(b','),
        KeyCode::Period => Some(b'.'),
        KeyCode::Minus => Some(b'-'),
        KeyCode::Apostrophe => Some(b'\''),
        KeyCode::Slash => Some(b'/'),
        _ => None,
    }
}

fn update_rtc(cpu: &mut Cpu) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = now.as_secs() % 60;
    let minutes = (now.as_secs() / 60) % 60;
    let hours = (now.as_secs() / 3600) % 24;
    cpu.memory[RTC_START] = seconds as u8;
    cpu.memory[RTC_START + 1] = minutes as u8;
    cpu.memory[RTC_START + 2] = hours as u8;
    cpu.memory[0x43002] |= 8;
}

fn rate_budget(rate: f64, elapsed: f64, remainder: &mut f64) -> usize {
    let exact = rate * elapsed.max(0.0) + *remainder;
    let result = exact.floor() as usize;
    *remainder = exact - result as f64;
    result
}

fn screen_size(cpu: &Cpu) -> (usize, usize) {
    if cpu.memory[GFX_MODE_ADDR] == 1 {
        (640, 480)
    } else {
        (320, 240)
    }
}

fn render_cpu(cpu: &Cpu, pixels: &mut [u32]) {
    let (width, height) = screen_size(cpu);
    let framebuffer = cpu.composited_framebuffer();
    for y in 0..height {
        for x in 0..width {
            let color = framebuffer[y * width + x];
            let [red, green, blue] = cpu.palette_color(color);
            pixels[y * width + x] = rgb(red, green, blue);
        }
    }
}

const fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    ((red as u32) << 16) | ((green as u32) << 8) | blue as u32
}

fn load_font() -> Vec<[u8; 8]> {
    let bytes = include_str!("../../fonts/font5x7.asm")
        .lines()
        .filter_map(|line| line.split_once(".data hex").map(|(_, bytes)| bytes))
        .flat_map(|bytes| {
            bytes
                .split(',')
                .filter_map(|value| u8::from_str_radix(value.trim(), 16).ok())
        })
        .collect::<Vec<_>>();
    bytes.as_chunks::<8>().0.to_vec()
}

fn draw_text(
    pixels: &mut [u32],
    width: usize,
    x: usize,
    y: usize,
    text: &str,
    color: u32,
    font: &[[u8; 8]],
) {
    for (character_index, byte) in text.bytes().enumerate() {
        let glyph_index = byte.saturating_sub(32) as usize;
        let Some(glyph) = font.get(glyph_index) else {
            continue;
        };
        let glyph_x = x + character_index * 6;
        for (row, bits) in glyph.iter().copied().enumerate().take(7) {
            for column in 0..5 {
                if bits & (0x80 >> column) != 0 {
                    let address = (y + row) * width + glyph_x + column;
                    if let Some(pixel) = pixels.get_mut(address) {
                        *pixel = color;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_c_compiler_builds_and_rust_executes_program() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let program = root.join("examples").join("factorial.c");
        let image = build_program(root, &program).unwrap();
        assert_eq!(image.entry_point, 0);
        assert!(image.segments.len() > 1);
        let mut cpu = Cpu::new();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        let count = cpu.run(50_000).unwrap();
        assert!(
            !cpu.running,
            "program did not halt after {count} instructions"
        );
    }

    #[test]
    fn retained_assembler_builds_commands_and_directive_examples() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for name in ["commands.asm", "directives.asm", "example.asm"] {
            let program = root.join("examples").join(name);
            let image = build_program(root, &program).unwrap();
            let mut cpu = Cpu::new();
            cpu.load_program(&image.segments, image.entry_point)
                .unwrap();
            let count = cpu.run(50_000).unwrap();
            assert!(
                !cpu.running,
                "{name} did not halt after {count} instructions"
            );
        }
    }
}
