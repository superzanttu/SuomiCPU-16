use gif::{Encoder, Frame, Repeat};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use suomicpu16::Cpu;

const WIDTH: usize = 640;
const HEIGHT: usize = 480;
const INSTRUCTIONS_PER_FRAME: usize = 300_000;
const GALLERY_START: &str = "<!-- module-gif-gallery:start -->";
const GALLERY_END: &str = "<!-- module-gif-gallery:end -->";

fn main() {
    if let Err(error) = run() {
        eprintln!("sc16gallery: {error}");
        std::process::exit(1);
    }
}

struct Options {
    modules_dir: PathBuf,
    output_dir: PathBuf,
    readme: PathBuf,
    run_seconds: f64,
    gif_seconds: f64,
    fps: usize,
    capture_fps: usize,
    console_input: String,
    update_readme: bool,
}

struct Module {
    name: String,
    source: Option<PathBuf>,
    error: Option<String>,
}

fn parse_options() -> Result<Option<Options>, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut options = Options {
        modules_dir: root.join("modules"),
        output_dir: root.join("docs").join("module-gifs"),
        readme: root.join("README.md"),
        run_seconds: 10.0,
        gif_seconds: 15.0,
        fps: 30,
        capture_fps: 10,
        console_input: "SC-16".to_owned(),
        update_readme: true,
    };
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--help" || argument == "-h" {
            return Ok(None);
        }
        if argument == "--no-readme" {
            options.update_readme = false;
            continue;
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("missing value for {argument}"))?;
        match argument.as_str() {
            "--modules-dir" => options.modules_dir = PathBuf::from(value),
            "--output-dir" => options.output_dir = PathBuf::from(value),
            "--readme" => options.readme = PathBuf::from(value),
            "--run-seconds" => {
                options.run_seconds = value
                    .parse()
                    .map_err(|_| format!("invalid --run-seconds value: {value}"))?;
            }
            "--gif-seconds" => {
                options.gif_seconds = value
                    .parse()
                    .map_err(|_| format!("invalid --gif-seconds value: {value}"))?;
            }
            "--fps" => {
                options.fps = value
                    .parse()
                    .map_err(|_| format!("invalid --fps value: {value}"))?;
            }
            "--capture-fps" => {
                options.capture_fps = value
                    .parse()
                    .map_err(|_| format!("invalid --capture-fps value: {value}"))?;
            }
            "--console-input" => options.console_input = value,
            _ => return Err(format!("unknown option: {argument}")),
        }
    }
    if !options.run_seconds.is_finite()
        || options.run_seconds <= 0.0
        || !options.gif_seconds.is_finite()
        || options.gif_seconds <= 0.0
    {
        return Err("--run-seconds and --gif-seconds must be positive and finite".to_owned());
    }
    if options.fps == 0 || options.capture_fps == 0 || options.capture_fps > options.fps {
        return Err("FPS values must be positive and capture-fps cannot exceed fps".to_owned());
    }
    if options.run_seconds * options.fps as f64 > usize::MAX as f64 {
        return Err("capture duration is too large".to_owned());
    }
    Ok(Some(options))
}

