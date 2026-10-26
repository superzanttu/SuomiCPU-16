use std::path::PathBuf;

use suomicpu16::cpu::{Cpu, PrivilegeMode};
use suomicpu16::disk::{FolderBootMode, FolderDisk};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shared_bios_image_matches_its_offline_assembly_source() {
    let root = repository_root();
    let source = suomicpu16::assembler::assemble_file(&root.join("bios.asm"), 0).unwrap();
    let binary = suomicpu16::image::read_image(&root.join("bios.bin")).unwrap();
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
    for index in 1..=17 {
        let id = format!("APP{index:05}");
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
fn tank16_boot_disk_contains_runtime_maps_and_tile_assets() {
    let tank = repository_root().join("disks").join("APP00016");
    let disk = FolderDisk::open(&tank).unwrap();
    assert!(disk.read_file("maps/FINLAND.json", 0, 64).is_ok());
    assert!(disk.read_file("assets/TANK16_TILES.bin", 0, 64).is_ok());

    let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
    cpu.mount_folder_disk(&tank).unwrap();
    cpu.privilege_mode = PrivilegeMode::User;
    cpu.memory[0x1000..0x1012].copy_from_slice(b"maps/FINLAND.json\0");
    cpu.memory[0x43100 + 22..0x43100 + 26].copy_from_slice(&0x1000u32.to_be_bytes());
    cpu.memory[0x43100 + 26..0x43100 + 30].copy_from_slice(&0x2000u32.to_be_bytes());
    cpu.memory[0x43100 + 30..0x43100 + 32].copy_from_slice(&64u16.to_be_bytes());
    cpu.write(0x43100, 0x42);
    assert_eq!(cpu.memory[0x43110], 0);
    assert_eq!(cpu.memory[0x2000], b'{');

    cpu.write(0x43100, 0x40);
    assert_eq!(cpu.memory[0x43110], 1);
    assert_eq!(cpu.memory[0x2000], b'{');

    cpu.memory[0x4310a..0x4310c].copy_from_slice(&0u16.to_be_bytes());
    cpu.memory[0x4311a..0x4311e].copy_from_slice(&0x3000u32.to_be_bytes());
    cpu.memory[0x4311e..0x43120].copy_from_slice(&128u16.to_be_bytes());
    cpu.write(0x43100, 0x80);
    assert_eq!(cpu.memory[0x43110], 0);
    let count = u16::from_be_bytes([cpu.memory[0x4310e], cpu.memory[0x4310f]]);
    let mut found_map = false;
    for index in 0..count {
        cpu.memory[0x4310a..0x4310c].copy_from_slice(&index.to_be_bytes());
        cpu.memory[0x4311a..0x4311e].copy_from_slice(&0x3000u32.to_be_bytes());
        cpu.memory[0x4311e..0x43120].copy_from_slice(&128u16.to_be_bytes());
        cpu.write(0x43100, 0x80);
        assert_eq!(cpu.memory[0x43110], 0);
        let name = cpu.memory[0x3000..0x3080]
            .iter()
            .take_while(|byte| **byte != 0)
            .copied()
            .collect::<Vec<_>>();
        if name == b"maps/FINLAND.json" {
            found_map = true;
            break;
        }
    }
    assert!(found_map);
}

#[test]
fn folder_filesystem_header_works_from_a_cpu_executed_c_program() {
    let root = repository_root();
    let disk_path = root.join("disks").join("APP00001");
    let source = root.join("tests").join("folder_fs_read.c");
    let image = suomicpu16::image::load_program(&source).unwrap();
    let expected = FolderDisk::open(&disk_path)
        .unwrap()
        .read_file("main.bin", 0, 3)
        .unwrap()
        .0;
    let mut cpu = Cpu::with_memory_size(1 << 20).unwrap();
    cpu.load_program(&image.segments, image.entry_point)
        .unwrap();
    cpu.mount_folder_disk(&disk_path).unwrap();
    cpu.privilege_mode = PrivilegeMode::User;
    cpu.run(100_000).unwrap();
    assert!(!cpu.running, "C filesystem probe did not return");
    assert_eq!(cpu.memory[0x43110], 0);
    assert_eq!(&cpu.memory[0x40000..0x40003], expected.as_slice());
}
