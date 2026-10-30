//! Verifies checked-in firmware and folder-disk packages end to end.
//! Checks reproducible BIOS assembly, packaged-program execution, and disk capacity.

use std::path::PathBuf;

use suomicpu16::cpu::Cpu;
use suomicpu16::disk::{FolderBootMode, FolderDisk};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shared_bios_image_matches_its_offline_assembly_source() {
    let root = repository_root();
    let source =
        suomicpu16::assembler::assemble_file(&root.join("firmware").join("bios.asm"), 0).unwrap();
    let binary = suomicpu16::image::read_image(&root.join("firmware").join("bios.bin")).unwrap();
    assert_eq!(binary.entry_point, 0);
    assert_eq!(binary.segments.len(), source.segments.len());
    for (actual, expected) in binary.segments.iter().zip(&source.segments) {
        assert_eq!(actual.address, expected.address);
        assert_eq!(actual.data, expected.data);
        assert_eq!(actual.permissions, expected.permissions);
        assert_eq!(actual.alignment, expected.alignment);
    }
}

#[test]
fn supported_sisuos_disk_package_validates_as_a_kernel_boot_disk() {
    let path = repository_root().join("disks").join("sisuos");
    let disk = FolderDisk::open(&path).unwrap();
    assert_eq!(disk.manifest().id, "sisuos");
    assert!(disk.used_bytes().unwrap() <= disk.manifest().capacity_bytes);
    let boot = disk.load_boot_image().unwrap();
    assert_eq!(boot.mode, FolderBootMode::Kernel);
    assert!(!boot.image.segments.is_empty());
}

#[test]
fn high_filesystem_bank_reads_nested_disk_assets() {
    let tank = repository_root().join("disks").join("tank-16");
    let disk = FolderDisk::open(&tank).unwrap();
    assert!(disk.read_file("maps/FINLAND.json", 0, 64).is_ok());
    assert!(disk.read_file("assets/TANK16_TILES.bin", 0, 64).is_ok());

    let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
    cpu.mount_folder_disk(&tank).unwrap();
    for (offset, byte) in b"maps/FINLAND.json\0".iter().enumerate() {
        cpu.write(suomicpu16::machine::FILESYSTEM_PATH + offset as u32, *byte);
    }
    cpu.write(suomicpu16::machine::FILESYSTEM_OPERATION, 0x42);
    cpu.write(suomicpu16::machine::FILESYSTEM_LENGTH, 0);
    cpu.write(suomicpu16::machine::FILESYSTEM_LENGTH + 1, 64);
    cpu.write(suomicpu16::machine::FILESYSTEM_COMMAND, 1);
    assert_eq!(cpu.read(suomicpu16::machine::FILESYSTEM_STATUS), 0);
    assert_eq!(cpu.read(suomicpu16::machine::FILESYSTEM_DATA), b'{');
}