fn print_help() {
    println!("Usage: cargo run --bin sc16gallery -- [options]");
    println!("Run C modules in the Rust SC-16 CPU and generate looping GIF captures.");
    println!("  --modules-dir PATH   Module folders (default: modules/)");
    println!("  --output-dir PATH    GIF output folder (default: docs/module-gifs/)");
    println!("  --readme PATH        README gallery to update");
    println!("  --run-seconds N      Simulated runtime per module (default: 10)");
    println!("  --gif-seconds N      GIF loop duration (default: 15)");
    println!("  --fps N              Emulated frames per second (default: 30)");
    println!("  --capture-fps N      GIF sampling rate (default: 10)");
    println!("  --console-input TEXT Line supplied to text-mode programs");
    println!("  --no-readme          Do not update the README gallery");
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let Some(options) = parse_options().map_err(io::Error::other)? else {
        print_help();
        return Ok(());
    };
    if !options.modules_dir.is_dir() {
        return Err(format!(
            "modules folder does not exist: {}",
            options.modules_dir.display()
        )
        .into());
    }
    let modules = discover_modules(&options.modules_dir)?;
    if modules.is_empty() {
        return Err(format!(
            "no module folders found in {}",
            options.modules_dir.display()
        )
        .into());
    }

    let mut results = Vec::with_capacity(modules.len());
    let mut generated = 0;
    let mut failures = 0;
    let mut used_slugs = Vec::new();
    for module in modules {
        let slug = slug(&module.name, &mut used_slugs);
        let destination = options.output_dir.join(format!("{slug}.gif"));
        match capture_module(&module, &destination, &options) {
            Ok(count) => {
                println!(
                    "{}: captured {count} frames -> {}",
                    module.name,
                    destination.display()
                );
                results.push((module, Some(destination), None));
                generated += 1;
            }
            Err(error) => {
                eprintln!("{}: ERROR: {error}", module.name);
                results.push((module, None, Some(error)));
                failures += 1;
            }
        }
    }
    if options.update_readme {
        match update_readme(&options.readme, &results) {
            Ok(()) => println!("Updated module gallery in {}", options.readme.display()),
            Err(error) => {
                eprintln!("README update failed: {error}");
                failures += 1;
            }
        }
    }
    println!("Generated {generated} module GIF(s); {failures} failure(s).");
    if failures > 0 {
        return Err(format!("{failures} gallery operation(s) failed").into());
    }
    Ok(())
}

fn discover_modules(directory: &Path) -> io::Result<Vec<Module>> {
    let mut folders = fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            Ok(entry.file_type()?.is_dir().then(|| entry.path()))
        })
        .collect::<Result<Vec<_>, io::Error>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    folders.sort_by_key(|path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });

    folders
        .into_iter()
        .map(|folder| {
            let mut sources = fs::read_dir(&folder)?
                .map(|entry| {
                    let entry = entry?;
                    let path = entry.path();
                    Ok((entry.file_type()?.is_file()
                        && path
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("c")))
                    .then_some(path))
                })
                .collect::<Result<Vec<_>, io::Error>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            sources.sort_by_key(|path| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
            });
            let main = sources
                .iter()
                .find(|path| {
                    path.file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("main.c"))
                })
                .cloned();
            let (source, error) = match sources.len() {
                0 => (None, Some("No C source file found.".to_owned())),
                1 => (sources.pop(), None),
                _ => match main {
                    Some(path) => (Some(path), None),
                    None => (
                        None,
                        Some("Multiple C source files found; expected main.c.".to_owned()),
                    ),
                },
            };
            Ok(Module {
                name: folder
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                source,
                error,
            })
        })
        .collect()
}

fn capture_module(module: &Module, destination: &Path, options: &Options) -> Result<usize, String> {
    let source = module.source.as_ref().ok_or_else(|| {
        module
            .error
            .clone()
            .unwrap_or_else(|| "No source file found.".into())
    })?;
    let image = suomicpu16::image::build_source(source).map_err(|error| error.to_string())?;
    let mut cpu = Cpu::new();
    let source_disk = source
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("disks");
    cpu.configure_disk(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("disks"),
        source_disk,
    )
    .map_err(|error| format!("could not configure module storage: {error}"))?;
    cpu.load_program(&image.segments, image.entry_point)
        .map_err(|error| error.to_string())?;
    let source_text = fs::read_to_string(source).map_err(|error| error.to_string())?;
    let text_mode = source_text.contains("SCREEN_");
    let console_bytes = if text_mode {
        Some(format!("{}\n", options.console_input).into_bytes())
    } else {
        None
    };
    let total_frames = (options.run_seconds * options.fps as f64).ceil() as usize;
    let capture_interval = 1.0 / options.capture_fps as f64;
    let start_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut next_capture = 0.0;
    let mut input_sent = false;
    let mut frames = Vec::new();
    for frame_index in 0..total_frames {
        let elapsed = (frame_index + 1) as f64 / options.fps as f64;
        update_rtc(&mut cpu, start_timestamp.saturating_add(elapsed as u64));
        if !input_sent && elapsed >= 1.0 {
            if let Some(input) = &console_bytes {
                cpu.receive_console_input(input);
            }
            input_sent = true;
        }
        cpu.begin_frame();
        cpu.run(INSTRUCTIONS_PER_FRAME)
            .map_err(|error| error.to_string())?;
        if elapsed + f64::EPSILON >= next_capture || frames.is_empty() {
            frames.push(capture_frame(&cpu, text_mode));
            while next_capture <= elapsed {
                next_capture += capture_interval;
            }
        }
    }
    if frames.is_empty() {
        frames.push(capture_frame(&cpu, text_mode));
    }
    let palette = gallery_palette(&cpu);
    write_gif(destination, &frames, &palette, options.gif_seconds)
        .map_err(|error| error.to_string())?;
    Ok(frames.len())
}

