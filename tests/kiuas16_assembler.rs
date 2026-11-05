use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use suomicpu16::assembler::rv64::assemble_boot_rom;
use suomicpu16::kiuas16::runtime::run_headless;
use suomicpu16::kiuas16::{BOOT_ROM_BYTES, Kiuas16Machine};

#[test]
fn independent_instruction_bytes() {
    let cases = [
        ("addi a0, zero, -1", 0xfff00513_u32),
        ("addiw a0, a0, -1", 0xfff5051b),
        ("lui t0, 0x10000", 0x100002b7),
        ("auipc ra, 1", 0x00001097),
        ("sd ra, 8(sp)", 0x00113423),
        ("ld ra, 8(sp)", 0x00813083),
        ("sb a0, 0(s0)", 0x00a40023),
        ("lbu a0, -1(s0)", 0xfff44503),
        ("beq a0, a1, 8", 0x00b50463),
        ("bne a0, zero, -4", 0xfe051ee3),
        ("beq zero, zero, -4096", 0x80000063),
        ("beq zero, zero, 4092", 0x7e000ee3),
        ("jal ra, 8", 0x008000ef),
        ("j -4", 0xffdff06f),
        ("j -1048576", 0x8000006f),
        ("j 1048572", 0x7fdff06f),
        ("jalr ra, -4(t0)", 0xffc280e7),
        ("slli a0, a0, 63", 0x03f51513),
        ("srai a0, a0, 63", 0x43f55513),
        ("sraiw a0, a0, 31", 0x41f5551b),
        ("add a0, a1, a2", 0x00c58533),
        ("subw a0, a1, a2", 0x40c5853b),
        ("mul a0, a1, a2", 0x02c58533),
        ("ret", 0x00008067),
        ("fence", 0x0ff0000f),
        ("lr.w a0, (t0)", 0x1002a52f),
        ("lr.d.aq a0, (t0)", 0x1402b52f),
        ("amoswap.w a0, a1, (t0)", 0x08b2a52f),
        ("amoswap.d.rl zero, a0, (t0)", 0x0aa2b02f),
        ("amoswap.w.aqrl a0, a1, (t0)", 0x0eb2a52f),
        ("wfi", 0x10500073),
    ];
    for (source, expected) in cases {
        let rom = assemble_boot_rom(source).unwrap();
        assert_eq!(&rom[..4], &expected.to_le_bytes(), "{source}");
        assert_eq!(rom.len(), BOOT_ROM_BYTES);
        assert!(rom[4..].iter().all(|byte| *byte == 0));
    }
}

#[test]
fn labels_alignment_and_little_endian_data() {
    let rom = assemble_boot_rom(
        ".globl start\nstart: j end\n.byte 0xff, -1\n.balign 8\n\
         value: .word 0x12345678\n.dword start\nend: j start",
    )
    .unwrap();
    assert_eq!(&rom[..4], &0x0140006f_u32.to_le_bytes());
    assert_eq!(&rom[4..8], &[255, 255, 0, 0]);
    assert_eq!(&rom[8..12], &0x12345678_u32.to_le_bytes());
    assert_eq!(&rom[12..20], &0x1000_u64.to_le_bytes());
    assert_eq!(&rom[20..24], &0xfedff06f_u32.to_le_bytes());
}

#[test]
fn signed_constant_edges_and_pc_relative_addresses_execute() {
    for value in [i32::MIN, -2049, -2048, -1, 0, 2047, 2048, i32::MAX] {
        let rom = assemble_boot_rom(&format!(
            "li a0, {value}\nla a1, data\nld a2, 0(a1)\nwfi\n.balign 8\ndata: .dword -7"
        ))
        .unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
        let report = run_headless(&mut machine, 12, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0);
        assert_eq!(machine.core().register(10), Some(i64::from(value) as u64));
        assert_eq!(machine.core().register(12), Some((-7_i64) as u64));
    }
}

