//! Runs the host-facing SC-16 emulator environment.
//! Handles BIOS and program startup, windowed/headless execution, rendering,
//! input, audio, debug-monitor windows, and runtime integration tests.

use crate::cpu::{BIOS_AUTOBOOT_WAIT_MARKER, BIOS_FIRMWARE_MARKER, Cpu, PrivilegeMode};
use crate::disk::{FolderBootMode, FolderDisk};
use crate::image::{ImageError, load_prebuilt_program};
use crate::net::DEFAULT_PORT;
use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound, stop_sound};
use macroquad::prelude::{
    BLACK, Color, Conf, DrawTextureParams, FilterMode, KeyCode, MouseButton, Rect, Texture2D,
    WHITE, clear_background, draw_rectangle, draw_text as draw_ui_text, draw_texture_ex,
    get_char_pressed, get_keys_down, get_keys_pressed, is_key_pressed, is_mouse_button_pressed,
    is_mouse_button_released, is_quit_requested, mouse_position, mouse_wheel, next_frame,
    screen_height, screen_width, set_fullscreen, vec2,
};
#[cfg(not(target_arch = "wasm32"))]
use minifb::{
    Key as NativeKey, KeyRepeat, MouseButton as NativeMouseButton, MouseMode,
    Window as NativeWindow, WindowOptions,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SOUND_RATE: u32 = 22_050;
const SOUND_FPS: u32 = 60;
const TOOLBAR_HEIGHT: f32 = 36.0;
const CONSOLE_WINDOW_WIDTH: usize = 480;
const CONSOLE_WINDOW_HEIGHT: usize = 200;
const CONSOLE_TOOLBAR_HEIGHT: usize = 28;
const RUNTIME_INSTRUCTION_CHUNK: usize = 5_000;
const RUNTIME_CPU_SLICE: Duration = Duration::from_millis(30);
const BUG_MONITOR_BUTTON: (f32, f32) = (8.0, 112.0);
const CONSOLE_CLOSE_BUTTON: (usize, usize) = (6, 104);
const BIOS_TEXT_COLUMNS: usize = 53;
const BIOS_TEXT_ROWS: usize = 25;
const BIOS_TEXT_CELL_WIDTH: usize = 12;
const BIOS_TEXT_CELL_HEIGHT: usize = 16;
const BIOS_TEXT_LEFT: usize = (640 - BIOS_TEXT_COLUMNS * BIOS_TEXT_CELL_WIDTH) / 2;
const BIOS_TEXT_TOP: usize = (480 - BIOS_TEXT_ROWS * BIOS_TEXT_CELL_HEIGHT) / 2;
#[cfg(test)]
const BIOS_MODULE_ROW_TOP: usize = BIOS_TEXT_TOP + 6 * BIOS_TEXT_CELL_HEIGHT;
#[cfg(test)]
const BIOS_MODULE_ROW_HEIGHT: usize = BIOS_TEXT_CELL_HEIGHT;
#[cfg(test)]
const BIOS_VISIBLE_MODULES: usize = 12;
const BIOS_PHOSPHOR: u32 = 0xff75ff9a;
const BIOS_PHOSPHOR_BRIGHT: u32 = 0xffc8ffd5;

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
    command: crate::machine::SoundCommand,
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

    async fn apply(&mut self, command: crate::machine::SoundCommand) -> Result<(), RuntimeError> {
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

fn synthesize_sound(command: crate::machine::SoundCommand) -> Vec<u8> {
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
        toolbar_visible: bool,
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

        let toolbar_height = if toolbar_visible { TOOLBAR_HEIGHT } else { 0.0 };
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
        if toolbar_visible {
            draw_toolbar();
        }
        next_frame().await;
    }
}

#[cfg(test)]
#[derive(Clone)]
struct BootDisk {
    name: String,
    path: PathBuf,
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
enum BiosMouseAction {
    SelectDisk(usize),
}

#[derive(Debug, PartialEq, Eq)]
enum RuntimeControl {
    FullscreenToggled(bool),
    ConsoleToggled(bool),
    ConsoleClosed,
}

#[cfg(not(target_arch = "wasm32"))]
struct ConsoleWindow {
    window: NativeWindow,
    pixels: Vec<u32>,
    left_mouse_down: bool,
}

#[cfg(target_arch = "wasm32")]
struct ConsoleWindow;

impl ConsoleWindow {
    fn new() -> Result<Self, RuntimeError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let window = NativeWindow::new(
                "SC-16 Bug Monitor | F2 close",
                CONSOLE_WINDOW_WIDTH,
                CONSOLE_WINDOW_HEIGHT,
                WindowOptions {
                    resize: true,
                    ..WindowOptions::default()
                },
            )
            .map_err(|error| {
                RuntimeError::Window(format!("cannot open SC-16 Bug Monitor window: {error}"))
            })?;
            Ok(Self {
                window,
                pixels: vec![rgb(8, 12, 18); CONSOLE_WINDOW_WIDTH * CONSOLE_WINDOW_HEIGHT],
                left_mouse_down: false,
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

    fn runtime_control_pressed(&mut self) -> Option<RuntimeControl> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if !self.window.is_open() {
                return Some(RuntimeControl::ConsoleClosed);
            }
            self.window.update();
            if !self.window.is_open() {
                return Some(RuntimeControl::ConsoleClosed);
            }
            let keyboard_control = self
                .window
                .is_key_pressed(NativeKey::F2, KeyRepeat::No)
                .then_some(RuntimeControl::ConsoleToggled(false));
            let mouse_down = self.window.get_mouse_down(NativeMouseButton::Left);
            let mouse_clicked = mouse_down && !self.left_mouse_down;
            self.left_mouse_down = mouse_down;
            keyboard_control.or_else(|| {
                if mouse_clicked {
                    self.window
                        .get_mouse_pos(MouseMode::Clamp)
                        .and_then(|position| console_toolbar_control(position))
                } else {
                    None
                }
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            None
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

// ...existing code...
#[derive(Clone)]
struct BootTarget {
    source: PathBuf,
    folder_disk: bool,
}

pub fn window_conf() -> Conf {
    // ...existing code...
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
    let mut framebuffer = Vec::new();
    let mut canvas = Canvas::new();
    let mut audio = AudioEngine::new();
    let mut cpu = match fresh_runtime_cpu(root, program, disk_id, network_port) {
        Ok(cpu) => cpu,
        Err(error) => {
            show_bios_startup_error(
                &error.to_string(),
                &font,
                &mut pixels,
                &mut canvas,
                fullscreen,
            )
            .await;
            return Ok(());
        }
    };
    cpu.debug_console.log_info("emulator", "BIOS started");
    let mut console_visible = false;
    let mut console_window: Option<ConsoleWindow> = None;
    let mut fullscreen = fullscreen;
    if let Some(source) = program {
        let target = BootTarget {
            source: source.to_path_buf(),
            folder_disk: source.is_dir(),
        };
        boot_target(&mut cpu, &target)?;
    }

    let mut old_keys = HashSet::new();
    let mut cpu_budget_remaining = 0;
    let mut cpu_frame_started = false;
    let mut presented_size = screen_size(&cpu);
    let mut active_boot_disk = program.filter(|path| path.is_dir()).map(Path::to_path_buf);
    let runtime_started = Instant::now();
    let mut last_terminal_cursor_phase = true;
    let mut bios_autoboot_wait_started = None;
    let mut bios_autoboot_timeout_sent = false;
    loop {
        if runtime_should_close(is_quit_requested(), true, false) {
            break;
        }
        let pressed_keys = get_keys_pressed();
        if bios_autoboot_timeout_due(
            cpu.memory[BIOS_AUTOBOOT_WAIT_MARKER as usize] == 1,
            pressed_keys.contains(&KeyCode::Space),
            &mut bios_autoboot_wait_started,
            &mut bios_autoboot_timeout_sent,
            Instant::now(),
        ) {
            cpu.signal_keyboard_event(0x8a);
        }
        let keys = get_keys_down();
        let shifted = keys.contains(&KeyCode::LeftShift) || keys.contains(&KeyCode::RightShift);
        let controlled =
            keys.contains(&KeyCode::LeftControl) || keys.contains(&KeyCode::RightControl);
        let mut control_key_count = 0;
        let mut handled_control_keys = HashSet::new();
        for key in pressed_keys.iter().copied() {
            if cpu.privilege_mode == PrivilegeMode::Supervisor
                && cpu.guest_text.enabled
                && cpu.memory[BIOS_FIRMWARE_MARKER as usize] == 1
            {
                let bios_key = match key {
                    KeyCode::Escape => Some(27),
                    KeyCode::F9 => Some(0x88),
                    KeyCode::F10 => Some(0x89),
                    _ => None,
                };
                if let Some(bios_key) = bios_key {
                    cpu.signal_keyboard_event(bios_key);
                    control_key_count += 1;
                    handled_control_keys.insert(key);
                    continue;
                }
            } else if key == KeyCode::Escape {
                return Ok(());
            }
            if let Some(control) = apply_runtime_control(
                key,
                &mut fullscreen,
                &mut console_visible,
                shifted,
                controlled,
            ) {
                control_key_count += 1;
                handled_control_keys.insert(key);
                match control {
                    RuntimeControl::FullscreenToggled(fullscreen) => set_fullscreen(fullscreen),
                    RuntimeControl::ConsoleToggled(_) => {}
                    RuntimeControl::ConsoleClosed => {}
                }
            }
        }
        let console_control = console_window
            .as_mut()
            .and_then(|window| window.runtime_control_pressed());
        if let Some(control) = console_control {
            match control {
                RuntimeControl::ConsoleToggled(visible) => console_visible = visible,
                RuntimeControl::ConsoleClosed => {
                    console_visible = false;
                    console_window = None;
                    cpu.debug_console
                        .log_debug("emulator", "Bug Monitor window closed");
                }
                RuntimeControl::FullscreenToggled(fullscreen) => set_fullscreen(fullscreen),
            }
        }
        if !fullscreen
            && is_mouse_button_pressed(MouseButton::Left)
            && let Some(control) = toolbar_control(mouse_position(), console_visible)
        {
            match control {
                RuntimeControl::ConsoleToggled(visible) => console_visible = visible,
                RuntimeControl::ConsoleClosed => {}
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
        let key_control_only = control_key_count == pressed_keys.len();
        if runtime_should_close(
            false,
            cpu.has_pending_work(),
            !pressed_keys.is_empty() && !key_control_only,
        ) {
            break;
        }
        if cpu.has_pending_work() {
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
                receive_terminal_key(&mut cpu, key);
            }
            let typed_characters = std::iter::from_fn(get_char_pressed).collect::<Vec<_>>();
            receive_typed_text(&mut cpu, typed_characters);
            update_rtc(&mut cpu);
            if cpu_budget_remaining == 0 {
                if !cpu_frame_started {
                    cpu.begin_frame();
                    cpu_frame_started = true;
                }
                cpu_budget_remaining = 300_000;
            }
            if let Err(error) = run_instruction_slice(&mut cpu, &mut cpu_budget_remaining) {
                if let Some(path) = cpu.take_booted_disk_path() {
                    active_boot_disk = Some(path);
                }
                let details =
                    runtime_cpu_failure_details(&cpu, program, active_boot_disk.as_deref(), &error);
                cpu.debug_console.log_error("emulator", &details);
                return Err(RuntimeError::Window(details));
            }
            if let Some(path) = cpu.take_booted_disk_path() {
                active_boot_disk = Some(path);
            }
            if cpu_budget_remaining == 0 || cpu.frame_yield || !cpu.has_pending_work() {
                cpu_budget_remaining = 0;
                cpu_frame_started = false;
            }
            for command in cpu.take_sound_commands() {
                audio.apply(command).await?;
            }
            cpu.end_mouse_frame();
        }
        let cursor_phase = terminal_cursor_phase();
        let capture_display = if cpu.terminal_active {
            cpu.terminal_dirty || last_terminal_cursor_phase != cursor_phase
        } else {
            should_capture_display_frame(cpu.frame_presented, cpu.guest_text.enabled)
        };
        if capture_display {
            if cpu.terminal_active {
                render_terminal(&cpu.console, &mut pixels, &font, cursor_phase);
                presented_size = (640, 480);
                cpu.terminal_dirty = false;
                last_terminal_cursor_phase = cursor_phase;
            } else {
                render_cpu_with_framebuffer(&cpu, &mut pixels, &font, &mut framebuffer);
                presented_size = screen_size(&cpu);
            }
        }
        let (width, height) = presented_size;
        if !cpu.has_pending_work() && !cpu.terminal_active {
            fill_rect(
                &mut pixels,
                width,
                0,
                height.saturating_sub(18),
                width,
                18,
                rgb(10, 14, 24),
            );
            draw_text_limited(
                &mut pixels,
                width,
                (4, height.saturating_sub(13)),
                "PROGRAM HALTED - PRESS ANY KEY TO CLOSE",
                width / 6,
                0xffff_d000,
                &font,
            );
        }
        if let Some(window) = console_window.as_mut() {
            if window.is_open() {
                window.present(&cpu.debug_console.text_snapshot(), &font)?;
            } else {
                console_window = None;
                console_visible = false;
            }
        }
        canvas.present(&pixels, (width, height), !fullscreen).await;
    }
    Ok(())
}

pub fn run_headless(root: &Path, program: &Path, budget: usize) -> Result<usize, RuntimeError> {
    run_headless_with_devices(root, program, budget, None, DEFAULT_PORT)
}

// ...existing code...
pub fn run_headless_with_devices(
    root: &Path,
    program: &Path,
    budget: usize,
    disk_id: Option<&str>,
    network_port: u16,
) -> Result<usize, RuntimeError> {
    let mut cpu = Cpu::new();
    cpu.set_network_port(network_port);
    cpu.enable_scnet(crate::sc_net::DEFAULT_PORT);
    if program.is_dir() {
        cpu.configure_disk(root.join("disks"), root.join("disks"))
            .map_err(RuntimeError::Window)?;
    } else {
        configure_disk(&mut cpu, root, program)?;
    }
    if let Some(disk_id) = disk_id {
        cpu.mount_data_disk(disk_id).map_err(RuntimeError::Window)?;
    }
    if program.is_dir() {
        boot_folder_disk(&mut cpu, program)?;
    } else {
        let image = load_prebuilt_program(program)?;
        cpu.load_program(&image.segments, image.entry_point)
            .map_err(|error| RuntimeError::Window(error.to_string()))?;
    }
    let executed = cpu
        .run(budget)
        .map_err(|error| RuntimeError::Window(error.to_string()))?;
    if cpu.has_pending_work() {
        return Err(RuntimeError::Window(format!(
            "instruction budget exhausted after {executed} instructions"
        )));
    }
    Ok(executed)
}
#[cfg(test)]
fn discover_boot_disks(root: &Path) -> Result<Vec<BootDisk>, RuntimeError> {
    FolderDisk::discover_paths(root)
        .map_err(RuntimeError::Window)?
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| RuntimeError::Window("invalid disk directory name".to_owned()))?
                .to_owned();
            Ok(BootDisk { name, path })
        })
        .collect()
}

async fn show_bios_startup_error(
    error: &str,
    font: &[[u8; 8]],
    pixels: &mut [u32],
    canvas: &mut Canvas,
    fullscreen: bool,
) {
    loop {
        draw_bios_error(pixels, font, error);
        canvas.present(pixels, (640, 480), !fullscreen).await;
        if is_key_pressed(KeyCode::Escape) {
            return;
        }
    }
}

fn draw_bios_error(pixels: &mut [u32], font: &[[u8; 8]], error: &str) {
    pixels.fill(0);
    draw_bios_text(pixels, font, 1, 2, "SUOMICPU-16 BIOS", BIOS_PHOSPHOR_BRIGHT);
    draw_bios_text(pixels, font, 4, 2, "FIRMWARE STARTUP ERROR", BIOS_PHOSPHOR);
    draw_bios_text(pixels, font, 6, 2, error, BIOS_PHOSPHOR_BRIGHT);
    draw_bios_text(
        pixels,
        font,
        20,
        2,
        "RESTART AFTER CORRECTING THE BIOS IMAGE",
        BIOS_PHOSPHOR,
    );
    draw_bios_text(pixels, font, 21, 2, "PRESS ESCAPE TO EXIT", BIOS_PHOSPHOR);
}

#[cfg(test)]
fn draw_bios_selector(
    pixels: &mut [u32],
    font: &[[u8; 8]],
    modules: &[BootDisk],
    (selected, scroll_offset): (usize, usize),
    message: Option<&str>,
) {
    pixels.fill(0);
    draw_bios_text(pixels, font, 0, 2, "SUOMICPU-16 BIOS", BIOS_PHOSPHOR_BRIGHT);
    draw_bios_text(pixels, font, 1, 2, "POWER-ON SELF TEST: OK", BIOS_PHOSPHOR);

    let disk_name = modules
        .get(selected)
        .map(|disk| disk.name.as_str())
        .unwrap_or("NONE");
    draw_bios_text(
        pixels,
        font,
        3,
        2,
        &format!("DISK: {disk_name}"),
        BIOS_PHOSPHOR,
    );
    draw_bios_text(
        pixels,
        font,
        4,
        2,
        "SELECT A FOLDER DISK TO BOOT",
        BIOS_PHOSPHOR,
    );

    draw_bios_text(pixels, font, 5, 2, "NO  DISK ID", BIOS_PHOSPHOR);
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
        let text_row = (y - BIOS_TEXT_TOP) / BIOS_TEXT_CELL_HEIGHT;
        if is_selected {
            fill_rect(
                pixels,
                640,
                BIOS_TEXT_LEFT,
                y,
                BIOS_TEXT_COLUMNS * BIOS_TEXT_CELL_WIDTH,
                BIOS_TEXT_CELL_HEIGHT,
                rgb(0, 30, 12),
            );
        }
        let line = format!(
            "{} {:02} {}",
            if is_selected { '>' } else { ' ' },
            index + 1,
            module.name
        );
        draw_bios_text(
            pixels,
            font,
            text_row,
            2,
            &line,
            if is_selected {
                BIOS_PHOSPHOR_BRIGHT
            } else {
                BIOS_PHOSPHOR
            },
        );
    }

    let position = if modules.is_empty() {
        "NO DISKS FOUND".to_owned()
    } else {
        format!("DISK {:02}/{:02}", selected + 1, modules.len())
    };
    draw_bios_text(pixels, font, 20, 2, &position, BIOS_PHOSPHOR);
    draw_bios_text(
        pixels,
        font,
        21,
        2,
        "ARROWS SELECT  ENTER BOOT  ESC EXIT  F2 MONITOR",
        BIOS_PHOSPHOR,
    );
    if let Some(message) = message {
        draw_bios_text(
            pixels,
            font,
            23,
            2,
            &format!("ERROR: {message}"),
            BIOS_PHOSPHOR_BRIGHT,
        );
    }
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
fn bios_mouse_action(
    position: (f32, f32),
    screen_size: (f32, f32),
    module_count: usize,
    scroll_offset: usize,
) -> Option<BiosMouseAction> {
    let (x, y) = screen_to_logical(position, screen_size, (640, 480))?;
    if !(32..608).contains(&x) || y < BIOS_MODULE_ROW_TOP {
        return None;
    }
    let row = (y - BIOS_MODULE_ROW_TOP) / BIOS_MODULE_ROW_HEIGHT;
    if row >= BIOS_VISIBLE_MODULES {
        return None;
    }
    let index = scroll_offset.saturating_add(row);
    (index < module_count).then_some(BiosMouseAction::SelectDisk(index))
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
    let source_directory = source.parent().unwrap_or_else(|| Path::new("."));
    if source_directory.join("disk.json").is_file() {
        cpu.configure_disk(root.join("disks"), source_directory.to_path_buf())
            .map_err(RuntimeError::Window)?;
        return cpu
            .mount_folder_disk(source_directory)
            .map_err(RuntimeError::Window);
    }
    let program_directory = source_directory.parent().filter(|directory| {
        source_directory
            .file_name()
            .is_some_and(|name| name == "src")
            && directory.join("program.json").is_file()
    });
    let assets = program_directory.map_or_else(
        || source_directory.join("disks"),
        |directory| directory.join("assets").join("disk"),
    );
    cpu.configure_disk(root.join("disks"), assets)
        .map_err(RuntimeError::Window)
}

fn fresh_runtime_cpu(
    root: &Path,
    source: Option<&Path>,
    disk_id: Option<&str>,
    network_port: u16,
) -> Result<Cpu, RuntimeError> {
    let mut cpu = Cpu::new();
    cpu.set_network_port(network_port);
    cpu.enable_scnet(crate::sc_net::DEFAULT_PORT);
    if let Some(source) = source.filter(|source| !source.is_dir()) {
        configure_disk(&mut cpu, root, source)?;
    } else {
        cpu.configure_disk(root.join("disks"), root.join("disks"))
            .map_err(RuntimeError::Window)?;
    }
    cpu.configure_bios_settings(root.join("config").join("bios.json"))
        .map_err(RuntimeError::Window)?;
    if let Some(disk_id) = disk_id.filter(|_| source.is_some()) {
        cpu.mount_data_disk(disk_id).map_err(RuntimeError::Window)?;
    } else if source.is_none() {
        cpu.set_bios_preselected_disk(disk_id);
    }
    let bios_path = root.join("firmware").join("bios.bin");
    if !bios_path.try_exists()? {
        return Err(RuntimeError::Window(format!(
            "required common BIOS is missing: {}",
            bios_path.display()
        )));
    }
    let image = crate::image::read_image(&bios_path)?;
    if image.entry_point != 0 {
        return Err(RuntimeError::Window(
            "common BIOS entry point must be address 0".to_owned(),
        ));
    }
    cpu.prepare_image_entry();
    cpu.load_program(&image.segments, image.entry_point)
        .map_err(|error| RuntimeError::Window(format!("invalid common BIOS: {error}")))?;
    cpu.privilege_mode = PrivilegeMode::Supervisor;
    cpu.debug_console
        .log_info("emulator", "trusted BIOS started in kernel mode");
    Ok(cpu)
}

fn boot_target(cpu: &mut Cpu, target: &BootTarget) -> Result<(), RuntimeError> {
    if target.folder_disk {
        let mode = boot_folder_disk(cpu, &target.source)?;
        let disk = FolderDisk::open(&target.source).map_err(RuntimeError::Window)?;
        cpu.debug_console.log_info(
            "bios",
            &format!(
                "Loaded boot device {} id={} label={:?} program={} mode={:?} entry={:#06x} segments={}",
                target.source.display(),
                disk.manifest().id,
                disk.manifest().label,
                disk_boot_program(mode),
                mode,
                cpu.pc,
                cpu.loaded_image_segments.len(),
            ),
        );
        return Ok(());
    }
    let name = target
        .source
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("program");
    cpu.debug_console
        .log_info("emulator", &format!("Loading {name}"));
    let image = load_prebuilt_program(&target.source).map_err(|error| {
        cpu.debug_console
            .log_error("emulator", &format!("Loading {name} failed: {error}"));
        error
    })?;
    cpu.validate_program(&image.segments, image.entry_point)
        .map_err(|error| {
            cpu.debug_console
                .log_error("emulator", &format!("Loading {name} failed: {error}"));
            RuntimeError::Window(error.to_string())
        })?;
    cpu.prepare_image_entry();
    cpu.load_program(&image.segments, image.entry_point)
        .map_err(|error| {
            cpu.debug_console
                .log_error("emulator", &format!("Loading {name} failed: {error}"));
            RuntimeError::Window(error.to_string())
        })?;
    cpu.console.clear_input();
    cpu.terminal_active = true;
    cpu.debug_console.log_info(
        "emulator",
        &format!(
            "Program loaded path={} mode={:?} entry={:#06x} segments={}",
            target.source.display(),
            cpu.privilege_mode,
            cpu.pc,
            cpu.loaded_image_segments.len(),
        ),
    );
    Ok(())
}

fn boot_folder_disk(cpu: &mut Cpu, disk_path: &Path) -> Result<FolderBootMode, RuntimeError> {
    let disk = FolderDisk::open(disk_path).map_err(|error| {
        cpu.debug_console
            .log_error("bios", &format!("Disk {}: {error}", disk_path.display()));
        RuntimeError::Window(error)
    })?;
    let boot = disk.load_boot_image().map_err(|error| {
        cpu.debug_console
            .log_error("bios", &format!("Disk {}: {error}", disk_path.display()));
        RuntimeError::Window(error)
    })?;
    let disk_id = disk.manifest().id.clone();
    let disk_label = disk.manifest().label.clone();
    cpu.debug_console.log_info(
        "bios",
        &format!(
            "Boot candidate path={} id={} label={:?} program={} mode={:?} entry={:#06x} segments={}",
            disk_path.display(),
            disk_id,
            disk_label,
            disk_boot_program(boot.mode),
            boot.mode,
            boot.image.entry_point,
            boot.image.segments.len(),
        ),
    );
    cpu.validate_program(&boot.image.segments, boot.image.entry_point)
        .map_err(|error| {
            cpu.debug_console.log_error("bios", &error.to_string());
            RuntimeError::Window(error.to_string())
        })?;
    cpu.prepare_image_entry();
    cpu.load_program(&boot.image.segments, boot.image.entry_point)
        .map_err(|error| {
            cpu.debug_console.log_error("bios", &error.to_string());
            RuntimeError::Window(error.to_string())
        })?;
    cpu.mount_folder_disk(disk_path).map_err(|error| {
        cpu.debug_console.log_error("bios", &error);
        RuntimeError::Window(error)
    })?;
    match boot.mode {
        FolderBootMode::Kernel => cpu.privilege_mode = PrivilegeMode::Supervisor,
        FolderBootMode::User => {
            let user_stack = cpu.initial_user_stack_pointer();
            cpu.enter_user_mode(cpu.pc, user_stack).map_err(|error| {
                cpu.debug_console.log_error("bios", &error.to_string());
                RuntimeError::Window(error.to_string())
            })?;
        }
    }
    cpu.running = true;
    cpu.console.clear_input();
    cpu.terminal_active = true;
    Ok(boot.mode)
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
        KeyCode::Up => 0x80,
        KeyCode::Down => 0x81,
        KeyCode::Left => 0x82,
        KeyCode::Right => 0x83,
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
    cpu.update_rtc(now.as_secs());
}

fn bios_autoboot_timeout_due(
    waiting: bool,
    space_pressed: bool,
    started_at: &mut Option<Instant>,
    timeout_sent: &mut bool,
    now: Instant,
) -> bool {
    if !waiting {
        *started_at = None;
        *timeout_sent = false;
        return false;
    }
    if *timeout_sent {
        return false;
    }
    let started_at = *started_at.get_or_insert(now);
    if !space_pressed && now.duration_since(started_at) >= Duration::from_secs(5) {
        *timeout_sent = true;
        return true;
    }
    false
}

fn run_instruction_slice(
    cpu: &mut Cpu,
    budget_remaining: &mut usize,
) -> Result<usize, crate::cpu::CpuError> {
    let started = Instant::now();
    let mut total_executed = 0;
    while *budget_remaining > 0
        && started.elapsed() < RUNTIME_CPU_SLICE
        && cpu.has_pending_work()
        && !cpu.frame_yield
    {
        let chunk = (*budget_remaining).min(RUNTIME_INSTRUCTION_CHUNK);
        let executed = cpu.run(chunk)?;
        *budget_remaining = budget_remaining.saturating_sub(executed);
        total_executed += executed;
        if executed == 0 {
            break;
        }
    }
    Ok(total_executed)
}

fn runtime_cpu_failure_details(
    cpu: &Cpu,
    program: Option<&Path>,
    boot_disk: Option<&Path>,
    error: &crate::cpu::CpuError,
) -> String {
    let launch_program = program.map_or_else(
        || "BIOS selector".to_owned(),
        |path| path.display().to_string(),
    );
    let (disk, disk_program) = match boot_disk {
        Some(path) => match FolderDisk::open(path) {
            Ok(disk) => {
                let identity = format!(
                    "{} (id={}, label={:?})",
                    path.display(),
                    disk.manifest().id,
                    disk.manifest().label
                );
                match disk.load_boot_image() {
                    Ok(boot) => {
                        let filename = match boot.mode {
                            FolderBootMode::Kernel => "kernel.bin",
                            FolderBootMode::User => "main.bin",
                        };
                        (identity, format!("{filename} ({:?})", boot.mode))
                    }
                    Err(error) => (identity, format!("unavailable ({error})")),
                }
            }
            Err(error) => (
                path.display().to_string(),
                format!("disk inspection failed ({error})"),
            ),
        },
        None => ("none selected".to_owned(), "unknown".to_owned()),
    };
    format!(
        "{error}; launch_program={launch_program}; disk={disk}; disk_program={disk_program}; execution_mode={:?}; pc={:#06x}; sp={:#06x}; kernel_sp={:#06x}; task={:?}; mmu_enabled={}; page_directory={:#06x}; last_page_fault={:?}; loaded_segments={:?}",
        cpu.privilege_mode,
        cpu.pc,
        cpu.sp,
        cpu.kernel_sp,
        cpu.current_task(),
        cpu.is_mmu_enabled(),
        cpu.page_directory(),
        cpu.last_page_fault(),
        cpu.loaded_image_segments,
    )
}

fn disk_boot_program(mode: FolderBootMode) -> &'static str {
    match mode {
        FolderBootMode::Kernel => "kernel.bin",
        FolderBootMode::User => "main.bin",
    }
}

fn halted_window_should_close(cpu_running: bool, has_keypress: bool) -> bool {
    !cpu_running && has_keypress
}

fn runtime_should_close(quit_requested: bool, cpu_running: bool, has_keypress: bool) -> bool {
    quit_requested || halted_window_should_close(cpu_running, has_keypress)
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
    if !cpu.terminal_active {
        return;
    }
    let text = characters
        .into_iter()
        .filter(|character| !character.is_control())
        .collect::<String>();
    if !text.is_empty() {
        cpu.receive_console_input(text.as_bytes());
    }
}

fn receive_terminal_key(cpu: &mut Cpu, key: KeyCode) {
    match key {
        KeyCode::Enter => {
            if cpu.terminal_active {
                cpu.receive_console_input(b"\n");
            }
            cpu.signal_keyboard_event(13);
        }
        KeyCode::Backspace => {
            if cpu.terminal_active {
                cpu.receive_console_input(b"\x08");
            }
            cpu.signal_keyboard_event(8);
        }
        KeyCode::Up if cpu.terminal_active => cpu.receive_console_input(b"\x1b[A"),
        KeyCode::Down if cpu.terminal_active => cpu.receive_console_input(b"\x1b[B"),
        KeyCode::Right if cpu.terminal_active => cpu.receive_console_input(b"\x1b[C"),
        KeyCode::Left if cpu.terminal_active => cpu.receive_console_input(b"\x1b[D"),
        _ => {}
    }
}

fn apply_runtime_control(
    key: KeyCode,
    fullscreen: &mut bool,
    console_visible: &mut bool,
    shifted: bool,
    controlled: bool,
) -> Option<RuntimeControl> {
    match key {
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

fn toolbar_control(position: (f32, f32), console_visible: bool) -> Option<RuntimeControl> {
    let (x, y) = position;
    if !(5.0..31.0).contains(&y) {
        return None;
    }
    let (left, width) = BUG_MONITOR_BUTTON;
    if (left..left + width).contains(&x) {
        return Some(RuntimeControl::ConsoleToggled(!console_visible));
    }
    None
}

fn draw_toolbar() {
    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        TOOLBAR_HEIGHT,
        Color::from_rgba(12, 16, 24, 255),
    );
    let (x, width) = BUG_MONITOR_BUTTON;
    draw_rectangle(x, 5.0, width, 26.0, Color::from_rgba(43, 54, 72, 255));
    draw_ui_text(
        "BUG MONITOR",
        x + 8.0,
        23.0,
        14.0,
        Color::from_rgba(238, 242, 248, 255),
    );
}

fn draw_console_lines(pixels: &mut [u32], lines: &[String], font: &[[u8; 8]]) {
    pixels.fill(rgb(8, 12, 18));
    fill_rect(
        pixels,
        CONSOLE_WINDOW_WIDTH,
        0,
        0,
        CONSOLE_WINDOW_WIDTH,
        CONSOLE_TOOLBAR_HEIGHT,
        rgb(16, 22, 34),
    );
    let (left, width) = CONSOLE_CLOSE_BUTTON;
    fill_rect(
        pixels,
        CONSOLE_WINDOW_WIDTH,
        left,
        4,
        width,
        20,
        rgb(43, 54, 72),
    );
    draw_text(
        pixels,
        CONSOLE_WINDOW_WIDTH,
        left + 7,
        10,
        "CLOSE MONITOR",
        rgb(238, 242, 248),
        font,
    );
    for (row, line) in lines
        .iter()
        .skip(
            lines
                .len()
                .saturating_sub((CONSOLE_WINDOW_HEIGHT - CONSOLE_TOOLBAR_HEIGHT) / 8),
        )
        .enumerate()
    {
        draw_text(
            pixels,
            CONSOLE_WINDOW_WIDTH,
            0,
            CONSOLE_TOOLBAR_HEIGHT + row * 8,
            line,
            rgb(190, 220, 190),
            font,
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn console_toolbar_control((x, y): (f32, f32)) -> Option<RuntimeControl> {
    if !(4.0..24.0).contains(&y) {
        return None;
    }
    let (left, width) = CONSOLE_CLOSE_BUTTON;
    (left as f32..(left + width) as f32)
        .contains(&x)
        .then_some(RuntimeControl::ConsoleToggled(false))
}

fn screen_size(cpu: &Cpu) -> (usize, usize) {
    if cpu.graphics_mode() == 1 {
        (640, 480)
    } else {
        (320, 240)
    }
}

#[cfg(test)]
fn render_cpu(cpu: &Cpu, pixels: &mut [u32], font: &[[u8; 8]]) {
    let mut framebuffer = Vec::new();
    render_cpu_with_framebuffer(cpu, pixels, font, &mut framebuffer);
}

fn render_cpu_with_framebuffer(
    cpu: &Cpu,
    pixels: &mut [u32],
    font: &[[u8; 8]],
    framebuffer: &mut Vec<u8>,
) {
    let (width, height) = screen_size(cpu);
    framebuffer.resize(width * height, 0);
    cpu.composited_framebuffer_into(framebuffer);
    for y in 0..height {
        for x in 0..width {
            let color = framebuffer[y * width + x];
            let [red, green, blue] = cpu.palette_color(color);
            pixels[y * width + x] = rgb(red, green, blue);
        }
    }
    if cpu.guest_text.enabled {
        draw_guest_text(
            pixels,
            width,
            height,
            &cpu.guest_text.cells,
            &cpu.palette,
            font,
        );
    }
}

fn terminal_cursor_phase() -> bool {
    (macroquad::prelude::get_time() * 2.0) as u64 % 2 == 0
}

fn render_terminal(
    console: &crate::console::ConsoleDevice,
    pixels: &mut [u32],
    font: &[[u8; 8]],
    cursor_phase: bool,
) {
    let width = 640;
    let height = 480;
    pixels[..width * height].fill(rgb(0, 0, 0));
    let left = 8;
    let top = 8;
    for (row, line) in console.lines().enumerate() {
        for (column, byte) in line.iter().copied().enumerate() {
            if byte == b' ' {
                continue;
            }
            let character = if (32..=126).contains(&byte) {
                byte
            } else {
                b'?'
            };
            draw_glyph_scaled(
                pixels,
                width,
                (left + column * 6, top + row * 8),
                character,
                rgb(220, 220, 220),
                font,
                1,
            );
        }
    }
    if console.cursor_visible() && cursor_phase {
        let (column, row) = console.cursor_position();
        fill_rect(
            pixels,
            width,
            left + column.min(crate::console::COLUMNS - 1) * 6,
            top + row.min(crate::console::ROWS - 1) * 8 + 7,
            5,
            1,
            rgb(220, 220, 220),
        );
    }
}

fn draw_guest_text(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    cells: &[crate::guest_text::GuestTextCell],
    palette: &[[u8; 3]; 256],
    font: &[[u8; 8]],
) {
    let scale = if width >= 640 { 2 } else { 1 };
    let cell_width = 6 * scale;
    let cell_height = 8 * scale;
    let left = width.saturating_sub(crate::guest_text::GUEST_TEXT_COLUMNS * cell_width) / 2;
    let top = height.saturating_sub(crate::guest_text::GUEST_TEXT_ROWS * cell_height) / 2;

    for (index, cell) in cells.iter().enumerate() {
        let row = index / crate::guest_text::GUEST_TEXT_COLUMNS;
        let column = index % crate::guest_text::GUEST_TEXT_COLUMNS;
        let x = left + column * cell_width;
        let y = top + row * cell_height;
        let [red, green, blue] = palette[cell.background as usize];
        let background = rgb(red, green, blue);
        for pixel_y in y..y + cell_height {
            for pixel_x in x..x + cell_width {
                if let Some(pixel) = pixels.get_mut(pixel_y * width + pixel_x) {
                    *pixel = background;
                }
            }
        }

        let character = if (32..=126).contains(&cell.character) {
            cell.character
        } else {
            b'?'
        };
        let [red, green, blue] = palette[cell.foreground as usize];
        draw_glyph_scaled(
            pixels,
            width,
            (x + scale / 2, y + scale / 2),
            character,
            rgb(red, green, blue),
            font,
            scale,
        );
    }
}

fn should_capture_display_frame(frame_presented: bool, text_console_active: bool) -> bool {
    frame_presented || text_console_active
}

const fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    ((red as u32) << 16) | ((green as u32) << 8) | blue as u32
}

fn load_font() -> Vec<[u8; 8]> {
    let bytes = include_str!("../../firmware/fonts/font5x7.asm")
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
    draw_text_scaled(pixels, width, (x, y), text, color, font, 1);
}

fn draw_bios_text(
    pixels: &mut [u32],
    font: &[[u8; 8]],
    row: usize,
    column: usize,
    text: &str,
    color: u32,
) {
    if row >= BIOS_TEXT_ROWS || column >= BIOS_TEXT_COLUMNS {
        return;
    }
    let y_base = BIOS_TEXT_TOP + row * BIOS_TEXT_CELL_HEIGHT;
    let text = text
        .bytes()
        .take(BIOS_TEXT_COLUMNS - column)
        .map(|byte| {
            if (32..=126).contains(&byte) {
                byte
            } else {
                b'?'
            }
        })
        .collect::<Vec<_>>();

    for (character_offset, byte) in text.into_iter().enumerate() {
        let Some(glyph) = font.get(byte.saturating_sub(32) as usize) else {
            continue;
        };
        let x_base = BIOS_TEXT_LEFT + (column + character_offset) * BIOS_TEXT_CELL_WIDTH + 1;
        for (glyph_row, bits) in glyph.iter().copied().enumerate().take(7) {
            for glyph_column in 0..5 {
                if bits & (0x80 >> glyph_column) == 0 {
                    continue;
                }
                for scale_y in 0..2 {
                    for scale_x in 0..2 {
                        let x = x_base + glyph_column * 2 + scale_x;
                        let y = y_base + 1 + glyph_row * 2 + scale_y;
                        if let Some(pixel) = pixels.get_mut(y * 640 + x) {
                            *pixel = color;
                        }
                    }
                }
            }
        }
    }
}

fn draw_text_scaled(
    pixels: &mut [u32],
    width: usize,
    (x, y): (usize, usize),
    text: &str,
    color: u32,
    font: &[[u8; 8]],
    scale: usize,
) {
    for (character_index, byte) in text.bytes().enumerate() {
        draw_glyph_scaled(
            pixels,
            width,
            (x + character_index * 6 * scale, y),
            byte,
            color,
            font,
            scale,
        );
    }
}

fn draw_glyph_scaled(
    pixels: &mut [u32],
    width: usize,
    (x, y): (usize, usize),
    byte: u8,
    color: u32,
    font: &[[u8; 8]],
    scale: usize,
) {
    let glyph_index = byte.saturating_sub(32) as usize;
    let Some(glyph) = font.get(glyph_index) else {
        return;
    };
    for (row, bits) in glyph.iter().copied().enumerate().take(7) {
        for column in 0..5 {
            if bits & (0x80 >> column) != 0 {
                for scale_y in 0..scale {
                    for scale_x in 0..scale {
                        let pixel_x = x + column * scale + scale_x;
                        let pixel_y = y + row * scale + scale_y;
                        if pixel_x < width
                            && let Some(pixel) = pixels.get_mut(pixel_y * width + pixel_x)
                        {
                            *pixel = color;
                        }
                    }
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
    draw_text_limited_scaled(
        pixels,
        width,
        (x, y),
        text,
        (max_characters, 1),
        color,
        font,
    );
}

fn draw_text_limited_scaled(
    pixels: &mut [u32],
    width: usize,
    (x, y): (usize, usize),
    text: &str,
    (max_characters, scale): (usize, usize),
    color: u32,
    font: &[[u8; 8]],
) {
    let character_count = text.chars().count();
    if character_count <= max_characters {
        draw_text_scaled(pixels, width, (x, y), text, color, font, scale);
        return;
    }
    let visible_characters = max_characters.saturating_sub(3);
    let mut clipped = text.chars().take(visible_characters).collect::<String>();
    clipped.push_str("...");
    draw_text_scaled(pixels, width, (x, y), &clipped, color, font, scale);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn prebuild_test_image(source: &Path, label: &str) -> (PathBuf, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "sc16-runtime-image-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let image = crate::image::build_source(source).unwrap();
        let path = directory.join(format!("{label}.sc16img"));
        crate::image::write_image(&path, &image).unwrap();
        (directory, path)
    }

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
            CONSOLE_TOOLBAR_HEIGHT,
            "line-09",
            rgb(190, 220, 190),
            &font,
        );

        assert_eq!(pixels.len(), 480 * 200);
        assert_eq!(pixels[0], rgb(16, 22, 34));
        assert_eq!(
            pixels[CONSOLE_TOOLBAR_HEIGHT * CONSOLE_WINDOW_WIDTH],
            rgb(8, 12, 18)
        );
        assert_eq!(
            &pixels[CONSOLE_TOOLBAR_HEIGHT * CONSOLE_WINDOW_WIDTH
                ..(CONSOLE_TOOLBAR_HEIGHT + 8) * CONSOLE_WINDOW_WIDTH],
            &expected_first_row[CONSOLE_TOOLBAR_HEIGHT * CONSOLE_WINDOW_WIDTH
                ..(CONSOLE_TOOLBAR_HEIGHT + 8) * CONSOLE_WINDOW_WIDTH]
        );
    }

    #[test]
    fn guest_console_renders_its_full_text_surface_and_cursor_in_the_main_display() {
        let mut console = crate::console::ConsoleDevice::new();
        for byte in b"SC-16" {
            console.write_data(*byte);
        }
        let font = load_font();
        let mut pixels = vec![rgb(255, 0, 0); 640 * 480];
        render_terminal(&console, &mut pixels, &font, true);

        assert_eq!(pixels[0], rgb(0, 0, 0));
        let left = 8;
        let top = 8;
        assert!(
            pixels[top * 640 + left..top * 640 + left + 6]
                .iter()
                .any(|pixel| *pixel == rgb(220, 220, 220))
        );
        assert_eq!(pixels[(top + 7) * 640 + left + 5 * 6], rgb(220, 220, 220));
        for byte in b"\x1b[?25l" {
            console.write_data(*byte);
        }
        render_terminal(&console, &mut pixels, &font, true);
        assert_eq!(pixels[(top + 7) * 640 + left + 5 * 6], rgb(0, 0, 0));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn console_toolbar_buttons_dispatch_controls_when_the_console_has_focus() {
        assert_eq!(
            console_toolbar_control((20.0, 12.0)),
            Some(RuntimeControl::ConsoleToggled(false))
        );
        assert_eq!(console_toolbar_control((20.0, 30.0)), None);
        for x in [120.0, 220.0, 320.0] {
            assert_eq!(console_toolbar_control((x, 12.0)), None);
        }
    }

    #[test]
    fn cpu_failure_diagnostics_include_boot_target_and_machine_state() {
        let cpu = Cpu::with_memory_size(1 << 20).unwrap();
        let program = Path::new("examples").join("preemptive_os_kernel.asm");
        let disk_root = std::env::temp_dir().join(format!(
            "sc16-diagnostic-disk-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let disk = disk_root.join("sisuos");
        fs::create_dir_all(&disk).unwrap();
        fs::write(
            disk.join("disk.json"),
            r#"{"version":1,"id":"sisuos","label":"SisuOS","capacity_bytes":65536,"read_only":false,"protected_files":["kernel.bin"]}"#,
        )
        .unwrap();
        let kernel = crate::assembler::assemble("HALT", 0x4000).unwrap();
        crate::image::write_image(&disk.join("kernel.bin"), &kernel).unwrap();
        let details = runtime_cpu_failure_details(
            &cpu,
            Some(&program),
            Some(&disk),
            &crate::cpu::CpuError::DoubleFault,
        );

        let expected_program = format!("launch_program={}", program.display());
        let expected_disk = format!("disk={}", disk.display());
        for expected in [
            "CPU stopped after a double fault",
            "id=sisuos",
            "label=\"SisuOS\"",
            "execution_mode=Supervisor",
            "pc=0x0000",
            "mmu_enabled=false",
            "loaded_segments=[]",
        ] {
            assert!(
                details.contains(expected),
                "missing {expected:?}: {details}"
            );
        }
        assert!(details.contains(&expected_program), "{details}");
        assert!(details.contains(&expected_disk), "{details}");
        assert!(
            details.contains("disk_program=kernel.bin (Kernel)"),
            "{details}"
        );
        fs::remove_dir_all(disk_root).unwrap();
    }

    #[test]
    fn bios_disk_discovery_sorts_candidates_without_validating_unselected_metadata() {
        let directory = std::env::temp_dir().join(format!(
            "sc16-bios-discovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let disks = directory.join("disks");
        fs::create_dir_all(disks.join("DISK0002")).unwrap();
        fs::create_dir_all(disks.join("DISK0001")).unwrap();
        fs::create_dir_all(disks.join(".not-a-disk")).unwrap();
        fs::write(disks.join("ignored.c"), "not a disk").unwrap();
        fs::write(disks.join("DISK0001").join("disk.json"), "{}").unwrap();
        let found = discover_boot_disks(&disks).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|module| module.name.as_str())
                .collect::<Vec<_>>(),
            ["DISK0001", "DISK0002"]
        );
        assert_eq!(found[0].path, disks.join("DISK0001"));
        assert!(FolderDisk::open(&found[0].path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn bios_disk_discovery_reports_missing_collection() {
        let missing = std::env::temp_dir()
            .join(format!("sc16-bios-missing-{}", std::process::id()))
            .join("disks");
        assert!(discover_boot_disks(&missing).is_err());
    }

    fn boot_test_disk(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "sc16-boot-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let disk = root.join("DISK0001");
        fs::create_dir_all(&disk).unwrap();
        fs::write(
            disk.join("disk.json"),
            r#"{"version":1,"id":"DISK0001","label":"Boot","capacity_bytes":65536,"read_only":true}"#,
        ).unwrap();
        disk
    }

    #[test]
    fn bios_boot_executes_fixed_images_at_their_entry_points_in_both_modes() {
        for (file, mode) in [
            ("kernel.bin", PrivilegeMode::Supervisor),
            ("main.bin", PrivilegeMode::User),
        ] {
            let disk = boot_test_disk(file);
            let image = crate::assembler::assemble("LDI R0, 42\nHALT", 0x100).unwrap();
            crate::image::write_image(&disk.join(file), &image).unwrap();
            fs::write(disk.join("main.c"), "not valid C").unwrap();
            let target = BootTarget {
                source: disk.clone(),
                folder_disk: true,
            };
            let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
            boot_target(&mut cpu, &target).unwrap();
            assert_eq!(cpu.pc, 0x100);
            assert_eq!(cpu.privilege_mode, mode);
            assert_eq!(cpu.loaded_image_segments.len(), 1);
            let expected_sp = match mode {
                PrivilegeMode::Supervisor => cpu.memory.len() as u32 - 1,
                PrivilegeMode::User => cpu.initial_user_stack_pointer(),
            };
            assert_eq!(
                cpu.loaded_image_segments[0].permissions,
                crate::image::SegmentPermissions::READ_WRITE_EXECUTE
            );
            assert_eq!(cpu.run(10).unwrap(), 2);
            assert_eq!(cpu.registers[0], 42);
            assert!(!cpu.running);
            cpu.registers[1] = 99;
            cpu.flags.zero = true;
            cpu.sp = 0x100;
            cpu.write(crate::machine::INTERRUPT_ENABLE, 1);
            boot_target(&mut cpu, &target).unwrap();
            assert!(cpu.running);
            assert_eq!(cpu.pc, 0x100);
            assert_eq!(cpu.privilege_mode, mode);
            assert_eq!(cpu.registers, [0; 8]);
            assert_eq!(cpu.flags, crate::cpu::Flags::default());
            assert_eq!(cpu.sp, expected_sp);
            assert_eq!(cpu.kernel_sp, cpu.memory.len() as u32 - 1);
            assert_eq!(cpu.memory[0x43002], 0);
            assert_eq!(cpu.loaded_image_segments.len(), 1);
            fs::remove_dir_all(disk.parent().unwrap()).unwrap();
        }
    }

    #[test]
    fn bios_boots_kernel_disk_into_a_protected_scheduled_user_task() {
        let disk = boot_test_disk("kernel");
        let image =
            crate::assembler::assemble(include_str!("../../examples/minimal_kernel.asm"), 0)
                .unwrap();
        crate::image::write_image(&disk.join("kernel.bin"), &image).unwrap();
        let target = BootTarget {
            source: disk.clone(),
            folder_disk: true,
        };
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();

        boot_target(&mut cpu, &target).unwrap();
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(cpu.pc, 0x4000);
        let run_result = cpu.run(128);
        assert!(
            run_result.is_ok(),
            "kernel boot failed at {:#x}: {run_result:?}; page fault {:?}",
            cpu.pc,
            cpu.last_page_fault()
        );

        assert!(
            cpu.is_mmu_enabled(),
            "MMU disabled at {:#x} in {:?}, running={}, root={:#x}, task={:?}, regs={:?}",
            cpu.pc,
            cpu.privilege_mode,
            cpu.running,
            cpu.page_directory(),
            cpu.task_context(0),
            cpu.registers
        );
        assert_eq!(cpu.page_directory(), 0x2000);
        assert_eq!(
            cpu.task_context(0).unwrap().state,
            crate::scheduler::TaskState::Exited
        );
        assert_eq!(cpu.current_task(), None);
        assert_eq!(cpu.read_u16(crate::machine::INTERRUPT_PIT_PERIOD), 0);

        boot_target(&mut cpu, &target).unwrap();
        assert_eq!(cpu.pc, 0x4000);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(cpu.current_task(), None);
        assert!(!cpu.is_mmu_enabled());
        cpu.run(128).unwrap();
        assert_eq!(
            cpu.task_context(0).unwrap().state,
            crate::scheduler::TaskState::Exited
        );
        fs::remove_dir_all(disk.parent().unwrap()).unwrap();
    }

    #[test]
    fn bios_kernel_runs_spawned_syscall_lifecycle() {
        let disk = boot_test_disk("syscall-lifecycle");
        fs::write(
            disk.join("disk.json"),
            r#"{"version":1,"id":"DISK0001","label":"Boot","capacity_bytes":65536,"read_only":false}"#,
        )
        .unwrap();
        fs::write(disk.join("input.txt"), b"hello").unwrap();

        for (source, output) in [
            (
                include_str!("../../examples/syscall_kernel.asm"),
                disk.join("kernel.bin"),
            ),
            (
                include_str!("../../examples/syscall_child.asm"),
                disk.join("child.bin"),
            ),
        ] {
            let image = crate::assembler::assemble(source, 0).unwrap();
            crate::image::write_image(&output, &image).unwrap();
        }

        let root = disk.parent().unwrap().to_path_buf();
        let disks = root.join("disks");
        fs::create_dir(&disks).unwrap();
        let boot_disk = disks.join("DISK0001");
        fs::rename(&disk, &boot_disk).unwrap();
        let firmware =
            crate::assembler::assemble(include_str!("../../firmware/bios.asm"), 0).unwrap();
        fs::create_dir_all(root.join("firmware")).unwrap();
        crate::image::write_image(&root.join("firmware").join("bios.bin"), &firmware).unwrap();

        let mut cpu = fresh_runtime_cpu(&root, None, None, DEFAULT_PORT).unwrap();
        cpu.run(5000).unwrap_or_else(|error| {
            panic!(
                "BIOS cold start failed: {error}; PC={:#x}, fault={:?}, registers={:?}",
                cpu.pc,
                cpu.last_page_fault(),
                cpu.registers
            )
        });
        cpu.signal_keyboard_event(13);
        cpu.run(30000).unwrap();

        assert_eq!(
            cpu.last_page_fault(),
            Some(crate::mmu::PageFault {
                virtual_address: 0x4000,
                access: crate::mmu::MemoryAccess::Read,
                reason: crate::mmu::PageFaultReason::SupervisorOnly
            })
        );
        assert_eq!(cpu.current_task(), None);
        assert_eq!(
            cpu.task_context(0).unwrap().state,
            crate::scheduler::TaskState::Exited
        );
        assert_eq!(cpu.task_context(0).unwrap().exit_code, Some(42));
        assert!(
            cpu.console
                .output()
                .windows(b"hello".len())
                .any(|output| output == b"hello"),
            "console={:?}, parent={:?}, child={:?}, page_fault={:?}",
            cpu.console.output(),
            cpu.task_context(0),
            cpu.task_context(1),
            cpu.last_page_fault()
        );
        assert_eq!(fs::read(boot_disk.join("result.txt")).unwrap(), b"hello");
        assert_eq!(cpu.read_u16(crate::machine::INTERRUPT_PIT_PERIOD), 0);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bios_cold_start_preloads_common_firmware_and_boots_selected_disks() {
        let disk = boot_test_disk("cold-start");
        let root = disk.parent().unwrap();
        fs::create_dir(root.join("disks")).unwrap();
        let packaged_disk = root.join("disks").join("DISK0001");
        fs::rename(&disk, &packaged_disk).unwrap();
        let second_disk = root.join("disks").join("DISK0002");
        fs::create_dir(&second_disk).unwrap();
        fs::write(
            second_disk.join("disk.json"),
            r#"{"version":1,"id":"DISK0002","label":"Second Device Name That Must Remain On One Row 123456789","capacity_bytes":65536,"read_only":true}"#,
        )
        .unwrap();
        let firmware =
            crate::assembler::assemble(include_str!("../../firmware/bios.asm"), 0).unwrap();
        fs::create_dir_all(root.join("firmware")).unwrap();
        crate::image::write_image(root.join("firmware").join("bios.bin").as_path(), &firmware)
            .unwrap();
        let first_image = crate::assembler::assemble("LDI R0, 42\nHALT", 0x100).unwrap();
        crate::image::write_image(&packaged_disk.join("main.bin"), &first_image).unwrap();
        let second_image = crate::assembler::assemble("LDI R0, 84\nHALT", 0x100).unwrap();
        crate::image::write_image(&second_disk.join("main.bin"), &second_image).unwrap();
        assert_eq!(
            FolderDisk::discover_paths(&root.join("disks"))
                .unwrap()
                .len(),
            2
        );
        let mut cpu = fresh_runtime_cpu(root, None, None, DEFAULT_PORT).unwrap();
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        assert!(cpu.running);
        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.read(crate::machine::INPUT_KEY), 0);
        assert_eq!(
            &cpu.memory[..firmware.segments[0].data.len()],
            firmware.segments[0].data.as_slice()
        );
        cpu.run(5000).unwrap_or_else(|error| {
            panic!(
                "BIOS cold start failed: {error}; PC={:#x}, fault={:?}, registers={:?}",
                cpu.pc,
                cpu.last_page_fault(),
                cpu.registers
            )
        });
        assert!(cpu.guest_text.enabled);
        let text = cpu
            .guest_text
            .cells
            .iter()
            .map(|cell| char::from(cell.character))
            .collect::<String>();
        let firmware_registers = [
            cpu.read(crate::machine::FIRMWARE_INDEX),
            cpu.read(crate::machine::FIRMWARE_INDEX + 1),
            cpu.read(crate::machine::FIRMWARE_STATUS),
        ];
        let pending_key = cpu.read(crate::machine::INPUT_KEY);
        assert!(
            text.contains("DISK0001"),
            "{text:?} pc={:#x} regs={:?} key={} id={:?} registers={:?} console={:?}",
            cpu.pc,
            cpu.registers,
            pending_key,
            &cpu.memory[0x5000..0x5009],
            firmware_registers,
            cpu.debug_console.text_snapshot()
        );
        assert!(text.contains("BIOS SETUP UTILITY"), "{text:?}");
        assert!(text.contains("SETTINGS"), "{text:?}");
        assert!(text.contains("SC-NET node ID"), "{text:?}");
        assert!(text.contains(&cpu.bios_settings().network_name), "{text:?}");
        let title_position = text.find("BIOS SETUP UTILITY").unwrap();
        let title_cell = cpu.guest_text.cells[title_position];
        assert_eq!(title_cell.foreground, 6);
        assert_eq!(title_cell.background, 4);
        let selected_position = text.find("DISK0001").unwrap();
        assert_eq!(cpu.guest_text.cells[selected_position].foreground, 5);
        cpu.signal_keyboard_event(0x83);
        cpu.run(5000).unwrap();
        let text = cpu
            .guest_text
            .cells
            .iter()
            .map(|cell| char::from(cell.character))
            .collect::<String>();
        assert!(text.contains("DISK0002"), "{text:?}");
        let rows = cpu
            .guest_text
            .cells
            .chunks(crate::guest_text::GUEST_TEXT_COLUMNS)
            .map(|row| {
                row.iter()
                    .map(|cell| char::from(cell.character))
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        let device_name_rows = rows
            .iter()
            .filter(|row| row.contains("Second Device Name"))
            .count();
        assert_eq!(device_name_rows, 1, "{rows:?}");
        assert!(
            !rows.iter().any(|row| row.contains("123456789")),
            "{rows:?}"
        );
        assert!(text.contains("LEFT/RIGHT changes device."), "{text:?}");
        assert!(!text.contains("WARNING"), "{text:?}");
        assert!(!text.contains("Save changes with F10"), "{text:?}");
        cpu.signal_keyboard_event(13);
        cpu.run(5000).unwrap();
        let firmware_status = cpu.read(crate::machine::FIRMWARE_STATUS);
        let pending_key = cpu.read(crate::machine::INPUT_KEY);
        assert_eq!(
            cpu.registers[0],
            84,
            "pc={:#x} mode={:?} running={} status={} key={} regs={:?} text={:?}",
            cpu.pc,
            cpu.privilege_mode,
            cpu.running,
            firmware_status,
            pending_key,
            cpu.registers,
            cpu.debug_console.text_snapshot()
        );
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(
            cpu.bios_settings().preferred_disk.as_deref(),
            Some("DISK0002")
        );
        assert!(!cpu.guest_text.enabled);
        cpu.write(crate::machine::FIRMWARE_COMMAND, 1);
        assert_eq!(cpu.read(crate::machine::FIRMWARE_STATUS), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn direct_images_and_source_builds_mount_their_program_assets() {
        let disk = boot_test_disk("direct-program-assets");
        let root = disk.parent().unwrap();
        fs::write(disk.join("probe.txt"), b"installed").unwrap();
        let program = root.join("programs/probe");
        fs::create_dir_all(program.join("src")).unwrap();
        fs::create_dir_all(program.join("assets/disk")).unwrap();
        fs::write(program.join("program.json"), b"{}").unwrap();
        fs::write(program.join("assets/disk/probe.txt"), b"seed").unwrap();
        for (image, expected) in [
            (disk.join("main.bin"), b"installed".as_slice()),
            (program.join("src/main.sc16img"), b"seed".as_slice()),
        ] {
            let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
            configure_disk(&mut cpu, root, &image).unwrap();
            for (offset, byte) in b"probe.txt\0".iter().enumerate() {
                cpu.write(crate::machine::FILESYSTEM_PATH + offset as u32, *byte);
            }
            cpu.write(crate::machine::FILESYSTEM_OPERATION, 0x42);
            cpu.write(crate::machine::FILESYSTEM_LENGTH, 0);
            cpu.write(crate::machine::FILESYSTEM_LENGTH + 1, expected.len() as u8);
            cpu.write(crate::machine::FILESYSTEM_COMMAND, 1);
            assert_eq!(cpu.read(crate::machine::FILESYSTEM_STATUS), 0);
            for (offset, expected_byte) in expected.iter().enumerate() {
                assert_eq!(
                    cpu.read(crate::machine::FILESYSTEM_DATA + offset as u32),
                    *expected_byte
                );
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bios_displays_persists_and_boots_a_maximum_length_program_id() {
        let disk = boot_test_disk("long-program-id");
        let root = disk.parent().unwrap();
        let id = "abcdefghijklmnopqrstuvwxyz012345";
        assert_eq!(id.len(), 32);
        fs::create_dir(root.join("disks")).unwrap();
        let target = root.join("disks").join(id);
        fs::rename(&disk, &target).unwrap();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(target.join("disk.json")).unwrap()).unwrap();
        manifest["id"] = serde_json::json!(id);
        fs::write(
            target.join("disk.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let program = crate::assembler::assemble("LDI R0, 42\nHALT", 0x100).unwrap();
        crate::image::write_image(&target.join("main.bin"), &program).unwrap();
        let firmware =
            crate::assembler::assemble(include_str!("../../firmware/bios.asm"), 0).unwrap();
        fs::create_dir(root.join("firmware")).unwrap();
        crate::image::write_image(&root.join("firmware/bios.bin"), &firmware).unwrap();
        let mut cpu = fresh_runtime_cpu(root, None, Some(id), DEFAULT_PORT).unwrap();
        cpu.run(5000).unwrap();
        assert_eq!(&cpu.memory[0x5160..0x5180], id.as_bytes());
        assert_eq!(cpu.memory[0x5180], 0);
        assert_eq!(cpu.memory[0x5022], 0);
        cpu.signal_keyboard_event(13);
        cpu.run(5000).unwrap();
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(cpu.registers[0], 42);
        let saved =
            crate::bios_settings::BiosSettings::load(&root.join("config").join("bios.json"))
                .unwrap();
        assert_eq!(saved.preferred_disk.as_deref(), Some(id));
        drop(cpu);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bios_settings_persist_preferred_disk_startup_and_network_name() {
        let disk = boot_test_disk("settings");
        let root = disk.parent().unwrap();
        let disks_root = root.join("disks");
        fs::create_dir(&disks_root).unwrap();
        let packaged_disk = disks_root.join("DISK0001");
        fs::rename(&disk, &packaged_disk).unwrap();
        let image = crate::assembler::assemble("LDI R0, 42\nHALT", 0x100).unwrap();
        crate::image::write_image(&packaged_disk.join("main.bin"), &image).unwrap();
        let firmware =
            crate::assembler::assemble(include_str!("../../firmware/bios.asm"), 0).unwrap();
        fs::create_dir_all(root.join("firmware")).unwrap();
        crate::image::write_image(root.join("firmware").join("bios.bin").as_path(), &firmware)
            .unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        fs::write(
            root.join("config").join("bios.json"),
            r#"{"version":1,"settings":{"score":12,"region":"eu"}}"#,
        )
        .unwrap();

        let mut cpu = fresh_runtime_cpu(root, None, None, DEFAULT_PORT).unwrap();
        cpu.run(5000).unwrap();
        let node_id = String::from_utf8(
            (0..16)
                .map(|offset| cpu.read(crate::machine::FIRMWARE_NETWORK_NAME + offset))
                .take_while(|byte| *byte != 0)
                .collect(),
        )
        .unwrap();
        assert_eq!(node_id, cpu.bios_settings().network_name);
        assert_eq!(node_id.len(), 16);
        assert!(node_id.starts_with("SC16-"));
        for _ in 0..2 {
            cpu.signal_keyboard_event(0x81);
            cpu.run(5000).unwrap();
        }
        cpu.signal_keyboard_event(13);
        cpu.run(5000).unwrap();
        let editing_node = cpu
            .guest_text
            .cells
            .iter()
            .map(|cell| char::from(cell.character))
            .collect::<String>();
        assert!(
            editing_node.contains("BIOS SETUP UTILITY"),
            "{editing_node:?}"
        );
        assert!(
            !editing_node.contains("Edit SC-NET node ID"),
            "{editing_node:?}"
        );
        assert!(
            editing_node.contains("Editing in place"),
            "{editing_node:?}"
        );
        let original_node = (0..16)
            .map(|offset| cpu.read(crate::machine::FIRMWARE_NETWORK_NAME + offset))
            .collect::<Vec<_>>();
        cpu.signal_keyboard_event(8);
        cpu.run(30_000).unwrap();
        cpu.signal_keyboard_event(27);
        cpu.run(30_000).unwrap();
        let current_node = (0..16)
            .map(|offset| cpu.read(crate::machine::FIRMWARE_NETWORK_NAME + offset))
            .collect::<Vec<_>>();
        assert_eq!(current_node, original_node);
        cpu.signal_keyboard_event(13);
        cpu.run(30_000).unwrap();
        for _ in 0..16 {
            cpu.signal_keyboard_event(8);
            cpu.run(5000).unwrap();
        }
        for character in b"NODE-0123456789A" {
            cpu.signal_keyboard_event(*character);
            cpu.run(5000).unwrap();
        }
        cpu.signal_keyboard_event(13);
        cpu.run(30_000).unwrap();
        cpu.signal_keyboard_event(0x80);
        cpu.run(30_000).unwrap();
        cpu.signal_keyboard_event(13);
        cpu.run(30_000).unwrap();
        cpu.signal_keyboard_event(0x83);
        cpu.run(30_000).unwrap();
        cpu.signal_keyboard_event(13);
        cpu.run(30_000).unwrap();
        for _ in 0..3 {
            cpu.signal_keyboard_event(0x81);
            cpu.run(30_000).unwrap();
        }
        cpu.signal_keyboard_event(13);
        cpu.run(30_000).unwrap();
        for _ in 0..2 {
            cpu.signal_keyboard_event(8);
            cpu.run(30_000).unwrap();
        }
        for character in b"42" {
            cpu.signal_keyboard_event(*character);
            cpu.run(30_000).unwrap();
        }
        cpu.signal_keyboard_event(13);
        cpu.run(30_000).unwrap();
        let edited_score = cpu
            .guest_text
            .cells
            .iter()
            .map(|cell| char::from(cell.character))
            .collect::<String>();
        assert!(edited_score.contains("score"), "{edited_score:?}");
        assert!(
            !edited_score.contains("Editing in place"),
            "{edited_score:?}"
        );
        assert_eq!(
            cpu.bios_settings().additional.get("score"),
            Some(&crate::bios_settings::BiosSettingValue::Integer(42)),
            "{:?}",
            cpu.debug_console.text_snapshot()
        );
        cpu.signal_keyboard_event(0x89);
        cpu.run(5000).unwrap();

        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("config").join("bios.json")).unwrap())
                .unwrap();
        let firmware_status = cpu.read(crate::machine::FIRMWARE_STATUS);
        assert_eq!(
            saved["preferred_disk"],
            "DISK0001",
            "saved settings: {saved}; BIOS status: {}; CPU: PC={:#x} mode={:?} registers={:?} logs={:?}",
            firmware_status,
            cpu.pc,
            cpu.privilege_mode,
            cpu.registers,
            cpu.debug_console.text_snapshot()
        );
        assert_eq!(saved["startup_behavior"], "boot_preferred");
        assert_eq!(saved["network_name"], "NODE-0123456789A");
        assert_eq!(saved["settings"]["score"], 42);
        assert_eq!(saved["settings"]["region"], "eu");
        drop(cpu);

        let mut setup_entry = fresh_runtime_cpu(root, None, None, DEFAULT_PORT).unwrap();
        setup_entry.signal_keyboard_event(b' ');
        setup_entry.run(5000).unwrap();
        assert_eq!(setup_entry.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(setup_entry.memory[BIOS_AUTOBOOT_WAIT_MARKER as usize], 0);
        let setup_text = setup_entry
            .guest_text
            .cells
            .iter()
            .map(|cell| char::from(cell.character))
            .collect::<String>();
        assert!(setup_text.contains("SETTINGS"), "{setup_text}");
        drop(setup_entry);

        let mut next_boot = fresh_runtime_cpu(root, None, None, DEFAULT_PORT).unwrap();
        assert_eq!(
            next_boot.bios_settings().startup_behavior,
            crate::bios_settings::StartupBehavior::BootPreferred
        );
        assert_eq!(
            next_boot.bios_settings().preferred_disk.as_deref(),
            Some("DISK0001")
        );
        assert_eq!(next_boot.bios_settings().network_name, "NODE-0123456789A");
        next_boot.run(5000).unwrap();
        assert_eq!(next_boot.privilege_mode, PrivilegeMode::Supervisor);
        assert_eq!(next_boot.memory[BIOS_AUTOBOOT_WAIT_MARKER as usize], 1);
        next_boot.signal_keyboard_event(0x8a);
        next_boot.run(5000).unwrap();
        assert_eq!(next_boot.privilege_mode, PrivilegeMode::User);
        assert_eq!(next_boot.registers[0], 42);
        drop(next_boot);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bios_enter_saves_and_boots_selected_higher_index_disk() {
        let disk = boot_test_disk("preferred-index");
        let root = disk.parent().unwrap().to_path_buf();
        let disks = root.join("disks");
        fs::create_dir(&disks).unwrap();
        for id in ["DISK0001", "DISK0002", "DISK0003"] {
            let path = disks.join(id);
            fs::create_dir(&path).unwrap();
            fs::write(
                path.join("disk.json"),
                format!(
                    r#"{{"version":1,"id":"{id}","label":"{id}","capacity_bytes":65536,"read_only":false}}"#
                ),
            )
            .unwrap();
            let result = if id == "DISK0003" { 84 } else { 42 };
            let image =
                crate::assembler::assemble(&format!("LDI R0, {result}\nHALT"), 0x100).unwrap();
            crate::image::write_image(&path.join("main.bin"), &image).unwrap();
        }
        fs::remove_dir_all(&disk).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        fs::write(
            root.join("config").join("bios.json"),
            r#"{"version":1,"preferred_disk":"DISK0001","startup_behavior":"boot_preferred"}"#,
        )
        .unwrap();
        let firmware =
            crate::assembler::assemble(include_str!("../../firmware/bios.asm"), 0).unwrap();
        fs::create_dir_all(root.join("firmware")).unwrap();
        crate::image::write_image(&root.join("firmware").join("bios.bin"), &firmware).unwrap();

        let mut cpu = fresh_runtime_cpu(&root, None, None, DEFAULT_PORT).unwrap();
        cpu.run(5000).unwrap();
        cpu.signal_keyboard_event(b' ');
        cpu.run(5000).unwrap();
        assert_eq!(cpu.memory[BIOS_AUTOBOOT_WAIT_MARKER as usize], 0);

        for _ in 0..2 {
            cpu.signal_keyboard_event(0x83);
            cpu.run(5000).unwrap();
        }
        let selected_index = cpu.read_u16(crate::machine::FIRMWARE_INDEX);
        assert_eq!(
            selected_index,
            2,
            "BIOS disk index, row, PC, and screen: index={}, row={}, PC={:#x}, screen={}",
            selected_index,
            cpu.memory[0x5021],
            cpu.pc,
            cpu.guest_text
                .cells
                .iter()
                .map(|cell| char::from(cell.character))
                .collect::<String>()
        );
        assert!(
            cpu.console.receive_buffer().is_empty(),
            "BIOS disk-selection Enter leaked into SisuOS stdin"
        );
        cpu.signal_keyboard_event(13);
        cpu.run(20_000).unwrap();

        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert_eq!(cpu.registers[0], 84);
        assert_eq!(
            cpu.bios_settings().preferred_disk.as_deref(),
            Some("DISK0003")
        );
        assert_eq!(
            cpu.bios_settings().startup_behavior,
            crate::bios_settings::StartupBehavior::BootPreferred
        );
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("config").join("bios.json")).unwrap())
                .unwrap();
        assert_eq!(saved["preferred_disk"], "DISK0003");
        assert_eq!(saved["startup_behavior"], "boot_preferred");
        drop(cpu);

        let mut automatic = fresh_runtime_cpu(&root, None, None, DEFAULT_PORT).unwrap();
        automatic.run(5000).unwrap();
        assert_eq!(automatic.memory[BIOS_AUTOBOOT_WAIT_MARKER as usize], 1);
        automatic.signal_keyboard_event(0x8a);
        automatic.run(5000).unwrap();
        assert_eq!(automatic.privilege_mode, PrivilegeMode::User);
        assert_eq!(automatic.registers[0], 84);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sisu_os_boots_from_bios_and_runs_preemptive_shell_commands() {
        let root = std::env::temp_dir().join(format!(
            "sc16-sisu-os-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let disks = root.join("disks");
        fs::create_dir_all(&disks).unwrap();
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
        let checked_in_disks = repository.join("disks");
        let checked_in_disk = crate::disk::FolderDisk::discover_paths(&checked_in_disks)
            .unwrap()
            .into_iter()
            .find(|path| {
                crate::disk::FolderDisk::open(path)
                    .is_ok_and(|disk| disk.manifest().label == "SisuOS")
            })
            .expect("checked-in SisuOS folder disk");
        let boot_disk_id = checked_in_disk
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let kernel_source = repository
            .join("programs")
            .join("sisuos")
            .join("src")
            .join("preemptive_os_kernel.asm");
        let shell_source = repository
            .join("programs")
            .join("sisuos")
            .join("src")
            .join("preemptive_os_shell.c");
        let demo_source = repository
            .join("programs")
            .join("sisuos")
            .join("src")
            .join("preemptive_os_demo.c");
        let kernel =
            crate::image::build_kernel_with_embedded_c_source(&kernel_source, &shell_source)
                .unwrap();
        let demo = crate::image::build_c_program_at(&demo_source, 0x30000).unwrap();
        assert_eq!(
            crate::image::read_image(&checked_in_disk.join("kernel.bin")).unwrap(),
            kernel,
            "checked-in kernel image is stale; run programs\\sisuos\\tools\\build.ps1"
        );
        assert_eq!(
            crate::image::read_image(&checked_in_disk.join("demo.bin")).unwrap(),
            demo,
            "checked-in C demo image is stale; run programs\\sisuos\\tools\\build.ps1"
        );
        let manifest = crate::disk::FolderDisk::open(&checked_in_disk).unwrap();
        assert_eq!(manifest.manifest().id, boot_disk_id);
        assert_eq!(manifest.manifest().label, "SisuOS");
        assert_eq!(
            manifest.manifest().protected_files,
            ["kernel.bin", "demo.bin"]
        );
        for entry in fs::read_dir(repository.join("disks")).unwrap() {
            let source_disk = entry.unwrap().path();
            if !source_disk.is_dir() {
                continue;
            }
            let target_disk = disks.join(source_disk.file_name().unwrap());
            fs::create_dir_all(&target_disk).unwrap();
            for file in fs::read_dir(source_disk).unwrap() {
                let source_file = file.unwrap().path();
                if source_file.is_file() {
                    fs::copy(
                        &source_file,
                        target_disk.join(source_file.file_name().unwrap()),
                    )
                    .unwrap();
                }
            }
        }
        let firmware =
            crate::assembler::assemble_file(&repository.join("firmware").join("bios.asm"), 0)
                .unwrap();
        assert_eq!(
            crate::image::read_image(&repository.join("firmware").join("bios.bin")).unwrap(),
            firmware,
            "checked-in BIOS image is stale; run .\\run.ps1 to rebuild it"
        );
        fs::create_dir_all(root.join("firmware")).unwrap();
        fs::copy(
            repository.join("firmware").join("bios.bin"),
            root.join("firmware").join("bios.bin"),
        )
        .unwrap();
        let mut settings = crate::bios_settings::BiosSettings::default();
        settings.preferred_disk = Some(boot_disk_id);
        settings
            .save(&root.join("config").join("bios.json"))
            .unwrap();

        let mut cpu = fresh_runtime_cpu(&root, None, None, DEFAULT_PORT).unwrap();
        cpu.run(5000).unwrap();
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        let selected_index = cpu.read_u16(crate::machine::FIRMWARE_INDEX);
        assert_eq!(
            FolderDisk::discover_paths(&disks).unwrap()[usize::from(selected_index)]
                .file_name()
                .unwrap(),
            checked_in_disk.file_name().unwrap()
        );
        cpu.receive_console_input(b"\n");
        cpu.signal_keyboard_event(13);
        let mut executed = 0;
        while executed < 30_000 && cpu.running {
            cpu.begin_frame();
            let mut budget = 5_000;
            match run_instruction_slice(&mut cpu, &mut budget) {
                Ok(frame_executed) => executed += frame_executed,
                Err(error) => panic!(
                    "{}",
                    runtime_cpu_failure_details(&cpu, None, Some(&checked_in_disk), &error)
                ),
            }
        }
        let firmware_status = cpu.read(crate::machine::FIRMWARE_STATUS);
        assert_eq!(
            cpu.privilege_mode,
            PrivilegeMode::User,
            "SisuOS did not enter user mode: PC={:#x}, status={}, running={}, logs={:?}, screen={}",
            cpu.pc,
            firmware_status,
            cpu.running,
            cpu.debug_console.text_snapshot(),
            cpu.guest_text
                .cells
                .iter()
                .map(|cell| char::from(cell.character))
                .collect::<String>()
        );
        update_rtc(&mut cpu);
        cpu.run(1000).unwrap();
        assert!(cpu.running, "SisuOS stopped after the RTC interrupt");
        let long_command = "x".repeat(80);
        for (index, command) in [
            "help",
            "list",
            "info",
            "run demo",
            "unknown",
            long_command.as_str(),
        ]
        .into_iter()
        .enumerate()
        {
            if index == 0 {
                cpu.receive_console_input(b"\x1b[A");
                receive_typed_text(&mut cpu, "helpx".chars());
                receive_terminal_key(&mut cpu, KeyCode::Backspace);
            } else {
                receive_typed_text(&mut cpu, command.chars());
            }
            receive_terminal_key(&mut cpu, KeyCode::Enter);
        }
        cpu.run(500_000).unwrap();

        let output = String::from_utf8_lossy(cpu.console.output()).into_owned();
        for expected in [
            "SisuOS interactive shell",
            "Type 'run demo' to see preemptive task switching.",
            "Commands: help, list, run demo, info",
            "Programs: demo",
            "SisuOS | Protected mode | Syscall ABI 1 | Preemptive scheduler active",
            "demo task ran",
            "demo exited with status 7",
            "Unknown command. Type help.",
            "Command too long (79 characters maximum)",
        ] {
            assert!(
                output.contains(expected),
                "missing {expected:?}; output={output}; PC={:#x}, running={}, privilege={:?}, fault={:?}, mappings={:02x?}/{:02x?}/{:02x?}, logs={:?}, screen={:?}",
                cpu.pc,
                cpu.running,
                cpu.privilege_mode,
                cpu.last_page_fault(),
                &cpu.memory[0x3040..0x3044],
                &cpu.memory[0x3050..0x3054],
                &cpu.memory[0x3240..0x3244],
                cpu.debug_console.text_snapshot(),
                cpu.console.text_snapshot()
            );
        }
        assert_eq!(
            cpu.read_u16(crate::machine::INTERRUPT_PIT_PERIOD),
            4,
            "SisuOS kernel did not program the high-address PIT bank"
        );
        let screen = cpu.console.text_snapshot().join("\n");
        assert!(cpu.terminal_active);
        assert!(cpu.terminal_dirty);
        assert!(screen.contains("SisuOS interactive shell"), "{screen}");
        assert!(
            screen.contains("Commands: help, list, run demo, info"),
            "{screen}"
        );
        assert!(
            !screen.contains("[INFO]") && !screen.contains("[DEBUG]"),
            "debug messages leaked into the guest console: {screen}"
        );
        assert!(
            !String::from_utf8_lossy(&cpu.debug_console.output())
                .contains("SisuOS interactive shell"),
            "guest console output leaked into Bug Monitor"
        );
        let child_progress = output.find("demo task ran").unwrap();
        let parent_wait = output.find("Waiting for demo...").unwrap();
        assert!(
            child_progress < parent_wait,
            "child did not progress while the parent was runnable: {output}"
        );
        assert_eq!(cpu.current_task(), Some(0));
        assert_eq!(
            cpu.task_context(0).unwrap().state,
            crate::scheduler::TaskState::Running
        );
        assert_eq!(
            cpu.task_context(1).unwrap().state,
            crate::scheduler::TaskState::Exited
        );
        assert_eq!(cpu.task_context(1).unwrap().exit_code, Some(7));
        assert_eq!(cpu.read_u16(crate::machine::INTERRUPT_PIT_PERIOD), 4);
        assert!(cpu.is_mmu_enabled());

        cpu.test_monotonic_ms = Some(1000);
        for command in ["run demo 2", "run demo 3", "ps", "help"] {
            receive_typed_text(&mut cpu, command.chars());
            receive_terminal_key(&mut cpu, KeyCode::Enter);
        }
        cpu.run(500_000).unwrap();
        let output = String::from_utf8_lossy(cpu.console.output()).into_owned();
        assert!(output.contains("Started background process 2"), "{output}");
        assert!(output.contains("Started background process 3"), "{output}");
        assert!(
            output.contains("PID PPID STATE PROGRAM\n0 - running sisuos\n"),
            "{output}"
        );
        assert!(output.contains("2 0 sleeping demo.bin\n"), "{output}");
        assert!(output.contains("3 0 sleeping demo.bin\n"), "{output}");
        assert!(
            output.contains("running=on CPU, ready=waiting for CPU\n"),
            "{output}"
        );
        assert_eq!(
            cpu.task_context(2).unwrap().state,
            crate::scheduler::TaskState::Blocked
        );
        assert_eq!(
            cpu.task_context(3).unwrap().state,
            crate::scheduler::TaskState::Blocked
        );
        assert!(!output.contains("BGP "), "{output}");
        assert!(output.ends_with("sc16> "), "{output}");

        cpu.test_monotonic_ms = Some(1999);
        cpu.run(100_000).unwrap();
        assert!(!String::from_utf8_lossy(cpu.console.output()).contains("BGP "));
        for second in 1..=3 {
            cpu.test_monotonic_ms = Some(1000 + second * 1000);
            cpu.run(200_000).unwrap();
            let output = String::from_utf8_lossy(cpu.console.output());
            for pid in [2, 3] {
                let duration = pid;
                if second <= duration {
                    let line = format!("BGP {pid} time {second}/{duration}\n");
                    assert_eq!(output.matches(&line).count(), 1, "{output}");
                }
            }
        }
        for pid in [2, 3] {
            assert_eq!(cpu.task_context(pid).unwrap().exit_code, Some(0));
        }
        assert_eq!(
            String::from_utf8_lossy(cpu.console.output())
                .matches("BGP ")
                .count(),
            5
        );
        let start = cpu.console.output().len();
        for command in [
            "ps",
            "run demo 0",
            "run demo -1",
            "run demo 65536",
            "run demo 2x",
        ] {
            receive_typed_text(&mut cpu, command.chars());
            receive_terminal_key(&mut cpu, KeyCode::Enter);
        }
        cpu.run(300_000).unwrap();
        let output = String::from_utf8_lossy(&cpu.console.output()[start..]);
        assert!(
            output.contains("PID PPID STATE PROGRAM\n0 - running sisuos\n"),
            "{output}"
        );
        assert!(
            !output.contains("2 0 ") && !output.contains("3 0 "),
            "{output}"
        );
        assert_eq!(output.matches("Usage: run demo X").count(), 4, "{output}");
        cpu.test_monotonic_ms = Some((1 << 30) - 500);
        receive_typed_text(&mut cpu, "run demo 1".chars());
        receive_terminal_key(&mut cpu, KeyCode::Enter);
        cpu.run(150_000).unwrap();
        cpu.test_monotonic_ms = Some(499);
        cpu.run(100_000).unwrap();
        assert!(!String::from_utf8_lossy(cpu.console.output()).contains("BGP 4"));
        cpu.test_monotonic_ms = Some(500);
        cpu.run(150_000).unwrap();
        assert_eq!(
            String::from_utf8_lossy(cpu.console.output())
                .matches("BGP 4 time 1/1\n")
                .count(),
            1
        );
        assert_eq!(cpu.task_context(4).unwrap().exit_code, Some(0));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bios_startup_rejects_missing_or_invalid_common_firmware() {
        let disk = boot_test_disk("missing-bios");
        let root = disk.parent().unwrap();
        let missing = match fresh_runtime_cpu(root, None, None, DEFAULT_PORT) {
            Err(error) => error.to_string(),
            Ok(_) => panic!("startup unexpectedly accepted missing firmware"),
        };
        assert!(missing.contains("required common BIOS is missing"));
        fs::create_dir_all(root.join("firmware")).unwrap();
        fs::write(root.join("firmware").join("bios.bin"), b"invalid firmware").unwrap();
        let invalid = match fresh_runtime_cpu(root, None, None, DEFAULT_PORT) {
            Err(error) => error.to_string(),
            Ok(_) => panic!("startup unexpectedly accepted invalid firmware"),
        };
        assert!(invalid.contains("invalid SC-16 image"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bios_boot_reports_both_not_boot_cases_and_recovers_with_a_valid_disk() {
        let disk = boot_test_disk("not-boot");
        let target = BootTarget {
            source: disk.clone(),
            folder_disk: true,
        };
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.pc = 0x80;
        let error = boot_target(&mut cpu, &target).unwrap_err().to_string();
        assert!(error.contains("NOT_BOOT"));
        assert_eq!(cpu.pc, 0x80);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        let image = crate::assembler::assemble("LDI R0, 27\nHALT", 0x100).unwrap();
        for file in ["main.bin", "kernel.bin"] {
            crate::image::write_image(&disk.join(file), &image).unwrap();
        }
        let error = boot_target(&mut cpu, &target).unwrap_err().to_string();
        assert!(error.contains("NOT_BOOT"));
        assert_eq!(cpu.pc, 0x80);
        fs::remove_file(disk.join("kernel.bin")).unwrap();
        boot_target(&mut cpu, &target).unwrap();
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        cpu.run(10).unwrap();
        assert_eq!(cpu.registers[0], 27);
        assert!(
            cpu.debug_console
                .text_snapshot()
                .join("\n")
                .contains("NOT_BOOT")
        );
        fs::remove_dir_all(disk.parent().unwrap()).unwrap();
    }

    #[test]
    fn bios_boot_revalidates_metadata_and_rejects_invalid_images_without_transfer() {
        let disk = boot_test_disk("validation");
        let target = BootTarget {
            source: disk.clone(),
            folder_disk: true,
        };
        let disks = discover_boot_disks(disk.parent().unwrap()).unwrap();
        assert_eq!(disks.len(), 1);
        let manifest = fs::read_to_string(disk.join("disk.json")).unwrap();
        let image = crate::assembler::assemble("HALT", 0x100).unwrap();
        crate::image::write_image(&disk.join("main.bin"), &image).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.pc = 0x80;
        for invalid in [
            "{}".to_owned(),
            manifest.replace("DISK0001", "DISK0002"),
            manifest.replace(
                "\"read_only\":true",
                "\"read_only\":true,\"mode\":\"kernel\"",
            ),
            manifest.replace("65536", "1"),
        ] {
            fs::write(disk.join("disk.json"), invalid).unwrap();
            assert!(boot_target(&mut cpu, &target).is_err());
            assert_eq!(cpu.pc, 0x80);
            assert_eq!(cpu.privilege_mode, PrivilegeMode::Supervisor);
        }
        fs::write(disk.join("disk.json"), manifest).unwrap();
        fs::write(disk.join("main.bin"), b"not an SC16 image").unwrap();
        assert!(boot_target(&mut cpu, &target).is_err());
        assert_eq!(cpu.pc, 0x80);
        crate::image::write_image(&disk.join("main.bin"), &image).unwrap();
        let oversized_image = crate::assembler::assemble("HALT", 0x200000).unwrap();
        crate::image::write_image(&disk.join("main.bin"), &oversized_image).unwrap();
        cpu.registers[3] = 1234;
        cpu.sp = 0x6000;
        cpu.enable_mmu(0x9000).unwrap();
        cpu.privilege_mode = PrivilegeMode::User;
        let memory = cpu.memory.to_vec();
        assert!(boot_target(&mut cpu, &target).is_err());
        assert_eq!(&cpu.memory[..], memory);
        assert_eq!(cpu.pc, 0x80);
        assert_eq!(cpu.sp, 0x6000);
        assert_eq!(cpu.registers[3], 1234);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert!(cpu.is_mmu_enabled());
        assert_eq!(cpu.page_directory(), 0x9000);
        let direct_target = BootTarget {
            source: disk.join("main.bin"),
            folder_disk: false,
        };
        assert!(boot_target(&mut cpu, &direct_target).is_err());
        assert_eq!(&cpu.memory[..], memory);
        assert_eq!(cpu.pc, 0x80);
        assert_eq!(cpu.sp, 0x6000);
        assert_eq!(cpu.registers[3], 1234);
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        assert!(cpu.is_mmu_enabled());
        assert_eq!(cpu.page_directory(), 0x9000);
        crate::image::write_image(&disk.join("main.bin"), &image).unwrap();
        boot_target(&mut cpu, &target).unwrap();
        assert_eq!(cpu.privilege_mode, PrivilegeMode::User);
        fs::remove_dir_all(disk.parent().unwrap()).unwrap();
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
        assert_eq!(
            [
                cpu.read(crate::machine::INPUT_KEYS + 1),
                cpu.read(crate::machine::INPUT_KEYS + 2),
                cpu.read(crate::machine::INPUT_KEYS + 3),
            ],
            [0x21, 0x05, 0x41]
        );

        previous = current;
        let current = HashSet::from([KeyCode::D, KeyCode::E, KeyCode::Z, KeyCode::J]);
        update_keys(&mut cpu, &previous, &current, false);
        assert_eq!(
            [
                cpu.read(crate::machine::INPUT_KEYS + 1),
                cpu.read(crate::machine::INPUT_KEYS + 2),
                cpu.read(crate::machine::INPUT_KEYS + 3),
            ],
            [0x02, 0x0a, 0x02]
        );
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
        assert_eq!(key_to_ascii(KeyCode::Up, false), Some(0x80));
        assert_eq!(key_to_ascii(KeyCode::Down, false), Some(0x81));
        assert_eq!(key_to_ascii(KeyCode::Space, true), Some(b' '));
        assert_eq!(key_to_ascii(KeyCode::F10, false), None);
    }

    #[test]
    fn display_text_input_preserves_unicode_characters_for_console_mmio() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        receive_typed_text(&mut cpu, ['k', 'ä', '!', '🙂']);
        assert!(cpu.console.receive_buffer().is_empty());
        cpu.terminal_active = true;
        receive_typed_text(&mut cpu, ['k', 'ä', '!', '\r', '\n', '🙂']);
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
    fn terminal_input_ignores_bios_keys_and_routes_keys_after_guest_boot() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        receive_terminal_key(&mut cpu, KeyCode::Enter);
        receive_terminal_key(&mut cpu, KeyCode::Backspace);
        receive_typed_text(&mut cpu, ['h', 'e', 'l', 'p']);
        assert!(cpu.console.receive_buffer().is_empty());
        assert_eq!(cpu.read(crate::machine::INPUT_KEY), 8);

        cpu.terminal_active = true;
        receive_terminal_key(&mut cpu, KeyCode::Enter);
        receive_terminal_key(&mut cpu, KeyCode::Backspace);
        receive_typed_text(&mut cpu, ['h', 'e', 'l', 'p']);
        assert_eq!(
            cpu.console
                .receive_buffer()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            b"\n\x08help"
        );
        cpu.console.clear_input();
        receive_terminal_key(&mut cpu, KeyCode::Left);
        assert_eq!(
            cpu.console
                .receive_buffer()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            b"\x1b[D"
        );
    }

    #[test]
    fn bios_autoboot_wait_requires_five_seconds_and_gives_space_priority() {
        let now = Instant::now();
        let mut started_at = None;
        let mut timeout_sent = false;

        assert!(!bios_autoboot_timeout_due(
            true,
            false,
            &mut started_at,
            &mut timeout_sent,
            now,
        ));
        assert!(!bios_autoboot_timeout_due(
            true,
            false,
            &mut started_at,
            &mut timeout_sent,
            now + Duration::from_millis(4_999),
        ));
        assert!(!bios_autoboot_timeout_due(
            true,
            true,
            &mut started_at,
            &mut timeout_sent,
            now + Duration::from_secs(5),
        ));
        assert!(bios_autoboot_timeout_due(
            true,
            false,
            &mut started_at,
            &mut timeout_sent,
            now + Duration::from_secs(5),
        ));
        assert!(!bios_autoboot_timeout_due(
            true,
            false,
            &mut started_at,
            &mut timeout_sent,
            now + Duration::from_secs(6),
        ));
        assert!(!bios_autoboot_timeout_due(
            false,
            false,
            &mut started_at,
            &mut timeout_sent,
            now + Duration::from_secs(6),
        ));
        assert!(!timeout_sent);
    }

    #[test]
    fn halted_display_waits_for_a_keypress_without_closing_a_running_cpu() {
        assert!(!halted_window_should_close(false, false));
        assert!(halted_window_should_close(false, true));
        assert!(!halted_window_should_close(true, true));
        assert!(runtime_should_close(true, true, false));
        assert!(runtime_should_close(false, false, true));
        assert!(!runtime_should_close(false, true, false));
    }

    #[test]
    fn runtime_fullscreen_control_toggles_without_changing_monitor_visibility() {
        let (mut fullscreen, mut console_visible) = (false, false);
        assert_eq!(
            apply_runtime_control(
                KeyCode::F11,
                &mut fullscreen,
                &mut console_visible,
                false,
                false,
            ),
            Some(RuntimeControl::FullscreenToggled(true))
        );
        assert_eq!(
            apply_runtime_control(
                KeyCode::F11,
                &mut fullscreen,
                &mut console_visible,
                false,
                false,
            ),
            Some(RuntimeControl::FullscreenToggled(false))
        );
        assert!(!fullscreen);
        assert!(!console_visible);
        assert_eq!(
            apply_runtime_control(
                KeyCode::A,
                &mut fullscreen,
                &mut console_visible,
                false,
                false,
            ),
            None
        );
    }

    #[test]
    fn runtime_only_reserves_f2_for_the_bug_monitor() {
        let (mut fullscreen, mut console_visible) = (false, false);
        let mut control =
            |key| apply_runtime_control(key, &mut fullscreen, &mut console_visible, false, false);

        for key in [KeyCode::F1, KeyCode::F3, KeyCode::F4, KeyCode::F5] {
            assert_eq!(control(key), None);
        }
        assert_eq!(
            control(KeyCode::F2),
            Some(RuntimeControl::ConsoleToggled(true))
        );
    }

    #[test]
    fn runtime_shift_control_f_toggles_fullscreen_and_f2_only_opens_windowed_console() {
        let (mut fullscreen, mut console_visible) = (false, false);
        assert_eq!(
            apply_runtime_control(
                KeyCode::F,
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
            toolbar_control((50.0, 18.0), false),
            Some(RuntimeControl::ConsoleToggled(true))
        );
        assert_eq!(
            toolbar_control((50.0, 18.0), true),
            Some(RuntimeControl::ConsoleToggled(false))
        );
        for x in [150.0, 240.0, 330.0, 440.0] {
            assert_eq!(toolbar_control((x, 18.0), false), None);
        }
        assert_eq!(toolbar_control((120.0, 18.0), false), None);
        assert_eq!(toolbar_control((50.0, 34.0), false), None);
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
        assert_eq!(cpu.read_u16(crate::machine::INPUT_MOUSE_X), 40);
        assert_eq!(cpu.read_u16(crate::machine::INPUT_MOUSE_Y), 30);
        assert_eq!(cpu.read(crate::machine::INPUT_MOUSE_BUTTONS), 1);
        assert_eq!(cpu.read_u16(crate::machine::INPUT_MOUSE_WHEEL), u16::MAX);

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
        assert_eq!(cpu.read(crate::machine::INPUT_MOUSE_BUTTONS), 1);
        assert_eq!(cpu.read(crate::machine::INPUT_MOUSE_EVENTS), 1 | 4 | 16);
        assert_eq!(cpu.read(crate::machine::INPUT_MOUSE_EVENTS), 0);
        assert_eq!(cpu.read_u16(crate::machine::INPUT_MOUSE_WHEEL), 1);
        cpu.end_mouse_frame();
        assert_eq!(cpu.read_u16(crate::machine::INPUT_MOUSE_WHEEL), 0);
    }

    #[test]
    fn display_renderer_maps_vram_palette_and_resolution_to_rgb_pixels() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.vram[0] = 1;
        cpu.vram[1] = 25;
        cpu.palette[1] = [255, 255, 255];
        cpu.palette[2] = [0, 0, 255];
        cpu.palette[25] = [0, 0, 255];
        let mut pixels = vec![0; 640 * 480];

        render_cpu(&cpu, &mut pixels, &load_font());
        assert_eq!(pixels[0], 0x00ff_ffff);
        assert_eq!(pixels[1], 0x0000_00ff);
        assert_eq!(pixels.len(), 640 * 480);

        cpu.set_graphics_mode(1).unwrap();
        cpu.vram[0] = 0x12;
        let mut high_resolution_pixels = vec![0; 640 * 480];
        render_cpu(&cpu, &mut high_resolution_pixels, &load_font());
        assert_eq!(high_resolution_pixels[0], 0x00ff_ffff);
        assert_eq!(high_resolution_pixels[1], 0x0000_00ff);
        assert_eq!(screen_size(&cpu), (640, 480));

        cpu.set_graphics_mode(0).unwrap();
        assert_eq!(screen_size(&cpu), (320, 240));
    }

    #[test]
    fn display_refresh_requires_a_presented_frame_or_enabled_text_display() {
        assert!(!should_capture_display_frame(false, false));
        assert!(should_capture_display_frame(true, false));
        assert!(should_capture_display_frame(false, true));
    }

    #[test]
    fn game_frame_renders_high_graphics_bank_operations() {
        use crate::machine::*;

        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.palette[2] = [12, 34, 56];
        cpu.palette[3] = [78, 90, 123];

        cpu.write(GRAPHICS_COLOR, 2);
        cpu.write_u16(GRAPHICS_X, 0);
        cpu.write_u16(GRAPHICS_Y, 0);
        cpu.write(GRAPHICS_OPERATION, GRAPHICS_OP_PIXEL);
        cpu.write(GRAPHICS_COMMAND, 1);
        cpu.write(GRAPHICS_COLOR, 3);
        cpu.write_u16(GRAPHICS_X, 1);
        cpu.write(GRAPHICS_OPERATION, GRAPHICS_OP_PIXEL);
        cpu.write(GRAPHICS_COMMAND, 1);

        let mut pixels = vec![0; 640 * 480];
        render_cpu(&cpu, &mut pixels, &load_font());
        assert_eq!(pixels[0], rgb(12, 34, 56));
        assert_eq!(pixels[1], rgb(78, 90, 123));
    }

    #[test]
    fn guest_text_overlays_graphics_at_both_display_resolutions() {
        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.set_graphics_mode(1).unwrap();
        cpu.vram[0] = 0x10;
        cpu.palette[1] = [0, 255, 0];
        cpu.palette[2] = [0, 0, 255];
        cpu.palette[3] = [255, 0, 0];
        cpu.write(crate::machine::FIRMWARE_TEXT_FOREGROUND, 3);
        cpu.write(crate::machine::FIRMWARE_TEXT_BACKGROUND, 2);
        cpu.write(crate::machine::FIRMWARE_TEXT_DATA, b'A');
        cpu.write(crate::machine::FIRMWARE_TEXT_CONTROL, 2);

        let font = load_font();
        let mut high_resolution_pixels = vec![0; 640 * 480];
        render_cpu(&cpu, &mut high_resolution_pixels, &font);
        assert_eq!(high_resolution_pixels[0], rgb(0, 255, 0));
        assert_eq!(
            high_resolution_pixels[BIOS_TEXT_TOP * 640 + BIOS_TEXT_LEFT],
            rgb(0, 0, 255)
        );
        assert!(
            high_resolution_pixels
                [BIOS_TEXT_TOP * 640 + BIOS_TEXT_LEFT..(BIOS_TEXT_TOP + 16) * 640]
                .contains(&rgb(255, 0, 0))
        );

        cpu.set_graphics_mode(0).unwrap();
        cpu.vram[0] = 1;
        let mut low_resolution_pixels = vec![0; 640 * 480];
        render_cpu(&cpu, &mut low_resolution_pixels, &font);
        let x = (320 - crate::guest_text::GUEST_TEXT_COLUMNS * 6) / 2;
        let y = (240 - crate::guest_text::GUEST_TEXT_ROWS * 8) / 2;
        assert_eq!(low_resolution_pixels[0], rgb(0, 255, 0));
        assert_eq!(low_resolution_pixels[y * 320 + x], rgb(0, 0, 255));
        assert!(low_resolution_pixels[y * 320 + x..(y + 8) * 320].contains(&rgb(255, 0, 0)));
    }

    #[test]
    fn high_audio_commands_feed_the_existing_pcm_renderer() {
        use crate::machine::*;

        for waveform in 0..=2 {
            let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
            cpu.write(AUDIO_CHANNEL, 2);
            cpu.write_u16(AUDIO_FREQUENCY, 440);
            cpu.write_u16(AUDIO_FRAMES, 60);
            cpu.write_u16(AUDIO_WAVEFORM, waveform);
            cpu.write_u16(AUDIO_VOLUME, 50);
            cpu.write(AUDIO_COMMAND, 1);
            let commands = cpu.take_sound_commands();
            assert_eq!(commands.len(), 1);
            let command = commands[0];
            assert_eq!(command.channel, 2);
            let wav = synthesize_sound(command);
            assert_eq!(&wav[..4], b"RIFF");
            assert_eq!(&wav[8..12], b"WAVE");
            assert_eq!(wav.len(), 44 + 44_100);
            assert!(wav[44..].iter().any(|byte| *byte != 0));
            cpu.write(AUDIO_COMMAND, 2);
            assert_eq!(cpu.take_sound_commands()[0].frequency, 0);
        }
    }

    #[test]
    fn sound_synthesis_produces_valid_pcm_wav_with_requested_duration() {
        let wav = synthesize_sound(crate::machine::SoundCommand {
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
        let wav = synthesize_sound(crate::machine::SoundCommand {
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
    fn bios_keyboard_disk_selection_clamps() {
        assert_eq!(bios_move_selection(0, -1, 5), 0);
        assert_eq!(bios_move_selection(0, 1, 5), 1);
        assert_eq!(bios_move_selection(4, 1, 5), 4);
        assert_eq!(bios_move_selection(4, -1, 5), 3);
        assert_eq!(bios_move_selection(99, 0, 5), 4);
        assert_eq!(bios_move_selection(99, 1, 0), 0);
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
    fn bios_mouse_selects_only_visible_disks() {
        assert_eq!(
            bios_mouse_action((150.0, 140.0), (640.0, 480.0), 20, 2),
            Some(BiosMouseAction::SelectDisk(2))
        );
        assert_eq!(
            bios_mouse_action((150.0, 164.0), (640.0, 480.0), 20, 2),
            Some(BiosMouseAction::SelectDisk(3))
        );
        assert_eq!(
            bios_mouse_action((200.0, 96.0), (640.0, 480.0), 20, 0),
            None
        );
        assert_eq!(
            bios_mouse_action((550.0, 96.0), (640.0, 480.0), 20, 0),
            None
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
    fn bios_text_can_render_at_double_size_without_wrapping() {
        let font = vec![[0xf8; 8]; 128];
        let mut pixels = vec![0; 24 * 20];
        draw_text_scaled(&mut pixels, 24, (1, 1), "A", 1, &font, 2);
        assert_eq!(pixels[25], 1);
        assert_eq!(pixels[2 * 24 + 2], 1);
        assert_eq!(pixels[14 * 24 + 10], 1);
        assert_eq!(pixels[37], 0);
        assert_eq!(pixels[15 * 24 + 1], 0);
    }

    #[test]
    fn bios_text_renders_crisp_double_size_pixels_without_effects() {
        let mut font = vec![[0; 8]; 128];
        font[usize::from(b'A' - 32)][0] = 0x80;
        let mut pixels = vec![0; 640 * 480];

        draw_bios_text(&mut pixels, &font, 0, 0, "A", BIOS_PHOSPHOR);

        let x = BIOS_TEXT_LEFT + 1;
        let y = BIOS_TEXT_TOP + 1;
        for pixel_y in y..y + 2 {
            for pixel_x in x..x + 2 {
                assert_eq!(pixels[pixel_y * 640 + pixel_x], BIOS_PHOSPHOR);
            }
        }
        assert_eq!(pixels[y * 640 + x + 2], 0);
        assert!(!pixels.contains(&0xff005020));
    }

    #[test]
    fn bios_selector_renders_empty_state_and_persistent_error_message() {
        let font = vec![[0xf8; 8]; 128];
        let mut pixels = vec![0; 640 * 480];
        draw_bios_selector(&mut pixels, &font, &[], (0, 0), None);
        assert_eq!(pixels[0], 0);
        assert!(pixels.contains(&BIOS_PHOSPHOR));
        assert!(!pixels.contains(&0xff005020));

        let without_error = pixels.clone();
        draw_bios_selector(
            &mut pixels,
            &font,
            &[],
            (0, 0),
            Some("NOT_BOOT: no boot executable exists"),
        );
        assert_ne!(pixels, without_error);
        assert!(pixels.contains(&BIOS_PHOSPHOR_BRIGHT));
    }

    #[test]
    fn bios_selector_renders_selected_disk_id_and_selection_highlight() {
        let font = vec![[0xf8; 8]; 128];
        let disks = vec![BootDisk {
            name: "DISK0001".to_owned(),
            path: PathBuf::from("DISK0001"),
        }];
        let mut pixels = vec![0; 640 * 480];
        draw_bios_selector(&mut pixels, &font, &disks, (0, 0), None);
        let mut expected = vec![0; 640 * 480];
        draw_bios_text(&mut expected, &font, 3, 2, "DISK: DISK0001", BIOS_PHOSPHOR);
        let y = BIOS_TEXT_TOP + 3 * BIOS_TEXT_CELL_HEIGHT;
        assert_eq!(
            &pixels[y * 640..(y + BIOS_TEXT_CELL_HEIGHT) * 640],
            &expected[y * 640..(y + BIOS_TEXT_CELL_HEIGHT) * 640],
        );
        assert_eq!(
            pixels[BIOS_MODULE_ROW_TOP * 640 + BIOS_TEXT_LEFT],
            rgb(0, 30, 12)
        );
    }

    #[test]
    fn instruction_slices_resume_a_sleeping_cpu_without_treating_it_as_halted() {
        let image = crate::assembler::assemble("SVC\nHALT\n.address 0x300\nRTI", 0x100).unwrap();
        let mut cpu = Cpu::with_memory_size(1 << 16).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.load_idt(0x800).unwrap();
        cpu.set_idt_descriptor(
            crate::isa::SVC_VECTOR,
            crate::interrupts::IdtDescriptor {
                handler: 0x300,
                present: true,
                privilege: 3,
                gate_type: crate::interrupts::GateType::Interrupt,
            },
        )
        .unwrap();
        let task = cpu.create_user_task(0x100, 0x7000, 0xf000, None).unwrap();
        cpu.start_task(task).unwrap();
        cpu.test_monotonic_ms = Some(1000);
        cpu.registers[0] = crate::syscall::SyscallNumber::SleepMillis as i64;
        cpu.registers[1] = 1000;
        cpu.registers[7] = crate::syscall::ABI_VERSION;
        cpu.step().unwrap();
        assert!(!cpu.running);
        assert!(!runtime_should_close(false, cpu.has_pending_work(), true));

        cpu.test_monotonic_ms = Some(1999);
        let mut budget = 100;
        assert!(run_instruction_slice(&mut cpu, &mut budget).unwrap() > 0);
        assert_eq!(cpu.current_task(), None);
        assert!(cpu.has_pending_work());

        cpu.test_monotonic_ms = Some(2000);
        let mut budget = 100;
        assert!(run_instruction_slice(&mut cpu, &mut budget).unwrap() >= 2);
        assert_eq!(cpu.current_task(), Some(task));
        assert_eq!(cpu.pc, 0x104);
        assert!(!cpu.has_pending_work());
        assert!(runtime_should_close(false, cpu.has_pending_work(), true));
    }

    #[test]
    fn low_disk_window_is_ordinary_ram_for_user_code() {
        let source =
            "unsigned char *r = 0x43100; int main(void) { r[0] = 0x41; r[16] = 0xa5; return 0; }";
        let image = crate::c_compiler::compile_minimal_program(source, "low-disk-ram-test.c")
            .unwrap()
            .expect("low-disk RAM test should compile");
        let mut cpu = Cpu::new();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.privilege_mode = PrivilegeMode::User;
        cpu.run(10_000).unwrap();

        assert_eq!(cpu.memory[0x43100], 0x41);
        assert_eq!(cpu.memory[0x43110], 0xa5);
    }

    #[test]
    fn headless_runtime_executes_retained_assembler_examples() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for name in ["commands.asm", "directives.asm"] {
            let source = root.join("examples").join(name);
            let (directory, program) = prebuild_test_image(&source, name);
            let count = run_headless(root, &program, 50_000).unwrap_or_else(|error| {
                panic!("{name} failed through the headless runtime: {error}")
            });
            assert!(count > 0, "{name} executed no instructions");
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn headless_runtime_reports_instruction_budget_exhaustion() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let source = root.join("examples").join("commands.asm");
        let (directory, program) = prebuild_test_image(&source, "commands");

        let error = run_headless(root, &program, 1).unwrap_err();

        assert!(
            error.to_string().contains("instruction budget exhausted"),
            "{error}"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn headless_runtime_executes_prebuilt_image_from_supported_c_source() {
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
        let image = crate::image::build_source(&source).unwrap();
        let program = directory.join("minimal.sc16img");
        crate::image::write_image(&program, &image).unwrap();

        let result = run_headless(root, &program, 64);
        fs::remove_dir_all(&directory).unwrap();

        let executed = result.expect("prebuilt C image should run headlessly");
        assert!(executed > 0);
    }

    #[test]
    fn headless_runtime_rejects_source_compilation() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let source = root.join("examples").join("directives.asm");

        let error = run_headless(root, &source, 50_000).unwrap_err();

        assert!(error.to_string().contains("compile source with sc16build"));
    }

    #[test]
    fn headless_runtime_boots_prebuilt_folder_disk_images() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let directory = std::env::temp_dir().join(format!(
            "sc16-headless-disk-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let disk = directory.join("TEST0001");
        fs::create_dir_all(&disk).unwrap();
        let image = crate::assembler::assemble("loop:\n    JMP loop\n", 0).unwrap();
        crate::image::write_image(&disk.join("main.bin"), &image).unwrap();
        fs::write(
            disk.join("disk.json"),
            r#"{"version":1,"id":"TEST0001","label":"Runtime test","capacity_bytes":65536,"read_only":false,"protected_files":["main.bin"]}"#,
        )
        .unwrap();

        let error = run_headless(root, &disk, 8).unwrap_err();

        fs::remove_dir_all(&directory).unwrap();
        assert!(
            error.to_string().contains("instruction budget exhausted"),
            "{error}"
        );
    }
}
