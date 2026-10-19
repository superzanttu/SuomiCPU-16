use crate::cpu::{Cpu, GpuLine, GpuSoundCommand, GpuTriangle};
use crate::image::{ImageError, ProgramImage, build_source, load_binary};
use crate::net::DEFAULT_PORT;
use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound, stop_sound};
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
const SOUND_RATE: u32 = 22_050;
const SOUND_FPS: u32 = 60;
const BIOS_VISIBLE_MODULES: usize = 12;
const BIOS_MODULE_ROW_TOP: usize = 128;
const BIOS_MODULE_ROW_HEIGHT: usize = 24;

#[derive(Debug)]
pub enum RuntimeError {
    Image(ImageError),
    Window(String),
    Io(std::io::Error),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Image(error) => write!(formatter, "{error}"),
            Self::Window(error) => write!(formatter, "window error: {error}"),
            Self::Io(error) => write!(formatter, "{error}"),
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

struct ActiveSound {
    command: GpuSoundCommand,
    sound: Sound,
}

struct AudioEngine {
    channels: [Option<ActiveSound>; 4],
}

impl AudioEngine {
    fn new() -> Self {
        Self {
            channels: std::array::from_fn(|_| None),
        }
    }

    async fn apply(&mut self, command: GpuSoundCommand) -> Result<(), RuntimeError> {
        let channel = command.channel as usize;
        if command.frequency == 0 {
            if let Some(active) = self.channels[channel].take() {
                stop_sound(&active.sound);
            }
            return Ok(());
        }
        if command.frames == 0
            && self.channels[channel]
                .as_ref()
                .is_some_and(|active| active.command == command)
        {
            return Ok(());
        }
        if let Some(active) = self.channels[channel].take() {
            stop_sound(&active.sound);
        }
        let wav = synthesize_sound(command);
        let sound = load_sound_from_bytes(&wav).await.map_err(|error| {
            RuntimeError::Window(format!("cannot load synthesized sound: {error}"))
        })?;
        play_sound(
            &sound,
            PlaySoundParams {
                looped: command.frames == 0,
                volume: 1.0,
            },
        );
        self.channels[channel] = Some(ActiveSound { command, sound });
        Ok(())
    }
}

fn synthesize_sound(command: GpuSoundCommand) -> Vec<u8> {
    let frequency = u32::from(command.frequency).clamp(20, SOUND_RATE / 2);
    let period = (SOUND_RATE + frequency / 2) / frequency;
    let sample_count = if command.frames == 0 {
        if command.waveform == 1 {
            SOUND_RATE / 2
        } else {
            let quarter_second = SOUND_RATE / 4;
            period * (quarter_second / period).max(1)
        }
    } else {
        u32::from(command.frames) * SOUND_RATE / SOUND_FPS
    } as usize;
    let volume = if command.volume == 0 {
        100
    } else {
        command.volume.min(100)
    };
    let amplitude = 32_767i32 * i32::from(volume.max(1)) / 100 / 2;
    let data_bytes = sample_count * 2;
    let mut wav = Vec::with_capacity(44 + data_bytes);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36u32 + data_bytes as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&SOUND_RATE.to_le_bytes());
    wav.extend_from_slice(&(SOUND_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_bytes as u32).to_le_bytes());

    let mut noise_state = 0x16c0_ffee ^ u32::from(command.frequency);
    for index in 0..sample_count {
        let phase = index as u32 % period;
        let mut sample = match command.waveform {
            1 => {
                if phase == 0 {
                    noise_state ^= noise_state << 13;
                    noise_state ^= noise_state >> 17;
                    noise_state ^= noise_state << 5;
                }
                let range = (amplitude * 2 + 1) as u32;
                (noise_state % range) as i32 - amplitude
            }
            2 => {
                let centered =
                    (phase as i64 * 4 * amplitude as i64 / period as i64) - (2 * amplitude as i64);
                if centered < 0 {
                    (-centered - amplitude as i64) as i32
                } else {
                    (centered - amplitude as i64) as i32
                }
            }
            _ => {
                if phase < period / 2 {
                    amplitude
                } else {
                    -amplitude
                }
            }
        };
        if command.frames != 0 {
            sample = (sample as i64 * (sample_count - index) as i64 / sample_count as i64) as i32;
        }
        wav.extend_from_slice(&(sample as i16).to_le_bytes());
    }
    wav
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
    run_with_devices(root, program, fullscreen, None, DEFAULT_PORT).await
}

