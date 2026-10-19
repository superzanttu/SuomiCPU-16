use std::path::{Path, PathBuf};

use suomicpu16::runtime::window_conf;

#[macroquad::main(window_conf)]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("SuomiCPU-16: {error}");
        #[cfg(not(target_arch = "wasm32"))]
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut program = None;
    let mut fullscreen = false;
    let mut headless = false;
    let mut disk_id = None;
    let mut network_port = suomicpu16::net::DEFAULT_PORT;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--help" || argument == "-h" {
            println!(
                "Usage: suomicpu16 [--fullscreen] [--disk ID] [--net-port PORT] [PROGRAM.asm|PROGRAM.c|PROGRAM.bin]"
            );
            println!(
                "       suomicpu16 --headless [--disk ID] [--net-port PORT] PROGRAM.asm|PROGRAM.c|PROGRAM.bin"
            );
            println!("Without a program, open the BIOS module selector.");
            return Ok(());
        } else if argument == "--fullscreen" {
            fullscreen = true;
        } else if argument == "--headless" {
            headless = true;
        } else if argument == "--disk" {
            let value = arguments
                .next()
                .ok_or("--disk requires an 8-character disk ID")?;
            disk_id = Some(value.into_string().map_err(|_| "disk ID must be ASCII")?);
        } else if argument == "--net-port" {
            let value = arguments
                .next()
                .ok_or("--net-port requires a UDP port number")?;
            let value = value.into_string().map_err(|_| "UDP port must be ASCII")?;
            network_port = value
                .parse::<u16>()
                .map_err(|_| format!("invalid UDP port: {value}"))?;
        } else if argument.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option: {}", argument.to_string_lossy()).into());
        } else if program.is_some() {
            return Err("only one program path may be specified".into());
        } else {
            program = Some(PathBuf::from(argument));
        }
    }
    let root = std::env::var_os("SC16_ROOT")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let program_path = program.as_deref();
    if let Some(path) = program_path {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        if !path.is_file() {
            return Err(format!("program file does not exist: {}", path.display()).into());
        }
        let path = Path::new(&path);
        if headless {
            let steps = suomicpu16::runtime::run_headless_with_devices(
                &root,
                path,
                50_000_000,
                disk_id.as_deref(),
                network_port,
            )?;
            println!("Program halted after {steps} instructions.");
            return Ok(());
        }
        return suomicpu16::runtime::run_with_devices(
            &root,
            Some(path),
            fullscreen,
            disk_id.as_deref(),
            network_port,
        )
        .await
        .map_err(Into::into);
    }
    if headless {
        return Err("--headless requires a program file".into());
    }
    suomicpu16::runtime::run_with_devices(&root, None, fullscreen, disk_id.as_deref(), network_port)
        .await
        .map_err(Into::into)
}
