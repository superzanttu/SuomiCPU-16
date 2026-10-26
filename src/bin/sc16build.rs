use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("sc16build: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let Some(source) = arguments.next() else {
        return Err("usage: sc16build <source.asm|source.c|source.bin|source.sc16img> [output.img] | smoke-modules | import-block-disk <legacy.json> <destination-folder> | package-disk <source> <disk-folder> <label> <capacity-bytes> [--kernel] | package-modules <disk-root> <capacity-bytes>".into());
    };
    if source == "--help" || source == "-h" {
        println!("Usage: sc16build <source.asm|source.c|source.bin|source.sc16img> [output.img]");
        println!("Build SC-16 source, wrap a flat binary, or validate an existing image.");
        println!("       sc16build smoke-modules");
        println!("Compile every bootable BIOS module as a smoke check.");
        println!("       sc16build package-modules <disk-root> <capacity-bytes>");
        println!("Package every bootable C module and its local disk assets.");
        println!("       sc16build import-block-disk <legacy.json> <destination-folder>");
        println!("Import a legacy JSON block disk without modifying its source.");
        println!(
            "       sc16build package-disk <source.asm|source.c|source.bin|source.sc16img> <disk-folder> <label> <capacity-bytes> [--kernel]"
        );
        println!("Build a source program into a new folder-backed boot disk.");
        return Ok(());
    }
    if source == "smoke-modules" {
        if arguments.next().is_some() {
            return Err("usage: sc16build smoke-modules".into());
        }
        return smoke_modules();
    }
    if source == "package-modules" {
        let Some(disk_root) = arguments.next() else {
            return Err("usage: sc16build package-modules <disk-root> <capacity-bytes>".into());
        };
        let Some(capacity) = arguments
            .next()
            .and_then(|value| value.into_string().ok())
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return Err("usage: sc16build package-modules <disk-root> <capacity-bytes>".into());
        };
        if arguments.next().is_some() {
            return Err("usage: sc16build package-modules <disk-root> <capacity-bytes>".into());
        }
        return package_modules(Path::new(&disk_root), capacity);
    }
    if source == "import-block-disk" {
        let Some(legacy_disk) = arguments.next() else {
            return Err(
                "usage: sc16build import-block-disk <legacy.json> <destination-folder>".into(),
            );
        };
        let Some(destination) = arguments.next() else {
            return Err(
                "usage: sc16build import-block-disk <legacy.json> <destination-folder>".into(),
            );
        };
        if arguments.next().is_some() {
            return Err(
                "usage: sc16build import-block-disk <legacy.json> <destination-folder>".into(),
            );
        }
        let destination = PathBuf::from(destination);
        let disk =
            suomicpu16::disk::import_legacy_block_disk(Path::new(&legacy_disk), &destination)?;
        println!(
            "Imported {} ({} bytes, read_only={}) -> {}",
            disk.manifest().id,
            disk.used_bytes()?,
            disk.manifest().read_only,
            destination.display()
        );
        return Ok(());
    }
    if source == "package-disk" {
        let usage = "usage: sc16build package-disk <source.asm|source.c|source.bin|source.sc16img> <disk-folder> <label> <capacity-bytes> [--kernel]";
        let Some(program_source) = arguments.next() else {
            return Err(usage.into());
        };
        let Some(disk_folder) = arguments.next() else {
            return Err(usage.into());
        };
        let Some(label) = arguments.next().and_then(|value| value.into_string().ok()) else {
            return Err(usage.into());
        };
        let Some(capacity) = arguments
            .next()
            .and_then(|value| value.into_string().ok())
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return Err(usage.into());
        };
        let kernel = match arguments.next() {
            None => false,
            Some(option) if option == "--kernel" => true,
            Some(_) => return Err(usage.into()),
        };
        if arguments.next().is_some() {
            return Err(usage.into());
        }
        let source = PathBuf::from(program_source);
        let disk_folder = PathBuf::from(disk_folder);
        package_disk(&source, &disk_folder, &label, capacity, kernel)?;
        println!(
            "Packaged {} as {} -> {}",
            source.display(),
            if kernel { "kernel.bin" } else { "main.bin" },
            disk_folder.display()
        );
        return Ok(());
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
        return Err(
            "usage: sc16build <source.asm|source.c|source.bin|source.sc16img> [output.img]".into(),
        );
    }
    if output.exists() && source == output.canonicalize()? {
        return Err("output path must not overwrite the source".into());
    }
    let image = match source.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("bin") => {
            suomicpu16::image::load_binary(&source)?
        }
        Some(extension)
            if extension.eq_ignore_ascii_case("img")
                || extension.eq_ignore_ascii_case("sc16img") =>
        {
            suomicpu16::image::read_image(&source)?
        }
        Some(extension) if extension.eq_ignore_ascii_case("json") => {
            suomicpu16::image::load_program(&source)?
        }
        _ => suomicpu16::image::build_source(&source)?,
    };
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