pub async fn run_with_devices(
    root: &Path,
    program: Option<&Path>,
    fullscreen: bool,
    disk_id: Option<&str>,
    network_port: u16,
) -> Result<(), RuntimeError> {
    set_fullscreen(fullscreen);

    let mut cpu = Cpu::new();
    cpu.set_network_port(network_port);
    let font = load_font();
    let mut pixels = vec![0u32; 640 * 480];
    let mut canvas = Canvas::new();
    let mut audio = AudioEngine::new();
    if let Some(source) = program {
        configure_disk(&mut cpu, root, source)?;
        if let Some(disk_id) = disk_id {
            cpu.mount_data_disk(disk_id).map_err(RuntimeError::Window)?;
        }
        let image = build_program(root, source)?;
        cpu.load_program(&image.segments, image.entry_point)
            .map_err(|error| RuntimeError::Window(error.to_string()))?;
    } else {
        cpu.configure_disk(root.join("disks"), root.join("disks"))
            .map_err(RuntimeError::Window)?;
        let modules = discover_modules(&root.join("modules"))?;
        if modules.is_empty() {
            return Err(RuntimeError::Window(
                "no runnable C modules were found in modules/".to_owned(),
            ));
        }
        if !select_and_boot(
            root,
            &mut cpu,
            &mut canvas,
            &mut pixels,
            &font,
            &modules,
            disk_id,
        )
        .await?
        {
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
        for command in cpu.take_sound_commands() {
            audio.apply(command).await?;
        }
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
    run_headless_with_devices(root, program, budget, None, DEFAULT_PORT)
}

pub fn run_headless_with_devices(
    root: &Path,
    program: &Path,
    budget: usize,
    disk_id: Option<&str>,
    network_port: u16,
) -> Result<usize, RuntimeError> {
    let mut cpu = Cpu::new();
    cpu.set_network_port(network_port);
    configure_disk(&mut cpu, root, program)?;
    if let Some(disk_id) = disk_id {
        cpu.mount_data_disk(disk_id).map_err(RuntimeError::Window)?;
    }
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
    disk_id: Option<&str>,
) -> Result<bool, RuntimeError> {
    let mut selected = 0usize;
    let mut scroll_offset = 0usize;
    let disks = cpu.available_disks().map_err(RuntimeError::Window)?;
    let mut selected_disk = match disk_id {
        Some(disk_id) => disks
            .iter()
            .position(|disk| disk == disk_id)
            .map(|index| index + 1)
            .ok_or_else(|| {
                RuntimeError::Window(format!("virtual data disk {disk_id:?} does not exist"))
            })?,
        None => 0,
    };
    loop {
        draw_bios_selector(
            pixels,
            font,
            modules,
            selected,
            scroll_offset,
            &disks,
            selected_disk,
        );
        canvas
            .present(pixels, 640, 480, &[], &[], &cpu.palette)
            .await;
        if is_key_pressed(KeyCode::Escape) {
            return Ok(false);
        }
        if is_key_pressed(KeyCode::Up) {
            selected = selected.saturating_sub(1);
            scroll_offset = bios_scroll_offset(selected, scroll_offset, modules.len());
        } else if is_key_pressed(KeyCode::Down) {
            selected = (selected + 1).min(modules.len() - 1);
            scroll_offset = bios_scroll_offset(selected, scroll_offset, modules.len());
        } else if is_key_pressed(KeyCode::D) || is_key_pressed(KeyCode::Right) {
            selected_disk = (selected_disk + 1) % (disks.len() + 1);
        } else if is_key_pressed(KeyCode::Left) {
            selected_disk = (selected_disk + disks.len()) % (disks.len() + 1);
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
            configure_disk(cpu, root, source)?;
            if selected_disk != 0 {
                cpu.mount_data_disk(&disks[selected_disk - 1])
                    .map_err(RuntimeError::Window)?;
            }
            let image = build_program(root, source)?;
            cpu.load_program(&image.segments, image.entry_point)
                .map_err(|error| RuntimeError::Window(error.to_string()))?;
            return Ok(true);
        }
    }
}

fn draw_bios_selector(
    pixels: &mut [u32],
    font: &[[u8; 8]],
    modules: &[BootModule],
    selected: usize,
    scroll_offset: usize,
    disks: &[String],
    selected_disk: usize,
) {
    pixels.fill(rgb(7, 10, 20));
    fill_rect(pixels, 640, 24, 18, 592, 82, rgb(16, 22, 38));
    fill_rect(pixels, 640, 24, 108, 592, 326, rgb(11, 15, 28));
    draw_text(pixels, 640, 40, 31, "SUOMICPU-16 BIOS", 0xffffd000, font);
    draw_text(pixels, 640, 40, 49, "MODULE MANAGER", 0xff90a8c8, font);
    draw_text(
        pixels,
        640,
        40,
        79,
        "UP/DOWN SELECT   ENTER START   ESC EXIT",
        0xffc2cee0,
        font,
    );

    let disk_name = if selected_disk == 0 {
        "NONE"
    } else {
        disks
            .get(selected_disk - 1)
            .map(String::as_str)
            .unwrap_or("NONE")
    };
    fill_rect(pixels, 640, 420, 73, 184, 27, rgb(35, 31, 19));
    draw_text(
        pixels,
        640,
        430,
        83,
        &format!("DISK: {disk_name}  < >"),
        0xffffd000,
        font,
    );

    draw_text(pixels, 640, 42, 111, "MODULE", 0xff90a8c8, font);
    draw_text(pixels, 640, 250, 111, "DESCRIPTION", 0xff90a8c8, font);
    let first = scroll_offset.min(modules.len().saturating_sub(BIOS_VISIBLE_MODULES));
    for (row, (index, module)) in modules
        .iter()
        .enumerate()
        .skip(first)
        .take(BIOS_VISIBLE_MODULES)
        .enumerate()
    {
        let y = BIOS_MODULE_ROW_TOP + row * BIOS_MODULE_ROW_HEIGHT;
        let is_selected = index == selected;
        let background = if is_selected {
            rgb(37, 47, 68)
        } else if row % 2 == 0 {
            rgb(14, 20, 34)
        } else {
            rgb(11, 15, 28)
        };
        fill_rect(pixels, 640, 32, y, 576, 22, background);
        let name_color = if is_selected { 0xffffffff } else { 0xffd0d9e8 };
        let number = format!("{:02}", index + 1);
        draw_text(pixels, 640, 42, y + 7, &number, 0xff7185a3, font);
        draw_text_limited(pixels, 640, (66, y + 7), &module.name, 27, name_color, font);
        draw_text_limited(
            pixels,
            640,
            (250, y + 7),
            &module.description,
            60,
            if is_selected { 0xffe2e8f2 } else { 0xff9eacc1 },
            font,
        );
    }

    let position = if modules.is_empty() {
        "NO MODULES FOUND".to_owned()
    } else {
        format!("MODULE {:02}/{:02}", selected + 1, modules.len())
    };
    draw_text(pixels, 640, 42, 445, &position, 0xff90a8c8, font);
    draw_text(
        pixels,
        640,
        250,
        445,
        "D / LEFT / RIGHT: CHANGE DISK",
        0xff90a8c8,
        font,
    );
}

fn bios_scroll_offset(selected: usize, current_offset: usize, module_count: usize) -> usize {
    let max_offset = module_count.saturating_sub(BIOS_VISIBLE_MODULES);
    let offset = current_offset.min(max_offset);
    if selected < offset {
        selected
    } else if selected >= offset + BIOS_VISIBLE_MODULES {
        selected + 1 - BIOS_VISIBLE_MODULES
    } else {
        offset
    }
}

fn fill_rect(
    pixels: &mut [u32],
    width: usize,
    x: usize,
    y: usize,
    rect_width: usize,
    rect_height: usize,
    color: u32,
) {
    let height = pixels.len() / width;
    for row in y..y.saturating_add(rect_height).min(height) {
        let start = row * width + x.min(width);
        let end = row * width + x.saturating_add(rect_width).min(width);
        pixels[start..end].fill(color);
    }
}

fn configure_disk(cpu: &mut Cpu, root: &Path, source: &Path) -> Result<(), RuntimeError> {
    let module_disk = source
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("disks");
    cpu.configure_disk(root.join("disks"), module_disk)
        .map_err(RuntimeError::Window)
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
                let pixel_x = glyph_x + column;
                let pixel_y = y + row;
                if bits & (0x80 >> column) != 0
                    && pixel_x < width
                    && let Some(pixel) = pixels.get_mut(pixel_y * width + pixel_x)
                {
                    *pixel = color;
                }
            }
        }
    }
}