#[test]
fn malformed_sources_diagnose_with_line_numbers() {
    let cases = [
        ("addi x32, x0, 0", "register"),
        ("addi a0, a0, 2048", "out of range"),
        ("addi a0", "operands"),
        ("sd ra, 2048(sp)", "out of range"),
        ("ld a0, sp", "offset(register)"),
        ("lr.w a0, 4(t0)", "zero offset"),
        ("amoswap.d a0, a1, -8(t0)", "zero offset"),
        ("amoswap.b a0, a1, (t0)", "width"),
        ("lr.w.acquire a0, (t0)", "suffix"),
        ("amoswap.d.aq.rl a0, a1, (t0)", "suffix"),
        ("lr.d a0, a1, (t0)", "operands"),
        ("amoswap.w a0, (t0)", "operands"),
        ("slli a0, a0, 64", "shift amount"),
        ("slliw a0, a0, 32", "shift amount"),
        ("j missing", "undefined label"),
        ("j 2", "aligned"),
        ("beq a0, a1, 4096", "out of range"),
        ("jal ra, 1048576", "out of range"),
        ("la a0, 9223372036854775807", "displacement"),
        ("li a0, 2147483648", "32-bit"),
        ("a: nop\na: nop", "duplicate label"),
        ("1: nop", "invalid label"),
        (".byte 256", "out of range"),
        (".word 4294967296", "out of range"),
        (".byte 1\nnop", "aligned"),
        (".balign 3", "power of two"),
        (".zero -1", "bounds"),
        (".zero 61441", "bounds"),
        (".zero 61440\nnop", "exceeds"),
        (".unknown 1", "unsupported directive"),
        (".globl absent", "undefined"),
        (".text junk", "no operands"),
        ("#include \"machine.h\"", "preprocessor"),
        ("# define SIZE 1", "preprocessor"),
        ("# 1 \"guest.S\"", "preprocessor"),
        ("unsupported a0", "unsupported RV64 instruction"),
    ];
    for (source, message) in cases {
        let diagnostic = assemble_boot_rom(source).unwrap_err().to_string();
        assert!(diagnostic.starts_with("line "), "{source}: {diagnostic}");
        assert!(diagnostic.contains(message), "{source}: {diagnostic}");
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Files(PathBuf);

impl Files {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "kiuas16-assembler-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory)
    }

    fn assemble(&self) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_kiuas16-asm"))
            .arg(self.0.join("guest.S"))
            .arg(self.0.join("rom.bin"))
            .output()
            .unwrap()
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        for entry in std::fs::read_dir(&self.0).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(&self.0).unwrap();
    }
}

#[test]
fn source_assembled_rom_calls_and_storage_execute_through_production_cli() {
    let files = Files::new();
    std::fs::write(
        files.0.join("guest.S"),
        include_str!("fixtures/kiuas16_storage.S"),
    )
    .unwrap();
    let mut disk = vec![0; 1024];
    disk[512..].fill(b'R');
    std::fs::write(files.0.join("disk.raw"), &disk).unwrap();
    let build = files.assemble();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    for writable in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"));
        command
            .arg("--rom")
            .arg(files.0.join("rom.bin"))
            .arg("--root")
            .arg(files.0.join("disk.raw"))
            .args(["--steps", "300", "--deny-traps"]);
        if writable {
            command.arg("--root-writable");
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            result.stdout,
            [b'K', b'R', 0, u8::from(!writable), 0, 1, 3, 0]
        );
        let mut expected = disk.clone();
        if writable {
            expected[512] = b'W';
        }
        assert_eq!(std::fs::read(files.0.join("disk.raw")).unwrap(), expected);
    }
}

#[test]
fn assembled_storage_guest_restores_stack_and_preserves_saved_registers() {
    let rom = assemble_boot_rom(include_str!("fixtures/kiuas16_storage.S")).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 20, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert_eq!(output, b"K");
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0x10000000));
    assert_eq!(machine.core().register(10), Some(75));
}

#[test]
fn cli_does_not_overwrite_source_or_output_after_invalid_assembly() {
    let files = Files::new();
    let source = files.0.join("guest.S");
    std::fs::write(&source, "bad_instruction\n").unwrap();
    std::fs::write(files.0.join("rom.bin"), b"keep").unwrap();
    assert!(!files.assemble().status.success());
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"keep");
    let result = Command::new(env!("CARGO_BIN_EXE_kiuas16-asm"))
        .arg(&source)
        .arg(&source)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("overwrite"));
    assert_eq!(std::fs::read(&source).unwrap(), b"bad_instruction\n");
}
