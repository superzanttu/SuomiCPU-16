//! Provides offline SC-16 build and disk-packaging commands.
//! Dispatches source/image builds, BIOS generation, program installation/migration,
//! and kernel/user folder-disk packaging.

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
        return Err("usage: sc16build <source.asm|source.c|source.bin|source.sc16img> [output.img] | build-c-at <source.c> <output.img> <base-address> | build-bios [bios.asm] [bios.bin] | build-programs | package-disk <source> <disk-folder> <label> <capacity-bytes> [--kernel] [--kernel-user-c <source.c>] | install-programs".into());
    };
    if source == "--help" || source == "-h" {
        println!("       sc16build build-program <slug> | install-program <slug>");
        println!("       sc16build build-programs | install-programs");
        println!("Program build/install commands use every programs/<slug>/program.json manifest.");
        println!("       sc16build migrate-program-disks --dry-run|--apply");
        println!("Program manifests: programs/<slug>/program.json; builds: build/programs/<slug>.");
        println!(
            "Installation preserves local disk data; conflicting modified package files are errors."
        );
        println!("Usage: sc16build <source.asm|source.c|source.bin|source.sc16img> [output.img]");
        println!("Build SC-16 source, wrap a flat binary, or validate an existing image.");
        println!("       sc16build build-bios [bios.asm] [bios.bin]");
        println!("Assemble the shared BIOS firmware into an SC-16 image.");
        println!("       sc16build build-c-at <source.c> <output.img> <base-address>");
        println!("Compile a C program for a relocated SC-16 address space.");
        println!(
            "       sc16build package-disk <source.asm|source.c|source.bin|source.sc16img> <disk-folder> <label> <capacity-bytes> [--kernel] [--kernel-user-c <source.c>]"
        );
        println!(
            "Build a source program into a new folder-backed boot disk; embed C user code in a kernel with --kernel-user-c."
        );
        return Ok(());
    }
    if source.to_str().is_some_and(|command| {
        matches!(
            command,
            "build-program"
                | "install-program"
                | "build-programs"
                | "install-programs"
                | "migrate-program-disks"
        )
    }) {
        let command = source.to_str().ok_or("command must be UTF-8")?;
        let arguments = arguments
            .map(|value| value.into_string().map_err(|_| "arguments must be UTF-8"))
            .collect::<Result<Vec<_>, _>>()?;
        return suomicpu16::programs::command(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            command,
            &arguments,
        );
    }
    if source == "build-c-at" {
        let usage = "usage: sc16build build-c-at <source.c> <output.img> <base-address>";
        let Some(c_source) = arguments.next().map(PathBuf::from) else {
            return Err(usage.into());
        };
        let Some(output) = arguments.next().map(PathBuf::from) else {
            return Err(usage.into());
        };
        let Some(base) = arguments
            .next()
            .and_then(|value| value.into_string().ok())
            .and_then(|value| {
                value.strip_prefix("0x").map_or_else(
                    || value.parse().ok(),
                    |hex| u32::from_str_radix(hex, 16).ok(),
                )
            })
        else {
            return Err(usage.into());
        };
        if arguments.next().is_some() {
            return Err(usage.into());
        }
        let image = suomicpu16::image::build_c_program_at(&c_source, base)?;
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
        return Ok(());
    }
    if source == "build-bios" {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
        let bios_source = arguments
            .next()
            .map(PathBuf::from)
            .unwrap_or_else(|| repository.join("firmware").join("bios.asm"));
        let bios_output = arguments
            .next()
            .map(PathBuf::from)
            .unwrap_or_else(|| bios_source.with_extension("bin"));
        if arguments.next().is_some() {
            return Err("usage: sc16build build-bios [bios.asm] [bios.bin]".into());
        }
        build_bios(&bios_source, &bios_output)?;
        println!(
            "Built shared BIOS firmware {} -> {}",
            bios_source.display(),
            bios_output.display()
        );
        return Ok(());
    }
    if source == "package-disk" {
        let usage = "usage: sc16build package-disk <source.asm|source.c|source.bin|source.sc16img> <disk-folder> <label> <capacity-bytes> [--kernel] [--kernel-user-c <source.c>]";
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
        let mut kernel = false;
        let mut kernel_user_c = None;
        while let Some(option) = arguments.next() {
            if option == "--kernel" {
                kernel = true;
            } else if option == "--kernel-user-c" {
                let Some(path) = arguments.next() else {
                    return Err(usage.into());
                };
                kernel_user_c = Some(PathBuf::from(path));
            } else {
                return Err(usage.into());
            }
        }
        if kernel_user_c.is_some() && !kernel {
            return Err("--kernel-user-c requires --kernel".into());
        }
        let source = PathBuf::from(program_source);
        let disk_folder = PathBuf::from(disk_folder);
        package_disk_with_embedded_c(
            &source,
            &disk_folder,
            &label,
            capacity,
            kernel,
            kernel_user_c.as_deref(),
        )?;
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

fn build_bios(source: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let source = source.canonicalize()?;
    if output.exists() && source == output.canonicalize()? {
        return Err("BIOS output path must not overwrite its source".into());
    }
    let image = suomicpu16::assembler::assemble_file(&source, 0)?;
    if image.entry_point != 0 {
        return Err("shared BIOS entry point must be address 0".into());
    }
    suomicpu16::image::write_image(output, &image)?;
    Ok(())
}

#[cfg(test)]
fn package_disk(
    source: &Path,
    disk_folder: &Path,
    label: &str,
    capacity_bytes: u64,
    kernel: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    package_disk_with_embedded_c(source, disk_folder, label, capacity_bytes, kernel, None)
}

fn package_disk_with_embedded_c(
    source: &Path,
    disk_folder: &Path,
    label: &str,
    capacity_bytes: u64,
    kernel: bool,
    embedded_c_user: Option<&Path>,
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
    let image = if let Some(user_source) = embedded_c_user {
        if !kernel {
            return Err("--kernel-user-c requires --kernel".into());
        }
        suomicpu16::image::build_kernel_with_embedded_c_source(source, user_source)?
    } else {
        match source.extension().and_then(|extension| extension.to_str()) {
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
        }
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
    fn packages_source_into_non_overwriting_folder_disk() {
        let root = temporary_directory();
        fs::create_dir_all(root.join("firmware")).unwrap();
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
        let kernel_example = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join("minimal_kernel.asm");
        package_disk(
            &kernel_example,
            &kernel_disk_path,
            "Kernel disk",
            65536,
            true,
        )
        .unwrap();
        let kernel_disk = suomicpu16::disk::FolderDisk::open(&kernel_disk_path).unwrap();
        assert!(kernel_disk.root().join("kernel.bin").is_file());
        assert!(!kernel_disk.root().join("main.bin").exists());

        assert!(package_disk(&source, &user_disk_path, "Other", 4096, false).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn builds_shared_bios_image_and_refuses_to_overwrite_its_source() {
        let root = temporary_directory();
        fs::create_dir_all(root.join("firmware")).unwrap();
        let source = root.join("firmware").join("bios.asm");
        let output = root.join("firmware").join("bios.bin");
        fs::write(&source, ".address 0\nstart:\n    HALT\n").unwrap();

        build_bios(&source, &output).unwrap();

        let image = suomicpu16::image::read_image(&output).unwrap();
        assert_eq!(image.entry_point, 0);
        assert_eq!(
            image.segments,
            suomicpu16::assembler::assemble_file(&source, 0)
                .unwrap()
                .segments
        );
        assert!(build_bios(&source, &source).is_err());

        fs::remove_dir_all(root).unwrap();
    }
}