fn draw_text_limited(
    pixels: &mut [u32],
    width: usize,
    (x, y): (usize, usize),
    text: &str,
    max_characters: usize,
    color: u32,
    font: &[[u8; 8]],
) {
    let character_count = text.chars().count();
    if character_count <= max_characters {
        draw_text(pixels, width, x, y, text, color, font);
        return;
    }
    let visible_characters = max_characters.saturating_sub(3);
    let mut clipped = text.chars().take(visible_characters).collect::<String>();
    clipped.push_str("...");
    draw_text(pixels, width, x, y, &clipped, color, font);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_synthesis_produces_valid_pcm_wav_with_requested_duration() {
        let wav = synthesize_sound(GpuSoundCommand {
            channel: 0,
            frequency: 440,
            frames: 60,
            waveform: 0,
            volume: 50,
        });
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 44_100);
        assert_eq!(wav.len(), 44 + 44_100);
    }

    #[test]
    fn looping_noise_synthesis_generates_half_second_of_pcm() {
        let wav = synthesize_sound(GpuSoundCommand {
            channel: 3,
            frequency: 220,
            frames: 0,
            waveform: 1,
            volume: 100,
        });
        assert_eq!(wav.len(), 44 + (SOUND_RATE as usize));
    }

    #[test]
    fn bios_selection_scroll_keeps_every_module_reachable() {
        let module_count = 37;
        let mut selected = 0;
        let mut offset = 0;
        for next in 1..module_count {
            selected = next;
            offset = bios_scroll_offset(selected, offset, module_count);
            assert!(selected >= offset);
            assert!(selected < offset + BIOS_VISIBLE_MODULES);
        }
        assert_eq!(selected, module_count - 1);
        assert_eq!(offset + BIOS_VISIBLE_MODULES, module_count);

        for previous in (0..module_count - 1).rev() {
            selected = previous;
            offset = bios_scroll_offset(selected, offset, module_count);
            assert!(selected >= offset);
            assert!(selected < offset + BIOS_VISIBLE_MODULES);
        }
        assert_eq!(selected, 0);
        assert_eq!(offset, 0);
    }

    #[test]
    fn bios_text_clipping_does_not_wrap_overflow_into_next_row() {
        let font = vec![[0xf8; 8]; 128];
        let mut pixels = vec![0; 20];
        draw_text(&mut pixels, 10, 8, 0, "A", 1, &font);
        assert_eq!(&pixels[..10], &[0, 0, 0, 0, 0, 0, 0, 0, 1, 1]);
        assert_eq!(&pixels[10..18], &[0; 8]);
        assert_eq!(&pixels[18..], &[1, 1]);
    }

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
