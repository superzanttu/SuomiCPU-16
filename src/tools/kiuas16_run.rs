use std::ffi::OsString;
use std::path::PathBuf;

use suomicpu16::kiuas16::runtime::run_headless;
use suomicpu16::kiuas16::{BOOT_ROM_BYTES, FileSectorMedia, Kiuas16Machine};

const USAGE: &str = "kiuas16-run --rom <rom.bin> --steps <count> [--root <disk.raw>] [--root-writable] [--serial-input <bytes.bin>] [--deny-traps]";

#[derive(Debug)]
struct Options {
    rom: PathBuf,
    steps: u64,
    root: Option<PathBuf>,
    root_writable: bool,
    serial_input: Option<PathBuf>,
    deny_traps: bool,
}

fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Option<Options>, String> {
    let mut arguments = arguments.into_iter();
    let mut rom = None;
    let mut steps = None;
    let mut root = None;
    let mut root_writable = false;
    let mut serial_input = None;
    let mut deny_traps = false;
    while let Some(argument) = arguments.next() {
        if argument == "--help" || argument == "-h" {
            return Ok(None);
        }
        if argument == "--root-writable" {
            if root_writable {
                return Err("duplicate --root-writable".into());
            }
            root_writable = true;
            continue;
        }
        if argument == "--deny-traps" {
            if deny_traps {
                return Err("duplicate --deny-traps".into());
            }
            deny_traps = true;
            continue;
        }
        let target = if argument == "--rom" {
            &mut rom
        } else if argument == "--root" {
            &mut root
        } else if argument == "--serial-input" {
            &mut serial_input
        } else if argument == "--steps" {
            if steps.is_some() {
                return Err("duplicate --steps".into());
            }
            let value = arguments.next().ok_or("missing value for --steps")?;
            let count = value
                .to_str()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|count| *count != 0)
                .ok_or("--steps requires a nonzero unsigned 64-bit integer")?;
            steps = Some(count);
            continue;
        } else {
            return Err(format!("unknown option: {}", argument.to_string_lossy()));
        };
        if target.is_some() {
            return Err(format!("duplicate {}", argument.to_string_lossy()));
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("missing value for {}", argument.to_string_lossy()))?;
        if value.to_string_lossy().starts_with("--") {
            return Err(format!("missing value for {}", argument.to_string_lossy()));
        }
        *target = Some(PathBuf::from(value));
    }
    if root_writable && root.is_none() {
        return Err("--root-writable requires --root".into());
    }
    Ok(Some(Options {
        rom: rom.ok_or("--rom is required")?,
        steps: steps.ok_or("--steps is required")?,
        root,
        root_writable,
        serial_input,
        deny_traps,
    }))
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let Some(options) = parse(std::env::args_os().skip(1))? else {
        println!("Usage: {USAGE}");
        println!("Execute Kiuas16 ROM from reset for a bounded number of virtual cycles.");
        println!("Root media is read-only unless --root-writable is specified.");
        println!("UART bytes go to stdout; diagnostics go to stderr. No boot success is inferred.");
        println!("Guest traps are delivered normally; --deny-traps makes their presence an error.");
        return Ok(());
    };
    let rom = std::fs::read(&options.rom)?;
    if rom.len() != BOOT_ROM_BYTES {
        return Err(format!(
            "ROM must contain exactly {BOOT_ROM_BYTES} bytes, got {}",
            rom.len()
        )
        .into());
    }
    let input = match options.serial_input {
        Some(path) => std::fs::read(path)?,
        None => Vec::new(),
    };
    let mut machine = Kiuas16Machine::new(rom)
        .map_err(|error| format!("machine construction failed: {error:?}"))?;
    if let Some(path) = options.root {
        let media = FileSectorMedia::open(path, !options.root_writable)?;
        machine
            .bus_mut()
            .attach_root_block(Box::new(media))
            .map_err(|error| format!("root attachment failed: {error:?}"))?;
    }
    let stdout = std::io::stdout();
    let report = run_headless(&mut machine, options.steps, &input, &mut stdout.lock())?;
    eprintln!(
        "Step budget exhausted: steps={} retired={} traps={} interrupts={} waiting={} pc={:#x} time={} serial-in={}/{} serial-out={}",
        report.steps,
        report.retired,
        report.traps,
        report.interrupts,
        report.waiting,
        machine.core().pc,
        machine.virtual_time(),
        report.input_accepted,
        input.len(),
        report.output_bytes,
    );
    if let Some(trap) = report.last_trap {
        eprintln!("Last delivered synchronous trap: {trap:?}");
    }
    if options.deny_traps && report.traps != 0 {
        return Err(format!("run delivered {} synchronous trap(s)", report.traps).into());
    }
    if report.input_accepted != input.len() {
        return Err("step budget ended before all serial input reached the UART".into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("kiuas16-run: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn required_options_and_read_only_default() {
        let options = parse(arguments(&["--rom", "boot.bin", "--steps", "12"]))
            .unwrap()
            .unwrap();
        assert_eq!(options.rom, PathBuf::from("boot.bin"));
        assert_eq!(options.steps, 12);
        assert!(!options.root_writable);
        assert!(options.root.is_none());
        assert!(parse(arguments(&["--help"])).unwrap().is_none());
    }

    #[test]
    fn invalid_and_duplicate_options_fail() {
        for values in [
            vec![],
            vec!["--rom"],
            vec!["--rom", "--steps", "1"],
            vec!["--rom", "boot.bin"],
            vec!["--steps", "1"],
            vec!["--rom", "boot.bin", "--steps", "0"],
            vec!["--rom", "boot.bin", "--steps", "-1"],
            vec!["--rom", "boot.bin", "--steps", "18446744073709551616"],
            vec!["--rom", "a", "--rom", "b", "--steps", "1"],
            vec!["--rom", "a", "--steps", "1", "--steps", "2"],
            vec!["--rom", "a", "--steps", "1", "--root-writable"],
            vec!["--root-writable", "--root-writable"],
            vec!["--deny-traps", "--deny-traps"],
            vec!["--unknown"],
        ] {
            assert!(parse(arguments(&values)).is_err(), "{values:?}");
        }
    }
}