fn update_rtc(cpu: &mut Cpu, timestamp: u64) {
    cpu.memory[0x43010] = (timestamp % 60) as u8;
    cpu.memory[0x43011] = ((timestamp / 60) % 60) as u8;
    cpu.memory[0x43012] = ((timestamp / 3600) % 24) as u8;
    cpu.memory[0x43002] |= 8;
}

fn capture_frame(cpu: &Cpu, text_mode: bool) -> Vec<u8> {
    if text_mode {
        return render_console(cpu.console.output());
    }
    let framebuffer = cpu.composited_framebuffer();
    let source_width = if cpu.memory[0x43014] == 1 {
        WIDTH
    } else {
        WIDTH / 2
    };
    let source_height = if source_width == WIDTH {
        HEIGHT
    } else {
        HEIGHT / 2
    };
    let mut frame = vec![0; WIDTH * HEIGHT];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            frame[y * WIDTH + x] =
                framebuffer[(y * source_height / HEIGHT) * source_width + x * source_width / WIDTH];
        }
    }
    frame
}

fn render_console(output: &[u8]) -> Vec<u8> {
    let mut lines = vec![Vec::new()];
    for byte in output {
        match *byte {
            b'\r' => {}
            b'\n' => lines.push(Vec::new()),
            b'\t' => {
                let spaces = 4 - lines.last().map_or(0, |line| line.len() % 4);
                lines
                    .last_mut()
                    .unwrap()
                    .extend(std::iter::repeat_n(b' ', spaces));
            }
            byte => {
                let line = lines.last_mut().unwrap();
                line.push(byte);
                if line.len() == 80 {
                    lines.push(Vec::new());
                }
            }
        }
    }
    let font = console_font();
    let visible = &lines[lines.len().saturating_sub(25)..];
    let mut frame = vec![0; WIDTH * HEIGHT];
    for (line_index, line) in visible.iter().enumerate() {
        for (column, byte) in line.iter().copied().take(80).enumerate() {
            let glyph_index = byte.saturating_sub(32) as usize;
            let Some(glyph) = font.get(glyph_index) else {
                continue;
            };
            for (glyph_y, bits) in glyph.iter().copied().enumerate().take(7) {
                for glyph_x in 0..5 {
                    if bits & (0x80 >> glyph_x) == 0 {
                        continue;
                    }
                    let x = column * 8 + glyph_x;
                    let y = 20 + line_index * 16 + glyph_y * 2;
                    frame[y * WIDTH + x] = 3;
                    frame[(y + 1) * WIDTH + x] = 3;
                }
            }
        }
    }
    frame
}

