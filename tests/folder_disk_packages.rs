//! Verifies checked-in firmware and folder-disk packages end to end.
//! Checks reproducible BIOS assembly, packaged-program execution, disk capacity,
//! and legacy block-disk import/export preservation.

use std::path::PathBuf;

use suomicpu16::cpu::{Cpu, PrivilegeMode};
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
fn packaged_modules_validate_and_execute_from_folder_disks() {
    let disks = repository_root().join("disks");
    for program in suomicpu16::programs::catalog(&repository_root())
        .unwrap()
        .into_iter()
        .filter(|p| p.slug != "sisuos")
    {
        let id = program.slug;
        let path = disks.join(&id);
        let disk = FolderDisk::open(&path).unwrap();
        assert_eq!(disk.manifest().id, id);
        assert!(disk.used_bytes().unwrap() <= disk.manifest().capacity_bytes);
        let boot = disk.load_boot_image().unwrap();
        assert_eq!(boot.mode, FolderBootMode::User, "unexpected mode on {id}");

        let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&boot.image.segments, boot.image.entry_point)
            .unwrap();
        cpu.mount_folder_disk(&path).unwrap();
        cpu.privilege_mode = match boot.mode {
            FolderBootMode::Kernel => PrivilegeMode::Supervisor,
            FolderBootMode::User => PrivilegeMode::User,
        };
        assert!(cpu.run(1000).unwrap() > 0, "no guest code ran on {id}");
    }
}

#[test]
fn imported_block_disks_preserve_the_source_and_export_all_blocks() {
    let disks = repository_root().join("disks");
    for id in ["DISK0001", "DISK0002"] {
        let source = disks.join(format!("{id}.json"));
        let imported = FolderDisk::open(&disks.join(id)).unwrap();
        assert!(source.is_file(), "legacy source was removed for {id}");
        assert_eq!(imported.manifest().id, id);
        assert_eq!(imported.used_bytes().unwrap(), 1024);
        assert_eq!(imported.list_files().unwrap().len(), 4);
    }
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
