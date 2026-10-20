use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    if let Err(error) = run() {
        eprintln!("sc16build: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let Some(source) = arguments.next() else {
        return Err(
            "usage: sc16build <source.asm|source.c|source.bin> [output.img] | smoke-modules".into(),
        );
    };
    if source == "--help" || source == "-h" {
        println!("Usage: sc16build <source.asm|source.c|source.bin> [output.img]");
        println!("Build SC-16 source or wrap a flat binary as a segmented image.");
        println!("       sc16build smoke-modules");
        println!("Compile every bootable BIOS module as a smoke check.");
        return Ok(());
    }
    if source == "smoke-modules" {
        if arguments.next().is_some() {
            return Err("usage: sc16build smoke-modules".into());
        }
        return smoke_modules();
    }
    if source.to_string_lossy().starts_with('-') {
        return Err(format!("unknown option: {}", source.to_string_lossy()).into());
    }
    let source = PathBuf::from(source).canonicalize()?;
    let output = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| source.with_extension("sc16img"));
    if arguments.next().is_some() {
        return Err("usage: sc16build <source.asm|source.c|source.bin> [output.img]".into());
    }
    if output.exists() && source == output.canonicalize()? {
        return Err("output path must not overwrite the source".into());
    }
    let image = suomicpu16::image::load_program(&source)?;
    suomicpu16::image::write_image(&output, &image)?;
    println!(
        "Built {} bytes in {} segment(s), entry point {:#x} -> {}",
        image
            .segments
            .iter()
            .map(|segment| segment.data.len())
            .sum::<usize>(),
        image.segments.len(),
        image.entry_point,
        output.display()
    );
    Ok(())
}

struct BootModule {
    name: String,
    source: Option<PathBuf>,
    error: Option<String>,
}

fn discover_boot_modules(directory: &Path) -> Result<Vec<BootModule>, std::io::Error> {
    let mut module_dirs = fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            Ok(entry.file_type()?.is_dir().then(|| entry.path()))
        })
        .collect::<Result<Vec<_>, std::io::Error>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    module_dirs.sort_by_key(|path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });

    let mut modules = Vec::with_capacity(module_dirs.len());
    for module_dir in module_dirs {
        let mut sources = fs::read_dir(&module_dir)?
            .map(|entry| {
                let entry = entry?;
                let path = entry.path();
                let is_source = entry.file_type()?.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("c"));
                Ok(is_source.then_some(path))
            })
            .collect::<Result<Vec<_>, std::io::Error>>()?
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
            0 => (None, Some("No .c files found in module folder.".to_owned())),
            1 => (sources.pop(), None),
            _ => match main {
                Some(path) => (Some(path), None),
                None => (
                    None,
                    Some("Multiple .c files found; expected main.c.".to_owned()),
                ),
            },
        };
        modules.push(BootModule {
            name: module_dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            source,
            error,
        });
    }
    Ok(modules)
}

fn smoke_modules() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let modules = discover_boot_modules(&root.join("modules"))?;
    let temp_dir = std::env::temp_dir().join(format!(
        "sc16-smoke-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::create_dir(&temp_dir)?;
    let mut compiled = 0usize;
    let mut skipped = 0usize;
    let mut failures = Vec::new();

    for (index, module) in modules.iter().enumerate() {
        let Some(source) = &module.source else {
            skipped += 1;
            println!(
                "Skipped {}: {}",
                module.name,
                module.error.as_deref().unwrap_or("no source")
            );
            continue;
        };
        let result = if source.canonicalize().ok()
            == Some(root.join("modules/Tank-16/main.c").canonicalize()?)
        {
            suomicpu16::tank16_map::compile_runtime_tank16(&root.join("modules/Tank-16"))
                .map(|_| ())
                .map_err(std::io::Error::other)
        } else {
            let output = temp_dir.join(format!("{index}.bin"));
            Command::new("python")
                .arg(root.join("tools/c_compiler.py"))
                .arg(source)
                .arg("--quiet")
                .arg("--output")
                .arg(output)
                .current_dir(root)
                .output()
                .and_then(|result| {
                    if result.status.success() {
                        Ok(())
                    } else {
                        let details = String::from_utf8_lossy(&result.stderr).trim().to_owned();
                        Err(std::io::Error::other(if details.is_empty() {
                            format!("C compiler exited with {}", result.status)
                        } else {
                            details
                        }))
                    }
                })
        };
        match result {
            Ok(()) => {
                println!("Compiled {}: {}", module.name, source.display());
                compiled += 1;
            }
            Err(error) => {
                eprintln!("Failed {}: {error}", module.name);
                failures.push(module.name.clone());
            }
        }
    }
    let cleanup_result = fs::remove_dir_all(&temp_dir);
    cleanup_result?;
    if !failures.is_empty() {
        return Err(format!(
            "{} of {} BIOS modules failed to compile: {}",
            failures.len(),
            compiled + failures.len(),
            failures.join(", ")
        )
        .into());
    }
    println!("Compiled {compiled} BIOS modules; skipped {skipped} without a bootable C source");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "sc16build-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn module_discovery_selects_main_for_multi_source_modules() {
        let root = temporary_directory();
        fs::create_dir_all(root.join("Alpha")).unwrap();
        fs::create_dir_all(root.join("beta")).unwrap();
        fs::create_dir_all(root.join("Empty")).unwrap();
        fs::write(root.join("Alpha/extra.c"), "").unwrap();
        fs::write(root.join("Alpha/main.c"), "").unwrap();
        fs::write(root.join("beta/game.C"), "").unwrap();

        let modules = discover_boot_modules(&root).unwrap();
        assert_eq!(
            modules
                .iter()
                .map(|module| module.name.as_str())
                .collect::<Vec<_>>(),
            ["Alpha", "beta", "Empty"]
        );
        assert_eq!(
            modules[0].source.as_deref(),
            Some(root.join("Alpha/main.c").as_path())
        );
        assert_eq!(
            modules[1].source.as_deref(),
            Some(root.join("beta/game.C").as_path())
        );
        assert!(modules[2].source.is_none());
        assert_eq!(
            modules[2].error.as_deref(),
            Some("No .c files found in module folder.")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn module_discovery_reports_ambiguous_sources_without_main() {
        let root = temporary_directory();
        fs::create_dir_all(root.join("Ambiguous")).unwrap();
        fs::write(root.join("Ambiguous/first.c"), "").unwrap();
        fs::write(root.join("Ambiguous/second.c"), "").unwrap();

        let modules = discover_boot_modules(&root).unwrap();
        assert!(modules[0].source.is_none());
        assert_eq!(
            modules[0].error.as_deref(),
            Some("Multiple .c files found; expected main.c.")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