fn console_font() -> Vec<[u8; 8]> {
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

fn gallery_palette(cpu: &Cpu) -> Vec<u8> {
    let mut palette = (0..=255)
        .flat_map(|value| [value, value, value])
        .collect::<Vec<_>>();
    for index in 0..15 {
        let [red, green, blue] = cpu.palette_color(index);
        let offset = index as usize * 3;
        palette[offset..offset + 3].copy_from_slice(&[red, green, blue]);
    }
    palette
}

fn write_gif(
    destination: &Path,
    frames: &[Vec<u8>],
    palette: &[u8],
    duration_seconds: f64,
) -> io::Result<()> {
    if frames.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot create a GIF without captured frames",
        ));
    }
    if palette.len() != 256 * 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "GIF palette must contain 256 colors",
        ));
    }
    if frames.iter().any(|frame| frame.len() != WIDTH * HEIGHT) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "every GIF frame must match 640x480",
        ));
    }
    let total_centiseconds = (duration_seconds * 100.0).round() as u64;
    if total_centiseconds < frames.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "GIF duration is too short for the captured frames",
        ));
    }
    let frame_count = frames.len() as u64;
    let base_delay = total_centiseconds / frame_count;
    let extra_delays = total_centiseconds % frame_count;
    fs::create_dir_all(destination.parent().unwrap_or_else(|| Path::new(".")))?;
    let mut file = File::create(destination)?;
    {
        let mut encoder = Encoder::new(&mut file, WIDTH as u16, HEIGHT as u16, palette)
            .map_err(io::Error::other)?;
        encoder
            .set_repeat(Repeat::Infinite)
            .map_err(io::Error::other)?;
        for (index, pixels) in frames.iter().enumerate() {
            let delay = base_delay + u64::from((index as u64) < extra_delays);
            let mut frame =
                Frame::from_indexed_pixels(WIDTH as u16, HEIGHT as u16, pixels.clone(), None);
            frame.delay = u16::try_from(delay).map_err(io::Error::other)?;
            encoder.write_frame(&frame).map_err(io::Error::other)?;
        }
    }
    file.flush()
}

fn slug(name: &str, used: &mut Vec<String>) -> String {
    let mut normalized = String::new();
    let mut pending_separator = false;
    for character in name.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            if pending_separator && !normalized.is_empty() {
                normalized.push('-');
            }
            normalized.push(character);
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }
    if normalized.is_empty() {
        normalized.push_str("module");
    }
    let base = normalized.clone();
    let mut suffix = 2;
    while used.contains(&normalized) {
        normalized = format!("{base}-{suffix}");
        suffix += 1;
    }
    used.push(normalized.clone());
    normalized
}

fn update_readme(
    readme: &Path,
    modules: &[(Module, Option<PathBuf>, Option<String>)],
) -> io::Result<()> {
    let mut rows = vec![
        "## Module gallery".to_owned(),
        String::new(),
        "Each animation captures a module running in the SC-16 emulator.".to_owned(),
        String::new(),
        "| Module | Animation |".to_owned(),
        "| --- | --- |".to_owned(),
    ];
    for (module, gif_path, error) in modules {
        let name = module.name.replace('|', "\\|");
        if let Some(gif_path) = gif_path {
            let relative = pathdiff(readme.parent().unwrap_or_else(|| Path::new(".")), gif_path);
            rows.push(format!(
                "| {name} | <img src=\"{}\" alt=\"{name} running in SC-16\" width=\"320\"> |",
                relative.to_string_lossy().replace('\\', "/")
            ));
        } else {
            rows.push(format!(
                "| {name} | Capture unavailable: {} |",
                error.as_deref().unwrap_or("not generated")
            ));
        }
    }
    let gallery = [
        GALLERY_START.to_owned(),
        rows.join("\n"),
        GALLERY_END.to_owned(),
    ]
    .join("\n");
    let contents = fs::read_to_string(readme)?;
    let start = contents.find(GALLERY_START);
    let end = contents.find(GALLERY_END);
    let updated = match (start, end) {
        (None, None) => format!("{}\n\n{}\n", contents.trim_end(), gallery),
        (Some(start), Some(end)) if end >= start => {
            let end = end + GALLERY_END.len();
            format!("{}{}{}", &contents[..start], gallery, &contents[end..])
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "README gallery markers are incomplete: {}",
                    readme.display()
                ),
            ));
        }
    };
    fs::write(readme, updated)
}

