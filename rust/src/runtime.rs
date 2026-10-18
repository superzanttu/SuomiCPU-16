use crate::cpu::Cpu;
use crate::image::{ImageError, ProgramImage, build_source, load_binary};
use minifb::{Key, KeyRepeat, Scale, ScaleMode, Window, WindowOptions};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const VRAM_START: usize = 0x50000;
const SPEED_ADDR: usize = 0x43003;
const RTC_START: usize = 0x43010;
const GFX_MODE_ADDR: usize = 0x43014;
const SCREEN_FPS: u32 = 30;
const PALETTE: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (255, 255, 255),
    (255, 48, 48),
    (48, 220, 64),
    (64, 96, 255),
    (255, 224, 0),
    (0, 224, 224),
    (224, 64, 224),
    (255, 144, 0),
    (150, 150, 150),
    (80, 80, 80),
    (128, 0, 0),
    (255, 128, 192),
    (150, 100, 50),
    (46, 30, 22),
    (0, 0, 0),
];

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

#[derive(Clone)]
struct BootModule {
    name: String,
    description: String,
    source: Option<PathBuf>,
    error: Option<String>,
}

pub fn run(root: &Path, program: Option<&Path>, fullscreen: bool) -> Result<(), RuntimeError> {
    if fullscreen {
        return Err(RuntimeError::Window(
            "fullscreen mode is not implemented in the Rust runtime yet".to_owned(),
        ));
    }
    if !root.join("tools").join("sc16_build_image.py").is_file() {
        return Err(RuntimeError::MissingBuildTools(
            root.join("tools").join("sc16_build_image.py"),
        ));
    }
    let mut window = Window::new(
        "SuomiCPU-16 Emulator (Rust)",
        960,
        720,
        WindowOptions {
            scale: Scale::X1,
            scale_mode: ScaleMode::AspectRatioStretch,
            ..WindowOptions::default()
        },
    )
    .map_err(|error| RuntimeError::Window(error.to_string()))?;
    window.set_target_fps(SCREEN_FPS as usize);

    let mut cpu = Cpu::new();
    let font = load_font();
    let mut pixels = vec![0u32; 640 * 480];
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
        if !select_and_boot(root, &mut cpu, &mut window, &mut pixels, &font, &modules)? {
            return Ok(());
        }
    }

    let mut old_keys = Vec::new();
    let mut speed_remainder = 0.0;
    let mut last_frame = Instant::now();
    while window.is_open() && !window.is_key_down(Key::Escape) && cpu.running {
        let keys = window.get_keys();
        update_keys(&mut cpu, &old_keys, &keys);
        old_keys = keys;
        for key in window.get_keys_pressed(KeyRepeat::No) {
            if key == Key::Enter {
                cpu.receive_console_input(b"\n");
                cpu.signal_keyboard_event(13);
            } else if key == Key::Backspace {
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
        window
            .update_with_buffer(&pixels[..width * height], width, height)
            .map_err(|error| RuntimeError::Window(error.to_string()))?;
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

fn select_and_boot(
    root: &Path,
    cpu: &mut Cpu,
    window: &mut Window,
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
        window
            .update_with_buffer(pixels, 640, 480)
            .map_err(|error| RuntimeError::Window(error.to_string()))?;
        if !window.is_open() || window.is_key_pressed(Key::Escape, KeyRepeat::No) {
            return Ok(false);
        }
        if window.is_key_pressed(Key::Up, KeyRepeat::Yes) {
            selected = selected.saturating_sub(1);
        } else if window.is_key_pressed(Key::Down, KeyRepeat::Yes) {
            selected = (selected + 1).min(modules.len() - 1);
        } else if window.is_key_pressed(Key::Enter, KeyRepeat::No) {
            let module = &modules[selected];
            if let Some(error) = &module.error {
                draw_text(pixels, 640, 32, 440, error, 0xffff6060, font);
                window
                    .update_with_buffer(pixels, 640, 480)
                    .map_err(|error| RuntimeError::Window(error.to_string()))?;
                std::thread::sleep(Duration::from_secs(2));
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
        std::thread::sleep(Duration::from_millis(16));
    }
}

fn update_keys(cpu: &mut Cpu, previous: &[Key], current: &[Key]) {
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

fn key_masks() -> [(Key, u32); 25] {
    [
        (Key::Left, 1),
        (Key::A, 1),
        (Key::Right, 2),
        (Key::D, 2),
        (Key::Up, 4),
        (Key::W, 4),
        (Key::Down, 8),
        (Key::S, 8),
        (Key::Space, 16),
        (Key::Enter, 32),
        (Key::Tab, 64),
        (Key::Q, 0x100),
        (Key::E, 0x200),
        (Key::LeftShift, 0x400),
        (Key::RightShift, 0x400),
        (Key::X, 0x400),
        (Key::LeftCtrl, 0x800),
        (Key::RightCtrl, 0x800),
        (Key::Z, 0x800),
        (Key::C, 0x1_0000),
        (Key::J, 0x2_0000),
        (Key::L, 0x4_0000),
        (Key::I, 0x8_0000),
        (Key::K, 0x10_0000),
        (Key::Period, 0x20_0000),
    ]
}

fn key_to_ascii(key: Key) -> Option<u8> {
    match key {
        Key::A => Some(b'a'),
        Key::B => Some(b'b'),
        Key::C => Some(b'c'),
        Key::D => Some(b'd'),
        Key::E => Some(b'e'),
        Key::F => Some(b'f'),
        Key::G => Some(b'g'),
        Key::H => Some(b'h'),
        Key::I => Some(b'i'),
        Key::J => Some(b'j'),
        Key::K => Some(b'k'),
        Key::L => Some(b'l'),
        Key::M => Some(b'm'),
        Key::N => Some(b'n'),
        Key::O => Some(b'o'),
        Key::P => Some(b'p'),
        Key::Q => Some(b'q'),
        Key::R => Some(b'r'),
        Key::S => Some(b's'),
        Key::T => Some(b't'),
        Key::U => Some(b'u'),
        Key::V => Some(b'v'),
        Key::W => Some(b'w'),
        Key::X => Some(b'x'),
        Key::Y => Some(b'y'),
        Key::Z => Some(b'z'),
        Key::Key0 => Some(b'0'),
        Key::Key1 => Some(b'1'),
        Key::Key2 => Some(b'2'),
        Key::Key3 => Some(b'3'),
        Key::Key4 => Some(b'4'),
        Key::Key5 => Some(b'5'),
        Key::Key6 => Some(b'6'),
        Key::Key7 => Some(b'7'),
        Key::Key8 => Some(b'8'),
        Key::Key9 => Some(b'9'),
        Key::Space => Some(b' '),
        Key::Comma => Some(b','),
        Key::Period => Some(b'.'),
        Key::Minus => Some(b'-'),
        Key::Apostrophe => Some(b'\''),
        Key::Slash => Some(b'/'),
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
    for y in 0..height {
        for x in 0..width {
            let color = if width == 320 {
                cpu.memory[VRAM_START + y * width + x]
            } else {
                let packed = cpu.memory[VRAM_START + y * (width / 2) + x / 2];
                if x & 1 == 0 {
                    packed >> 4
                } else {
                    packed & 0xf
                }
            };
            pixels[y * width + x] = palette_color(color);
        }
    }
}

fn palette_color(index: u8) -> u32 {
    let (red, green, blue) = if index < PALETTE.len() as u8 {
        PALETTE[index as usize]
    } else if index == 25 {
        (0, 0, 255)
    } else {
        (index, index, index)
    };
    rgb(red, green, blue)
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