fn package_disk(
    source: &Path,
    disk_folder: &Path,
    label: &str,
    capacity_bytes: u64,
    kernel: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let id = disk_folder
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid disk folder name")
        })?;
    let executable = if kernel { "kernel.bin" } else { "main.bin" };
    let manifest = serde_json::json!({
        "version": 1,
        "id": id,
        "label": label,
        "capacity_bytes": capacity_bytes,
        "read_only": false,
        "protected_files": [executable],
    });
    let manifest_text = serde_json::to_string(&manifest)?;
    let parsed_manifest = suomicpu16::disk::parse_folder_disk_manifest(&manifest_text)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
    let image = match source.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("bin") => {
            suomicpu16::image::load_binary(source)?
        }
        Some(extension)
            if extension.eq_ignore_ascii_case("img")
                || extension.eq_ignore_ascii_case("sc16img") =>
        {
            suomicpu16::image::read_image(source)?
        }
        _ => suomicpu16::image::build_source(source)?,
    };
    if disk_folder.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "disk destination already exists",
        )
        .into());
    }
    if let Some(parent) = disk_folder.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(disk_folder)?;
    let result = (|| {
        fs::write(
            disk_folder.join("disk.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        suomicpu16::image::write_image(&disk_folder.join(executable), &image)?;
        let mounted = suomicpu16::disk::FolderDisk::open(disk_folder)
            .map_err(|error| std::io::Error::other(error))?;
        if mounted.manifest().id != parsed_manifest.id {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "packaged disk ID mismatch",
            )
            .into());
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(disk_folder);
    }
    result
}

fn package_modules(
    disk_root: &Path,
    capacity_bytes: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let modules = discover_boot_modules(&repository.join("modules"))?;
    fs::create_dir_all(disk_root)?;
    let mut index = 1u32;
    for module in modules {
        let Some(source) = module.source else {
            println!(
                "Skipped {}: {}",
                module.name,
                module.error.as_deref().unwrap_or("no boot source")
            );
            continue;
        };
        let disk_id = format!("APP{index:05}");
        let disk_path = disk_root.join(&disk_id);
        package_disk(&source, &disk_path, &module.name, capacity_bytes, false)?;
        let package_result = (|| {
            let asset_root = source
                .parent()
                .ok_or_else(|| "module source has no parent directory")?
                .join("disks");
            if asset_root.exists() {
                copy_module_assets(&asset_root, &disk_path)?;
            }
            suomicpu16::disk::FolderDisk::open(&disk_path).map_err(std::io::Error::other)?;
            Ok::<(), Box<dyn std::error::Error>>(())
        })();
        if let Err(error) = package_result {
            let _ = fs::remove_dir_all(&disk_path);
            return Err(error);
        }
        println!(
            "Packaged {} as main.bin -> {}",
            module.name,
            disk_path.display()
        );
        index += 1;
    }
    Ok(())
}

fn copy_module_assets(source: &Path, destination: &Path) -> Result<(), std::io::Error> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("module asset is a symbolic link: {}", source_path.display()),
            ));
        }
        if file_type.is_dir() {
            fs::create_dir(&destination_path)?;
            copy_module_assets(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path)?;
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unsupported module asset: {}", source_path.display()),
            ));
        }
    }
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
    let mut compiled = 0usize;
    let mut skipped = 0usize;
    let mut failures = Vec::new();

    for module in &modules {
        let Some(source) = &module.source else {
            skipped += 1;
            println!(
                "Skipped {}: {}",
                module.name,
                module.error.as_deref().unwrap_or("no source")
            );
            continue;
        };
        let result = suomicpu16::image::build_source(source).map(|_| ());
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

    #[test]
    fn packages_source_into_non_overwriting_folder_disk() {
        let root = temporary_directory();
        fs::create_dir_all(&root).unwrap();
        let source = root.join("main.asm");
        fs::write(&source, "start: LDI R0, 42\nHALT\n").unwrap();
        let user_disk_path = root.join("USER0001");
        package_disk(&source, &user_disk_path, "User disk", 4096, false).unwrap();

        let user_disk = suomicpu16::disk::FolderDisk::open(&user_disk_path).unwrap();
        assert_eq!(user_disk.manifest().label, "User disk");
        assert!(user_disk.root().join("main.bin").is_file());
        assert!(!user_disk.root().join("kernel.bin").exists());
        assert!(source.exists());
        let boot_image = user_disk.load_boot_image().unwrap();
        let mut cpu = suomicpu16::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&boot_image.image.segments, boot_image.image.entry_point)
            .unwrap();
        cpu.run(16).unwrap();
        assert_eq!(cpu.registers[0], 42);

        let prebuilt_image_path = root.join("prebuilt.sc16img");
        let prebuilt_image = suomicpu16::image::build_source(&source).unwrap();
        suomicpu16::image::write_image(&prebuilt_image_path, &prebuilt_image).unwrap();
        let packaged_image_path = root.join("PACK0001");
        package_disk(
            &prebuilt_image_path,
            &packaged_image_path,
            "Prebuilt image",
            4096,
            false,
        )
        .unwrap();
        let packaged_image = suomicpu16::disk::FolderDisk::open(&packaged_image_path).unwrap();
        assert_eq!(
            packaged_image.load_boot_image().unwrap().image,
            prebuilt_image
        );

        let kernel_disk_path = root.join("KERN0001");
        package_disk(&source, &kernel_disk_path, "Kernel disk", 4096, true).unwrap();
        let kernel_disk = suomicpu16::disk::FolderDisk::open(&kernel_disk_path).unwrap();
        assert!(kernel_disk.root().join("kernel.bin").is_file());
        assert!(!kernel_disk.root().join("main.bin").exists());

        assert!(package_disk(&source, &user_disk_path, "Other", 4096, false).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
