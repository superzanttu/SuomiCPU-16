use crate::cpu::{Cpu, GpuLine, GpuSoundCommand, GpuTriangle, VSYNC_ADDR};
use crate::image::{ImageError, load_program};
use crate::net::DEFAULT_PORT;
use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound, stop_sound};
use macroquad::prelude::{
    BLACK, Color, Conf, DrawTextureParams, FilterMode, KeyCode, MouseButton, Rect, Texture2D,
    WHITE, clear_background, draw_line, draw_rectangle, draw_text as draw_ui_text, draw_texture_ex,
    draw_triangle, get_char_pressed, get_keys_down, get_keys_pressed, is_key_down, is_key_pressed,
    is_mouse_button_pressed, is_mouse_button_released, mouse_position, mouse_wheel, next_frame,
    screen_height, screen_width, set_fullscreen, vec2,
};
#[cfg(not(target_arch = "wasm32"))]
use minifb::{Window as NativeWindow, WindowOptions};
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
const TOOLBAR_HEIGHT: f32 = 36.0;
const CONSOLE_WINDOW_WIDTH: usize = 480;
const CONSOLE_WINDOW_HEIGHT: usize = 200;
const TOOLBAR_BUTTONS: [(&str, f32, f32); 4] = [
    ("PAUSE", 8.0, 104.0),
    ("RESET", 120.0, 80.0),
    ("REBOOT", 208.0, 88.0),
    ("CONSOLE", 304.0, 96.0),
];

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

    fn stop_all(&mut self) {
        for active in &mut self.channels {
            if let Some(active) = active.take() {
                stop_sound(&active.sound);
            }
        }
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
        logical_size: (usize, usize),
        triangles: &[GpuTriangle],
        lines: &[GpuLine],
        palette: &[[u8; 3]; 256],
        toolbar: Option<ToolbarState>,
    ) {
        let (width, height) = logical_size;
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

        let toolbar_height = if toolbar.is_some() {
            TOOLBAR_HEIGHT
        } else {
            0.0
        };
        let available_height = (screen_height() - toolbar_height).max(1.0);
        let scale = (screen_width() / width as f32).min(available_height / height as f32);
        let destination = vec2(width as f32 * scale, height as f32 * scale);
        let position = vec2(
            (screen_width() - destination.x) / 2.0,
            toolbar_height + (available_height - destination.y) / 2.0,
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
        if let Some(toolbar) = toolbar {
            draw_toolbar(toolbar);
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

#[derive(Debug, PartialEq, Eq)]
enum BiosMouseAction {
    SelectModule(usize),
    PreviousDisk,
    NextDisk,
}

#[derive(Debug, PartialEq, Eq)]
enum RuntimeControl {
    PauseToggled(bool),
    FullscreenToggled(bool),
    ConsoleToggled(bool),
    ResetRequested,
    RebootRequested,
}

#[derive(Clone, Copy)]
struct ToolbarState {
    paused: bool,
}

#[cfg(not(target_arch = "wasm32"))]
struct ConsoleWindow {
    window: NativeWindow,
    pixels: Vec<u32>,
}

#[cfg(target_arch = "wasm32")]
struct ConsoleWindow;

impl ConsoleWindow {
    fn new() -> Result<Self, RuntimeError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let window = NativeWindow::new(
                "SC-16 Console",
                CONSOLE_WINDOW_WIDTH,
                CONSOLE_WINDOW_HEIGHT,
                WindowOptions {
                    resize: true,
                    ..WindowOptions::default()
                },
            )
            .map_err(|error| {
                RuntimeError::Window(format!("cannot open SC-16 console window: {error}"))
            })?;
            Ok(Self {
                window,
                pixels: vec![rgb(8, 12, 18); CONSOLE_WINDOW_WIDTH * CONSOLE_WINDOW_HEIGHT],
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            Ok(Self)
        }
    }

    fn is_open(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.window.is_open()
        }
        #[cfg(target_arch = "wasm32")]
        {
            true
        }
    }

    fn present(&mut self, lines: &[String], font: &[[u8; 8]]) -> Result<(), RuntimeError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            draw_console_lines(&mut self.pixels, lines, font);
            self.window
                .update_with_buffer(&self.pixels, CONSOLE_WINDOW_WIDTH, CONSOLE_WINDOW_HEIGHT)
                .map_err(|error| {
                    RuntimeError::Window(format!("cannot update SC-16 console window: {error}"))
                })?;
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (lines, font);
        Ok(())
    }
}

#[derive(Clone)]
struct BootTarget {
    source: PathBuf,
    disk_id: Option<String>,
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

    let font = load_font();
    let mut pixels = vec![0u32; 640 * 480];
    let mut canvas = Canvas::new();
    let mut audio = AudioEngine::new();
    let modules = if program.is_none() {
        Some(discover_modules(&root.join("modules"))?)
    } else {
        None
    };
    let mut cpu = fresh_runtime_cpu(root, program, disk_id, network_port, None)?;
    let mut target = if let Some(source) = program {
        let target = BootTarget {
            source: source.to_path_buf(),
            disk_id: disk_id.map(str::to_owned),
        };
        boot_target(&mut cpu, &target)?;
        Some(target)
    } else {
        let Some(target) = select_and_boot(
            root,
            &mut cpu,
            &mut canvas,
            &mut pixels,
            &font,
            modules.as_deref().expect("BIOS modules were loaded"),
            disk_id,
        )
        .await?
        else {
            return Ok(());
        };
        Some(target)
    };

    let mut old_keys = HashSet::new();
    let mut speed_remainder = 0.0;
    let mut last_frame = Instant::now();
    let mut presented_size = screen_size(&cpu);
    let mut presented_triangles = Vec::new();
    let mut presented_lines = Vec::new();
    let mut presented_palette = cpu.palette;
    let runtime_started = Instant::now();
    let mut paused = false;
    let mut console_visible = false;
    let mut console_window = None;
    let mut fullscreen = fullscreen;
    while !is_key_down(KeyCode::Escape) {
        let pressed_keys = get_keys_pressed();
        let keys = get_keys_down();
        let shifted = keys.contains(&KeyCode::LeftShift) || keys.contains(&KeyCode::RightShift);
        let controlled =
            keys.contains(&KeyCode::LeftControl) || keys.contains(&KeyCode::RightControl);
        let mut pending_action = None;
        let mut control_key_count = 0;
        let mut handled_control_keys = HashSet::new();
        for key in pressed_keys.iter().copied() {
            if let Some(control) = apply_runtime_control(
                key,
                &mut paused,
                &mut fullscreen,
                &mut console_visible,
                shifted,
                controlled,
            ) {
                control_key_count += 1;
                handled_control_keys.insert(key);
                match control {
                    RuntimeControl::FullscreenToggled(fullscreen) => set_fullscreen(fullscreen),
                    RuntimeControl::ResetRequested => {
                        pending_action = Some(RuntimeControl::ResetRequested)
                    }
                    RuntimeControl::RebootRequested => {
                        pending_action = Some(RuntimeControl::RebootRequested)
                    }
                    RuntimeControl::PauseToggled(paused) => {
                        cpu.console.write_text(if paused {
                            "Emulation paused"
                        } else {
                            "Emulation continued"
                        });
                    }
                    RuntimeControl::ConsoleToggled(_) => {}
                }
                last_frame = Instant::now();
            }
        }
        if !fullscreen
            && is_mouse_button_pressed(MouseButton::Left)
            && let Some(control) = toolbar_control(mouse_position(), paused, console_visible)
        {
            match control {
                RuntimeControl::PauseToggled(new_paused) => {
                    paused = new_paused;
                    cpu.console.write_text(if paused {
                        "Emulation paused"
                    } else {
                        "Emulation continued"
                    });
                }
                RuntimeControl::ResetRequested | RuntimeControl::RebootRequested => {
                    pending_action = Some(control);
                }
                RuntimeControl::ConsoleToggled(visible) => console_visible = visible,
                RuntimeControl::FullscreenToggled(_) => {}
            }
        }
        if fullscreen {
            console_visible = false;
            console_window = None;
        } else if console_visible && console_window.is_none() {
            console_window = Some(ConsoleWindow::new()?);
        } else if !console_visible {
            console_window = None;
        }
        if let Some(action @ (RuntimeControl::ResetRequested | RuntimeControl::RebootRequested)) =
            pending_action
        {
            audio.stop_all();
            let previous_console = cpu.console.clone();
            if action == RuntimeControl::ResetRequested
                && let Some(modules) = &modules
            {
                cpu = fresh_runtime_cpu(root, None, None, network_port, Some(&previous_console))?;
                let Some(new_target) = select_and_boot(
                    root,
                    &mut cpu,
                    &mut canvas,
                    &mut pixels,
                    &font,
                    modules,
                    target.as_ref().and_then(|target| target.disk_id.as_deref()),
                )
                .await?
                else {
                    return Ok(());
                };
                target = Some(new_target);
            } else {
                let Some(active_target) = target.as_ref() else {
                    return Ok(());
                };
                cpu = fresh_runtime_cpu(
                    root,
                    Some(&active_target.source),
                    active_target.disk_id.as_deref(),
                    network_port,
                    Some(&previous_console),
                )?;
                boot_target(
                    &mut cpu,
                    target.as_ref().expect("active program target exists"),
                )?;
            }
            cpu.console
                .write_text(if action == RuntimeControl::ResetRequested {
                    "System reset complete"
                } else {
                    "Program restarted"
                });
            paused = false;
            old_keys.clear();
            speed_remainder = 0.0;
            last_frame = Instant::now();
            continue;
        }
        let key_control_only = control_key_count == pressed_keys.len();
        if halted_window_should_close(cpu.running, !pressed_keys.is_empty() && !key_control_only) {
            break;
        }
        if cpu.running {
            update_keys(&mut cpu, &old_keys, &keys, shifted);
            old_keys = keys;
            let (width, height) = screen_size(&cpu);
            let toolbar_height = if fullscreen { 0.0 } else { TOOLBAR_HEIGHT };
            let logical_mouse = screen_to_logical_clamped_with_toolbar(
                mouse_position(),
                (screen_width(), screen_height()),
                (width, height),
                toolbar_height,
            );
            let wheel_delta = mouse_wheel().1.round() as i16;
            update_mouse(
                &mut cpu,
                logical_mouse,
                wheel_delta,
                [
                    is_mouse_button_pressed(MouseButton::Left),
                    is_mouse_button_pressed(MouseButton::Right),
                ],
                [
                    is_mouse_button_released(MouseButton::Left),
                    is_mouse_button_released(MouseButton::Right),
                ],
                runtime_started.elapsed().as_millis() as u64,
            );
            for key in pressed_keys.iter().copied() {
                if handled_control_keys.contains(&key) {
                    continue;
                }
                if key == KeyCode::Enter {
                    cpu.receive_console_input(b"\n");
                    cpu.signal_keyboard_event(13);
                } else if key == KeyCode::Backspace {
                    cpu.receive_console_input(b"\x08");
                    cpu.signal_keyboard_event(8);
                }
            }
            let typed_characters = std::iter::from_fn(get_char_pressed).collect::<Vec<_>>();
            receive_typed_text(&mut cpu, typed_characters);
            update_rtc(&mut cpu);
            cpu.begin_frame();
            if !paused {
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
                cpu.end_mouse_frame();
            } else {
                cpu.end_mouse_frame();
                last_frame = Instant::now();
            }
        }
        if should_capture_display_frame(cpu.memory[VSYNC_ADDR] != 0, cpu.frame_presented) {
            render_cpu(&cpu, &mut pixels);
            presented_size = screen_size(&cpu);
            presented_triangles.clone_from(&cpu.gpu_triangles);
            presented_lines.clone_from(&cpu.gpu_lines);
            presented_palette = cpu.palette;
        }
        let (width, height) = presented_size;
        if !cpu.running || paused {
            fill_rect(
                &mut pixels,
                width,
                0,
                height.saturating_sub(18),
                width,
                18,
                rgb(10, 14, 24),
            );
            let status = if paused {
                "PAUSED - F10 TO RESUME"
            } else {
                "PROGRAM HALTED - PRESS ANY KEY TO CLOSE"
            };
            draw_text_limited(
                &mut pixels,
                width,
                (4, height.saturating_sub(13)),
                status,
                width / 6,
                0xffff_d000,
                &font,
            );
        }
        if let Some(window) = console_window.as_mut() {
            if window.is_open() {
                window.present(&cpu.console.text_snapshot(), &font)?;
            } else {
                console_window = None;
                console_visible = false;
            }
        }
        canvas
            .present(
                &pixels,
                (width, height),
                &presented_triangles,
                &presented_lines,
                &presented_palette,
                (!fullscreen).then_some(ToolbarState { paused }),
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
    let image = load_program(program)?;
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
        let description = if let Some(path) = &source {
            let contents = fs::read_to_string(path)?;
            let line = contents
                .trim_start_matches('\u{feff}')
                .lines()
                .next()
                .unwrap_or("")
                .trim();
            let description = if let Some(line) = line.strip_prefix("//") {
                line.trim().to_owned()
            } else if let Some(line) = line.strip_prefix("/*") {
                line.strip_suffix("*/")
                    .unwrap_or(line)
                    .trim_start_matches('*')
                    .trim()
                    .to_owned()
            } else {
                line.to_owned()
            };
            if description.is_empty() {
                "No description".to_owned()
            } else {
                description
            }
        } else {
            error.clone().unwrap_or_else(|| "No description".to_owned())
        };
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
) -> Result<Option<BootTarget>, RuntimeError> {
    let mut selected = 0usize;
    let mut scroll_offset = 0usize;
    let mut last_clicked_module = None;
    let mut message = None;
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
            (selected, scroll_offset),
            &disks,
            selected_disk,
            message.as_deref(),
        );
        canvas
            .present(pixels, (640, 480), &[], &[], &cpu.palette, None)
            .await;
        let wheel_delta = mouse_wheel().1;
        if wheel_delta != 0.0
            && let Some((x, y)) = screen_to_logical(
                mouse_position(),
                (screen_width(), screen_height()),
                (640, 480),
            )
            && (32..608).contains(&x)
            && (BIOS_MODULE_ROW_TOP
                ..BIOS_MODULE_ROW_TOP + BIOS_VISIBLE_MODULES * BIOS_MODULE_ROW_HEIGHT)
                .contains(&y)
        {
            (selected, scroll_offset) = bios_scroll_modules(
                selected,
                scroll_offset,
                -(wheel_delta.signum() as isize) * 3,
                modules.len(),
            );
            last_clicked_module = None;
            message = None;
            continue;
        }
        let mut launch_selected = false;
        if is_mouse_button_pressed(MouseButton::Left)
            && let Some(action) = bios_mouse_action(
                mouse_position(),
                (screen_width(), screen_height()),
                modules.len(),
                scroll_offset,
            )
        {
            match action {
                BiosMouseAction::SelectModule(index) => {
                    selected = index;
                    scroll_offset = bios_scroll_offset(selected, scroll_offset, modules.len());
                    launch_selected =
                        bios_register_module_click(&mut last_clicked_module, index, Instant::now());
                    message = None;
                }
                BiosMouseAction::PreviousDisk => {
                    selected_disk = bios_cycle_disk(selected_disk, disks.len() + 1, false);
                    last_clicked_module = None;
                    message = None;
                }
                BiosMouseAction::NextDisk => {
                    selected_disk = bios_cycle_disk(selected_disk, disks.len() + 1, true);
                    last_clicked_module = None;
                    message = None;
                }
            }
            if !launch_selected {
                continue;
            }
        }
        if !launch_selected && is_key_pressed(KeyCode::Escape) {
            return Ok(None);
        } else if !launch_selected && is_key_pressed(KeyCode::Up) {
            selected = bios_move_selection(selected, -1, modules.len());
            scroll_offset = bios_scroll_offset(selected, scroll_offset, modules.len());
            message = None;
        } else if !launch_selected && is_key_pressed(KeyCode::Down) {
            selected = bios_move_selection(selected, 1, modules.len());
            scroll_offset = bios_scroll_offset(selected, scroll_offset, modules.len());
            message = None;
        } else if !launch_selected && (is_key_pressed(KeyCode::D) || is_key_pressed(KeyCode::Right))
        {
            selected_disk = bios_cycle_disk(selected_disk, disks.len() + 1, true);
            message = None;
        } else if !launch_selected && is_key_pressed(KeyCode::Left) {
            selected_disk = bios_cycle_disk(selected_disk, disks.len() + 1, false);
            message = None;
        } else if !launch_selected && !modules.is_empty() && is_key_pressed(KeyCode::Enter) {
            launch_selected = true;
        }
        if launch_selected {
            let Some(module) = modules.get(selected) else {
                continue;
            };
            if let Some(error) = &module.error {
                message = Some(format!("{}: {error}", module.name));
                last_clicked_module = None;
                continue;
            }
            let Some(source) = module.source.as_ref() else {
                message = Some(format!("{}: no source file found", module.name));
                continue;
            };
            let boot_result = (|| {
                configure_disk(cpu, root, source)?;
                if selected_disk != 0 {
                    cpu.mount_data_disk(&disks[selected_disk - 1])
                        .map_err(RuntimeError::Window)?;
                }
                let image = load_program(source)?;
                cpu.load_program(&image.segments, image.entry_point)
                    .map_err(|error| RuntimeError::Window(error.to_string()))?;
                Ok::<(), RuntimeError>(())
            })();
            if let Err(error) = boot_result {
                message = Some(format!("{}: {error}", module.name));
                continue;
            }
            return Ok(Some(BootTarget {
                source: source.clone(),
                disk_id: (selected_disk != 0).then(|| disks[selected_disk - 1].clone()),
            }));
        }
    }
}

fn draw_bios_selector(
    pixels: &mut [u32],
    font: &[[u8; 8]],
    modules: &[BootModule],
    (selected, scroll_offset): (usize, usize),
    disks: &[String],
    selected_disk: usize,
    message: Option<&str>,
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
    if let Some(message) = message {
        draw_text_limited(pixels, 640, (32, 465), message, 96, 0xffff6060, font);
    }
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

fn bios_scroll_modules(
    selected: usize,
    current_offset: usize,
    direction: isize,
    module_count: usize,
) -> (usize, usize) {
    if module_count == 0 {
        return (0, 0);
    }
    let max_offset = module_count.saturating_sub(BIOS_VISIBLE_MODULES);
    let current_offset = current_offset.min(max_offset);
    let offset = if direction < 0 {
        current_offset.saturating_sub(direction.unsigned_abs())
    } else {
        current_offset
            .saturating_add(direction as usize)
            .min(max_offset)
    };
    let selected = if selected < offset {
        offset
    } else if selected >= offset + BIOS_VISIBLE_MODULES {
        (offset + BIOS_VISIBLE_MODULES).min(module_count) - 1
    } else {
        selected.min(module_count - 1)
    };
    (selected, offset)
}

fn bios_move_selection(selected: usize, direction: isize, module_count: usize) -> usize {
    if module_count == 0 {
        return 0;
    }
    match direction.cmp(&0) {
        std::cmp::Ordering::Less => selected.min(module_count - 1).saturating_sub(1),
        std::cmp::Ordering::Equal => selected.min(module_count - 1),
        std::cmp::Ordering::Greater => selected.saturating_add(1).min(module_count - 1),
    }
}

fn bios_cycle_disk(selected: usize, choice_count: usize, forward: bool) -> usize {
    if choice_count == 0 {
        return 0;
    }
    let selected = selected % choice_count;
    if forward {
        if selected + 1 == choice_count {
            0
        } else {
            selected + 1
        }
    } else if selected == 0 {
        choice_count - 1
    } else {
        selected - 1
    }
}

fn bios_register_module_click(
    last_clicked_module: &mut Option<(usize, Instant)>,
    module_index: usize,
    now: Instant,
) -> bool {
    let is_double_click = last_clicked_module.is_some_and(|(previous_index, previous_time)| {
        previous_index == module_index
            && now.duration_since(previous_time) <= std::time::Duration::from_millis(400)
    });
    *last_clicked_module = if is_double_click {
        None
    } else {
        Some((module_index, now))
    };
    is_double_click
}

fn screen_to_logical(
    position: (f32, f32),
    screen_size: (f32, f32),
    logical_size: (usize, usize),
) -> Option<(usize, usize)> {
    if screen_size.0 <= 0.0 || screen_size.1 <= 0.0 || logical_size.0 == 0 || logical_size.1 == 0 {
        return None;
    }
    let scale = (screen_size.0 / logical_size.0 as f32).min(screen_size.1 / logical_size.1 as f32);
    let left = (screen_size.0 - logical_size.0 as f32 * scale) / 2.0;
    let top = (screen_size.1 - logical_size.1 as f32 * scale) / 2.0;
    let logical_x = (position.0 - left) / scale;
    let logical_y = (position.1 - top) / scale;
    if logical_x < 0.0
        || logical_y < 0.0
        || logical_x >= logical_size.0 as f32
        || logical_y >= logical_size.1 as f32
    {
        return None;
    }
    Some((logical_x as usize, logical_y as usize))
}

#[cfg(test)]
fn screen_to_logical_clamped(
    position: (f32, f32),
    screen_size: (f32, f32),
    logical_size: (usize, usize),
) -> Option<(usize, usize)> {
    if screen_size.0 <= 0.0 || screen_size.1 <= 0.0 || logical_size.0 == 0 || logical_size.1 == 0 {
        return None;
    }
    screen_to_logical_clamped_with_toolbar(position, screen_size, logical_size, 0.0)
}

fn screen_to_logical_clamped_with_toolbar(
    position: (f32, f32),
    screen_size: (f32, f32),
    logical_size: (usize, usize),
    toolbar_height: f32,
) -> Option<(usize, usize)> {
    if screen_size.0 <= 0.0
        || screen_size.1 <= toolbar_height
        || logical_size.0 == 0
        || logical_size.1 == 0
        || (toolbar_height > 0.0 && position.1 < toolbar_height)
    {
        return None;
    }
    let available_height = screen_size.1 - toolbar_height;
    let scale =
        (screen_size.0 / logical_size.0 as f32).min(available_height / logical_size.1 as f32);
    let left = (screen_size.0 - logical_size.0 as f32 * scale) / 2.0;
    let top = toolbar_height + (available_height - logical_size.1 as f32 * scale) / 2.0;
    let max_x = left + (logical_size.0 as f32 - 0.001) * scale;
    let max_y = top + (logical_size.1 as f32 - 0.001) * scale;
    let logical_x = (position.0.clamp(left, max_x) - left) / scale;
    let logical_y = (position.1.clamp(top, max_y) - top) / scale;
    Some((logical_x as usize, logical_y as usize))
}

fn bios_mouse_action(
    position: (f32, f32),
    screen_size: (f32, f32),
    module_count: usize,
    scroll_offset: usize,
) -> Option<BiosMouseAction> {
    let (x, y) = screen_to_logical(position, screen_size, (640, 480))?;
    if (420..604).contains(&x) && (73..100).contains(&y) {
        return Some(if x < 512 {
            BiosMouseAction::PreviousDisk
        } else {
            BiosMouseAction::NextDisk
        });
    }
    if !(32..608).contains(&x) || y < BIOS_MODULE_ROW_TOP {
        return None;
    }
    let row = (y - BIOS_MODULE_ROW_TOP) / BIOS_MODULE_ROW_HEIGHT;
    if row >= BIOS_VISIBLE_MODULES {
        return None;
    }
    let index = scroll_offset.saturating_add(row);
    (index < module_count).then_some(BiosMouseAction::SelectModule(index))
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

fn fresh_runtime_cpu(
    root: &Path,
    source: Option<&Path>,
    disk_id: Option<&str>,
    network_port: u16,
    previous_console: Option<&crate::console::ConsoleDevice>,
) -> Result<Cpu, RuntimeError> {
    let mut cpu = Cpu::new();
    cpu.set_network_port(network_port);
    if let Some(source) = source {
        configure_disk(&mut cpu, root, source)?;
    } else {
        cpu.configure_disk(root.join("disks"), root.join("disks"))
            .map_err(RuntimeError::Window)?;
    }
    if let Some(disk_id) = disk_id {
        cpu.mount_data_disk(disk_id).map_err(RuntimeError::Window)?;
    }
    if let Some(previous_console) = previous_console {
        cpu.console = previous_console.clone();
        cpu.console.reset(true);
    }
    Ok(cpu)
}

fn boot_target(cpu: &mut Cpu, target: &BootTarget) -> Result<(), RuntimeError> {
    let image = load_program(&target.source)?;
    cpu.load_program(&image.segments, image.entry_point)
        .map_err(|error| RuntimeError::Window(error.to_string()))
}

fn update_keys(
    cpu: &mut Cpu,
    previous: &HashSet<KeyCode>,
    current: &HashSet<KeyCode>,
    shifted: bool,
) {
    for (key, mask) in key_masks() {
        let was_down = previous.contains(&key);
        let is_down = current.contains(&key);
        if was_down != is_down {
            cpu.set_key(mask, is_down);
            if is_down && key != KeyCode::Enter {
                cpu.signal_keyboard_event(key_to_ascii(key, shifted).unwrap_or(0));
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

fn key_to_ascii(key: KeyCode, shifted: bool) -> Option<u8> {
    let byte = match key {
        KeyCode::A => b'a',
        KeyCode::B => b'b',
        KeyCode::C => b'c',
        KeyCode::D => b'd',
        KeyCode::E => b'e',
        KeyCode::F => b'f',
        KeyCode::G => b'g',
        KeyCode::H => b'h',
        KeyCode::I => b'i',
        KeyCode::J => b'j',
        KeyCode::K => b'k',
        KeyCode::L => b'l',
        KeyCode::M => b'm',
        KeyCode::N => b'n',
        KeyCode::O => b'o',
        KeyCode::P => b'p',
        KeyCode::Q => b'q',
        KeyCode::R => b'r',
        KeyCode::S => b's',
        KeyCode::T => b't',
        KeyCode::U => b'u',
        KeyCode::V => b'v',
        KeyCode::W => b'w',
        KeyCode::X => b'x',
        KeyCode::Y => b'y',
        KeyCode::Z => b'z',
        KeyCode::Key0 => {
            if shifted {
                b')'
            } else {
                b'0'
            }
        }
        KeyCode::Key1 => {
            if shifted {
                b'!'
            } else {
                b'1'
            }
        }
        KeyCode::Key2 => {
            if shifted {
                b'@'
            } else {
                b'2'
            }
        }
        KeyCode::Key3 => {
            if shifted {
                b'#'
            } else {
                b'3'
            }
        }
        KeyCode::Key4 => {
            if shifted {
                b'$'
            } else {
                b'4'
            }
        }
        KeyCode::Key5 => {
            if shifted {
                b'%'
            } else {
                b'5'
            }
        }
        KeyCode::Key6 => {
            if shifted {
                b'^'
            } else {
                b'6'
            }
        }
        KeyCode::Key7 => {
            if shifted {
                b'&'
            } else {
                b'7'
            }
        }
        KeyCode::Key8 => {
            if shifted {
                b'*'
            } else {
                b'8'
            }
        }
        KeyCode::Key9 => {
            if shifted {
                b'('
            } else {
                b'9'
            }
        }
        KeyCode::Space => b' ',
        KeyCode::Comma => {
            if shifted {
                b'<'
            } else {
                b','
            }
        }
        KeyCode::Period => {
            if shifted {
                b'>'
            } else {
                b'.'
            }
        }
        KeyCode::Minus => {
            if shifted {
                b'_'
            } else {
                b'-'
            }
        }
        KeyCode::Apostrophe => {
            if shifted {
                b'"'
            } else {
                b'\''
            }
        }
        KeyCode::Slash => {
            if shifted {
                b'?'
            } else {
                b'/'
            }
        }
        _ => return None,
    };
    Some(if shifted && byte.is_ascii_lowercase() {
        byte.to_ascii_uppercase()
    } else {
        byte
    })
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

fn halted_window_should_close(cpu_running: bool, has_keypress: bool) -> bool {
    !cpu_running && has_keypress
}

fn update_mouse(
    cpu: &mut Cpu,
    position: Option<(usize, usize)>,
    wheel_delta: i16,
    pressed: [bool; 2],
    released: [bool; 2],
    now_ms: u64,
) {
    if wheel_delta != 0 {
        cpu.set_mouse_wheel(wheel_delta);
    }
    let Some((x, y)) = position else {
        return;
    };
    let (x, y) = (x as u16, y as u16);
    cpu.set_mouse_position(x, y);
    for button in 0..2 {
        if pressed[button] {
            cpu.mouse_button(button, true, x, y, now_ms);
        }
        if released[button] {
            cpu.mouse_button(button, false, x, y, now_ms);
        }
    }
}

fn receive_typed_text(cpu: &mut Cpu, characters: impl IntoIterator<Item = char>) {
    let text = characters.into_iter().collect::<String>();
    if !text.is_empty() {
        cpu.receive_console_input(text.as_bytes());
    }
}

fn apply_runtime_control(
    key: KeyCode,
    paused: &mut bool,
    fullscreen: &mut bool,
    console_visible: &mut bool,
    shifted: bool,
    controlled: bool,
) -> Option<RuntimeControl> {
    match key {
        KeyCode::F10 => {
            *paused = !*paused;
            Some(RuntimeControl::PauseToggled(*paused))
        }
        KeyCode::F11 => {
            *fullscreen = !*fullscreen;
            Some(RuntimeControl::FullscreenToggled(*fullscreen))
        }
        KeyCode::F if shifted && controlled => {
            *fullscreen = !*fullscreen;
            Some(RuntimeControl::FullscreenToggled(*fullscreen))
        }
        KeyCode::F2 if !*fullscreen => {
            *console_visible = !*console_visible;
            Some(RuntimeControl::ConsoleToggled(*console_visible))
        }
        _ => None,
    }
}

fn toolbar_control(
    position: (f32, f32),
    paused: bool,
    console_visible: bool,
) -> Option<RuntimeControl> {
    let (x, y) = position;
    if !(5.0..31.0).contains(&y) {
        return None;
    }
    for (index, (_, left, width)) in TOOLBAR_BUTTONS.iter().enumerate() {
        if (*left..*left + *width).contains(&x) {
            return match index {
                0 => Some(RuntimeControl::PauseToggled(!paused)),
                1 => Some(RuntimeControl::ResetRequested),
                2 => Some(RuntimeControl::RebootRequested),
                3 => Some(RuntimeControl::ConsoleToggled(!console_visible)),
                _ => None,
            };
        }
    }
    None
}

fn draw_toolbar(state: ToolbarState) {
    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        TOOLBAR_HEIGHT,
        Color::from_rgba(12, 16, 24, 255),
    );
    for (index, (label, x, width)) in TOOLBAR_BUTTONS.iter().enumerate() {
        let label = if index == 0 && state.paused {
            "CONTINUE"
        } else {
            label
        };
        let active = index == 0 && state.paused;
        draw_rectangle(
            *x,
            5.0,
            *width,
            26.0,
            if active {
                Color::from_rgba(43, 95, 70, 255)
            } else {
                Color::from_rgba(43, 54, 72, 255)
            },
        );
        draw_ui_text(
            label,
            *x + 8.0,
            23.0,
            14.0,
            Color::from_rgba(238, 242, 248, 255),
        );
    }
}

fn draw_console_lines(pixels: &mut [u32], lines: &[String], font: &[[u8; 8]]) {
    pixels.fill(rgb(8, 12, 18));
    for (row, line) in lines
        .iter()
        .skip(lines.len().saturating_sub(CONSOLE_WINDOW_HEIGHT / 8))
        .enumerate()
    {
        draw_text(
            pixels,
            CONSOLE_WINDOW_WIDTH,
            0,
            row * 8,
            line,
            rgb(190, 220, 190),
            font,
        );
    }
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

fn should_capture_display_frame(vsync_enabled: bool, frame_presented: bool) -> bool {
    !vsync_enabled || frame_presented
}

const fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    ((red as u32) << 16) | ((green as u32) << 8) | blue as u32
}

fn load_font() -> Vec<[u8; 8]> {
    let bytes = include_str!("../fonts/font5x7.asm")
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
    fn console_window_renders_snapshot_with_small_font_in_a_separate_framebuffer() {
        let mut pixels = vec![0; CONSOLE_WINDOW_WIDTH * CONSOLE_WINDOW_HEIGHT];
        let font = load_font();
        let lines = (0..30)
            .map(|line| format!("line-{line:02}"))
            .collect::<Vec<_>>();
        draw_console_lines(&mut pixels, &lines, &font);
        let mut expected_first_row =
            vec![rgb(8, 12, 18); CONSOLE_WINDOW_WIDTH * CONSOLE_WINDOW_HEIGHT];
        draw_text(
            &mut expected_first_row,
            CONSOLE_WINDOW_WIDTH,
            0,
            0,
            "line-05",
            rgb(190, 220, 190),
            &font,
        );

        assert_eq!(pixels.len(), 480 * 200);
        assert_eq!(pixels[CONSOLE_WINDOW_WIDTH - 1], rgb(8, 12, 18));
        assert_eq!(
            &pixels[..CONSOLE_WINDOW_WIDTH * 8],
            &expected_first_row[..CONSOLE_WINDOW_WIDTH * 8]
        );
    }

    #[test]
    fn bios_module_discovery_sorts_selects_main_and_reads_descriptions() {
        let directory = std::env::temp_dir().join(format!(
            "sc16-bios-discovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let modules = directory.join("modules");
        fs::create_dir_all(modules.join("Alpha")).unwrap();
        fs::create_dir_all(modules.join("broken")).unwrap();
        fs::create_dir_all(modules.join("empty")).unwrap();
        fs::create_dir_all(modules.join("valid")).unwrap();
        fs::write(modules.join("ignored.c"), "not a module").unwrap();
        fs::write(modules.join("Alpha").join("alpha.c"), "// Alpha program\n").unwrap();
        fs::write(modules.join("broken").join("first.c"), "// First\n").unwrap();
        fs::write(modules.join("broken").join("second.C"), "// Second\n").unwrap();
        fs::write(
            modules.join("valid").join("main.C"),
            "\u{feff}/* Main program */\nint main(void) { return 0; }\n",
        )
        .unwrap();
        fs::write(modules.join("valid").join("helper.c"), "// Helper\n").unwrap();

        let found = discover_modules(&modules).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|module| module.name.as_str())
                .collect::<Vec<_>>(),
            ["Alpha", "broken", "empty", "valid"]
        );
        assert_eq!(found[0].description, "Alpha program");
        assert_eq!(
            found[0].source.as_deref(),
            Some(modules.join("Alpha").join("alpha.c").as_path())
        );
        assert_eq!(
            found[1].error.as_deref(),
            Some("Multiple .c files found; expected main.c.")
        );
        assert_eq!(
            found[2].error.as_deref(),
            Some("No .c files found in module folder.")
        );
        assert_eq!(found[3].description, "Main program",);
        assert_eq!(
            found[3].source.as_deref(),
            Some(modules.join("valid").join("main.C").as_path())
        );
        assert!(found[3].error.is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn bios_module_discovery_reports_missing_module_directory() {
        let missing = std::env::temp_dir()
            .join(format!("sc16-bios-missing-{}", std::process::id()))
            .join("modules");
        assert!(discover_modules(&missing).is_err());
    }

    #[test]
    fn display_key_mappings_update_all_sc16_key_bytes_and_release_cleanly() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        let mut previous = HashSet::new();
        let current = HashSet::from([
            KeyCode::A,
            KeyCode::Tab,
            KeyCode::Q,
            KeyCode::X,
            KeyCode::C,
            KeyCode::Period,
        ]);
        update_keys(&mut cpu, &previous, &current, false);
        assert_eq!(&cpu.memory[0x43004..0x43007], &[0x41, 0x05, 0x21]);

        previous = current;
        let current = HashSet::from([KeyCode::D, KeyCode::E, KeyCode::Z, KeyCode::J]);
        update_keys(&mut cpu, &previous, &current, false);
        assert_eq!(&cpu.memory[0x43004..0x43007], &[0x02, 0x0a, 0x02]);
    }

    #[test]
    fn display_text_input_maps_shifted_letters_digits_and_punctuation() {
        for (key, unshifted, shifted) in [
            (KeyCode::A, b'a', b'A'),
            (KeyCode::Key1, b'1', b'!'),
            (KeyCode::Key0, b'0', b')'),
            (KeyCode::Comma, b',', b'<'),
            (KeyCode::Period, b'.', b'>'),
            (KeyCode::Minus, b'-', b'_'),
            (KeyCode::Apostrophe, b'\'', b'"'),
            (KeyCode::Slash, b'/', b'?'),
        ] {
            assert_eq!(key_to_ascii(key, false), Some(unshifted));
            assert_eq!(key_to_ascii(key, true), Some(shifted));
        }
        assert_eq!(key_to_ascii(KeyCode::Space, true), Some(b' '));
        assert_eq!(key_to_ascii(KeyCode::F10, false), None);
    }

    #[test]
    fn display_text_input_preserves_unicode_characters_for_console_mmio() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        receive_typed_text(&mut cpu, ['k', 'ä', '!', '🙂']);
        assert_eq!(
            cpu.console
                .receive_buffer()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            "kä!🙂".as_bytes()
        );
    }

    #[test]
    fn display_rate_budget_accumulates_fractional_frames_without_negative_work() {
        let mut remainder = 0.0;
        assert_eq!(rate_budget(10.0, 0.05, &mut remainder), 0);
        assert_eq!(rate_budget(10.0, 0.05, &mut remainder), 1);
        assert_eq!(rate_budget(10.0, -1.0, &mut remainder), 0);
        assert_eq!(remainder, 0.0);
    }

    #[test]
    fn halted_display_waits_for_a_keypress_without_closing_a_running_cpu() {
        assert!(!halted_window_should_close(false, false));
        assert!(halted_window_should_close(false, true));
        assert!(!halted_window_should_close(true, true));
    }

    #[test]
    fn runtime_pause_and_fullscreen_controls_toggle_independently() {
        let (mut paused, mut fullscreen, mut console_visible) = (false, false, false);
        assert_eq!(
            apply_runtime_control(
                KeyCode::F10,
                &mut paused,
                &mut fullscreen,
                &mut console_visible,
                false,
                false,
            ),
            Some(RuntimeControl::PauseToggled(true))
        );
        assert!(paused);
        assert!(!fullscreen);
        assert_eq!(
            apply_runtime_control(
                KeyCode::F11,
                &mut paused,
                &mut fullscreen,
                &mut console_visible,
                false,
                false,
            ),
            Some(RuntimeControl::FullscreenToggled(true))
        );
        assert_eq!(
            apply_runtime_control(
                KeyCode::F10,
                &mut paused,
                &mut fullscreen,
                &mut console_visible,
                false,
                false,
            ),
            Some(RuntimeControl::PauseToggled(false))
        );
        assert!(!paused);
        assert!(fullscreen);
        assert_eq!(
            apply_runtime_control(
                KeyCode::A,
                &mut paused,
                &mut fullscreen,
                &mut console_visible,
                false,
                false,
            ),
            None
        );
    }

    #[test]
    fn runtime_shift_control_f_toggles_fullscreen_and_f2_only_opens_windowed_console() {
        let (mut paused, mut fullscreen, mut console_visible) = (false, false, false);
        assert_eq!(
            apply_runtime_control(
                KeyCode::F,
                &mut paused,
                &mut fullscreen,
                &mut console_visible,
                true,
                true,
            ),
            Some(RuntimeControl::FullscreenToggled(true))
        );
        assert_eq!(
            apply_runtime_control(
                KeyCode::F2,
                &mut paused,
                &mut fullscreen,
                &mut console_visible,
                false,
                false,
            ),
            None
        );
        assert!(!console_visible);

        assert_eq!(
            apply_runtime_control(
                KeyCode::F,
                &mut paused,
                &mut fullscreen,
                &mut console_visible,
                true,
                true,
            ),
            Some(RuntimeControl::FullscreenToggled(false))
        );
        assert_eq!(
            apply_runtime_control(
                KeyCode::F2,
                &mut paused,
                &mut fullscreen,
                &mut console_visible,
                false,
                false,
            ),
            Some(RuntimeControl::ConsoleToggled(true))
        );
        assert!(console_visible);
    }

    #[test]
    fn runtime_toolbar_controls_hit_expected_buttons_and_ignore_gaps() {
        assert_eq!(
            toolbar_control((50.0, 18.0), false, false),
            Some(RuntimeControl::PauseToggled(true))
        );
        assert_eq!(
            toolbar_control((150.0, 18.0), false, false),
            Some(RuntimeControl::ResetRequested)
        );
        assert_eq!(
            toolbar_control((250.0, 18.0), false, false),
            Some(RuntimeControl::RebootRequested)
        );
        assert_eq!(
            toolbar_control((350.0, 18.0), false, false),
            Some(RuntimeControl::ConsoleToggled(true))
        );
        assert_eq!(toolbar_control((115.0, 18.0), false, false), None);
        assert_eq!(toolbar_control((50.0, 34.0), false, false), None);
    }

    #[test]
    fn runtime_mouse_input_updates_cpu_registers_and_latches_click_events() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        update_mouse(
            &mut cpu,
            Some((40, 30)),
            -1,
            [true, false],
            [false, false],
            1000,
        );
        assert_eq!(cpu.read_u16(0x43008), 40);
        assert_eq!(cpu.read_u16(0x4300a), 30);
        assert_eq!(cpu.memory[0x4300c], 1);
        assert_eq!(cpu.read_u16(0x4300e), u16::MAX);

        update_mouse(
            &mut cpu,
            Some((41, 30)),
            0,
            [false, false],
            [true, false],
            1050,
        );
        update_mouse(
            &mut cpu,
            Some((41, 30)),
            2,
            [true, false],
            [false, false],
            1200,
        );
        assert_eq!(cpu.memory[0x4300c], 1);
        assert_eq!(cpu.read(0x4300d), 1 | 4 | 16);
        assert_eq!(cpu.read(0x4300d), 0);
        assert_eq!(cpu.read_u16(0x4300e), 1);
        cpu.end_mouse_frame();
        assert_eq!(cpu.read_u16(0x4300e), 0);
    }

    #[test]
    fn display_renderer_maps_vram_palette_and_resolution_to_rgb_pixels() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.memory[0x50000] = 1;
        cpu.memory[0x50001] = 25;
        cpu.palette[1] = [255, 255, 255];
        cpu.palette[2] = [0, 0, 255];
        cpu.palette[25] = [0, 0, 255];
        let mut pixels = vec![0; 640 * 480];

        render_cpu(&cpu, &mut pixels);
        assert_eq!(pixels[0], 0x00ff_ffff);
        assert_eq!(pixels[1], 0x0000_00ff);
        assert_eq!(pixels.len(), 640 * 480);

        cpu.memory[GFX_MODE_ADDR] = 1;
        cpu.memory[0x50000] = 0x12;
        let mut high_resolution_pixels = vec![0; 640 * 480];
        render_cpu(&cpu, &mut high_resolution_pixels);
        assert_eq!(high_resolution_pixels[0], 0x00ff_ffff);
        assert_eq!(high_resolution_pixels[1], 0x0000_00ff);
        assert_eq!(screen_size(&cpu), (640, 480));

        cpu.memory[GFX_MODE_ADDR] = 0;
        assert_eq!(screen_size(&cpu), (320, 240));
    }

    #[test]
    fn vsync_refreshes_only_after_a_gpu_present_command() {
        assert!(should_capture_display_frame(false, false));
        assert!(should_capture_display_frame(false, true));
        assert!(!should_capture_display_frame(true, false));
        assert!(should_capture_display_frame(true, true));
    }

    #[test]
    fn game_frame_integrates_gpu_mmio_layer_compositing_and_palette_rendering() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.memory[0x50000] = 2;
        cpu.memory[0x50001] = 2;
        cpu.palette[2] = [12, 34, 56];
        cpu.palette[3] = [78, 90, 123];

        cpu.write(0x43030, 1);
        cpu.write_u16(0x43022, 1);
        cpu.write_u16(0x43024, 0);
        cpu.write(0x43021, 3);
        cpu.write(0x43020, 2);

        let mut pixels = vec![0; 640 * 480];
        render_cpu(&cpu, &mut pixels);
        assert_eq!(pixels[0], rgb(12, 34, 56));
        assert_eq!(pixels[1], rgb(78, 90, 123));

        cpu.write(0x43031, 1);
        render_cpu(&cpu, &mut pixels);
        assert_eq!(pixels[1], rgb(12, 34, 56));
    }

    #[test]
    fn game_frame_submits_projected_3d_fill_and_wireframe_geometry() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.memory[0x43014] = 1;
        let scene = [
            3, 1, 0, 0, 0, 0, 0, 1000, 10000, 3, -1000, -1000, 0, 0, 1000, 0, 1000, -1000, 0, 0, 1,
            2, 2,
        ];
        for (index, value) in scene.into_iter().enumerate() {
            cpu.write_u16(0x1000 + (index * 2) as u32, value as u16);
        }
        cpu.write_u16(0x4302a, 0x1000);
        cpu.write(0x43020, 22);

        assert_eq!(
            cpu.gpu_triangles,
            [GpuTriangle {
                points: [(280, 280), (320, 201), (359, 280)],
                color: 2,
            }]
        );
        assert_eq!(cpu.gpu_lines.len(), 3);
        assert_eq!(
            cpu.gpu_lines[0],
            GpuLine {
                start: (280, 280),
                end: (320, 201),
                color: 2,
            }
        );
    }

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
    fn bios_wheel_scroll_moves_three_rows_and_keeps_selection_visible() {
        assert_eq!(bios_scroll_modules(0, 0, 3, 40), (3, 3));
        assert_eq!(bios_scroll_modules(3, 3, -3, 40), (3, 0));
        assert_eq!(bios_scroll_modules(0, 0, 99, 40), (28, 28));
        assert_eq!(bios_scroll_modules(2, 0, 3, 4), (2, 0));
        assert_eq!(bios_scroll_modules(0, 0, 3, 0), (0, 0));
    }

    #[test]
    fn bios_keyboard_selection_clamps_and_disk_selection_wraps() {
        assert_eq!(bios_move_selection(0, -1, 5), 0);
        assert_eq!(bios_move_selection(0, 1, 5), 1);
        assert_eq!(bios_move_selection(4, 1, 5), 4);
        assert_eq!(bios_move_selection(4, -1, 5), 3);
        assert_eq!(bios_move_selection(99, 0, 5), 4);
        assert_eq!(bios_move_selection(99, 1, 0), 0);

        assert_eq!(bios_cycle_disk(0, 3, false), 2);
        assert_eq!(bios_cycle_disk(0, 3, true), 1);
        assert_eq!(bios_cycle_disk(2, 3, true), 0);
        assert_eq!(bios_cycle_disk(2, 3, false), 1);
        assert_eq!(bios_cycle_disk(99, 0, true), 0);
    }

    #[test]
    fn fullscreen_pointer_coordinates_preserve_aspect_ratio_and_reject_bars() {
        assert_eq!(
            screen_to_logical((240.0, 0.0), (1920.0, 1080.0), (640, 480)),
            Some((0, 0))
        );
        assert_eq!(
            screen_to_logical((100.0, 0.0), (1920.0, 1080.0), (640, 480)),
            None
        );
        assert_eq!(
            screen_to_logical((960.0, 540.0), (1920.0, 1080.0), (640, 480)),
            Some((320, 240))
        );
        assert_eq!(
            screen_to_logical((1679.9, 1079.9), (1920.0, 1080.0), (640, 480)),
            Some((639, 479))
        );
        assert_eq!(
            screen_to_logical((480.0, 360.0), (960.0, 720.0), (640, 480)),
            Some((320, 240))
        );
    }

    #[test]
    fn guest_mouse_coordinates_clamp_letterbox_and_outside_positions_to_edges() {
        assert_eq!(
            screen_to_logical_clamped((-10.0, -10.0), (1920.0, 1080.0), (320, 240)),
            Some((0, 0))
        );
        assert_eq!(
            screen_to_logical_clamped((1920.0, 1080.0), (1920.0, 1080.0), (320, 240)),
            Some((319, 239))
        );
        assert_eq!(
            screen_to_logical_clamped((100.0, 540.0), (1920.0, 1080.0), (320, 240)),
            Some((0, 120))
        );
        assert_eq!(
            screen_to_logical_clamped_with_toolbar(
                (480.0, 30.0),
                (960.0, 756.0),
                (640, 480),
                TOOLBAR_HEIGHT,
            ),
            None
        );
        assert_eq!(
            screen_to_logical_clamped_with_toolbar(
                (480.0, 36.0),
                (960.0, 756.0),
                (640, 480),
                TOOLBAR_HEIGHT,
            ),
            Some((320, 0))
        );
    }

    #[test]
    fn bios_mouse_selects_visible_modules_and_cycles_data_disk() {
        assert_eq!(
            bios_mouse_action((150.0, 140.0), (640.0, 480.0), 20, 2),
            Some(BiosMouseAction::SelectModule(2))
        );
        assert_eq!(
            bios_mouse_action((150.0, 164.0), (640.0, 480.0), 20, 2),
            Some(BiosMouseAction::SelectModule(3))
        );
        assert_eq!(
            bios_mouse_action((500.0, 80.0), (640.0, 480.0), 20, 0),
            Some(BiosMouseAction::PreviousDisk)
        );
        assert_eq!(
            bios_mouse_action((550.0, 80.0), (640.0, 480.0), 20, 0),
            Some(BiosMouseAction::NextDisk)
        );
        assert_eq!(
            bios_mouse_action((10.0, 140.0), (640.0, 480.0), 20, 0),
            None
        );
        assert_eq!(
            bios_mouse_action((150.0, 420.0), (640.0, 480.0), 20, 0),
            None
        );
        assert_eq!(
            bios_mouse_action((150.0, 140.0), (640.0, 480.0), 2, 2),
            None
        );
    }

    #[test]
    fn bios_module_double_click_requires_same_module_within_400_milliseconds() {
        let start = Instant::now();
        let mut last_click = None;
        assert!(!bios_register_module_click(&mut last_click, 2, start));
        assert!(bios_register_module_click(
            &mut last_click,
            2,
            start + std::time::Duration::from_millis(400)
        ));

        assert!(!bios_register_module_click(
            &mut last_click,
            2,
            start + std::time::Duration::from_millis(401)
        ));
        assert!(!bios_register_module_click(
            &mut last_click,
            3,
            start + std::time::Duration::from_millis(500)
        ));
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
    fn bios_selector_renders_empty_state_and_persistent_error_message() {
        let font = vec![[0xf8; 8]; 128];
        let mut pixels = vec![0; 640 * 480];
        draw_bios_selector(&mut pixels, &font, &[], (0, 0), &[], 0, None);
        assert_eq!(pixels[0], rgb(7, 10, 20));

        draw_bios_selector(
            &mut pixels,
            &font,
            &[],
            (0, 0),
            &[],
            0,
            Some("broken: compiler rejected module"),
        );
        assert!(pixels.contains(&0xffff_6060));
    }

    #[test]
    fn rust_compiler_loads_ana_clock_module() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let program = root.join("modules").join("AnaClock").join("anaclock.c");
        let image = load_program(&program).expect("Rust C compiler should load AnaClock");
        assert_eq!(image.entry_point, 0);
        assert!(!image.segments.is_empty());
    }

    #[test]
    fn infinite_city_keeps_running_across_rendered_frames() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let program = root
            .join("modules")
            .join("Infinite CIty")
            .join("infinite_city.c");
        let image = load_program(&program).expect("Infinite City should compile and load");
        let mut cpu = Cpu::new();
        cpu.load_program(&image.segments, image.entry_point)
            .expect("Infinite City should load");
        cpu.set_key(2, true);

        for frame in 0..120 {
            cpu.begin_frame();
            for _ in 0..1_000_000 {
                cpu.step()
                    .expect("Infinite City instruction should execute");
                if !cpu.running {
                    panic!("Infinite City halted during rendered frame {frame}");
                }
                if cpu.frame_yield {
                    break;
                }
            }
            assert!(
                cpu.frame_yield,
                "Infinite City did not present frame {frame}"
            );
        }
    }

    #[test]
    fn headless_runtime_executes_retained_assembler_examples() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for name in ["commands.asm", "directives.asm", "example.asm"] {
            let program = root.join("examples").join(name);
            let count = run_headless(root, &program, 50_000).unwrap_or_else(|error| {
                panic!("{name} failed through the headless runtime: {error}")
            });
            assert!(count > 0, "{name} executed no instructions");
        }
    }

    #[test]
    fn headless_runtime_reports_instruction_budget_exhaustion() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let program = root.join("examples").join("commands.asm");

        let error = run_headless(root, &program, 1).unwrap_err();

        assert!(error.to_string().contains("instruction budget exhausted"));
    }

    #[test]
    fn headless_runtime_loads_and_executes_supported_c_source() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let directory = std::env::temp_dir().join(format!(
            "sc16-headless-c-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(directory.join("disks")).unwrap();
        let source = directory.join("minimal.c");
        fs::write(&source, "int main(void) { return 6 * 7; }\n").unwrap();

        let result = run_headless(root, &source, 64);
        fs::remove_dir_all(&directory).unwrap();

        let executed = result.expect("supported C source should run headlessly");
        assert!(executed > 0);
    }
}
