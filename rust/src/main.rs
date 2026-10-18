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
    let arguments = std::env::args_os().skip(1);
    for argument in arguments {
        if argument == "--help" || argument == "-h" {
            println!("Usage: suomicpu16 [--fullscreen] [PROGRAM.asm|PROGRAM.c|PROGRAM.bin]");
            println!("       suomicpu16 --headless PROGRAM.asm|PROGRAM.c|PROGRAM.bin");
            println!("Without a program, open the BIOS module selector.");
            return Ok(());
        } else if argument == "--fullscreen" {
            fullscreen = true;
        } else if argument == "--headless" {
            headless = true;
        } else if argument == "--disk" || argument == "--net-port" {
            return Err(format!(
                "{} is not available in the Rust runtime yet",
                argument.to_string_lossy()
            )
            .into());
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
            let steps = suomicpu16::runtime::run_headless(&root, path, 50_000_000)?;
            println!("Program halted after {steps} instructions.");
            return Ok(());
        }
        return suomicpu16::runtime::run(&root, Some(path), fullscreen)
            .await
            .map_err(Into::into);
    }
    if headless {
        return Err("--headless requires a program file".into());
    }
    suomicpu16::runtime::run(&root, None, fullscreen)
        .await
        .map_err(Into::into)
}