fn pathdiff(base: &Path, target: &Path) -> PathBuf {
    let base = base.components().collect::<Vec<_>>();
    let target = target.components().collect::<Vec<_>>();
    let shared = base
        .iter()
        .zip(&target)
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = PathBuf::new();
    for _ in shared..base.len() {
        relative.push("..");
    }
    for component in target.iter().skip(shared) {
        relative.push(component.as_os_str());
    }
    relative
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(label: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{label}-{}-{timestamp}", std::process::id()))
    }

    #[test]
    fn discovers_bios_compatible_module_sources() {
        let root = temp_path("sc16-gallery-discovery");
        fs::create_dir_all(root.join("single")).unwrap();
        fs::create_dir_all(root.join("multiple")).unwrap();
        fs::create_dir_all(root.join("broken")).unwrap();
        fs::create_dir_all(root.join("empty")).unwrap();
        fs::write(root.join("single/game.c"), "").unwrap();
        fs::write(root.join("multiple/main.c"), "").unwrap();
        fs::write(root.join("multiple/helper.c"), "").unwrap();
        fs::write(root.join("broken/one.c"), "").unwrap();
        fs::write(root.join("broken/two.c"), "").unwrap();

        let modules = discover_modules(&root).unwrap();
        assert_eq!(
            modules
                .iter()
                .map(|module| (
                    module.name.as_str(),
                    module.source.as_ref().and_then(|path| path.file_name())
                ))
                .collect::<Vec<_>>(),
            vec![
                ("broken", None),
                ("empty", None),
                ("multiple", Some(std::ffi::OsStr::new("main.c"))),
                ("single", Some(std::ffi::OsStr::new("game.c"))),
            ]
        );
        assert!(
            modules[0]
                .error
                .as_deref()
                .unwrap()
                .contains("expected main.c")
        );
        assert!(modules[1].error.as_deref().unwrap().contains("No C source"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writes_decodable_looping_gif_with_requested_duration() {
        let path = temp_path("sc16-gallery-gif").with_extension("gif");
        let mut palette = Vec::with_capacity(256 * 3);
        for value in 0..=255u8 {
            palette.extend_from_slice(&[value; 3]);
        }
        let mut first = vec![0; WIDTH * HEIGHT];
        first[0] = 1;
        let mut second = vec![2; WIDTH * HEIGHT];
        second[0] = 3;
        write_gif(&path, &[first, second], &palette, 0.15).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[..6], b"GIF89a");
        assert!(bytes.windows(11).any(|window| window == b"NETSCAPE2.0"));
        assert_eq!(bytes.last(), Some(&0x3b));
        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Gif).unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (WIDTH as u32, HEIGHT as u32)
        );
        let delays = bytes
            .windows(8)
            .filter(|window| window[..3] == [0x21, 0xf9, 0x04])
            .map(|window| u16::from_le_bytes([window[4], window[5]]))
            .collect::<Vec<_>>();
        assert_eq!(delays, [8, 7]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn readme_gallery_update_is_idempotent_and_emits_relative_links() {
        let root = temp_path("sc16-gallery-readme");
        fs::create_dir_all(&root).unwrap();
        let readme = root.join("README.md");
        fs::write(&readme, "# Project\n").unwrap();
        let module = Module {
            name: "Demo".to_owned(),
            source: Some(root.join("modules").join("Demo.c")),
            error: None,
        };
        let results = [(module, Some(root.join("docs").join("demo.gif")), None)];
        update_readme(&readme, &results).unwrap();
        let first = fs::read_to_string(&readme).unwrap();
        update_readme(&readme, &results).unwrap();
        let second = fs::read_to_string(&readme).unwrap();
        assert_eq!(first, second);
        assert!(first.contains("docs/demo.gif"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn console_rendering_uses_emulator_font_and_green_text() {
        let frame = render_console(b"SC-16\n");
        assert_eq!(frame.len(), WIDTH * HEIGHT);
        assert!(frame.contains(&3));
        assert_eq!(frame[0], 0);
    }
}
