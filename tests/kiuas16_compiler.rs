use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use suomicpu16::assembler::rv64::assemble_boot_rom;
use suomicpu16::c_compiler::rv64::compile_to_assembly;
use suomicpu16::kiuas16::runtime::run_headless;
use suomicpu16::kiuas16::{FileSectorMedia, Kiuas16Machine};

fn execute(source: &str) -> (Kiuas16Machine, Vec<u8>) {
    execute_with_budget(source, 10000)
}

fn execute_with_budget(source: &str, steps: u64) -> (Kiuas16Machine, Vec<u8>) {
    let assembly = compile_to_assembly(source, "guest.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, steps, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}\n{assembly}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    (machine, output)
}

#[test]
fn address_addends_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_address_addends.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"E*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 24];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    for (bytes, expected) in report[8..].chunks_exact(8).zip([23u64, 42]) {
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), expected);
    }
    let address = u64::from_le_bytes(report[..8].try_into().unwrap());
    assert!((0x1010..0x10000).contains(&address));
    assert_eq!(address % 8, 0);
    let table_address = address - 16;
    let mut table = [0; 24];
    machine.bus_mut().read(table_address, &mut table).unwrap();
    for (bytes, expected) in table.chunks_exact(8).zip([19u64, 23, 42]) {
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), expected);
    }
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn rom_origin_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_asm_org.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"G*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 24];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    for (bytes, expected) in report.chunks_exact(8).zip([0x2000u64, 42, 42]) {
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), expected);
    }
    let mut table = [0; 24];
    machine.bus_mut().read(0x2000, &mut table).unwrap();
    for (bytes, expected) in table.chunks_exact(8).zip([19u64, 23, 0x2010]) {
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), expected);
    }
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn data_location_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_data_location.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"O*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 24];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(u64::from_le_bytes(report[..8].try_into().unwrap()), 42);
    let table = u64::from_le_bytes(report[8..16].try_into().unwrap());
    let identity = u64::from_le_bytes(report[16..].try_into().unwrap());
    assert!((0x1008..0xffd0).contains(&table));
    assert_eq!(table & 7, 0);
    assert_eq!(identity, table + 8);
    let mut data = [0; 24];
    machine.bus_mut().read(table, &mut data).unwrap();
    assert_eq!(&data[..4], &(-8i32).to_le_bytes());
    assert_eq!(&data[4..8], &20i32.to_le_bytes());
    assert_eq!(&data[8..16], &identity.to_le_bytes());
    assert_eq!(&data[16..], &(table + 24).to_le_bytes());
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn data_expressions_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_data_expressions.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"R*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 32];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    for (bytes, expected) in report.chunks_exact(8).zip([42u64, (-16i64) as u64, 24, 32]) {
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), expected);
    }
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn bounded_alignment_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_bounded_alignment.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"A*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 32];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    let address = u64::from_le_bytes(report[..8].try_into().unwrap());
    assert!((0x1000..0xfff0).contains(&address));
    assert_eq!(address & 15, 0);
    assert_eq!(
        u64::from_le_bytes(report[8..16].try_into().unwrap()),
        address + 8
    );
    assert_eq!(
        u64::from_le_bytes(report[16..24].try_into().unwrap()),
        address + 9
    );
    assert_eq!(u64::from_le_bytes(report[24..].try_into().unwrap()), 732);
    let mut data = [0; 16];
    machine.bus_mut().read(address, &mut data).unwrap();
    assert_eq!(
        data,
        [
            19, 42, 42, 42, 42, 42, 42, 42, 23, 5, 0, 0, 7, 128, 128, 128
        ]
    );
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn data_aliases_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_data_aliases.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"D*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 32];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    for (bytes, expected) in
        report
            .chunks_exact(8)
            .zip([42u64, (-2147483648i64) as u64, 4294967295, u64::MAX])
    {
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), expected);
    }
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn filled_space_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_asm_space.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"B*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 16];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    let address = u64::from_le_bytes(report[..8].try_into().unwrap());
    assert!((0x1000..0xfff0).contains(&address));
    assert_eq!(u64::from_le_bytes(report[8..].try_into().unwrap()), 1189);
    let mut data = [0; 12];
    machine.bus_mut().read(address, &mut data).unwrap();
    assert_eq!(data, [42, 42, 42, 42, 255, 255, 255, 128, 128, 0, 0, 0]);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn unsigned_literals_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_unsigned_literals.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"U*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 24];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    for (bytes, expected) in report
        .chunks_exact(8)
        .zip([u64::MAX, 1u64 << 63, 0xfedcba9876543210])
    {
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), expected);
    }
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn string_data_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_asm_strings.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"S#,:*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 16];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    let address = u64::from_le_bytes(report[..8].try_into().unwrap());
    assert!((0x1000..0xfff0).contains(&address));
    assert_eq!(u64::from_le_bytes(report[8..].try_into().unwrap()), 42);
    let mut data = [0; 15];
    machine.bus_mut().read(address, &mut data).unwrap();
    assert_eq!(&data, b"S#,:*\0\0\n\0\xff\x80*\0\"\\");
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn tail_calls_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_tail.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"Q*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 8];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(u64::from_le_bytes(report), 42);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn counter_reads_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_counters.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"T*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 24];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    for bytes in report.chunks_exact(8) {
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), 1);
    }
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn translation_fence_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_sfence.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"V*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 16];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(u64::from_le_bytes(report[..8].try_into().unwrap()), 42);
    assert_eq!(u64::from_le_bytes(report[8..].try_into().unwrap()), 0);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn exponent_alignment_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_asm_alignment.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"E*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 24];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(u64::from_le_bytes(report[..8].try_into().unwrap()), 42);
    let table = u64::from_le_bytes(report[8..16].try_into().unwrap());
    let entry = u64::from_le_bytes(report[16..].try_into().unwrap());
    assert_eq!(table & 7, 0);
    assert_eq!(entry & 15, 0);
    assert!(table >= 0x1000 && entry > table && entry < 0x10000);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn halfword_data_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_halfwords.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"H*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 24];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    for (bytes, expected) in report.chunks_exact(8).zip([42i64, -1, 32768]) {
        assert_eq!(i64::from_le_bytes(bytes.try_into().unwrap()), expected);
    }
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn jump_forms_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_jump_forms.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"J*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 8];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(u64::from_le_bytes(report), 42);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn wide_immediates_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_wide_immediates.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"L*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 24];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    for (bytes, expected) in report
        .chunks_exact(8)
        .zip([i64::MAX, i64::MIN, 0x123456789abcdef])
    {
        assert_eq!(i64::from_le_bytes(bytes.try_into().unwrap()), expected);
    }
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn explicit_fences_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_fences.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"F*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 16];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(u64::from_le_bytes(report[..8].try_into().unwrap()), 42);
    assert_eq!(u64::from_le_bytes(report[8..].try_into().unwrap()), 1);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn scalar_pseudos_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_scalar_pseudos.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"P*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 24];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    for (bytes, expected) in report.chunks_exact(8).zip([42i64, -2147483606, 4]) {
        assert_eq!(i64::from_le_bytes(bytes.try_into().unwrap()), expected);
    }
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn word_shifts_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_word_shifts.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"W*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 32];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    for (bytes, expected) in report
        .chunks_exact(8)
        .zip([42i64, 42, -1073741782, -2147483564])
    {
        assert_eq!(i64::from_le_bytes(bytes.try_into().unwrap()), expected);
    }
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn rv64m_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_multiply.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"M*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 16];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(u64::from_le_bytes(report[..8].try_into().unwrap()), 42);
    assert_eq!(u64::from_le_bytes(report[8..].try_into().unwrap()), 1);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn branch_aliases_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_branch_aliases.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"B*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 8];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(u64::from_le_bytes(report), 42);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn unique_assembly_labels_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_unique_asm.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"U*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 8];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(u64::from_le_bytes(report), 42);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn unique_assembly_labels_ids_are_consistent_distinct_and_deterministic() {
    let mut source = String::from("int main(void) { long ids[12]; long same;");
    for index in 0..12 {
        source.push_str(&format!(
            "asm volatile(\"j .Lrv64_%=\\n.Lrv64_%=: addi %0, zero, %=\\naddi %[same], zero, %=\"
                : \"=r\"(ids[{index}]), [same] \"=r\"(same));
             if (ids[{index}] != same) return -1;"
        ));
    }
    source.push_str(
        "for (int i=0;i<12;i++) {
             if (ids[i]<0) return -2;
             for (int j=0;j<i;j++) if (ids[i]==ids[j]) return -3;
         }
         return ids[11]>9 ? 42 : -4; }",
    );
    let assembly = compile_to_assembly(&source, "unique-ids.c").unwrap();
    assert_eq!(
        assembly,
        compile_to_assembly(&source, "unique-ids.c").unwrap()
    );
    assert!(!assembly.contains("%="));
    assert_eq!(execute(&source).0.core().register(10), Some(42));
}

#[test]
fn unique_assembly_labels_preserve_escapes_basic_strings_and_zero_operand_forms() {
    let source = "static int nested(int n) {
            long value=n;
            asm volatile(\".Lnested_%=: addi %0, %0, 1\\nj .Lend_%=\\naddi %0, zero, -99\\n.Lend_%=:\"
                : \"+r\"(value));
            if(n) return value+nested(n-1);
            return value;
        }
        int main(void) {
            asm(\"nop # basic %= %%= %0 %[name]\");
            asm volatile(\".Lempty_%=: nop # escaped %%= %%0 %%[name]\" :);
            asm volatile(\"j .Lempty_%=\\n.Lempty_%=: nop\" : : : \"memory\");
            long value;
            asm(\"addi %0, %1, %= # literal %%= and id %=\\naddi %0, %0, -%=\" : \"=r\"(value) : \"r\"(nested(5)));
            return value==21 ? 42 : -1;
        }";
    let assembly = compile_to_assembly(source, "unique-escapes.c").unwrap();
    assert!(assembly.contains("nop # basic %= %%= %0 %[name]\n"));
    assert!(assembly.contains("nop # escaped %= %0 %[name]\n"));
    assert!(assembly.contains("# literal %= and id "));
    assert_eq!(execute(source).0.core().register(10), Some(42));
}

#[test]
fn unique_assembly_labels_invalid_templates_retain_source_lines_and_rom() {
    for template in [
        ".Lbad_%=: nop\\n%",
        ".Lbad_%=: nop\\n%z0",
        ".Lbad_%=: nop\\n%99",
        ".Lbad_%=: nop\\n%[missing]",
        ".Lbad_%=: nop\\n%[broken",
    ] {
        let source = format!(
            "#line 73 \"unique-header.h\"\nint main(void) {{ long value; asm(\"{template}\" : \"=r\"(value)); return 0; }}"
        );
        let diagnostic = compile_to_assembly(&source, "guest.c")
            .unwrap_err()
            .to_string();
        assert!(diagnostic.starts_with("guest.c:73:"), "{diagnostic}");
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success());
        assert!(String::from_utf8_lossy(&build.stderr).contains(":73:"));
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn lrsc_atomic_extrema_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_lrsc.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"A*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 8];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(u64::from_le_bytes(report), 42);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn numeric_local_labels_inline_assembly_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_numeric_labels.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"N*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 8];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(u64::from_le_bytes(report), 42);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn numeric_local_labels_reused_templates_keep_independent_targets() {
    let helper = "static inline long first(long value) {
        __asm__ volatile(
            \"1: addi %0, %0, -1\\nbne %0, zero, 1b\\nj 1f\\naddi %0, zero, -99\\n1: addi %0, %0, 21\"
            : \"+r\"(value));
        return value;
    }";
    let source = format!(
        "{helper}\n{}\nint main(void) {{ return first(4)+second(7); }}",
        helper.replace("first", "second")
    );
    assert_eq!(execute(&source).0.core().register(10), Some(42));
    for template in ["j 1f", "j 1b"] {
        let files = Files::new(&format!(
            "int main(void) {{ __asm__ volatile(\"{template}\"); return 0; }}"
        ));
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success());
        assert!(String::from_utf8_lossy(&build.stderr).contains("undefined numeric local label"));
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn static_inline_csr_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_inline.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"I*\n");
    assert_eq!(machine.core().register(10), Some(42));
    let mut report = [0; 16];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(
        u64::from_le_bytes(report[..8].try_into().unwrap()),
        0x1234567887654321
    );
    assert_eq!(u64::from_le_bytes(report[8..].try_into().unwrap()), 42);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn static_inline_spellings_specifier_order_and_redeclarations_execute() {
    for specifiers in [
        "static inline int",
        "inline static int",
        "static int inline",
        "int static inline",
        "inline int static",
        "int inline static",
        "static __inline int",
        "__inline__ static int",
        "static int __inline__",
        "static inline inline int",
    ] {
        let source = format!(
            "{specifiers} plus(int n);
             static int plus(int n) {{ return n+1; }}
             int main(void) {{ return plus(41); }}"
        );
        assert_eq!(
            execute(&source).0.core().register(10),
            Some(42),
            "{specifiers}"
        );
    }
    for declaration in [
        "inline int f(int n);",
        "extern inline int f(int n);",
        "int inline f(int n);",
    ] {
        let source = format!(
            "static int f(int n); {declaration}
            static int f(int n) {{ return n+2; }}
            int main(void) {{ return f(40); }}"
        );
        assert_eq!(execute(&source).0.core().register(10), Some(42));
    }
    let source = "typedef unsigned long word;
        static word inline plus(word value) { return value+1; }
        static inline word inspect(word value) { return plus(value); }
        int main(void) { return inspect(41UL); }";
    assert_eq!(execute(source).0.core().register(10), Some(42));
    let files = Files::new("#include \"helpers.h\"\nint main(void) { return plus(41); }");
    std::fs::write(files.0.join("helpers.h"),
        "#ifndef HELPERS_H\n#define HELPERS_H\n#define PRIVATE static __inline__\nPRIVATE int plus(int n) { return n+1; }\n#endif\n").unwrap();
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let rom = std::fs::read(files.0.join("rom.bin")).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let run = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(run.traps, 0);
    assert!(run.waiting > 0);
    assert_eq!(machine.core().register(10), Some(42));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn static_inline_callback_identity_recursion_and_stack_arguments_execute() {
    let source = "typedef long (*operation)(long,long,long,long,long,long,long,long,long);
        static inline long sum(long a,long b,long c,long d,long e,long f,long g,long h,long i) {
            if(a) return sum(a-1,b,c,d,e,f,g,h,i)+1;
            return b+c+d+e+f+g+h+i;
        }
        static operation table[2]={sum,sum};
        int calls;
        static inline long next(void) { return ++calls; }
        int main(void) {
            operation callback=sum;
            if(callback!=&sum || table[0]!=table[1]) return -1;
            long result=callback(next(),next(),next(),next(),next(),next(),next(),next(),next());
            return result==45 && calls==9 && callback==table[1] ? 77 : -2;
        }";
    assert_eq!(execute(source).0.core().register(10), Some(77));
}

#[test]
fn static_inline_variadic_format_and_noreturn_attributes_keep_existing_semantics() {
    let source = "#include <stdarg.h>
        static inline long collect(const char *text,...) __attribute__((format(printf,1,2))) {
            va_list list; va_start(list,text);
            long result=va_arg(list,long)+va_arg(list,long);
            va_end(list); return result;
        }
        int main(void) { return collect(\"%ld %ld\",19L,23L); }";
    assert_eq!(execute(source).0.core().register(10), Some(42));
    let invalid = source.replace("19L,23L", "19,23L");
    assert!(
        compile_to_assembly(&invalid, "inline-format.c")
            .unwrap_err()
            .to_string()
            .contains("incompatible type")
    );
    let source = "static inline void stop(void) __attribute__((noreturn)) {
            *(volatile unsigned char *)0x10000000UL='S';
            for(;;) __asm__ volatile(\"wfi\");
        }
        int main(void) { stop(); *(volatile unsigned char *)0x10000000UL='X'; return 0; }";
    let assembly = compile_to_assembly(source, "inline-noreturn.c").unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let mut output = Vec::new();
    let run = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(run.traps, 0);
    assert!(run.waiting > 0);
    assert_eq!(output, b"S");
    assert!(machine.core().register(2).unwrap() < 0x80010000);
    assert_eq!(machine.core().register(2).unwrap() % 16, 0);
}

#[test]
fn static_inline_independent_caller_observes_normal_scalar_abi() {
    let source="static inline long helper(long a,long b,long c,long d,long e,long f,long g,long h,long i) {
            return a+b+c+d+e+f+g+h+i;
        }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "inline-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Linline_entry\n", 1);
    assembly.push_str(
        "\n.Linline_entry:
        addi sp, sp, -32
        sd ra, 24(sp)
        li a0, 1
        li a1, 2
        li a2, 3
        li a3, 4
        li a4, 5
        li a5, 6
        li a6, 7
        li a7, 8
        li t0, 9
        sd t0, 0(sp)
        li s1, 123
        call .Lrv64_internal_helper
        li t0, 123
        bne s1, t0, .Linline_bad
        ld ra, 24(sp)
        addi sp, sp, 32
        ret
        .Linline_bad:
        ebreak\n",
    );
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let run = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(run.traps, 0, "{:?}", run.last_trap);
    assert!(run.waiting > 0);
    assert_eq!(machine.core().register(10), Some(45));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn static_inline_invalid_linkage_and_placements_preserve_rom() {
    for declaration in [
        "inline int f(void) { return 0; }",
        "extern inline int f(void) { return 0; }",
        "inline int f(void); static int f(void) { return 0; }",
        "static inline int object;",
        "static int inline object[4];",
        "typedef inline int number;",
        "static inline int;",
        "static int f(inline int n) { return n; }",
        "static int f(int inline n) { return n; }",
        "static int f(void) { inline int n=0; return n; }",
        "static inline int f(void);",
        "static inline int f(void) { return 0; } extern int f(void) { return 1; }",
    ] {
        let source = format!("{declaration} int main(void) {{ return 0; }}");
        assert!(
            compile_to_assembly(&source, "inline-invalid.c").is_err(),
            "{declaration}"
        );
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let result = files.compile();
        assert!(!result.status.success(), "{declaration}");
        assert!(!result.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    let source = "#line 137 \"inline-header.h\"\ninline int f(void) { return 0; }\nint main(void) { return 0; }";
    let diagnostic = compile_to_assembly(source, "inline.c")
        .unwrap_err()
        .to_string();
    assert!(diagnostic.contains("inline-header.h:137:"), "{diagnostic}");
    assert!(
        diagnostic.contains("external inline semantics"),
        "{diagnostic}"
    );
}

#[test]
fn nonstring_byte_buffer_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_nonstring.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"NODE\n");
    assert_eq!(machine.core().register(10), Some(262));
    let mut report = [0; 24];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    assert_eq!(u64::from_le_bytes(report[..8].try_into().unwrap()), 262);
    assert_eq!(u64::from_le_bytes(report[8..16].try_into().unwrap()), 4);
    let address = u64::from_le_bytes(report[16..].try_into().unwrap());
    assert_eq!(address % 32, 0);
    let mut buffer = [0; 8];
    machine.bus_mut().read(address, &mut buffer).unwrap();
    assert_eq!(&buffer, b"NODE\0\0\0\0");
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn nonstring_arrays_and_pointers_preserve_qualified_bytes_and_poisoned_startup() {
    let source = "#include <stdint.h>
        static const signed char signed_bytes[3] __attribute__((nonstring))={-1,0,127};
        __attribute__((nonstring)) unsigned char bytes[4]={0,255,1,128};
        static char empty[4] __attribute__((aligned(64),nonstring));
        static const signed char *pointer __attribute__((nonstring))=signed_bytes;
        volatile unsigned char *view __attribute__((__nonstring__))=bytes;
        int main(void) {
            volatile uint64_t *report=(volatile uint64_t *)0x80000000UL;
            report[0]=(uint64_t)signed_bytes; report[1]=(uint64_t)bytes;
            report[2]=(uint64_t)empty; report[3]=(uint64_t)pointer;
            report[4]=sizeof(signed_bytes); report[5]=sizeof(bytes);
            if(pointer[0]!=-1 || pointer[1]!=0 || pointer[2]!=127) return -1;
            if(empty[0] || empty[1] || empty[2] || empty[3]) return -2;
            view[2]=42; empty[3]='X';
            return bytes[2]+empty[3];
        }";
    let assembly = compile_to_assembly(source, "nonstring-bytes.c").unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    machine
        .bus_mut()
        .write(0x80010000, &vec![0xa5; 65536])
        .unwrap();
    let mut output = Vec::new();
    let run = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(run.traps, 0, "{:?}", run.last_trap);
    assert!(run.waiting > 0);
    assert!(output.is_empty());
    assert_eq!(machine.core().register(10), Some(130));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut report = [0; 48];
    machine.bus_mut().read(0x80000000, &mut report).unwrap();
    let words = report
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(words[0], words[3]);
    assert_eq!(words[2] % 64, 0);
    assert_eq!(&words[4..], &[3, 4]);
    for (address, expected) in [
        (words[0], [255, 0, 127, 0, 0, 0, 0, 0]),
        (words[1], [0, 255, 42, 128, 0, 0, 0, 0]),
        (words[2], [0, 0, 0, b'X', 0, 0, 0, 0]),
    ] {
        let mut bytes = [0; 8];
        machine.bus_mut().read(address, &mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }
    let mut trailing = [0; 8];
    machine.bus_mut().read(words[2] + 8, &mut trailing).unwrap();
    assert_eq!(trailing, [0xa5; 8]);
}

#[test]
fn nonstring_attributes_merge_per_object_without_changing_ordinary_calls() {
    for declaration in [
        "extern char raw[4] __attribute__((nonstring));",
        "__attribute__((nonstring)) extern char raw[4];",
        "extern __attribute__((__nonstring__)) char raw[4];",
        "extern char __attribute__((nonstring,nonstring)) raw[4];",
    ] {
        let source = format!(
            "{declaration}
             char raw[4]=\"ABCD\";
             char ordinary[4]=\"ok\";
             int f(const char *s,...) __attribute__((format(printf,1,2))) {{ return 42; }}
             int main(void) {{ return raw[3]=='D' ? f(\"%s %p %c\",ordinary,(void *)raw,raw[0]) : -1; }}"
        );
        assert_eq!(execute(&source).0.core().register(10), Some(42));
    }
    let source = "char raw[4]=\"ABCD\", ordinary[4]=\"ok\";
        int f(const char *s,...) __attribute__((format(printf,1,2))) { return 7; }
        int main(void) { return f(\"%s\",ordinary); }
        extern char raw[4] __attribute__((nonstring));";
    assert_eq!(execute(source).0.core().register(10), Some(7));
    let source = source.replace("f(\"%s\",ordinary)", "f(\"%s\",raw)");
    assert!(
        compile_to_assembly(&source, "late-nonstring.c")
            .unwrap_err()
            .to_string()
            .contains("not a nonstring object")
    );
    let source = "char raw[4] __attribute__((nonstring)),ordinary[4]=\"ok\";
        int f(const char *s,...) __attribute__((format(printf,1,2))) { return 7; }
        int main(void) { return f(\"%s\",ordinary); }";
    assert_eq!(execute(source).0.core().register(10), Some(7));
}

#[test]
fn nonstring_known_string_uses_diagnose_without_overwriting_rom() {
    for expression in [
        "raw",
        "raw+1",
        "1+raw",
        "&raw[0]",
        "&raw[2]-1",
        "(const char *)raw",
        "flag ? raw : \"ok\"",
        "flag ? \"ok\" : raw",
        "pointer",
        "pointer+1",
        "(const char *)(const void *)signed_raw",
        "(const char *)(const void *)unsigned_raw",
        "volatile_pointer",
        "pointer++",
        "(pointer=raw)",
    ] {
        let source = format!(
            "char raw[4] __attribute__((nonstring))=\"ABCD\";
             signed char signed_raw[4] __attribute__((nonstring));
             unsigned char unsigned_raw[4] __attribute__((nonstring));
             const char *pointer __attribute__((nonstring))=raw;
             volatile char *volatile_pointer __attribute__((nonstring))=raw;
             int flag;
             int f(const char *s,...) __attribute__((format(printf,1,2))) {{ return 0; }}
             int main(void) {{ return f(\"%s\",{expression}); }}"
        );
        let diagnostic = compile_to_assembly(&source, "nonstring-use.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("not a nonstring object"),
            "{expression}: {diagnostic}"
        );
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let result = files.compile();
        assert!(!result.status.success(), "{expression}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("not a nonstring object"),
            "{expression}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    for call in [
        "f(raw)",
        "f(pointer)",
        "f(\"%.2s\",raw)",
        "f(\"%*s\",4,raw)",
    ] {
        let source = format!(
            "char raw[4] __attribute__((nonstring))=\"ABCD\";
             const char *pointer __attribute__((nonstring))=raw;
             int f(const char *s,...) __attribute__((format(printf,1,2))) {{ return 0; }}
             int main(void) {{ return {call}; }}"
        );
        assert!(
            compile_to_assembly(&source, "nonstring-format.c")
                .unwrap_err()
                .to_string()
                .contains("not a nonstring object")
        );
    }
}

#[test]
fn nonstring_invalid_types_arguments_and_placements_preserve_rom() {
    for declaration in [
        "int object __attribute__((nonstring));",
        "char object __attribute__((nonstring));",
        "int object[4] __attribute__((nonstring));",
        "int *object __attribute__((nonstring));",
        "char **object __attribute__((nonstring));",
        "char *object[4] __attribute__((nonstring));",
        "typedef char object __attribute__((nonstring));",
        "typedef char *object __attribute__((nonstring));",
        "int f(void) __attribute__((nonstring)) { return 0; }",
        "__attribute__((nonstring)) int;",
        "char object[4] __attribute__((nonstring(1)));",
        "int f(char *p __attribute__((nonstring))) { return 0; }",
        "int f(__attribute__((nonstring)) char *p) { return 0; }",
        "int f(void) { char object[4] __attribute__((nonstring)); return 0; }",
    ] {
        let source = format!("{declaration} int main(void) {{ return 0; }}");
        assert!(
            compile_to_assembly(&source, "nonstring-invalid.c").is_err(),
            "{declaration}"
        );
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        assert!(!files.compile().status.success(), "{declaration}");
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn nonstring_diagnostics_keep_mapped_locations_and_documented_unchecked_boundaries() {
    for (source, location) in [
        (
            "#line 137 \"bytes-header.h\"\nint wrong[4] __attribute__((nonstring));\nint main(void) { return 0; }",
            "bytes-header.h:137:",
        ),
        (
            "char raw[4] __attribute__((nonstring));\nint f(const char *s,...) __attribute__((format(printf,1,2))) { return 0; }\nint main(void) {\n#line 207 \"bytes-user.h\"\nreturn f(\"%s\",raw);\n}",
            "bytes-user.h:207:",
        ),
    ] {
        let diagnostic = compile_to_assembly(source, "nonstring.c")
            .unwrap_err()
            .to_string();
        assert!(diagnostic.contains(location), "{diagnostic}");
    }
    let source = "char raw[4] __attribute__((nonstring))=\"ABCD\";
        typedef int (*logger)(const char *s,...);
        int f(const char *s,...) __attribute__((format(printf,1,2))) { return 7; }
        int main(void) {
            const char *alias=raw; const char *format=\"%s\"; logger callback=f;
            return f(\"%s\",alias)+f(format,raw)+callback(\"%s\",raw)
                +f(\"%s\",1 ? \"ok\" : raw);
        }";
    assert_eq!(execute(source).0.core().register(10), Some(28));
}

#[test]
fn printf_format_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_format.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, b"F:7/9!%\n");
    assert_eq!(machine.core().register(10), Some(8));
    let mut bytes = [0; 8];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(u64::from_le_bytes(bytes), 8);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn printf_format_scalar_conversions_execute_after_default_promotions() {
    for (format, argument, ty, expected) in [
        ("%d", "(signed char)-7", "int", -7i64),
        ("%i", "(short)-8", "int", -8),
        ("%hhd", "(signed char)-9", "int", -9),
        ("%hd", "(short)-10", "int", -10),
        ("%hhu", "(unsigned char)250", "int", 250),
        ("%hu", "(unsigned short)65535", "int", 65535),
        ("%u", "4000000000U", "unsigned int", 4000000000),
        ("%o", "17U", "unsigned int", 17),
        ("%x", "19U", "unsigned int", 19),
        ("%X", "21U", "unsigned int", 21),
        ("%ld", "-4294967297L", "long", -4294967297),
        ("%li", "4294967297L", "long", 4294967297),
        ("%lu", "4294967298UL", "unsigned long", 4294967298),
        ("%lo", "4294967299UL", "unsigned long", 4294967299),
        ("%lx", "4294967300UL", "unsigned long", 4294967300),
        ("%lX", "4294967301UL", "unsigned long", 4294967301),
        ("%jd", "(intmax_t)-13", "intmax_t", -13),
        ("%ju", "(uintmax_t)23", "uintmax_t", 23),
        ("%zd", "(ptrdiff_t)-14", "ptrdiff_t", -14),
        ("%zu", "(size_t)24", "size_t", 24),
        ("%td", "(ptrdiff_t)-15", "ptrdiff_t", -15),
        ("%tu", "25UL", "unsigned long", 25),
        ("%c", "(_Bool)1", "int", 1),
    ] {
        let source = format!(
            "#include <stdarg.h>
             #include <stdint.h>
             #include <stddef.h>
             long collect(const char *text, ...) __attribute__((format(printf,1,2))) {{
                 va_list list; va_start(list,text);
                 long result=(long)va_arg(list,{ty}); va_end(list); return result;
             }}
             int main(void) {{
                 long value=collect(\"{format}\",{argument});
                 return value=={expected}L ? 77 : -1;
             }}"
        );
        let (machine, _) = execute(&source);
        assert_eq!(machine.core().register(10), Some(77), "{format}");
    }
}

#[test]
fn printf_format_stars_strings_pointers_and_escaped_percent_execute_once() {
    let (machine, _) = execute(
        "#include <stdarg.h>
         int calls;
         int next(void) { calls++; return calls; }
         long collect(const char *text, ...) __attribute__((format(printf,1,2))) {
             va_list list; va_start(list,text);
             int width=va_arg(list,int),precision=va_arg(list,int);
             unsigned long value=va_arg(list,unsigned long);
             const char *string=va_arg(list,const char *);
             const void *pointer=va_arg(list,const void *);
             int result=width==1 && precision==2 && value==4294967297UL
                 && string[0]=='A' && pointer==(const void *)string && calls==2;
             va_end(list); return result;
         }
         int main(void) {
             const char *text=\"A\";
             return collect(\"%% %-#0*.*lx %.1s %p\",next(),next(),4294967297UL,text,(const void *)text);
         }",
    );
    assert_eq!(machine.core().register(10), Some(1));
    for literal in ["%%", "%05d", "%+ d", "%10.3i", "%.d", "%d\\0%q"] {
        let args = if literal == "%%" { "" } else { ",7" };
        let source = format!(
            "int f(const char *s,...) __attribute__((format(printf,1,2))) {{ return 42; }}
             int main(void) {{ return f(\"{literal}\"{args}); }}"
        );
        assert_eq!(execute(&source).0.core().register(10), Some(42));
    }
}

#[test]
fn printf_format_attributes_merge_and_check_calls_before_late_redeclarations() {
    for declaration in [
        "__attribute__((format(printf,2,3))) static int f(int n,const char *s,...);",
        "static __attribute__((__format__(__printf__,2,3))) int f(int n,const char *s,...);",
        "static int __attribute__((format(printf,1+1,3))) f(int n,const char *s,...);",
        "static int f(int n,const char *s,...) __attribute__((format(printf,2,3),format(__printf__,2,3)));",
    ] {
        let source = format!(
            "{declaration}
             static int f(int n,const char *s,...) {{ return n; }}
             int main(void) {{ return (&f)(42,\"%ld\",7L); }}"
        );
        assert_eq!(execute(&source).0.core().register(10), Some(42));
    }
    let parameters = (0..31)
        .map(|index| format!("int p{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let arguments = (0..31).map(|_| "42").collect::<Vec<_>>().join(",");
    let source = format!(
        "int f({parameters},const char *s,...) __attribute__((format(printf,32,33))) {{ return p0; }}
         int main(void) {{ return f({arguments},\"\"); }}"
    );
    assert_eq!(execute(&source).0.core().register(10), Some(42));
    let invalid = source.replace(",\"\")", ",\"%d\")");
    assert!(
        compile_to_assembly(&invalid, "format-boundary.c")
            .unwrap_err()
            .to_string()
            .contains("more arguments")
    );
    let source = "int f(const char *s,...) { return 0; }
        int main(void) { return f(\"%ld\",1); }
        int f(const char *s,...) __attribute__((format(printf,1,2)));";
    assert!(
        compile_to_assembly(source, "late-format.c")
            .unwrap_err()
            .to_string()
            .contains("incompatible type")
    );
}

#[test]
fn printf_format_va_list_wrappers_check_syntax_without_reading_argument_types() {
    let source = "#include <stdarg.h>
        long render(const char *s,va_list args) __attribute__((format(printf,1,0))) {
            return va_arg(args,long);
        }
        int wrapper(const char *s,...) __attribute__((format(printf,1,2))) {
            va_list list; va_start(list,s);
            long value=render(\"%ld %s %*.*u\",list);
            va_end(list); return value;
        }
        int main(void) { return wrapper(\"%ld\",42L); }";
    assert_eq!(execute(source).0.core().register(10), Some(42));
    let invalid = source.replace("%ld %s %*.*u", "%n");
    assert!(
        compile_to_assembly(&invalid, "vformat.c")
            .unwrap_err()
            .to_string()
            .contains("unsupported printf")
    );
}

#[test]
fn printf_format_nonliteral_and_dynamic_callback_calls_remain_unchecked() {
    let (machine, _) = execute(
        "typedef int (*logger)(const char *s,...);
         int f(const char *s,...) __attribute__((format(printf,1,2))) { return 7; }
         int main(void) {
             const char *format=\"%n\"; logger callback=f;
             return f(format,1L)+callback(\"%q\",2L);
         }",
    );
    assert_eq!(machine.core().register(10), Some(14));
}

#[test]
fn printf_format_invalid_literals_and_types_preserve_existing_rom() {
    for (arguments, message) in [
        ("\"%d\"", "more arguments"),
        ("\"\",1", "unused arguments"),
        ("\"%%\",1", "unused arguments"),
        ("\"%d\",1L", "incompatible type"),
        ("\"%u\",1", "incompatible type"),
        ("\"%ld\",1", "incompatible type"),
        ("\"%lu\",1L", "incompatible type"),
        ("\"%d\",(void *)0", "incompatible type"),
        ("\"%s\",1", "incompatible type"),
        ("\"%s\",(int *)0", "incompatible type"),
        ("\"%p\",(char *)0", "incompatible type"),
        ("\"%*d\",1L,2", "incompatible type"),
        ("\"%.*d\",1U,2", "incompatible type"),
        ("\"%*.*d\",1,2", "more arguments"),
        ("\"%\"", "unterminated printf"),
        ("\"%0.\"", "unterminated printf"),
        ("\"%n\",(int *)0", "unsupported printf"),
        ("\"%f\",1", "unsupported printf"),
        ("\"%lc\",1", "unsupported printf"),
        ("\"%ls\",\"a\"", "unsupported printf"),
        ("\"%lld\",1L", "unsupported printf"),
        ("\"%llu\",1UL", "unsupported printf"),
        ("\"%Ld\",1L", "unsupported printf"),
        ("\"%1$d\",1", "unsupported printf"),
        ("\"%*1$d\",1,2", "unsupported printf"),
        ("\"%q\",1", "unsupported printf"),
        ("\"%+s\",\"text\"", "unsupported printf flags"),
        ("\"%#d\",1", "unsupported printf flags"),
        ("\"%.2c\",1", "unsupported printf flags"),
        ("\"%.1p\",(void *)0", "unsupported printf flags"),
    ] {
        let source = format!(
            "int f(const char *s,...) __attribute__((format(printf,1,2))) {{ return 0; }}
             int main(void) {{ return f({arguments}); }}"
        );
        let diagnostic = compile_to_assembly(&source, "format-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(diagnostic.contains(message), "{arguments}: {diagnostic}");
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let result = files.compile();
        assert!(!result.status.success(), "{arguments}");
        assert!(String::from_utf8_lossy(&result.stderr).contains(message));
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn printf_format_invalid_attributes_redeclarations_and_placements_are_diagnosed() {
    for attribute in [
        "format(scanf,1,2)",
        "format(printf,0,2)",
        "format(printf,2,2)",
        "format(printf,1,1)",
        "format(printf,1,3)",
        "format(printf,-1,2)",
        "format(printf,1,1/0)",
        "format(printf,1,65536)",
        "format(printf,1,0),format(printf,1,2)",
        "format(printf,1)",
    ] {
        let source = format!(
            "int f(const char *s,...) __attribute__(({attribute})) {{ return 0; }}
             int main(void) {{ return 0; }}"
        );
        assert!(
            compile_to_assembly(&source, "format-attribute.c").is_err(),
            "{attribute}"
        );
    }
    for source in [
        "int f(int s,...) __attribute__((format(printf,1,2))) { return 0; }",
        "int f(const char **s,...) __attribute__((format(printf,1,2))) { return 0; }",
        "int f(const char *s) __attribute__((format(printf,1,2))) { return 0; }",
        "int f(const char *s,...) __attribute__((format(printf,1,2))); int f(const char *s,...) __attribute__((format(printf,1,0))) { return 0; }",
        "int object __attribute__((format(printf,1,2)));",
        "typedef int object __attribute__((format(printf,1,2)));",
        "__attribute__((format(printf,1,2))) int;",
        "int f(const char *s __attribute__((format(printf,1,2))),...) { return 0; }",
        "int f(void) { int local __attribute__((format(printf,1,2))); return 0; }",
    ] {
        let source = format!("{source} int main(void) {{ return 0; }}");
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        assert!(!files.compile().status.success(), "{source}");
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn printf_format_diagnostics_keep_mapped_call_and_declaration_locations() {
    for (source, location) in [
        (
            "int f(const char *s,...) __attribute__((format(printf,1,2))) { return 0; }\nint main(void) {\n#line 207 \"logger-user.h\"\nreturn f(\"%ld\",1);\n}",
            "logger-user.h:207:",
        ),
        (
            "#line 123 \"logger-header.h\"\nint f(int s,...) __attribute__((format(printf,1,2))) { return 0; }\nint main(void) { return 0; }",
            "logger-header.h:123:",
        ),
        (
            "int f(const char *s,...) __attribute__((format(printf,1,2))) { return 0; }\n#line 92 \"logger-header.h\"\nint f(const char *s,...) __attribute__((format(printf,1,0)));\nint main(void) { return 0; }",
            "logger-header.h:92:",
        ),
        (
            "#line 87 \"logger-header.h\"\nint f(const char *s,...) __attribute__((format(scanf,1,2))) { return 0; }\nint main(void) { return 0; }",
            "logger-header.h:87:",
        ),
    ] {
        let diagnostic = compile_to_assembly(source, "format.c")
            .unwrap_err()
            .to_string();
        assert!(diagnostic.contains(location), "{diagnostic}");
    }
}

#[test]
fn scalar_varargs_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_varargs.c");
    let (mut machine, output) = execute(source);
    assert_eq!(machine.core().register(10), Some(132));
    assert_eq!(output, b"V\x84\n");
    let mut bytes = [0; 8];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(u64::from_le_bytes(bytes), 132);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn scalar_varargs_cross_every_named_register_and_stack_boundary() {
    for named in [1usize, 2, 6, 7, 8, 9, 10, 31, 32] {
        for extra in [0, 1, 2, 7, 9, 23, 31] {
            if named + extra > 32 {
                continue;
            }
            let parameters = (0..named)
                .map(|i| format!("long p{i}"))
                .collect::<Vec<_>>()
                .join(",");
            let arguments = (0..named + extra)
                .map(|i| format!("{}L", i + 1))
                .collect::<Vec<_>>()
                .join(",");
            let checks = (0..named)
                .map(|i| format!("if(p{i}!={}L) return -1;", i + 1))
                .collect::<String>();
            let expected = (named + 1..=named + extra).sum::<usize>();
            let source = format!(
                "#include <stdarg.h>
                 long collect({parameters},...) {{
                     {checks}
                     va_list args;
                     va_start(args,p{last});
                     long sum=0;
                     for(int i=0;i<{extra};i++) sum += va_arg(args,long);
                     va_end(args);
                     return sum;
                 }}
                 int main(void) {{ return collect({arguments}); }}",
                last = named - 1
            );
            let (machine, output) = execute(&source);
            assert_eq!(
                machine.core().register(10),
                Some(expected as u64),
                "named={named} extra={extra}"
            );
            assert!(output.is_empty());
        }
    }
}

#[test]
fn scalar_varargs_promote_narrow_arguments_preserve_pointer_types_and_copy_cursors() {
    let (mut machine, _) = execute(
        "#include <stdarg.h>
         #include <stdint.h>
         typedef long (*callback)(long);
         long plus(long n) { return n+3; }
         long inspect(int last, ...) {
             va_list args,copy;
             va_start(args,last);
             va_copy(copy,args);
             volatile uint64_t *report=(volatile uint64_t *)0x80000000UL;
             report[0]=va_arg(args,int);
             report[1]=va_arg(args,int);
             report[2]=va_arg(args,int);
             report[3]=va_arg(args,int);
             report[4]=va_arg(args,int);
             report[5]=va_arg(args,int);
             report[6]=va_arg(args,unsigned int);
             report[7]=va_arg(args,long);
             report[8]=va_arg(args,unsigned long);
             const int *pointer=va_arg(args,const int *);
             callback operation=va_arg(args,callback);
             void *null=va_arg(args,void *);
             report[9]=*pointer;
             report[10]=operation(39);
             report[11]=null==0;
             report[12]=va_arg(copy,int);
             va_end(args); va_end(copy);
             return 77;
         }
         int main(void) {
             _Bool flag=1;
             char plain=255;
             signed char byte=-7;
             unsigned char octet=254;
             short small=-1234;
             unsigned short half=65535;
             const int object=37;
             return inspect(0,flag,plain,byte,octet,small,half,0xfedcba98U,
                 -0x123456788765432L,0xfedcba9876543210UL,&object,plus,(void *)0);
         }",
    );
    assert_eq!(machine.core().register(10), Some(77));
    let mut bytes = [0; 104];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    let expected = [
        1u64,
        255,
        (-7i64) as u64,
        254,
        (-1234i64) as u64,
        65535,
        0xfedcba98,
        (-0x123456788765432i64) as u64,
        0xfedcba9876543210,
        37,
        42,
        1,
        1,
    ]
    .iter()
    .flat_map(|value| value.to_le_bytes())
    .collect::<Vec<_>>();
    assert_eq!(bytes.as_slice(), expected);
}

#[test]
fn scalar_varargs_recursive_indirect_dispatch_and_argument_side_effects_execute_once() {
    let (machine,output)=execute(
        "#include <stdarg.h>
         typedef long (*collector)(int,...);
         int calls;
         long next(void) { calls++; return calls; }
         long consume(int count,va_list args) {
             long result=0;
             while(count--) result+=va_arg(args,long);
             return result;
         }
         long recursive(int depth,...) {
             va_list args,copy;
             va_start(args,depth);
             long first=va_arg(args,long);
             va_copy(copy,args);
             long rest=consume(9,copy);
             if (depth) rest+=recursive(depth-1,first,1L,2L,3L,4L,5L,6L,7L,8L,9L);
             va_end(args); va_end(copy);
             return first+rest;
         }
         collector table[2]={recursive,0};
         int targets;
         collector choose(void) { targets++; return table[0]; }
         int main(void) {
             long result=choose()(2,next(),next(),next(),next(),next(),next(),next(),next(),next(),next());
             if(calls!=10 || targets!=1 || result!=147) return 1;
             return 77;
         }");
    assert_eq!(machine.core().register(10), Some(77));
    assert!(output.is_empty());
}

#[test]
fn scalar_varargs_list_operands_evaluate_once_and_restart_uses_original_arguments() {
    let (machine, output) = execute(
        "#include <stdarg.h>
         va_list cursors[3];
         int addresses;
         va_list *where(void) { addresses++; return cursors+1; }
         int inspect(int last,...) {
             int i=0;
             va_start(cursors[i++],last);
             va_copy(*where(),cursors[0]);
             long first=va_arg(*where(),long);
             long again=va_arg(cursors[0],long);
             va_end(*where());
             va_start(cursors[2],last);
             long restart=va_arg(cursors[2],long);
             if(i!=1 || addresses!=3 || first!=17 || again!=17 || restart!=17) return 1;
             last=999;
             va_start(cursors[0],last);
             if(va_arg(cursors[0],long)!=17) return 2;
             return 77;
         }
         int main(void) { return inspect(0,17L); }",
    );
    assert_eq!(machine.core().register(10), Some(77));
    assert!(output.is_empty());
}

#[test]
fn scalar_varargs_independent_caller_checks_save_area_stack_slots_and_preserved_registers() {
    let source = "#include <stdarg.h>
         unsigned long inspect(unsigned long last,...) {
             va_list args;
             va_start(args,last);
             unsigned long total=last;
             for(int i=0;i<10;i++) total+=va_arg(args,unsigned long);
             va_end(args);
             return total;
         }
         int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "varargs-caller.c")
        .unwrap()
        .replacen("call main\n", "call .Lva_entry\n", 1);
    assembly.push_str(
        "\n.Lva_entry:
         addi sp, sp, -48
         sd ra, 40(sp)
         li s0, 88
         li s1, 99
         li a0, 1
         li a1, 2
         li a2, 3
         li a3, 4
         li a4, 5
         li a5, 6
         li a6, 7
         li a7, 8
         li t0, 9
         sd t0, 0(sp)
         li t0, 10
         sd t0, 8(sp)
         li t0, 11
         sd t0, 16(sp)
         li t0, 123
         sd t0, 24(sp)
         call inspect
         ld t0, 24(sp)
         ld ra, 40(sp)
         addi sp, sp, 48
         ret\n",
    );
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    for (register, value) in [(2, 0x80010000), (8, 88), (9, 99), (10, 66), (5, 123)] {
        assert_eq!(machine.core().register(register), Some(value));
    }
}

#[test]
fn scalar_varargs_independent_callee_checks_default_promotions_and_outgoing_stack() {
    let source = "long foreign(int last,...) { return 0; }
         int main(void) {
             signed char byte=-7;
             unsigned short half=65535;
             _Bool flag=1;
             return foreign(1,byte,half,flag,0xfedcba98U,5L,6UL,7L,8UL,9L);
         }";
    let mut assembly = compile_to_assembly(source, "varargs-callee.c")
        .unwrap()
        .replacen("\nforeign:\n", "\n.Lunused_va_foreign:\n", 1);
    assembly.push_str(
        "\nforeign:
         andi t0, sp, 15
         bne t0, zero, .Lbad_va
         li t0, 1
         bne a0, t0, .Lbad_va
         li t0, -7
         bne a1, t0, .Lbad_va
         li t0, 65535
         bne a2, t0, .Lbad_va
         li t0, 1
         bne a3, t0, .Lbad_va
         li t0, -19088744
         bne a4, t0, .Lbad_va
         li t0, 5
         bne a5, t0, .Lbad_va
         li t0, 6
         bne a6, t0, .Lbad_va
         li t0, 7
         bne a7, t0, .Lbad_va
         ld t0, 0(sp)
         li t1, 8
         bne t0, t1, .Lbad_va
         ld t0, 8(sp)
         li t1, 9
         bne t0, t1, .Lbad_va
         li a0, 77
         ret
         .Lbad_va:
         li a0, -1
         ret\n",
    );
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(77));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn scalar_varargs_cursor_matches_independent_register_save_and_incoming_stack_layout() {
    for named in [1usize, 7, 8, 9, 10, 30] {
        let parameters = (0..named)
            .map(|i| format!("long p{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let source = format!(
            "#include <stdarg.h>
             long foreign(va_list list) {{ return 0; }}
             long collect({parameters},...) {{
                 va_list list;
                 va_start(list,p{});
                 return foreign(list);
             }}
             int main(void) {{ return 0; }}",
            named - 1
        );
        let mut assembly = compile_to_assembly(&source, "va-layout.c")
            .unwrap()
            .replacen("call main\n", "call .Lva_layout_entry\n", 1)
            .replacen("\nforeign:\n", "\n.Lunused_va_layout_foreign:\n", 1);
        let extra = 2;
        let outgoing = ((named + extra).saturating_sub(8) * 8).next_multiple_of(16);
        let allocation = outgoing + 16;
        assembly.push_str(&format!(
            "\n.Lva_layout_entry:\naddi sp, sp, -{allocation}\nsd ra, {outgoing}(sp)\nmv s1, sp\n"
        ));
        for index in 0..named + extra {
            if index < 8 {
                assembly.push_str(&format!("li a{index}, {}\n", index + 1));
            } else {
                assembly.push_str(&format!(
                    "li t0, {}\nsd t0, {}(sp)\n",
                    index + 1,
                    (index - 8) * 8
                ));
            }
        }
        assembly.push_str(&format!(
            "call collect\nld ra, {outgoing}(sp)\naddi sp, sp, {allocation}\nret
             foreign:
             addi t0, s1, {}
             bne a0, t0, .Lbad_va_layout
             ld t0, 0(a0)
             li t1, {}
             bne t0, t1, .Lbad_va_layout
             ld t0, 8(a0)
             li t1, {}
             bne t0, t1, .Lbad_va_layout
             li a0, 77
             ret
             .Lbad_va_layout:
             li a0, -1
             ret\n",
            (named as isize - 8) * 8,
            named + 1,
            named + 2
        ));
        let mut machine =
            Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000)
                .unwrap();
        let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0, "named={named}: {:?}", report.last_trap);
        assert!(report.waiting > 0);
        assert_eq!(machine.core().register(10), Some(77), "named={named}");
        assert_eq!(machine.core().register(2), Some(0x80010000));
        assert_eq!(machine.core().register(8), Some(0));
    }
}

#[test]
fn scalar_varargs_skipped_and_sizeof_operands_do_not_advance_the_cursor() {
    let (machine, output) = execute(
        "#include <stdarg.h>
         int collect(int last,...) {
             va_list args;
             va_start(args,last);
             if(sizeof(va_arg(args,long))!=8) return 1;
             if(0 && va_arg(args,long)) return 2;
             long a=1 ? va_arg(args,long) : va_arg(args,long);
             if(a!=17 || va_arg(args,long)!=25) return 3;
             va_end(args);
             return a+25;
         }
         int main(void) { return collect(0,17L,25L); }",
    );
    assert_eq!(machine.core().register(10), Some(42));
    assert!(output.is_empty());
}

#[test]
fn scalar_varargs_pointer_to_pointer_and_callback_object_arguments_remain_typed_rvalues() {
    let (machine, output) = execute(
        "#include <stdarg.h>
         typedef long (*callback)(long);
         long plus(long n) { return n+3; }
         int collect(int last,...) {
             va_list args;
             va_start(args,last);
             int **object=va_arg(args,int **);
             const unsigned char *text=va_arg(args,const unsigned char *);
             callback *operation=va_arg(args,callback *);
             va_end(args);
             return **object+*text+(*operation)(4);
         }
         int main(void) {
             int object=17;
             int *pointer=&object;
             const unsigned char bytes[2]={18,0};
             callback operation=plus;
             return collect(0,&pointer,bytes,&operation);
         }",
    );
    assert_eq!(machine.core().register(10), Some(42));
    assert!(output.is_empty());
}

#[test]
fn scalar_varargs_invalid_builtin_types_arguments_and_declarations_preserve_rom() {
    for body in [
        "va_start(args,first);",
        "va_start(args,last+0);",
        "va_start(args,0);",
        "{ int last=1; va_start(args,last); }",
        "va_start((va_list)args,last);",
        "va_start(0,last);",
        "int not_list=0; va_start(not_list,last);",
        "va_arg(args,char);",
        "va_arg(args,signed char);",
        "va_arg(args,unsigned char);",
        "va_arg(args,_Bool);",
        "va_arg(args,short);",
        "va_arg(args,unsigned short);",
        "va_arg(args,void);",
        "va_arg(args,double);",
        "va_arg(args,float);",
        "va_arg(args,long long);",
        "va_arg(args,missing);",
        "va_arg((va_list)args,long);",
        "va_arg(0,long);",
        "va_arg(args,int)=1;",
        "++va_arg(args,int);",
        "&va_arg(args,long);",
        "va_copy(args,1);",
        "va_copy(0,args);",
        "va_end(0);",
        "__builtin_va_start(args);",
        "__builtin_va_start(args,last,1);",
        "__builtin_va_copy(args);",
        "__builtin_va_end(args,args);",
        "int __builtin_va_start=1;",
        "if(0) va_arg(args,char);",
    ] {
        let source = format!(
            "#include <stdarg.h>\nint inspect(int first,int last,...) {{ va_list args; {body} return 0; }} int main(void) {{ return 0; }}"
        );
        let diagnostic = compile_to_assembly(&source, "varargs-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("varargs-invalid.c:"),
            "{body}: {diagnostic}"
        );
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let result = files.compile();
        assert!(!result.status.success(), "{body}");
        assert!(!result.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    for source in [
        "#include <stdarg.h>\nint f(int last) { va_list args; va_start(args,last); return 0; } int main(void) { return 0; }",
        "int f(...) { return 0; } int main(void) { return 0; }",
        "int f(int x,...) { return x; } int f(int); int main(void) { return 0; }",
        "typedef int (*fn)(int); int f(int x,...) { return x; } fn object=f; int main(void) { return 0; }",
        "typedef int (*fn)(int,...); int f(int x) { return x; } fn object=f; int main(void) { return 0; }",
        "int f(int a,int b,...) { return a+b; } int main(void) { return f(1); }",
        "int f(int a,...) { return a; } void nothing(void) {} int main(void) { return f(1,nothing()); }",
        "int __builtin_va_arg; int main(void) { return 0; }",
        "typedef int __builtin_va_end; int main(void) { return 0; }",
        "enum { __builtin_va_copy=1 }; int main(void) { return 0; }",
        "int main(int x,...) { return x; }",
    ] {
        assert!(
            compile_to_assembly(source, "varargs-invalid.c").is_err(),
            "{source}"
        );
    }
    let extra = (0..33).map(|_| "1").collect::<Vec<_>>().join(",");
    let source =
        format!("int f(int n,...) {{ return n; }} int main(void) {{ return f({extra}); }}");
    assert!(compile_to_assembly(&source, "too-many-varargs.c").is_err());
}

#[test]
fn noreturn_panic_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_noreturn.c");
    let assembly = compile_to_assembly(source, "panic.c").unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(output, b"P\x2a\n");
    let stack = machine.core().register(2).unwrap();
    assert!(stack < 0x80010000);
    assert_eq!(stack & 15, 0);
    assert!(machine.core().register(8).unwrap() > stack);
    let mut bytes = [0; 16];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(u64::from_le_bytes(bytes[..8].try_into().unwrap()), 7);
    assert_eq!(u64::from_le_bytes(bytes[8..].try_into().unwrap()), 42);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn noreturn_function_attributes_merge_across_declarations_without_changing_call_types() {
    for (before, definition, after) in [
        (
            "void stop(int);",
            "__attribute__((noreturn)) void stop(int code)",
            "void stop(int);",
        ),
        (
            "void stop(int) __attribute__((noreturn));",
            "void stop(int code)",
            "",
        ),
        ("", "void stop(int code) __attribute__((__noreturn__))", ""),
        (
            "",
            "void stop(int code)",
            "__attribute__((noreturn)) void stop(int);",
        ),
        (
            "static void stop(int) __attribute__((noreturn));",
            "static void stop(int code)",
            "extern void stop(int);",
        ),
        (
            "",
            "void __attribute((noreturn, __noreturn__)) stop(int code)",
            "",
        ),
        (
            "__attribute__((noreturn)) extern void stop(int);",
            "void stop(int code)",
            "extern void stop(int);",
        ),
        (
            "",
            "__attribute__((noreturn)) void __attribute__((noreturn)) stop(int code) __attribute__((noreturn))",
            "",
        ),
    ] {
        let source = format!(
            "{before}
             {definition} {{
                 *(volatile unsigned char *)0x10000000UL = code;
                 for (;;) asm volatile(\"wfi\");
             }}
             {after}
             typedef void (*exit_fn)(int);
             exit_fn handler=stop;
             int main(void) {{
                 if (0) stop(1);
                 handler(75);
                 *(volatile unsigned char *)0x10000000UL='X';
                 return 1;
             }}"
        );
        let assembly = compile_to_assembly(&source, "noreturn-decl.c").unwrap();
        let mut machine =
            Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000)
                .unwrap();
        let mut output = Vec::new();
        let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
        assert_eq!(
            report.traps, 0,
            "{before} {definition} {after}: {:?}",
            report.last_trap
        );
        assert!(report.waiting > 0);
        assert_eq!(output, b"K");
        assert_eq!(machine.core().register(2).unwrap() & 15, 0);
    }
}

#[test]
fn noreturn_recursive_stack_calls_preserve_exact_arguments_and_ordinary_function_behavior() {
    let source = "long sum9(long a,long b,long c,long d,long e,long f,long g,long h,long i) {
             return a+b+c+d+e+f+g+h+i;
         }
         void ordinary(volatile long *value) { *value=3; return; }
         __attribute__((noreturn)) long stop(int depth, signed char byte, unsigned int word,
             long d,long e,long f,long g,long h,unsigned long ninth) {
             long local=2;
             ordinary(&local);
             if (depth) stop(depth-1,byte,word,d,e,f,g,h,ninth);
             volatile unsigned long *report=(volatile unsigned long *)0x80000000UL;
             report[0]=byte; report[1]=word; report[2]=ninth;
             report[3]=sum9(local,2,3,4,5,6,7,8,9);
             report[4]=d+e+f+g+h;
             for (;;) asm volatile(\"wfi\" : : : \"memory\");
         }
         int main(void) { stop(2,-7,0xfedcba98U,4,5,6,7,8,0x1234567887654321UL); return 1; }";
    let assembly = compile_to_assembly(source, "noreturn-stack.c").unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert!(output.is_empty());
    let mut bytes = [0; 40];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    let expected = [(-7i64) as u64, 0xfedcba98, 0x1234567887654321, 47, 30]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    assert_eq!(bytes.as_slice(), expected);
    assert_eq!(machine.core().register(2).unwrap() & 15, 0);
}

#[test]
fn noreturn_fallthrough_traps_instead_of_returning_even_when_traps_resume() {
    use suomicpu16::kiuas16::CoreTrapCause;
    for (result, before, after, body, indirect) in [
        ("void", "", "", "", false),
        ("int", "", "", "", false),
        (
            "void",
            "void stop(void);",
            "",
            "if (0) for (;;) asm(\"wfi\");",
            true,
        ),
        (
            "void",
            "",
            "void stop(void) __attribute__((noreturn));",
            "",
            false,
        ),
        (
            "void",
            "",
            "",
            "goto end; for (;;) asm(\"wfi\"); end: ;",
            true,
        ),
        ("void", "", "", "ordinary();", false),
    ] {
        let source = format!(
            "void ordinary(void) {{ return; }}
             {before}
             {result} stop(void) {} {{
                 *(volatile unsigned char *)0x10000000UL = 'N';
                 {body}
             }}
             {after}
             typedef {result} (*exit_fn)(void);
             int main(void) {{
                 asm volatile(\"la t0, .Lresume_noreturn\\ncsrw mtvec, t0\");
                 exit_fn handler=stop;
                 {};
                 *(volatile unsigned char *)0x10000000UL = 'X';
                 return 99;
             }}",
            if after.is_empty() {
                "__attribute__((noreturn))"
            } else {
                ""
            },
            if indirect { "handler()" } else { "stop()" },
        );
        let mut assembly = compile_to_assembly(&source, "noreturn-fallthrough.c").unwrap();
        assembly.push_str(
            "\n.Lresume_noreturn:
             csrr t0, mepc
             addi t0, t0, 4
             csrw mepc, t0
             mret\n",
        );
        let rom = assemble_boot_rom(&assembly).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom.clone(), 0x20000).unwrap();
        let mut output = Vec::new();
        let report = run_headless(&mut machine, 1000, &[], &mut output).unwrap();
        assert!(report.traps > 2, "{source}");
        assert_eq!(report.waiting, 0);
        let trap = report.last_trap.unwrap();
        assert_eq!(trap.cause, CoreTrapCause::Breakpoint);
        assert_eq!(
            &rom[(trap.pc - 0x1000) as usize..(trap.pc - 0x1000) as usize + 4],
            &0x00100073u32.to_le_bytes()
        );
        assert_eq!(output, b"N");
        assert!(machine.core().register(2).unwrap() < 0x80010000);
        assert_eq!(machine.core().register(2).unwrap() & 15, 0);
    }
    let source = "void stop(void) __attribute__((noreturn)) {}
         int main(void) { stop(); return 99; }";
    let files = Files::new(source);
    assert!(files.compile().status.success());
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "1000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(!run.status.success());
    let diagnostic = String::from_utf8_lossy(&run.stderr);
    assert!(diagnostic.contains("synchronous trap(s)"), "{diagnostic}");
}

#[test]
fn noreturn_invalid_returns_placements_and_arguments_preserve_existing_rom() {
    for source in [
        "void stop(void) __attribute__((noreturn)) { return; } int main(void) { return 0; }",
        "int stop(void) __attribute__((noreturn)) { return 3; } int main(void) { return 0; }",
        "void stop(void) { return; } void stop(void) __attribute__((noreturn)); int main(void) { return 0; }",
        "void stop(void) __attribute__((noreturn)) { if (0) return; for (;;) ; } int main(void) { return 0; }",
        "void stop(void) __attribute__((noreturn)) { while (0) { return; } } int main(void) { return 0; }",
        "void stop(void) __attribute__((noreturn)) { do { return; } while (0); } int main(void) { return 0; }",
        "void stop(void) __attribute__((noreturn)) { for (;;) return; } int main(void) { return 0; }",
        "void stop(void) __attribute__((noreturn)) { switch (0) { case 0: return; } } int main(void) { return 0; }",
        "void stop(void) __attribute__((noreturn)) { end: return; } int main(void) { return 0; }",
        "int main(void) __attribute__((noreturn)) { for (;;) ; }",
        "__attribute__((noreturn)) int object; int main(void) { return 0; }",
        "int object __attribute__((noreturn)); int main(void) { return 0; }",
        "typedef int (*fn)(void) __attribute__((noreturn)); int main(void) { return 0; }",
        "typedef int (*fn)(void); fn object __attribute__((noreturn)); int main(void) { return 0; }",
        "__attribute__((noreturn)) typedef int value; int main(void) { return 0; }",
        "__attribute__((noreturn)) int; int main(void) { return 0; }",
        "void stop(int __attribute__((noreturn)) value) { for (;;) ; } int main(void) { return 0; }",
        "void stop(void) __attribute__((noreturn())); int main(void) { return 0; }",
        "void stop(void) __attribute__((noreturn(1))); int main(void) { return 0; }",
        "void stop(void) __attribute__((noreturn, aligned(16))) { for (;;) ; } int main(void) { return 0; }",
        "void stop(void) __attribute__((noreturn, format(printf,1,2))); int main(void) { return 0; }",
        "void stop(void) __attribute__((noreturn)); int main(void) { return 0; }",
        "void stop(void) __attribute__((noreturn)) { for (;;) ; } int stop(void); int main(void) { return 0; }",
        "int main(void) { int value __attribute__((noreturn)); return 0; }",
    ] {
        let diagnostic = compile_to_assembly(source, "noreturn-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("noreturn-invalid.c:"),
            "{source}: {diagnostic}"
        );
        let files = Files::new(source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let result = files.compile();
        assert!(!result.status.success(), "{source}");
        assert!(!result.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    let source = "void stop(void) __attribute__((noreturn)) {\n#line 207 \"panic-header.h\"\nreturn;\n}\nint main(void) { return 0; }";
    let diagnostic = compile_to_assembly(source, "noreturn-source.c")
        .unwrap_err()
        .to_string();
    assert!(diagnostic.contains("panic-header.h:207:"), "{diagnostic}");
    assert!(
        diagnostic.contains("noreturn function cannot contain a return"),
        "{diagnostic}"
    );
    let source =
        "void stop(void) __attribute__((noreturn)) { for (;;) ; } int main(void) { return 0; }";
    assert!(
        suomicpu16::c_compiler::compile_minimal_program_to_assembly(source, "legacy-noreturn.c")
            .is_err()
    );
}

#[test]
fn aligned_global_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_aligned.c");
    let assembly = compile_to_assembly(source, "aligned.c").unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 8208]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 100000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(42));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    assert_eq!(output, b"A\x07\n");
    let mut memory = [0; 8208];
    machine.bus_mut().read(0x80010000, &mut memory).unwrap();
    let mut expected = [0; 8208];
    expected[0] = 0x5a;
    expected[64] = 3;
    expected[79] = 7;
    expected[80..88].copy_from_slice(&0x80010040u64.to_le_bytes());
    expected[88..96].copy_from_slice(&0x80011000u64.to_le_bytes());
    expected[4096..4104].copy_from_slice(&0x80010040u64.to_le_bytes());
    expected[8184..8192].copy_from_slice(&0x0123456789abcdefu64.to_le_bytes());
    expected[8192..].fill(0xa5);
    assert_eq!(memory, expected);
    let mut report_bytes = [0; 32];
    machine
        .bus_mut()
        .read(0x80000000, &mut report_bytes)
        .unwrap();
    let expected_report = [0x80010040u64, 0x80011000, 0x80010040, 0x0123456789abcdef]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    assert_eq!(report_bytes.as_slice(), expected_report);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "100000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn aligned_globals_merge_redeclarations_and_keep_attributes_per_declarator() {
    let (mut machine, _) = execute(
        "#define A(n) __attribute__((aligned(n)))
         typedef unsigned long word;
         enum { CACHE = 64 };
         extern word b A(CACHE);
         extern word b;
         extern word z A(128);
         __attribute__((__aligned__(16), aligned(32))) word a = 1, b = 2;
         word c A(64) = 3, d = 4;
         static A(32) word e = 5;
         word __attribute((aligned(sizeof(long) * 8))) f = 6;
         word g A(16) __attribute__((aligned(64))) = 7;
         word z;
         extern word z A(256);
         extern word b A(16);
         int main(void) {
             unsigned long *report = (unsigned long *)0x80000000UL;
             report[0]=(unsigned long)&a; report[1]=(unsigned long)&b;
             report[2]=(unsigned long)&c; report[3]=(unsigned long)&d;
             report[4]=(unsigned long)&e; report[5]=(unsigned long)&f;
             report[6]=(unsigned long)&g; report[7]=(unsigned long)&z;
             if (a+b+c+d+e+f+g != 28 || z) return 1;
             b += 10;
             return b;
         }",
    );
    assert_eq!(machine.core().register(10), Some(12));
    let mut bytes = [0; 64];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    for (index, offset) in [0u64, 64, 128, 136, 160, 192, 256, 512].iter().enumerate() {
        assert_eq!(
            u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap()),
            0x80010000 + offset,
            "object {index}"
        );
    }
}

#[test]
fn aligned_globals_preserve_symbolic_callbacks_and_static_pointer_offsets() {
    let (mut machine, _) = execute_with_budget(
        "typedef int (*op)(int);
         int twice(int n) { return n*2; }
         int plus(int n) { return n+3; }
         unsigned char a = 7;
         op callbacks[3] __attribute__((aligned(256))) = {twice, plus, 0};
         unsigned short data[4] __attribute__((aligned(64))) = {1,2,3,4};
         unsigned short *view __attribute__((aligned(128))) = data+2;
         op *dispatch = callbacks+1;
         int main(void) {
             unsigned long *report = (unsigned long *)0x80000000UL;
             report[0]=(unsigned long)callbacks; report[1]=(unsigned long)data;
             report[2]=(unsigned long)view; report[3]=(unsigned long)dispatch;
             if (a != 7 || view[0] != 3 || dispatch[1] != 0) return 1;
             return callbacks[0](view[1]) + (*dispatch)(view[0]);
         }",
        100000,
    );
    assert_eq!(machine.core().register(10), Some(14));
    let mut bytes = [0; 32];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    for (index, address) in [0x80010100u64, 0x80010140, 0x80010144, 0x80010108]
        .iter()
        .enumerate()
    {
        assert_eq!(
            u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap()),
            *address
        );
    }
    let mut callbacks = [0; 24];
    machine.bus_mut().read(0x80010100, &mut callbacks).unwrap();
    for entry in callbacks[..16].chunks_exact(8) {
        let address = u64::from_le_bytes(entry.try_into().unwrap());
        assert!((0x1000..0x10000).contains(&address));
        assert_eq!(address & 3, 0);
    }
    assert_ne!(&callbacks[..8], &callbacks[8..16]);
    assert_eq!(&callbacks[16..], &[0; 8]);
}

#[test]
fn aligned_globals_cover_power_of_two_limits_and_full_ram_budget() {
    for alignment in (0..=15).map(|shift| 1usize << shift) {
        let source = format!(
            "unsigned char a = 7;
             unsigned char b __attribute__((aligned({alignment})));
             int main(void) {{
                 unsigned long address = (unsigned long)&b;
                 *(unsigned long *)0x80000000UL = address;
                 return b + a;
             }}"
        );
        let assembly = compile_to_assembly(&source, "alignment-limit.c").unwrap();
        let mut machine =
            Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000)
                .unwrap();
        machine.bus_mut().write(0x80010000, &[0xa5; 65536]).unwrap();
        let report = run_headless(&mut machine, 300000, &[], &mut Vec::new()).unwrap();
        assert_eq!(
            report.traps, 0,
            "alignment {alignment}: {:?}",
            report.last_trap
        );
        assert!(report.waiting > 0, "alignment {alignment}");
        assert_eq!(
            machine.core().register(10),
            Some(7),
            "alignment {alignment}"
        );
        let mut bytes = [0; 8];
        machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
        let offset = 8usize.next_multiple_of(alignment.max(8));
        assert_eq!(u64::from_le_bytes(bytes), 0x80010000 + offset as u64);
        let mut after = [0; 8];
        machine
            .bus_mut()
            .read(0x80010000 + offset as u64 + 8, &mut after)
            .unwrap();
        assert_eq!(after, [0xa5; 8]);
        assert_eq!(machine.core().register(2), Some(0x80010000));
        assert_eq!(machine.core().register(8), Some(0));
    }
    let assembly = compile_to_assembly(
        "unsigned char all[65536] __attribute__((aligned(65536)));
         int main(void) {
             if ((unsigned long)all != 0x80010000UL || all[0] || all[65535]) return 1;
             all[0]=21; all[65535]=42;
             return all[0]+all[65535];
         }",
        "aligned-full.c",
    )
    .unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x21000).unwrap();
    machine
        .bus_mut()
        .write(0x80010000, &vec![0xa5; 65544])
        .unwrap();
    let report = run_headless(&mut machine, 300000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(63));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut bytes = vec![0; 65544];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    assert_eq!(bytes[0], 21);
    assert!(bytes[1..65535].iter().all(|byte| *byte == 0));
    assert_eq!(bytes[65535], 42);
    assert_eq!(&bytes[65536..], &[0xa5; 8]);
}

#[test]
fn aligned_globals_copy_initialized_padding_and_clear_bss_without_touching_following_ram() {
    for alignment in [1usize, 2, 4, 8, 16, 32, 64, 256, 4096, 32768] {
        let source = format!(
            "unsigned char a=1;
             unsigned char b __attribute__((aligned({alignment})))=2;
             unsigned char c;
             int main(void) {{
                 *(unsigned long *)0x80000000UL = (unsigned long)&b;
                 return a+b+c;
             }}"
        );
        let assembly = compile_to_assembly(&source, "aligned-data.c").unwrap();
        let mut machine =
            Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000)
                .unwrap();
        machine.bus_mut().write(0x80010000, &[0xa5; 65536]).unwrap();
        let report = run_headless(&mut machine, 400000, &[], &mut Vec::new()).unwrap();
        assert_eq!(
            report.traps, 0,
            "alignment {alignment}: {:?}",
            report.last_trap
        );
        assert!(report.waiting > 0);
        assert_eq!(machine.core().register(10), Some(3));
        let offset = 8usize.next_multiple_of(alignment.max(8));
        let end = offset + 16;
        let mut bytes = vec![0; end + 16];
        machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
        let mut expected = vec![0; end + 16];
        expected[0] = 1;
        expected[offset] = 2;
        expected[end..].fill(0xa5);
        assert_eq!(bytes, expected, "alignment {alignment}");
        let mut address = [0; 8];
        machine.bus_mut().read(0x80000000, &mut address).unwrap();
        assert_eq!(u64::from_le_bytes(address), 0x80010000 + offset as u64);
        assert_eq!(machine.core().register(2), Some(0x80010000));
        assert_eq!(machine.core().register(8), Some(0));
    }
}

#[test]
fn aligned_globals_reject_unsupported_attributes_placements_and_capacity_without_overwriting_rom() {
    for declaration in [
        "int object __attribute__((aligned));",
        "int object __attribute__((aligned()));",
        "int object __attribute__((aligned(0)));",
        "int object __attribute__((aligned(-8)));",
        "int object __attribute__((aligned(3)));",
        "int object __attribute__((aligned(131072)));",
        "int object __attribute__((aligned(1UL << 63)));",
        "int object __attribute__((aligned(1 << 32)));",
        "int object __attribute__((aligned(1 / 0)));",
        "int size; int object __attribute__((aligned(size)));",
        "int side(void) { return 16; } int object __attribute__((aligned(1 ? 16 : side())));",
        "int object __attribute__((aligned(8), packed));",
        "int object __attribute__((section(\".data\")));",
        "int object __attribute__((unknown));",
        "__attribute__((aligned(1))) typedef int object;",
        "typedef int object __attribute__((aligned(16)));",
        "__attribute__((aligned(1))) int function(void) { return 0; }",
        "int function(void) __attribute__((aligned(16)));",
        "int function(__attribute__((aligned(1))) int value) { return value; }",
        "int function(int value __attribute__((aligned(16)))) { return value; }",
        "__attribute__((aligned(1))) int;",
        "int a = 1; int b __attribute__((aligned(65536)));",
        "int a; int b[8193] __attribute__((aligned(32768)));",
        "unsigned char a=1; unsigned char b[32769] __attribute__((aligned(32768))) = {2};",
        "extern int object __attribute__((aligned(16)));",
        "int object __attribute__((aligned(16))); long object;",
    ] {
        let source = format!("{declaration}\nint main(void) {{ return 0; }}");
        let diagnostic = compile_to_assembly(&source, "aligned-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("aligned-invalid.c:"),
            "{declaration}: {diagnostic}"
        );
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{declaration}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    for source in [
        "int main(void) { int value __attribute__((aligned(16))); return 0; }",
        "int main(void) { __attribute__((aligned(16))) int value; return 0; }",
        "int main(void) { int __attribute__((aligned(16))) value; return 0; }",
    ] {
        assert!(
            compile_to_assembly(source, "aligned-local.c").is_err(),
            "{source}"
        );
    }
    let source = "int global __attribute__((aligned(16))); int main(void) { return 0; }";
    assert!(
        suomicpu16::c_compiler::compile_minimal_program_to_assembly(source, "legacy-aligned.c")
            .is_err()
    );
}

#[test]
fn aligned_globals_rom_padding_overflow_preserves_existing_output_and_attribute_locations() {
    let source = "unsigned char a=1;
         unsigned char b[30000] __attribute__((aligned(32768))) = {2};
         int main(void) { return b[0]+a; }";
    let assembly = compile_to_assembly(source, "aligned-rom.c").unwrap();
    assert!(
        assemble_boot_rom(&assembly)
            .unwrap_err()
            .to_string()
            .contains("exceeds")
    );
    let files = Files::new(source);
    std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
    let build = files.compile();
    assert!(!build.status.success());
    assert!(!build.stderr.is_empty());
    assert_eq!(
        std::fs::read(files.0.join("rom.bin")).unwrap(),
        b"previous ROM"
    );
    for (attribute, message) in [
        ("aligned(3)", "power of two"),
        ("packed", "unsupported RV64 attribute"),
    ] {
        let source = format!(
            "#line 123 \"alignment-header.h\"\nint object __attribute__(({attribute}));\nint main(void) {{ return 0; }}"
        );
        let diagnostic = compile_to_assembly(&source, "aligned-locations.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("alignment-header.h:123:"),
            "{diagnostic}"
        );
        assert!(diagnostic.contains(message), "{diagnostic}");
    }
}

#[test]
fn inline_assembly_csr_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_csr.c");
    let assembly = compile_to_assembly(source, "csr.c").unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(output, b"C\x19\n");
    assert_eq!(machine.core().register(10), Some(25));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut bytes = [0; 24];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    let expected = [0xfedcba9876543210u64, 0xfedcba9876543210, 25]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    assert_eq!(bytes.as_slice(), expected);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn inline_assembly_stages_side_effects_nested_calls_and_recursive_operands_once() {
    let (machine, _) = execute(
        "long data[2] = {11,22};
         int addresses; int sums;
         long *location(void) { addresses++; return data + 1; }
         long sum9(long a,long b,long c,long d,long e,long f,long g,long h,long i) {
             sums++; return a+b+c+d+e+f+g+h+i;
         }
         long recursive(int n) {
             long result = n;
             if (n) asm(\"add %0, %0, %1\" : \"+r\"(result) : \"r\"(recursive(n-1)));
             return result;
         }
         int main(void) {
             int index = 0;
             asm volatile(\"add %[first], %[n], %[m]\\n\\tadd %[second], %[second], %[k]\\nadd %[first], %[first], %[last]\"
                 : [first] \"=&r\"(data[index++]), [second] \"+&r\"(*location())
                 : [n] \"r\"(sum9(1,2,3,4,5,6,7,8,9)),
                   [m] \"r\"(sum9(1,1,1,1,1,1,1,1,1)),
                   [k] \"r\"(recursive(4)), [last] \"r\"(7) : \"memory\");
             if (data[0] != 61 || data[1] != 32 || addresses != 1 || index != 1 || sums != 2) return 1;
             return 77;
         }",
    );
    assert_eq!(machine.core().register(10), Some(77));
}

#[test]
fn inline_assembly_outputs_normalize_all_integer_types_without_overwriting_neighbors() {
    for (ty, size, expected) in [
        ("_Bool", 1, 1u64),
        ("char", 1, 255),
        ("signed char", 1, u64::MAX),
        ("unsigned char", 1, 255),
        ("short", 2, u64::MAX),
        ("unsigned short", 2, 65535),
        ("int", 4, u64::MAX),
        ("unsigned int", 4, u32::MAX as u64),
        ("long", 8, u64::MAX),
        ("unsigned long", 8, u64::MAX),
    ] {
        let source = format!(
            "{ty} global;
             int main(void) {{
                 {ty} local = 0;
                 volatile {ty} *target = (volatile {ty} *)0x80000008UL;
                 asm volatile(\"addi %0, zero, -1\\naddi %1, zero, -1\\naddi %2, zero, -1\"
                      : \"=r\"(local), \"=r\"(global), \"=r\"(*target) : : \"memory\");
                 if ((unsigned long)local != {expected}UL || (unsigned long)global != {expected}UL
                     || (unsigned long)*target != {expected}UL) return 1;
                 *target = 0;
                 asm(\"addi %0, zero, -1\" : \"+r\"(*target));
                 if ((unsigned long)*target != {expected}UL) return 2;
                 return 77;
             }}"
        );
        let assembly = compile_to_assembly(&source, "asm-width.c").unwrap();
        let mut machine =
            Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000)
                .unwrap();
        machine.bus_mut().write(0x80000000, &[0xa5; 32]).unwrap();
        let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0, "{ty}: {:?}", report.last_trap);
        assert!(report.waiting > 0, "{ty}");
        assert_eq!(machine.core().register(10), Some(77), "{ty}");
        assert_eq!(machine.core().register(2), Some(0x80010000));
        assert_eq!(machine.core().register(8), Some(0));
        let mut bytes = [0; 32];
        machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
        assert_eq!(&bytes[..8], &[0xa5; 8], "{ty}");
        assert_eq!(&bytes[8..8 + size], &expected.to_le_bytes()[..size], "{ty}");
        assert!(
            bytes[8 + size..].iter().all(|byte| *byte == 0xa5),
            "{ty}: {bytes:?}"
        );
    }
}

#[test]
fn inline_assembly_accepts_constant_expressions_pointer_values_and_zero_operand_forms() {
    let (machine, _) = execute(
        "#define READ \"csrr %0, mscratch\"
         #define MASK (1U << 4)
         typedef long (*callback)(long);
         long plus(long n) { return n+3; }
         int main(void) {
             long value = 39;
             long *pointer = 0;
             callback operation = 0;
             unsigned long scratch;
             asm(\"mv %0, %1\" : \"=r\"(pointer) : \"r\"(&value));
             asm(\"mv %0, %1\" : \"=r\"(operation) : \"r\"(plus));
             __asm volatile(\"csrwi mscratch, %[value]\" : : [value] \"i\"(MASK | 3));
             __asm__ __volatile(READ : \"=r\"(scratch));
             asm(\"addi %0, %0, %1\" : \"+r\"(scratch) : \"i\"(-1));
             asm(\"fence\");
             asm volatile(\"\" : : : \"memory\");
             asm(\"\");
             if (scratch != 18 || *pointer != 39 || operation(*pointer) != 42) return 1;
             return operation(*pointer);
         }",
    );
    assert_eq!(machine.core().register(10), Some(42));
}

#[test]
fn inline_assembly_memory_clobbers_preserve_volatile_load_store_order() {
    let (machine, output) = execute(
        "int main(void) {
             volatile unsigned char *uart = (volatile unsigned char *)0x10000000UL;
             unsigned char byte;
             asm volatile(\"lbu %0, 0(%1)\" : \"=r\"(byte) : \"r\"(uart) : \"memory\");
             asm volatile(\"sb %0, 0(%1)\" : : \"r\"(byte), \"r\"(uart) : \"memory\");
             asm volatile(\"lbu %0, 0(%1)\" : \"=r\"(byte) : \"r\"(uart) : \"memory\");
             return byte;
         }",
    );
    assert_eq!(output, b"\0");
    assert_eq!(machine.core().register(10), Some(0));
    let assembly = compile_to_assembly(
        "int main(void) {
             volatile unsigned char *uart = (volatile unsigned char *)0x10000000UL;
             unsigned char byte;
             asm volatile(\"lbu %0, 0(%1)\" : \"=r\"(byte) : \"r\"(uart) : \"memory\");
             *uart = byte;
             asm volatile(\"lbu %0, 0(%1)\" : \"=r\"(byte) : \"r\"(uart) : \"memory\");
             *uart = byte;
             return byte;
         }",
        "asm-uart.c",
    )
    .unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, b"QR", &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(output, b"QR");
    assert_eq!(machine.core().register(10), Some(b'R' as u64));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn inline_assembly_matches_an_independent_register_abi_callee() {
    let source = "long independent(long a,long b,long c,long d,long e,long f,long g,long h,long i) { return 0; }
         int main(void) {
             long result;
             asm(\"add %0, %1, %2\" : \"=r\"(result) : \"r\"(17L), \"r\"(25L));
             return independent(result,2,3,4,5,6,7,8,9);
         }";
    let mut assembly = compile_to_assembly(source, "asm-abi.c").unwrap().replacen(
        "\nindependent:\n",
        "\n.Lunused_independent:\n",
        1,
    );
    assembly.push_str(
        "\nindependent:
         andi t0, sp, 15
         bne t0, zero, bad_asm_abi
         addi t0, zero, 42
         bne a0, t0, bad_asm_abi
         ld t0, 0(sp)
         add a0, a0, a1
         add a0, a0, a2
         add a0, a0, a3
         add a0, a0, a4
         add a0, a0, a5
         add a0, a0, a6
         add a0, a0, a7
         add a0, a0, t0
         ret
         bad_asm_abi:
         addi a0, zero, -1
         ret\n",
    );
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(86));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn inline_assembly_six_outputs_preserve_saved_registers_and_template_percent_rules() {
    let source = "int main(void) {
             long a=1,b=2,c=3,d=4,e=5,f=6;
             asm(\"nop # basic % marker %%\");
             asm volatile(\"add %0, %0, %5\\nadd %1, %1, %5\\nadd %2, %2, %5\\nadd %3, %3, %5\\nadd %4, %4, %5\\naddi %5, %5, 6 # escaped %%\"
                 : \"+r\"(a), \"+r\"(b), \"+r\"(c), \"+r\"(d), \"+r\"(e), \"+r\"(f));
             return a+b+c+d+e+f;
         }";
    let mut assembly = compile_to_assembly(source, "asm-saved.c").unwrap();
    assert!(assembly.contains("nop # basic % marker %%\n"));
    assert!(assembly.contains("addi t6, t6, 6 # escaped %\n"));
    assembly = assembly.replacen("call main\n", "call .Lasm_saved_entry\n", 1);
    assembly.push_str("\n.Lasm_saved_entry:\naddi sp, sp, -16\nsd ra, 0(sp)\n");
    let registers = [8, 9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27];
    for register in registers {
        assembly.push_str(&format!("addi x{register}, zero, {}\n", 112 + register));
    }
    assembly.push_str("call main\nld ra, 0(sp)\naddi sp, sp, 16\nret\n");
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(57));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    for register in registers {
        assert_eq!(
            machine.core().register(register),
            Some(112 + register as u64),
            "x{register}"
        );
    }
}

#[test]
fn inline_assembly_invalid_operands_templates_and_constraints_preserve_existing_rom() {
    for body in [
        "asm(\"nop\" : \"r\"(value));",
        "asm(\"nop\" : \"=m\"(value));",
        "asm(\"nop\" : \"=r\"(1));",
        "asm(\"nop\" : \"=r\"(value+1));",
        "asm(\"nop\" : \"=r\"(fixed));",
        "asm(\"nop\" : \"=r\"(*readonly));",
        "asm(\"nop\" : \"=r\"((long)value));",
        "asm(\"nop\" : \"=r\"(array));",
        "asm(\"nop\" : : \"m\"(value));",
        "asm(\"nop\" : : \"ri\"(value));",
        "asm(\"nop\" : : \"0\"(value));",
        "asm(\"nop\" : : \"i\"(value));",
        "asm(\"nop\" : : \"i\"(1 ? 3 : side()));",
        "asm(\"nop\" : : \"i\"(1 / 0));",
        "asm(\"nop\" : : \"i\"(1 << 32));",
        "asm(\"nop\" : : \"i\"(&value));",
        "asm(\"nop\" : : \"r\"(nothing()));",
        "asm(\"nop\" : : : \"t0\");",
        "asm(\"nop\" : : : \"cc\");",
        "asm(\"mv %1, %0\" : \"=r\"(value));",
        "asm(\"mv %[missing], %0\" : \"=r\"(value));",
        "asm(\"mv %[broken, %0\" : \"=r\"(value));",
        "asm(\"mv %z0, %0\" : \"=r\"(value));",
        "asm(\"mv %999999999999999999999999999999, %0\" : \"=r\"(value));",
        "asm(\"%\" : \"=r\"(value));",
        "asm(\"nop\" : [same] \"=r\"(value) : [same] \"r\"(1));",
        "asm(\"nop\" : : \"r\"(1),\"r\"(2),\"r\"(3),\"r\"(4),\"r\"(5),\"r\"(6),\"r\"(7));",
        "asm goto(\"nop\" : : : : label); label: ;",
        "asm(value);",
        "asm(\"nop\\0\");",
        "asm(\"nop\" : \"=r\" value);",
        "if (0) asm(\"nop\" : : \"m\"(value));",
    ] {
        let source = format!(
            "int side(void) {{ return 1; }} void nothing(void) {{ }}
             int main(void) {{
                 int value = 2; const int fixed = 3; const int *readonly = &fixed; int array[2];
                 {body}
                 return 0;
             }}"
        );
        let diagnostic = compile_to_assembly(&source, "asm-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("asm-invalid.c:"),
            "{body}: {diagnostic}"
        );
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{body}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    for body in [
        "asm(\"csrwi mscratch, %0\" : : \"i\"(32));",
        "asm(\"csrr %0, missing\" : \"=r\"(value));",
        "asm(\"unsupported %0\" : : \"r\"(value));",
    ] {
        let source = format!("int main(void) {{ int value = 0; {body} return 0; }}");
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{body}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    assert!(
        suomicpu16::c_compiler::compile_minimal_program_to_assembly(
            "int main(void) { asm volatile(\"nop\" : : : \"memory\"); return 0; }",
            "legacy-asm.c"
        )
        .is_err()
    );
}

#[test]
fn callback_dispatch_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_dispatch.c");
    let assembly = compile_to_assembly(source, "dispatch.c").unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 128]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(output, b"DE\x03\n");
    assert_eq!(machine.core().register(10), Some(69));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut bytes = [0; 16];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(&bytes[..8], &[69, 0, 0, 0, 3, 0, 0, 0]);
    let mut table = [0; 32];
    machine
        .bus_mut()
        .read(
            u64::from_le_bytes(bytes[8..].try_into().unwrap()),
            &mut table,
        )
        .unwrap();
    assert_eq!(&table[..8], &table[8..16]);
    assert_eq!(&table[24..], &[0; 8]);
    for address in table[..24].chunks_exact(8) {
        let address = u64::from_le_bytes(address.try_into().unwrap());
        assert!((0x1000..0x10000).contains(&address));
        assert_eq!(address & 3, 0);
    }
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn callback_values_tables_addresses_returns_and_scalar_results_execute() {
    let (machine, output) = execute(
        "typedef int (*unary)(int);
         typedef unary same;
         typedef void (*sink)(int);
         typedef const char *(*reader)(void);
         typedef _Bool (*predicate)(unsigned long);
         static int add(int value) { return value + 1; }
         static int sub(int value) { return value - 1; }
         static void uart(int value) { *(volatile unsigned char *)0x10000000UL = value; }
         static const char *text(void) { return \"OK\"; }
         static _Bool truth(unsigned long value) { return value; }
         static unary initial = &*add;
         static unary zero;
         static unary table[] = { [2] = sub, [0] = 1 ? add : sub };
         static unary select(int n) { return n ? add : sub; }
         int main(void) {
             same local[] = { [2] = add, [0] = sub };
             unary *p = local;
             unary *q = &initial;
             if (sizeof(unary) != 8 || sizeof(local) != 24 || sizeof(*p) != 8) return 1;
             if (zero != 0 || table[1] || local[1] || p + 2 - p != 2) return 2;
             if (initial != add || *q != &add || *p != sub || p[2] != add) return 3;
             *q = sub;
             p[1] = select(1);
             unary chosen = 0 ? 0 : *q;
             unary empty = 1 ? 0 : chosen;
             if (empty || !chosen || (chosen == 0) || (0 != empty)) return 4;
             if (chosen(42) != 41 || (*p[1])(42) != 43 || (**p[1])(42) != 43) return 5;
             if ((&*p[1])(42) != 43 || (&add)(42) != 43) return 6;
             predicate test = truth;
             if (!test(0x8000000000000000UL) || test(0)) return 7;
             _Bool nonnull = chosen;
             _Bool isnull = empty;
             if (!nonnull || isnull) return 8;
             reader read = text;
             sink write = uart;
             const char *message = read();
             write(message[0]); write(message[1]);
             return p[1](41);
         }",
    );
    assert_eq!(output, b"OK");
    assert_eq!(machine.core().register(10), Some(42));
}

#[test]
fn callback_targets_and_nested_stack_arguments_evaluate_once_and_survive_calls() {
    let (machine, _) = execute(
        "typedef long (*wide)(long,long,long,long,long,long,long,long,long,long);
         typedef int (*unary)(int);
         int targets;
         int arguments;
         static long sum(long a,long b,long c,long d,long e,long f,long g,long h,long i,long j) {
             return a+b+c+d+e+f+g+h+i+j;
         }
         static wide get(void) { ++targets; return sum; }
         static long next(void) { return ++arguments; }
         static long clear(wide *p) { *p = 0; return 10; }
         static long recurse(int n, wide callback) {
             if (!n) return callback(1,2,3,4,5,6,7,8,9,10);
             return n + recurse(n - 1, callback);
         }
         static int plus(int n) { return n + 1; }
         int main(void) {
             wide callback = get();
             long value = get()(next(), next(), next(), next(), next(), next(), next(), next(),
                                get()(1,1,1,1,1,1,1,1,1,1), next());
             if (targets != 3 || arguments != 9 || value != 55) return 1;
             if (recurse(4, callback) != 65) return 2;
             unary table[2] = { plus, 0 };
             int index = 0;
             if (table[index++](41) != 42 || index != 1) return 3;
             wide last = callback;
             long result = callback(1,2,3,4,5,6,7,8,9,clear(&callback));
             if (result != 55 || callback != 0 || !last) return 4;
             return 42;
         }",
    );
    assert_eq!(machine.core().register(10), Some(42));
}

#[test]
fn callback_prototypes_and_abstract_types_use_exact_parameter_and_return_types() {
    let (machine, _) = execute(
        "int increment(int value);
         int (*global)(int) = increment;
         int call(int (*callback)(int), int value) { return callback(value); }
         int increment(const int value) { return value + 1; }
         int main(void) {
             if (sizeof(long (*)(long)) != 8) return 1;
             if ((int (*)(int))0 != 0 || global != &increment) return 2;
             return call(global, 41);
         }",
    );
    assert_eq!(machine.core().register(10), Some(42));
}

#[test]
fn callback_register_and_stack_abi_matches_independent_assembly() {
    let mut assembly = compile_to_assembly(
        "typedef unsigned int (*wide)(signed char,unsigned short,int,int,int,int,int,int,unsigned int);
         unsigned int foreign(signed char a,unsigned short b,int c,int d,int e,int f,int g,int h,unsigned int i) { return 0; }
         unsigned int probe(wide callback) { return callback(-1,65535,3,4,5,6,7,8,0x80000001U) + 1U; }
         int main(void) { return 0; }",
        "callback-abi.c",
    ).unwrap()
        .replacen("call main\n", "call .Lcallback_entry\n", 1)
        .replacen("foreign:\n", ".Lunused_foreign:\n", 1);
    assembly.push_str(
        ".Lcallback_entry:
         addi sp, sp, -16
         sd ra, 8(sp)
         li s0, 123
         li s1, 456
         la a0, foreign
         call probe
         ld ra, 8(sp)
         addi sp, sp, 16
         ret
         foreign:
         andi t0, sp, 15
         bne t0, zero, .Lcallback_bad
         li t0, -1
         bne a0, t0, .Lcallback_bad
         li t0, 65535
         bne a1, t0, .Lcallback_bad
         add t0, a2, a3
         add t0, t0, a4
         add t0, t0, a5
         add t0, t0, a6
         add t0, t0, a7
         li t1, 33
         bne t0, t1, .Lcallback_bad
         ld t0, 0(sp)
         li t1, -2147483647
         bne t0, t1, .Lcallback_bad
         li a0, -2147483647
         ret
         .Lcallback_bad:
         li a0, 99
         ret",
    );
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    for (register, value) in [
        (2, 0x80010000),
        (8, 123),
        (9, 456),
        (10, (-2147483646i64) as u64),
    ] {
        assert_eq!(machine.core().register(register), Some(value));
    }
}

#[test]
fn callback_zero_through_thirty_two_arguments_preserve_call_frames() {
    for count in [0, 1, 8, 9, 10, 11, 12, 31, 32] {
        let types = if count == 0 {
            "void".to_owned()
        } else {
            vec!["long"; count].join(",")
        };
        let parameters = if count == 0 {
            "void".to_owned()
        } else {
            (0..count)
                .map(|index| format!("long a{index}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        let arguments = (1..=count)
            .map(|index| format!("nested({index})"))
            .collect::<Vec<_>>()
            .join(",");
        let sum = if count == 0 {
            "42".to_owned()
        } else {
            (0..count)
                .map(|index| format!("a{index}"))
                .collect::<Vec<_>>()
                .join("+")
        };
        let source = format!(
            "typedef long (*callback)({types});
             long nested(long n) {{ return n; }}
             long sum({parameters}) {{ return {sum}; }}
             int main(void) {{
                 callback p = sum;
                 long first = p({arguments});
                 long second = (*p)({arguments});
                 if (first != second) return 1;
                 return first;
             }}"
        );
        let (machine, _) = execute_with_budget(&source, 30000);
        assert_eq!(
            machine.core().register(10),
            Some(if count == 0 {
                42
            } else {
                (count * (count + 1) / 2) as u64
            }),
            "{count}"
        );
    }
}

#[test]
fn callback_result_conversions_match_all_supported_integer_widths() {
    for (ty, value, expected) in [
        ("_Bool", "0x100000000UL", 1u64),
        ("char", "255", 255),
        ("signed char", "255", u64::MAX),
        ("unsigned char", "255", 255),
        ("short", "65535", u64::MAX),
        ("unsigned short", "65535", 65535),
        ("int", "0x80000001U", (-2147483647i64) as u64),
        ("unsigned int", "0x80000001U", 0x80000001),
        ("long", "-4294967297L", (-4294967297i64) as u64),
        ("unsigned long", "0xfedcba9876543210UL", 0xfedcba9876543210),
    ] {
        let source = format!(
            "typedef {ty} (*callback)(unsigned long);
             {ty} convert(unsigned long n) {{ return n; }}
             int main(void) {{
                 callback p = convert;
                 {ty} value = p({value});
                 *(volatile unsigned long *)0x80000000UL = value;
                 return 42;
             }}"
        );
        let (mut machine, _) = execute(&source);
        assert_eq!(machine.core().register(10), Some(42), "{ty}");
        let mut bytes = [0; 8];
        machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
        assert_eq!(u64::from_le_bytes(bytes), expected, "{ty}");
    }
}

#[test]
fn callback_invalid_types_operations_and_initializers_preserve_existing_rom() {
    for body in [
        "callback = wrong;",
        "callback = wide;",
        "callback = noargs;",
        "callback = pointer;",
        "callback = &object;",
        "callback = 1;",
        "void *p = callback;",
        "int *p = callback;",
        "callback = (void *)1;",
        "callback(1,2);",
        "callback();",
        "callback((int *)0);",
        "callback++;",
        "callback += 1;",
        "callback + 1;",
        "1 + callback;",
        "callback - add;",
        "callback < add;",
        "callback | 1;",
        "*callback = add;",
        "(&*callback) = add;",
        "(unary)callback = add;",
        "++*callback;",
        "add = callback;",
        "sizeof(add);",
        "sizeof(*callback);",
        "int *p = &callback;",
        "long number = (long)callback;",
        "callback = (unary)1;",
        "(void *)callback;",
        "unary *p = &callback; const int **q = p;",
        "long (*p)(int) = callback;",
        "__atomic_load_n(&callback, 0);",
        "if (0) callback(1,2);",
    ] {
        let source = format!(
            "typedef int (*unary)(int);
             int add(int n) {{ return n + 1; }}
             long wrong(int n) {{ return n; }}
             int wide(long n) {{ return n; }}
             int noargs(void) {{ return 1; }}
             int *pointer(int n) {{ return 0; }}
             int main(void) {{ int object = 0; unary callback = add; {body} return 0; }}"
        );
        let diagnostic = compile_to_assembly(&source, "callback-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("callback-invalid.c:"),
            "{body}: {diagnostic}"
        );
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{body}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    for source in [
        "typedef int (*old)(); int main(void) { return 0; }",
        "typedef int (*varargs)(...); int main(void) { return 0; }",
        "typedef int (**nested)(int); int main(void) { return 0; }",
        "typedef int (*fn)(int); int f(int n) { return n; } fn global = f(); int main(void) { return 0; }",
        "typedef int (*fn)(int); int f(int n) { return n; } fn other = f; fn global = other; int main(void) { return 0; }",
        "typedef int (*fn)(int); fn global = later; int later(int n) { return n; } int main(void) { return 0; }",
        "typedef int (*fn)(int); int missing(int); fn global = missing; int main(void) { return 0; }",
        "typedef int (*fn)(int); const fn global; int main(void) { return 0; }",
        "typedef int (*fn)(int); int f(void *p) { return 0; } fn global = f; int main(void) { return 0; }",
    ] {
        assert!(
            compile_to_assembly(source, "callback-invalid.c").is_err(),
            "{source}"
        );
    }
    let parameters = vec!["int"; 33].join(",");
    assert!(
        compile_to_assembly(
            &format!("typedef int (*callback)({parameters}); int main(void) {{ return 0; }}"),
            "callback-invalid.c"
        )
        .is_err()
    );
}

#[test]
fn target_headers_and_macros_execute_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_headers.c");
    let assembly = compile_to_assembly(source, "headers.c").unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 128]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(output, b"H\x11\x2a\x08\n");
    assert_eq!(machine.core().register(10), Some(59));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut bytes = [0; 24];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    let mut expected = [0; 24];
    expected[..4].copy_from_slice(&17u32.to_le_bytes());
    expected[12..16].copy_from_slice(&42u32.to_le_bytes());
    expected[16..].copy_from_slice(&8u64.to_le_bytes());
    assert_eq!(bytes, expected);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn target_integer_headers_match_lp64_sizes_limits_and_promotions() {
    let (machine, _) = execute(
        "#include <stdint.h>
         #include <stddef.h>
         #include <stdbool.h>
         #include <limits.h>
         #define VERIFY(type, bytes, minimum, maximum) \\
             if (sizeof(type) != bytes || (type)(minimum) != minimum || (type)(maximum) != maximum) return __LINE__;
         int main(void) {
             VERIFY(int8_t, 1, INT8_MIN, INT8_MAX)
             VERIFY(uint8_t, 1, 0, UINT8_MAX)
             VERIFY(int16_t, 2, INT16_MIN, INT16_MAX)
             VERIFY(uint16_t, 2, 0, UINT16_MAX)
             VERIFY(int32_t, 4, INT32_MIN, INT32_MAX)
             VERIFY(uint32_t, 4, 0, UINT32_MAX)
             VERIFY(int64_t, 8, INT64_MIN, INT64_MAX)
             VERIFY(uint64_t, 8, 0, UINT64_MAX)
             VERIFY(int_least8_t, 1, INT_LEAST8_MIN, INT_LEAST8_MAX)
             VERIFY(uint_least8_t, 1, 0, UINT_LEAST8_MAX)
             VERIFY(int_least16_t, 2, INT_LEAST16_MIN, INT_LEAST16_MAX)
             VERIFY(uint_least16_t, 2, 0, UINT_LEAST16_MAX)
             VERIFY(int_least32_t, 4, INT_LEAST32_MIN, INT_LEAST32_MAX)
             VERIFY(uint_least32_t, 4, 0, UINT_LEAST32_MAX)
             VERIFY(int_least64_t, 8, INT_LEAST64_MIN, INT_LEAST64_MAX)
             VERIFY(uint_least64_t, 8, 0, UINT_LEAST64_MAX)
             VERIFY(int_fast8_t, 8, INT_FAST8_MIN, INT_FAST8_MAX)
             VERIFY(uint_fast8_t, 8, 0, UINT_FAST8_MAX)
             VERIFY(int_fast16_t, 8, INT_FAST16_MIN, INT_FAST16_MAX)
             VERIFY(uint_fast16_t, 8, 0, UINT_FAST16_MAX)
             VERIFY(int_fast32_t, 8, INT_FAST32_MIN, INT_FAST32_MAX)
             VERIFY(uint_fast32_t, 8, 0, UINT_FAST32_MAX)
             VERIFY(int_fast64_t, 8, INT_FAST64_MIN, INT_FAST64_MAX)
             VERIFY(uint_fast64_t, 8, 0, UINT_FAST64_MAX)
             VERIFY(intptr_t, 8, INTPTR_MIN, INTPTR_MAX)
             VERIFY(uintptr_t, 8, 0, UINTPTR_MAX)
             VERIFY(intmax_t, 8, INTMAX_MIN, INTMAX_MAX)
             VERIFY(uintmax_t, 8, 0, UINTMAX_MAX)
             VERIFY(size_t, 8, 0, SIZE_MAX)
             VERIFY(ptrdiff_t, 8, PTRDIFF_MIN, PTRDIFF_MAX)
             if (sizeof(INT8_C(127)) != 4 || sizeof(UINT8_C(255)) != 4) return 1;
             if (sizeof(INT16_C(32767)) != 4 || sizeof(UINT16_C(65535)) != 4) return 2;
             if (sizeof(INT32_C(2147483647)) != 4 || sizeof(UINT32_C(4294967295)) != 4) return 3;
             if (sizeof(INT64_C(9223372036854775807)) != 8 || sizeof(UINT64_C(18446744073709551615)) != 8) return 4;
             if (sizeof(INTMAX_C(1)) != 8 || sizeof(UINTMAX_C(1)) != 8) return 5;
             if (UINT8_C(255) + 1 != 256 || UINT16_C(65535) + 1 != 65536) return 6;
             if (UINT32_C(4294967295) + 1U != 0 || UINT64_C(18446744073709551615) + 1UL != 0) return 7;
             if (LONG_MIN != INT64_MIN || ULONG_MAX != UINT64_MAX || SHRT_MIN != INT16_MIN) return 8;
             if (CHAR_MIN || CHAR_MAX != 255 || CHAR_BIT != 8 || sizeof(bool) != 1) return 9;
             bool flag = true;
             void *pointer = NULL;
             if (!__bool_true_false_are_defined || !flag || false || pointer != 0) return 10;
             return 42;
         }",
    );
    assert_eq!(machine.core().register(10), Some(42));
}

#[test]
fn rv64_preprocessor_conditions_use_signed_and_unsigned_intmax_widths() {
    let conditions = [
        "0xffffffff > 0",
        "0xffffffff + 1 == 0x100000000",
        "0xffffffffU + 1U == 0x100000000UL",
        "2147483647 + 1 == 2147483648",
        "(1 < 2) << 40 == 0x10000000000",
        "(!0) << 40 == 0x10000000000",
        "(1 && 1) << 40 == 0x10000000000",
        "~0U == 18446744073709551615UL",
        "-1 < 0 && !(-1 < 0U)",
        "(1 ? -1 : 0U) > 0",
        "(0 ? -1 : 0U) == 0",
        "0xffffffffffffffff + 1 == 0",
        "0xffffffffffffffffUL / 0x100000000UL == 0xffffffff",
        "-9223372036854775807L - 1L < -1",
        "(-9223372036854775807L - 1L) >> 63 == -1",
        "1 || 1 / 0",
        "!(0 && 1 / 0)",
        "(1 ? 7 : 1 / 0) == 7",
        "((1 ? 1 : 0) << 40) == 0x10000000000",
        "'A' == 65 && '\\xff' == 255",
        "defined(__kiuas16__) && defined __riscv && __riscv_xlen == 64",
        "!defined(UNKNOWN) && UNKNOWN == 0",
    ];
    for condition in conditions {
        let source = format!(
            "#if {condition}\n#define RESULT 42\n#else\n#error wrong intmax evaluation\n#endif\n\
             int main(void) {{ return RESULT; }}"
        );
        let (machine, _) = execute(&source);
        assert_eq!(machine.core().register(10), Some(42), "{condition}");
    }
}

#[test]
fn rv64_preprocessor_macros_rescan_stringify_paste_and_preserve_side_effects() {
    let (machine, output) = execute(
        "#define JOIN_RAW(a,b) a##b
         #define JOIN(a,b) JOIN_RAW(a,b)
         #define TEXT_RAW(value) #value
         #define TEXT(value) TEXT_RAW(value)
         #define PREFIX \"P\"
         #define SUFFIX \"!\"
         #define ADD(a,b) ((a) + (b))
         #define CALL ADD
         #define WRAP(x) x
         #define MMIO(address) (*(volatile unsigned char *)(address))
         #define OUT(x) MMIO(0x10000000UL) = (x)
         #define VALUE 17
         static const int early = VALUE;
         #undef VALUE
         #define VALUE 42
         static const int late = VALUE;
         #define EMPTY()
         #define BIAS \\
             3
         /* macros and comments
          * retain physical source lines */
         int calls;
         int next(void) { return ++calls; }
         int main(void) {
             int JOIN(result, _value) = CALL
                 (next(), WRAP(ADD(next(), BIAS)));
             EMPTY()
             const char *text = PREFIX TEXT(VALUE) SUFFIX;
             const char *raw = TEXT_RAW(VALUE);
             const char *comment = TEXT_RAW(a /* gap */ b);
             const char *quoted = TEXT_RAW(\"a\" \"b\");
             if (result_value != 6 || calls != 2 || early != 17 || late != 42) return 1;
             if (raw[0] != 'V' || comment[1] != ' ' || quoted[3] != ' ') return 2;
             if (text[0] != 'P' || text[3] != '!') return 3;
             OUT(text[1]); OUT(text[2]); OUT('\\n');
             return result_value;
         }",
    );
    assert_eq!(output, b"42\n");
    assert_eq!(machine.core().register(10), Some(6));
}

#[test]
fn rv64_preprocessor_nested_quoted_headers_guards_and_macro_includes_execute() {
    let files = Files::new(
        "#define PICK \"consumer.h\"
         #include PICK
         #include \"consumer.h\"
         #include <stdint.h>
         #include <stdint.h>
         #ifndef INCLUDED_BY_PARENT
         #error nested header macro missing
         #endif
         #if 0
         #include \"missing-inactive.h\"
         #error inactive
         #if 1 / 0
         #error inactive nested
         #endif
         #elif defined(INCLUDED_BY_PARENT)
         #define RESULT 42
         #else
         #error wrong branch
         #endif
         int main(void) { return helper(RESULT); }",
    );
    std::fs::write(
        files.0.join("consumer.h"),
        "#ifndef CONSUMER_H\n#define CONSUMER_H\n#include \"nested.h\"\n\
         static uint32_t helper(uint32_t value) { return value + OFFSET; }\n#endif",
    )
    .unwrap();
    std::fs::write(
        files.0.join("nested.h"),
        "#include <stdint.h>\n#define INCLUDED_BY_PARENT 1\n#define OFFSET 17\n",
    )
    .unwrap();
    let source = std::fs::read_to_string(files.0.join("guest.c")).unwrap();
    let assembly =
        compile_to_assembly(&source, &files.0.join("guest.c").to_string_lossy()).unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(59));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
}

#[test]
fn rv64_preprocessor_repeated_header_expansion_retains_each_inclusion_state() {
    let files = Files::new(
        "#define NAME first\n#define VALUE 17\n#include \"instance.h\"\n\
         #undef NAME\n#undef VALUE\n#define NAME second\n#define VALUE 42\n#include \"instance.h\"\n\
         int main(void) { return first + second; }",
    );
    std::fs::write(
        files.0.join("instance.h"),
        "static const int NAME = VALUE;\n",
    )
    .unwrap();
    let source = std::fs::read_to_string(files.0.join("guest.c")).unwrap();
    let assembly =
        compile_to_assembly(&source, &files.0.join("guest.c").to_string_lossy()).unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(59));
}

#[test]
fn rv64_preprocessor_line_file_and_header_diagnostics_preserve_locations() {
    let (machine, output) = execute(
        "#line 73 \"virtual-rv64.c\"
         int main(void) {
             const char *file = __FILE__;
             *(volatile unsigned char *)0x10000000UL = file[0];
             return __LINE__;
         }",
    );
    assert_eq!(output, b"v");
    assert_eq!(machine.core().register(10), Some(76));
    let diagnostic = compile_to_assembly(
        "#line 123 \"mapped.h\"\n#if 1 / 0\n#endif\nint main(void) { return 0; }",
        "original.c",
    )
    .unwrap_err()
    .to_string();
    assert!(diagnostic.contains("mapped.h:123:"), "{diagnostic}");
    let files = Files::new("#include \"broken.h\"\nint main(void) { return 0; }");
    std::fs::write(
        files.0.join("broken.h"),
        "#line 91 \"broken-virtual.h\"\n#error broken header\n",
    )
    .unwrap();
    let build = files.compile();
    let diagnostic = String::from_utf8_lossy(&build.stderr);
    assert!(!build.status.success());
    assert!(diagnostic.contains("broken-virtual.h:91:"), "{diagnostic}");
}

#[test]
fn rv64_preprocessor_invalid_input_and_legacy_headers_preserve_existing_rom() {
    for source in [
        "#include <stdio.h>\nint main(void) { return 0; }",
        "#include <does-not-exist.h>\nint main(void) { return 0; }",
        "#define BAD(a,a) a\nint main(void) { return 0; }",
        "#define BAD(...) 0\nint main(void) { return 0; }",
        "#define VALUE 1\n#define VALUE 2\nint main(void) { return VALUE; }",
        "#define ADD(a,b) ((a)+(b))\nint main(void) { return ADD(1); }",
        "#define ADD(a,b) ((a)+(b))\nint main(void) { return ADD(1,2; }",
        "#define BAD(a,b) a##b\nint main(void) { return BAD(+,*); }",
        "#pragma pack(1)\nint main(void) { return 0; }",
        "#error stop here\nint main(void) { return 0; }",
        "#warning ignored\nint main(void) { return 0; }",
        "#if 1\nint main(void) { return 0; }",
        "#else\nint main(void) { return 0; }",
        "#if 0\n#else\n#else\n#endif\nint main(void) { return 0; }",
        "#if defined()\n#endif\nint main(void) { return 0; }",
        "#if 1 / 0\n#endif\nint main(void) { return 0; }",
        "#if 9223372036854775807L + 1\n#endif\nint main(void) { return 0; }",
        "#if 1 << 64\n#endif\nint main(void) { return 0; }",
        "#if 18446744073709551616UL\n#endif\nint main(void) { return 0; }",
        "#if 1LL\n#endif\nint main(void) { return 0; }",
        "#if (int)1\n#endif\nint main(void) { return 0; }",
        "#if sizeof(int)\n#endif\nint main(void) { return 0; }",
        "#if 1 || (1,2)\n#endif\nint main(void) { return 0; }",
        "#line 0\nint main(void) { return 0; }",
        "#define A A\nint main(void) { return A; }",
    ] {
        let diagnostic = compile_to_assembly(source, "pp-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("pp-invalid.c:"),
            "{source}: {diagnostic}"
        );
        let files = Files::new(source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn compiled_c_recursion_eight_arguments_scopes_and_uart_execute() {
    let (machine, output) = execute(include_str!("fixtures/kiuas16_uart.c"));
    assert_eq!(output, b"KK=\n");
    assert_eq!(machine.core().register(10), Some(120));
}

#[test]
fn compiled_text_literals_character_buffers_and_uart_execute_through_cli() {
    let source = include_str!("fixtures/kiuas16_text.c");
    let assembly = compile_to_assembly(source, "text.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 128]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(output, b"Kiuas16: ROM ready\n\x80\xff\n");
    assert_eq!(machine.core().register(10), Some(19));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut addresses = [0; 24];
    machine.bus_mut().read(0x80000000, &mut addresses).unwrap();
    for (address, expected) in
        addresses
            .chunks_exact(8)
            .zip([&b"\x80\xff\0"[..], &b"\0"[..], &b"OK\0\0"[..]])
    {
        let address = u64::from_le_bytes(address.try_into().unwrap());
        let mut bytes = vec![0; expected.len()];
        machine.bus_mut().read(address, &mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }
    let mut table_address = [0; 8];
    machine
        .bus_mut()
        .read(0x80000018, &mut table_address)
        .unwrap();
    let mut entries = [0; 32];
    machine
        .bus_mut()
        .read(u64::from_le_bytes(table_address), &mut entries)
        .unwrap();
    assert_eq!(&entries[24..32], &[0; 8]);
    for (entry, expected) in
        entries[..24]
            .chunks_exact(8)
            .zip([&b"Kiuas16: \0"[..], &b"ROM \0"[..], &b"ready\n\0"[..]])
    {
        let mut bytes = vec![0; expected.len()];
        machine
            .bus_mut()
            .read(u64::from_le_bytes(entry.try_into().unwrap()), &mut bytes)
            .unwrap();
        assert_eq!(bytes, expected);
    }
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn character_promotions_aliases_updates_and_volatile_widths_execute() {
    let cases = [
        ("char c = 255; return c + 1;", 256),
        ("signed char c = 255; return c;", -1),
        ("signed char c = 128; return c;", -128),
        ("signed char c = -128; return c / 2;", -64),
        ("signed char c = -1; return c < 1U;", 0),
        ("signed char c = -128; return c >> 1;", -64),
        ("signed char c = 127; int old = c++; return old + c;", -1),
        ("signed char c = -128; return --c;", 127),
        ("signed char c = 127; return c += 1;", -128),
        ("char c = 255; return ++c;", 0),
        (
            "signed char c = 0; signed char *p = &c; *p = 255; return c;",
            -1,
        ),
        ("char c = 0; char *p = &c; *p = 255; return c;", 255),
        (
            "return sizeof(char) + sizeof(signed char) + sizeof(char *);",
            10,
        ),
        ("signed char c = -1; return sizeof(c + c);", 4),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }
    let (mut machine, _) = execute(
        "int main(void) {
            volatile signed char *s = (volatile signed char *)0x80000000UL;
            volatile char *u = (volatile char *)0x80000001UL;
            *s = 255; *u = 128;
            if (*s != -1 || *u != 128) return 1;
            *s += 2; *u += 127;
            return *s + *u;
        }",
    );
    assert_eq!(machine.core().register(10), Some(256));
    let mut bytes = [0; 3];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(bytes, [1, 255, 0]);
}

#[test]
fn string_array_initializers_preserve_escapes_embedded_zero_and_padding() {
    let source = r#"
        char global[] = {"A\0B"};
        signed char signed_bytes[] = {"\x80\377"};
        unsigned char unsigned_bytes[2] = "\200\377";
        int main(void) {
            char local[] = {"a" "b"};
            char exact[2] = "CD";
            char padded[5] = {"E"};
            char empty[] = "";
            if (sizeof(global) != 4 || sizeof(local) != 3 || sizeof(empty) != 1) return 1;
            if (sizeof(signed_bytes) != 3 || sizeof(unsigned_bytes) != 2) return 2;
            if (sizeof("A\0B") != 4 || global[1] || global[2] != 'B' || global[3]) return 3;
            if (signed_bytes[0] != -128 || signed_bytes[1] != -1 || signed_bytes[2]) return 4;
            if (unsigned_bytes[0] != 128 || unsigned_bytes[1] != 255) return 5;
            if (local[0] != 'a' || local[1] != 'b' || local[2] || empty[0]) return 6;
            if (exact[0] != 'C' || exact[1] != 'D' || padded[0] != 'E' || padded[4]) return 7;
            char bound[sizeof("XY")] = "XY";
            if (sizeof(bound) != 3 || bound[2]) return 8;
            return 47;
        }"#;
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(47));
}

#[test]
fn string_addresses_have_static_lifetime_and_use_final_layout() {
    let source = r#"
        char *saved = "ABC" + 1;
        volatile char *qualified = &"ABC"[2];
        char *choose = 0 ? (char *)0 : "Z";
        char *get(int depth) {
            char scratch[16] = "scratch";
            if (depth) return get(depth - 1);
            return "ABC";
        }
        int main(void) {
            char *p = get(5);
            char overwrite[16] = "overwrite";
            if (p[0] != 'A' || p[1] != 'B' || p[2] != 'C' || p[3]) return 1;
            if (*saved != 'B' || *qualified != 'C' || *choose != 'Z') return 2;
            if (sizeof(*p) != 1 || sizeof(p) != 8 || sizeof("ABC") != 4) return 3;
            if ("ABC"[1] != 'B' || *&"ABC"[2] != 'C') return 4;
            return 49;
        }"#;
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(49));
}

#[test]
fn character_abi_interoperates_with_independent_register_and_stack_calls() {
    let source = "
        signed char echo(signed char value) { return value; }
        char plain(char value) { return value; }
        signed char probe(signed char a, char b, unsigned char c,
                          int d, int e, int f, int g, int h, signed char i, char j) {
            return 0;
        }
        signed char caller(void) {
            return probe(-128, 255, 255, 0, 0, 0, 0, 0, echo(-1), 128);
        }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "character-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Lcharacter_entry\n", 1)
        .replacen("\nprobe:\n", "\n.Lunused_probe:\n", 1);
    assembly.push_str(
        ".Lcharacter_entry:
         addi sp, sp, -16
         sd ra, 0(sp)
         li a0, 255
         call echo
         mv s2, a0
         li a0, -1
         call plain
         mv s3, a0
         call caller
         mv s4, a0
         ld ra, 0(sp)
         addi sp, sp, 16
         ret
         probe:
         andi t0, sp, 15
         bne t0, zero, .Lcharacter_bad
         li t0, -128
         bne a0, t0, .Lcharacter_bad
         li t0, 255
         bne a1, t0, .Lcharacter_bad
         bne a2, t0, .Lcharacter_bad
         ld t1, 0(sp)
         li t0, -1
         bne t1, t0, .Lcharacter_bad
         ld t1, 8(sp)
         li t0, 128
         bne t1, t0, .Lcharacter_bad
         li a0, -2
         ret
         .Lcharacter_bad:
         li a0, 7
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(18), Some(u64::MAX));
    assert_eq!(machine.core().register(19), Some(255));
    assert_eq!(machine.core().register(20), Some(u64::MAX - 1));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn invalid_character_and_string_uses_diagnose_without_output_overwrite() {
    for source in [
        r#"int main(void) { char a[2] = "ABC"; return 0; }"#,
        r#"char a[2] = "ABC"; int main(void) { return 0; }"#,
        r#"int main(void) { int a[4] = "ABC"; return 0; }"#,
        r#"int a[4] = "ABC"; int main(void) { return 0; }"#,
        r#"int main(void) { unsigned char *p = "ABC"; return 0; }"#,
        r#"int main(void) { signed char *p = "ABC"; return 0; }"#,
        r#"int main(void) { char *p = 1; return 0; }"#,
        r#"int main(void) { char *p = "ABC"; signed char *s = p; return 0; }"#,
        r#"int main(void) { volatile char *p = "ABC"; char *q = p; return 0; }"#,
        r#"int main(void) { "ABC" = "DEF"; return 0; }"#,
        r#"int main(void) { ++"ABC"; return 0; }"#,
        r#"char *p = &"ABC"; int main(void) { return 0; }"#,
        r#"char full[65536]; char *p = "X"; int main(void) { return 0; }"#,
        "char a; signed char a; int main(void) { return 0; }",
        "int main(void) { volatile char c; char *p = &c; return 0; }",
        r#"int main(void) { return L"ABC"[0]; }"#,
    ] {
        let error = compile_to_assembly(source, "text-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("text-invalid.c"), "{source}: {error}");
    }
    let oversized = format!(
        "char *p = \"{}\"; int main(void) {{ return 0; }}",
        "A".repeat(65536)
    );
    let error = compile_to_assembly(&oversized, "text-invalid.c")
        .unwrap_err()
        .to_string();
    assert!(error.contains("65536-byte RAM budget"), "{error}");
    let files = Files::new(r#"int main(void) { char a[2] = "ABC"; return 0; }"#);
    std::fs::write(files.0.join("rom.bin"), b"previous").unwrap();
    assert!(!files.compile().status.success());
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"previous");
    let oversized_rom = format!(
        "char *p = \"{}\"; int main(void) {{ return p[0]; }}",
        "A".repeat(61440)
    );
    let files = Files::new(&oversized_rom);
    std::fs::write(files.0.join("rom.bin"), b"previous").unwrap();
    let build = files.compile();
    assert!(!build.status.success());
    assert!(
        String::from_utf8_lossy(&build.stderr).contains("ROM"),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"previous");
}

#[test]
fn persistent_global_cursors_resolve_after_layout_and_execute_through_cli() {
    let source = include_str!("fixtures/kiuas16_cursors.c");
    let assembly = compile_to_assembly(source, "cursors.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 64]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    assert_eq!(machine.core().register(10), Some(13));
    assert_eq!(output, [b'R', 6, 15, b'\n']);
    let mut bytes = [0; 56];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    let expected_words = [
        0x80010028_u64,
        0x80010027,
        0x80010027,
        0x10000000,
        u64::from_le_bytes([0, 10, 11, 12, 13, 14, 15, 0]),
        6,
        0,
    ];
    let expected: Vec<u8> = expected_words
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    assert_eq!(bytes.as_slice(), expected);
    let mut results = [0; 40];
    machine.bus_mut().read(0x80000000, &mut results).unwrap();
    let expected: Vec<u8> = [
        0x80010020_u64,
        0x80010027,
        0x80010027,
        0x80010028,
        0x80010028,
    ]
    .iter()
    .flat_map(|word| word.to_le_bytes())
    .collect();
    assert_eq!(results.as_slice(), expected);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn global_pointer_initializers_resolve_final_data_and_bss_object_addresses() {
    let source = "
        int words[4];
        int *a = &words[2];
        int *b = words + 4;
        int *c = &words[3] - 2;
        int *d = 1 + words;
        volatile int *e = &words[0];
        int *f = (int *)(volatile int *)&words[1];
        int *g = sizeof(int) == 4 ? &words[3] : (int *)0;
        int *h = (int *)0;
        int *i;
        unsigned long longs[3];
        unsigned long *j = longs + 2;
        unsigned int value = 17;
        unsigned int *k = &value;
        unsigned char bytes[9] = {7};
        unsigned char *l = bytes + 8;
        int words[4] = {1, 2, 3, 4};
        int main(void) {
            if (a != &words[2] || *a != 3 || b - words != 4) return 1;
            if (c != words + 1 || d != c || f != c) return 2;
            if (e != words || g != &words[3] || *g != 4) return 3;
            if (h || i || sizeof(h) != 8) return 4;
            if ((unsigned long)j != (unsigned long)longs + 16UL) return 5;
            if (*k != 17 || l != &bytes[8] || *l != 0) return 6;
            *a = -7;
            if (words[2] != -7) return 7;
            return 37;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(37));
}

#[test]
fn global_pointer_updates_and_assignments_preserve_addresses_across_nested_calls() {
    let source = "
        long values[4] = {3, 5, 7, 11};
        long *cursor = values;
        long *saved;
        int depth;
        int next(void) { ++depth; return depth; }
        long *set(long *p) { saved = p; return p; }
        int shadow(long *cursor) { return *cursor; }
        int main(void) {
            if (*cursor++ != 3 || cursor != values + 1) return 1;
            if (*++cursor != 7 || *cursor-- != 7 || *--cursor != 3) return 2;
            cursor += next();
            if (*cursor != 5 || depth != 1) return 3;
            cursor = set(values + next());
            if (cursor != values + 2 || saved != cursor || depth != 2) return 4;
            cursor -= 2;
            if (*cursor != 3 || shadow(values + 3) != 11) return 5;
            if ((unsigned long)cursor != (unsigned long)values) return 6;
            return 41;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(41));
}

#[test]
fn explicit_integer_pointer_initializers_preserve_full_width_and_qualification() {
    let source = "
        unsigned char *high = (unsigned char *)0xfedcba9876543210UL;
        unsigned int *word = (unsigned int *)(unsigned int)0x80000000U;
        long *negative = (long *)-1;
        unsigned long *wide = (unsigned long *)0x10000000000UL;
        volatile unsigned long *qualified = (volatile unsigned long *)0x80000000UL;
        int *null = (int *)0;
        int main(void) {
            if ((unsigned long)high != 0xfedcba9876543210UL) return 1;
            if ((unsigned long)word != 0x80000000UL) return 2;
            if ((unsigned long)negative != 0xffffffffffffffffUL) return 3;
            if ((unsigned long)wide != 0x10000000000UL) return 4;
            *qualified = 0x1122334455667788UL;
            if (null || !qualified) return 5;
            return 43;
        }";
    let (mut machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(43));
    let mut bytes = [0; 8];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(bytes, 0x1122334455667788_u64.to_le_bytes());
    for (offset, number) in [
        (0, 0xfedcba9876543210_u64),
        (8, u64::MAX),
        (16, 0),
        (24, 0x80000000),
        (32, 0x10000000000),
        (40, 0x80000000),
    ] {
        machine
            .bus_mut()
            .read(0x80010000 + offset, &mut bytes)
            .unwrap();
        assert_eq!(bytes, number.to_le_bytes());
    }
}

#[test]
fn tentative_pointer_definitions_and_zero_storage_survive_poisoned_ram() {
    let source = "
        unsigned long data[2];
        unsigned long *cursor;
        unsigned long *cursor = data + 1;
        unsigned long *cursor;
        unsigned long *empty;
        int main(void) {
            if (empty || cursor != data + 1 || *cursor != 0) return 1;
            *cursor = 77;
            return data[1];
        }";
    let assembly = compile_to_assembly(source, "pointer-bss.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 40]).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(77));
    let mut bytes = [0; 40];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    let expected: Vec<u8> = [0x80010010_u64, 0, 77, 0, 0xa5a5a5a5a5a5a5a5]
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    assert_eq!(bytes.as_slice(), expected);
}

#[test]
fn invalid_static_pointer_initializers_and_pointer_object_addresses_diagnose() {
    for source in [
        "int *p = 1; int main(void) { return 0; }",
        "int n; unsigned int *p = &n; int main(void) { return 0; }",
        "int n; long *p = &n; int main(void) { return 0; }",
        "int n; int *p = &n; int *q = p; int main(void) { return 0; }",
        "int n; int *p = &n; int **q = &p; int main(void) { return sizeof(&q); }",
        "int *p = &later; int later; int main(void) { return 0; }",
        "int n; int *p = &n; int *p = &n; int main(void) { return 0; }",
        "int *p; long *p; int main(void) { return 0; }",
        "int n; int *p = &n; int main(void) { return p(); }",
        "int *p = (int *)(1 / 0); int main(void) { return 0; }",
        "int n; int *p = &n + (1 / 0); int main(void) { return 0; }",
        "int n; int *p = n ? &n : (int *)0; int main(void) { return 0; }",
        "int n; int *p = (int *)(n + 1); int main(void) { return 0; }",
        "int n; volatile int *p = &n; int *q = p; int main(void) { return 0; }",
        "int n; int *p = (int *)(long)&n; int main(void) { return 0; }",
        "int n; int **p[2] = {(int **)&n}; int main(void) { return 0; }",
    ] {
        let error = compile_to_assembly(source, "pointer-static-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("pointer-static-invalid.c:"),
            "{source}: {error}"
        );
    }
}

#[test]
fn global_data_and_zero_storage_are_initialized_by_guest_startup() {
    let source = include_str!("fixtures/kiuas16_globals.c");
    let assembly = compile_to_assembly(source, "globals.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 65536]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    assert_eq!(machine.core().register(10), Some(148));
    assert_eq!(output, [b'G', 148, 4, 17, b'\n']);
    let mut results = [0; 32];
    machine.bus_mut().read(0x80000000, &mut results).unwrap();
    let addresses: Vec<u64> = results
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(addresses, [0x80010008, 0x80010010, 0x80010000, 0x80010018]);
    let expected: [&[u8]; 4] = [
        &148_u64.to_le_bytes(),
        &4_u32.to_le_bytes(),
        &[7, 11, 13, 17],
        &[7, 11, 13, 17, 0, 0, 0, 0],
    ];
    for (address, expected) in addresses.iter().zip(expected) {
        assert!((0x80010000..0x80020000).contains(address));
        assert_eq!(address % 8, 0);
        let mut bytes = vec![0; expected.len()];
        machine.bus_mut().read(*address, &mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }
    let mut padding_and_guard = [0; 8];
    machine
        .bus_mut()
        .read(0x80010020, &mut padding_and_guard)
        .unwrap();
    assert_eq!(padding_and_guard, [0xa5; 8]);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn global_constant_initializers_preserve_lp64_types_and_exact_object_bytes() {
    let source = "
        unsigned char a = {511};
        int b = -7;
        unsigned int c = 0x80000001U;
        long d = -4294967297L;
        unsigned long e = 0xfedcba9876543210UL;
        int f[3] = {{-1}, 0x80000000U};
        unsigned char g[] = {256, 511, 2};
        unsigned long h = sizeof(h) + sizeof(f);
        unsigned long i = (0xffffffffU + 2U) + (0 ? 1 / 0 : 7UL);
        int main(void) {
            if (a != 255 || b != -7 || c != 0x80000001U) return 1;
            if (d != -4294967297L || e != 0xfedcba9876543210UL) return 2;
            if (f[0] != -1 || f[1] != -2147483647 - 1 || f[2] != 0) return 3;
            if (g[0] != 0 || g[1] != 255 || g[2] != 2 || sizeof(g) != 3) return 4;
            if (h != 20UL || i != 8UL) return 5;
            return 29;
        }";
    let (mut machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(29));
    let mut bytes = [0; 80];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    let mut expected = [0; 80];
    expected[0] = 255;
    expected[8..12].copy_from_slice(&(-7_i32).to_le_bytes());
    expected[16..20].copy_from_slice(&0x80000001_u32.to_le_bytes());
    expected[24..32].copy_from_slice(&(-4294967297_i64).to_le_bytes());
    expected[32..40].copy_from_slice(&0xfedcba9876543210_u64.to_le_bytes());
    expected[40..44].copy_from_slice(&(-1_i32).to_le_bytes());
    expected[44..48].copy_from_slice(&0x80000000_u32.to_le_bytes());
    expected[56..59].copy_from_slice(&[0, 255, 2]);
    expected[64..72].copy_from_slice(&20_u64.to_le_bytes());
    expected[72..80].copy_from_slice(&8_u64.to_le_bytes());
    assert_eq!(bytes, expected);
}

#[test]
fn tentative_globals_persist_across_calls_and_local_shadowing() {
    let source = "
        int count;
        int count;
        int count = 3;
        int count;
        unsigned long values[3];
        unsigned long values[3];
        void add(int *target) { *target += 2; values[1] += 5; }
        int step(int depth) {
            add(&count);
            if (depth > 0) step(depth - 1);
            return count;
        }
        int shadow(int count) {
            count += 1;
            { int values[2] = {5}; if (sizeof(values) != 8) return 0; }
            return count + sizeof(values);
        }
        int main(void) {
            if (step(3) != 11 || values[1] != 20UL) return 1;
            if (shadow(7) != 32 || count != 11 || sizeof(values) != 24) return 2;
            if (count++ != 11 || ++count != 13) return 3;
            return count;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(13));
}

#[test]
fn global_data_only_and_bss_only_startup_do_not_depend_on_clean_ram() {
    for (declaration, check, expected) in [
        (
            "unsigned long state = 0xfedcba9876543210UL;",
            "return state == 0xfedcba9876543210UL;",
            1,
        ),
        ("unsigned long state;", "return state == 0;", 1),
        (
            "unsigned char state[9] = {7};",
            "return state[0] == 7 && state[1] == 0 && state[8] == 0;",
            1,
        ),
        (
            "unsigned char state[9];",
            "return state[0] == 0 && state[8] == 0;",
            1,
        ),
    ] {
        let source = format!("{declaration} int main(void) {{ {check} }}");
        let assembly = compile_to_assembly(&source, "startup.c").unwrap();
        let rom = assemble_boot_rom(&assembly).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        machine.bus_mut().write(0x80010000, &[0xa5; 32]).unwrap();
        let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0, "{declaration}");
        assert!(report.waiting > 0);
        assert_eq!(machine.core().register(10), Some(expected), "{declaration}");
        let mut untouched = [0; 8];
        machine.bus_mut().read(0x80010010, &mut untouched).unwrap();
        assert_eq!(untouched, [0xa5; 8]);
    }
}

#[test]
fn global_zero_storage_capacity_is_independent_of_stack_slot_limits() {
    let source = "
        unsigned char buffer[65536];
        int main(void) {
            if (buffer[0] != 0 || buffer[65535] != 0) return 1;
            buffer[0] = 7;
            buffer[65535] = 11;
            return buffer[0] + buffer[65535] + (sizeof(buffer) == 65536UL);
        }";
    let assembly = compile_to_assembly(source, "bss-capacity.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 65536]).unwrap();
    let report = run_headless(&mut machine, 300000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(19));
    let mut bytes = [0; 65536];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    assert_eq!(bytes[0], 7);
    assert_eq!(bytes[65535], 11);
    assert_eq!(&bytes[1..65535], &[0; 65534]);
}

#[test]
fn invalid_global_definitions_initializers_visibility_and_capacity_diagnose() {
    for source in [
        "int a = 1; int a = 2; int main(void) { return 0; }",
        "int a; long a; int main(void) { return 0; }",
        "int a[2]; int a[3]; int main(void) { return 0; }",
        "int a[2]; int a; int main(void) { return 0; }",
        "int f; int f(void) { return 1; } int main(void) { return 0; }",
        "int f(void) { return 1; } int f; int main(void) { return 0; }",
        "int main; int main(void) { return 0; }",
        "int _start; int main(void) { return 0; }",
        "int ***pointer; int main(void) { return 0; }",
        "volatile int state; int main(void) { int *p = &state; return 0; }",
        "int **table[2]; int main(void) { return 0; }",
        "int a[2][3]; int main(void) { return 0; }",
        "int a = 1 / 0; int main(void) { return 0; }",
        "int a = 2147483647 + 1; int main(void) { return 0; }",
        "int a; int b = a; int main(void) { return 0; }",
        "int a = a + 1; int main(void) { return 0; }",
        "int a = missing; int main(void) { return 0; }",
        "int f(void) { return 1; } int a = f(); int main(void) { return 0; }",
        "int a; long b = (long)&a; int main(void) { return 0; }",
        "int a[2] = {1, 2, 3}; int main(void) { return 0; }",
        "int a[2] = {}; int main(void) { return 0; }",
        "int a[2] = 1; int main(void) { return 0; }",
        "int a = {1, 2}; int main(void) { return 0; }",
        "int main(void) { return later; } int later;",
        "int main(void) { return sizeof(later); } int later[3];",
        "int object; int main(void) { return object(); }",
        "unsigned char a[65537]; int main(void) { return 0; }",
        "unsigned char a[65536]; int b; int main(void) { return 0; }",
        "unsigned char a[65529]; unsigned char b; int main(void) { return 0; }",
        "unsigned long a[18446744073709551615UL]; int main(void) { return 0; }",
    ] {
        let error = compile_to_assembly(source, "globals-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("globals-invalid.c:"), "{source}: {error}");
    }
}

#[test]
fn oversized_global_rom_image_is_rejected_without_overwriting_output() {
    let source = "unsigned char bytes[65536] = {7}; int main(void) { return bytes[0]; }";
    let assembly = compile_to_assembly(source, "large-data.c").unwrap();
    assert!(assemble_boot_rom(&assembly).is_err());
    let files = Files::new(source);
    std::fs::write(files.0.join("rom.bin"), b"keep").unwrap();
    let build = files.compile();
    assert!(!build.status.success());
    assert!(!build.stderr.is_empty());
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"keep");
}

#[test]
fn automatic_array_copy_executes_through_production_tools_with_exact_stack_bytes() {
    let source = include_str!("fixtures/kiuas16_arrays.c");
    let (mut machine, output) = execute_with_budget(source, 100000);
    assert_eq!(output, [b'A', 0x80, 0x7f, b'\n']);
    assert_eq!(machine.core().register(10), Some(256));
    let mut results = [0; 32];
    machine.bus_mut().read(0x80000000, &mut results).unwrap();
    let words: Vec<u64> = results
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(&words[2..], &[32640, 12]);
    assert_ne!(words[0], words[1]);
    let expected: Vec<u8> = (0..256).map(|i| i as u8).collect();
    for address in &words[..2] {
        assert!((0x8000fc00..0x80010000).contains(address));
        let mut bytes = [0; 256];
        machine.bus_mut().read(*address, &mut bytes).unwrap();
        assert_eq!(bytes.as_slice(), expected);
    }

    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "100000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn array_initializers_use_contiguous_declared_widths_and_zero_fill() {
    let source = "
            int next(int *counter) { return ++*counter; }
            int main(void) {
                long before = 0x1234567887654321L;
                int counter = 0;
                unsigned char bytes[9] = {256, 511, {258}};
                int signed_words[4] = {-1, -2147483647 - 1};
                unsigned int words[] = {0xffffffffU, 0x80000000U,};
                long longs[3] = {-4294967297L, 0x1234567887654321L};
                unsigned long wide[2] = {0xffffffffffffffffUL};
                int calls[] = {next(&counter), next(&counter), next(&counter)};
                long after = 0x7777777766666666L;
                if (sizeof(bytes) != 9 || sizeof(words) != 8 || sizeof(longs) != 24) return 1;
                if (bytes[0] != 0 || bytes[1] != 255 || bytes[2] != 2) return 2;
                for (int i = 3; i < 9; ++i) if (bytes[i] != 0) return 3;
                if (signed_words[0] != -1 || signed_words[1] != -2147483647 - 1) return 4;
                if (signed_words[2] != 0 || signed_words[3] != 0) return 5;
                if (words[0] != 0xffffffffU || words[1] != 0x80000000U) return 6;
                if (longs[0] != -4294967297L || longs[1] != before || longs[2] != 0) return 7;
                if (wide[0] != 0xffffffffffffffffUL || wide[1] != 0) return 8;
                if (counter != 3 || calls[0] != 1 || calls[1] != 2 || calls[2] != 3) return 9;
                if (after != 0x7777777766666666L || before != 0x1234567887654321L) return 10;
                if ((unsigned long)longs % 8 != 0 || (unsigned long)words % 4 != 0) return 11;
                unsigned char *p = &bytes[0];
                if (bytes != p || &bytes[8] - bytes != 8L || sizeof(bytes + 0) != 8) return 12;
                return 31;
            }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(31));
}

#[test]
fn recursive_array_objects_and_shadowed_lengths_remain_independent() {
    let source = "
            int recurse(int depth) {
                int values[3] = {depth, depth + 1};
                int result = depth > 0 ? recurse(depth - 1) : 0;
                return result + values[0] + values[1] + values[2];
            }
            int length(unsigned char bytes[7]) { return sizeof(bytes); }
            int main(void) {
                unsigned char bytes[13] = {0};
                unsigned long count = sizeof(bytes);
                {
                    unsigned long bytes[2] = {9};
                    if (sizeof(bytes) != 16 || bytes[1] != 0) return 1;
                    count += sizeof(bytes);
                }
                if (sizeof(bytes) != 13 || count != 29 || length(bytes) != 8) return 2;
                int n = 0;
                while (n < 3) {
                    int reset[2] = {n};
                    if (reset[1] != 0) return 3;
                    reset[1] = 77;
                    ++n;
                }
                return recurse(4);
            }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(25));
}

#[test]
fn array_bounds_use_lp64_types_instead_of_legacy_constant_rules() {
    for (bound, expected) in [
        ("3", 3),
        ("2 + 3 * 4", 14),
        ("sizeof(long)", 8),
        ("sizeof(unsigned char) + sizeof(int *)", 9),
        ("(0xffffffffU + 2U)", 1),
        ("(0xffffffffU >> 31)", 1),
        ("(0xffffffffffffffffUL >> 63)", 1),
        ("(1UL << 40) / (1UL << 39)", 2),
        ("((unsigned char)258)", 2),
        ("(-7L / 3L + 5)", 3),
        ("(-7L % 3L + 5)", 4),
        ("(~0UL & 7)", 7),
        ("((0xffffffffU < 0L) ? 9 : 2)", 2),
        ("((0xffffffffU < -1L) ? 9 : 2)", 2),
        ("((0xffffffffU < -1) ? 9 : 2)", 2),
        ("(0 ? 1 / 0 : 3)", 3),
        ("((0 && 1 / 0) + 2)", 2),
        ("((1 || 1 / 0) + 2)", 3),
        ("(9 & 7)", 1),
        ("(9 ^ 7)", 14),
        ("(8 | 7)", 15),
        ("(!0 + 1)", 2),
        ("(-8 >> 2) + 5", 3),
    ] {
        let (machine, _) = execute(&format!(
            "int main(void) {{ unsigned char bytes[{bound}]; return sizeof(bytes); }}"
        ));
        assert_eq!(machine.core().register(10), Some(expected), "{bound}");
    }
}

#[test]
fn array_capacity_counts_bytes_and_rejects_oversized_frames() {
    for (ty, length, size) in [
        ("unsigned char", 960, 960),
        ("int", 240, 960),
        ("unsigned int", 240, 960),
        ("long", 120, 960),
        ("unsigned long", 120, 960),
    ] {
        let (machine, _) = execute(&format!(
            "int main(void) {{ {ty} a[{length}]; a[0] = 7; a[{length}-1] = 11;
                    return a[0] + a[{length}-1] + sizeof(a); }}"
        ));
        assert_eq!(machine.core().register(10), Some(size + 18));
        let error = compile_to_assembly(
            &format!("int main(void) {{ {ty} a[{length}+1]; return 0; }}"),
            "array-capacity.c",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("120 local/parameter slots"), "{error}");
    }
    let (machine, _) = execute_with_budget(
        "int main(void) { unsigned char a[960] = {7}; return a[0] + a[959]; }",
        20000,
    );
    assert_eq!(machine.core().register(10), Some(7));
}

#[test]
fn invalid_array_declarations_initializers_and_uses_diagnose() {
    for body in [
        "int a[0];",
        "int a[-1];",
        "int a[2][3];",
        "int **a[2];",
        "int a[2] = {1, 2, 3};",
        "int a[2] = 1;",
        "int a[2] = {};",
        "int a[2] = {{1, 2}};",
        "int a[];",
        "int a[] = {};",
        "int a[2] = {1, missing};",
        "int a[2]; int b[2]; a = b;",
        "int a[2]; ++a;",
        "int a[2]; a++;",
        "int a[2]; a += 1;",
        "int a[2]; sizeof(&a);",
        "int n = 2; int a[n];",
        "unsigned char a[1 << 32];",
        "unsigned char a[1UL << 64];",
        "unsigned char a[1 << -1];",
        "unsigned char a[(-1 << 1) + 3];",
        "unsigned char a[1 / 0];",
        "unsigned char a[1 % 0];",
        "unsigned char a[2147483647 + 1];",
        "unsigned char a[(-2147483647 - 1) / -1];",
        "unsigned char a[(-2147483647 - 1) % -1 + 1];",
        "unsigned char a[(-9223372036854775807L - 1L) / -1L];",
        "unsigned char a[(-9223372036854775807L - 1L) % -1L + 1];",
        "unsigned long a[18446744073709551615UL];",
    ] {
        let source = format!("int main(void) {{ {body} return 0; }}");
        let error = compile_to_assembly(&source, "array-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("array-invalid.c:"), "{body}: {error}");
    }
}

#[test]
fn addressable_stack_objects_survive_recursion_and_execute_through_cli() {
    let source = include_str!("fixtures/kiuas16_objects.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, [b'S', 10, 255, b'\n']);
    assert_eq!(machine.core().register(10), Some(14));
    let mut bytes = [0; 9];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(&bytes[..8], &10_u64.to_le_bytes());
    assert_eq!(bytes[8], 255);

    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn aliased_scalar_locals_use_declared_width_after_pointer_writes() {
    let source = "
        int set(int *s, unsigned int *u, unsigned char *b, long *l, unsigned long *w) {
            *s = -7;
            *u = 0x80000001U;
            *b = 255;
            *l = -4294967297L;
            *w = 0xfedcba9876543210UL;
            return 0;
        }
        int main(void) {
            int s = 1;
            unsigned int u = 0;
            unsigned char b = 0;
            long l = 0;
            unsigned long w = 0;
            set(&s, &u, &b, &l, &w);
            if (s != -7 || u != 0x80000001U || b != 255) return 1;
            if (l != -4294967297L || w != 0xfedcba9876543210UL) return 2;
            if (s++ != -7 || ++s != -5) return 3;
            if (u++ != 0x80000001U || ++u != 0x80000003U) return 4;
            if (b++ != 255 || ++b != 1) return 5;
            if (l++ != -4294967297L || ++l != -4294967295L) return 6;
            if (w++ != 0xfedcba9876543210UL || ++w != 0xfedcba9876543212UL) return 7;
            int *p = &s;
            *p = 11;
            if (&*p != p || *&s != 11) return 8;
            return s;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(11));
}

#[test]
fn address_of_subscript_evaluates_once_without_loading_the_object() {
    let source = "
        unsigned long *base(int *count) {
            ++*count;
            return (unsigned long *)0x80000000UL;
        }
        int index(int *count) { ++*count; return 3; }
        int main(void) {
            int count = 0;
            unsigned long *p = &base(&count)[index(&count)];
            if (count != 2 || (unsigned long)p != 0x80000018UL) return 1;
            unsigned long *null = (unsigned long *)0;
            if (&*null != null) return 2;
            volatile unsigned int *unmapped = (volatile unsigned int *)0x12345000;
            if (&*unmapped != unmapped) return 3;
            return 17;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(17));
}

#[test]
fn lp64_sizeof_is_unsigned_long_and_never_evaluates_its_operand() {
    let source = "
        int side(int *n) { ++*n; return 1; }
        void nothing(void) { return; }
        int main(void) {
            int n = 0;
            int *p = &n;
            unsigned long sizes = sizeof(unsigned char) + sizeof(int)
                + sizeof(unsigned int) + sizeof(long) + sizeof(unsigned long)
                + sizeof(int *) + sizeof(volatile unsigned long *);
            if (sizes != 41UL || sizeof(n) != 4UL || sizeof(p) != 8UL) return 1;
            if (sizeof(++n) != 4 || sizeof(side(&n)) != 4) return 2;
            if (sizeof(*((long *)0)) != 8 || sizeof(*p++) != 4) return 3;
            if (sizeof(&n) != 8 || sizeof((unsigned char)1) != 1) return 4;
            if (sizeof((unsigned char)1 + (unsigned char)2) != 4) return 5;
            if (sizeof(1) - 5 > 0UL && n == 0 && p == &n) return 23;
            return 6;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(23));
}

#[test]
fn addressable_parameters_and_volatile_qualification_work_across_calls() {
    let source = "
        int set(volatile int *p) { *p = -3; return *p; }
        int args(int a, int b, int c, int d, int e, int f, int g, int h, int ninth) {
            set(&a);
            set(&ninth);
            return a + ninth + b + c + d + e + f + g + h;
        }
        int main(void) {
            int n = 0;
            int *p = &n;
            volatile int *v = p;
            if (v != p || p != v || v - p != 0L) return 1;
            if ((1 ? v : p) != p || (0 ? p : v) != v) return 2;
            int *explicit = (int *)v;
            if (explicit != p || (volatile int *)p != v) return 3;
            if (args(99, 2, 3, 4, 5, 6, 7, 8, 99) != 29) return 4;
            return set(p) == -3 && n == -3;
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(1));
}

#[test]
fn invalid_address_of_sizeof_and_pointer_qualifications_diagnose() {
    let cases = [
        "int main(void) { return (long)&1; }",
        "int main(void) { int n = 0; return (long)&(n + 1); }",
        "int main(void) { int n = 0; return (long)&n++; }",
        "int main(void) { int **p; return (long)&p; }",
        "int main(void) { return sizeof(void); }",
        "void f(void) { return; } int main(void) { return sizeof(f()); }",
        "int main(void) { return sizeof(missing); }",
        "int main(void) { return sizeof(++1); }",
        "int main(void) { return sizeof(float); }",
        "int main(void) { int n; volatile int *v = &n; int *p = v; return 0; }",
        "int main(void) { int n; unsigned int *p = &n; return 0; }",
        "int main(void) { int n; long *p = &n; return 0; }",
        "int main(void) { int *p = 1; return 0; }",
    ];
    for source in cases {
        let error = compile_to_assembly(source, "objects-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("objects-invalid.c"), "{source}: {error}");
    }
}

#[test]
fn typed_pointer_offsets_differences_comparisons_and_casts_execute() {
    let cases = [
        ("return (unsigned long)(p + 3) == 0x8000000cUL;", 1),
        ("return (unsigned long)(3 + p) == 0x8000000cUL;", 1),
        ("return (unsigned long)(p - 3) == 0x7ffffff4UL;", 1),
        ("return (unsigned long)(p + -2) == 0x7ffffff8UL;", 1),
        ("return (unsigned long)(p - -2) == 0x80000008UL;", 1),
        ("return (p + 7) - p;", 7),
        ("return p - (p + 7);", -7),
        ("return p == p + 0;", 1),
        ("return p != p + 1;", 1),
        ("return p < p + 1;", 1),
        ("return p <= p;", 1),
        ("return p > p - 1;", 1),
        ("return p >= p;", 1),
        ("if (p) return 7; return 0;", 7),
        ("return !p;", 0),
        ("return !(volatile int *)0;", 1),
        ("return p == (volatile int *)0;", 0),
        ("return (unsigned int)p == 0x80000000U;", 1),
        ("return (long)p == 0x80000000L;", 1),
        ("return (unsigned char)(p + 3);", 12),
        (
            "return (unsigned long)(p + 4294967295U) == 0x47ffffffcUL;",
            1,
        ),
        ("return (p + 4294967295U) - p == 4294967295L;", 1),
        ("return (volatile int *)0x8000000000000000UL > p;", 1),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!(
            "int main(void) {{ volatile int *p = (volatile int *)0x80000000UL; {body} }}"
        ));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }
}

#[test]
fn pointer_indexing_uses_element_width_and_evaluates_operands_once() {
    let source = "
        volatile long *base(volatile unsigned int *count) {
            *count = *count + 1;
            return (volatile long *)0x80000010UL;
        }
        int index(volatile unsigned int *count) {
            *count = *count + 1;
            return -1;
        }
        int main(void) {
            volatile unsigned int *count = (volatile unsigned int *)0x80000000UL;
            volatile long *p = (volatile long *)0x80000010UL;
            volatile unsigned char *b = (volatile unsigned char *)0x80000020UL;
            *count = 0;
            base(count)[index(count)] = 0x1234567887654321L;
            if (*count != 2) return 1;
            if (p[-1] != 0x1234567887654321L) return 2;
            1[b] = 258;
            if (*(b + 1) != 2) return 3;
            b[0] = 255;
            if ((b + 2) - b != 2L) return 4;
            return b[0] + 1[b];
        }";
    let (mut machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(257));
    let mut bytes = [0; 40];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(&bytes[..4], &2_u32.to_le_bytes());
    assert_eq!(&bytes[8..16], &0x1234567887654321_u64.to_le_bytes());
    assert_eq!(&bytes[32..34], &[255, 2]);
    assert_eq!(&bytes[16..32], &[0; 16]);
}

#[test]
fn independently_assembled_caller_checks_pointer_width_stride_and_difference() {
    let source = "
        volatile unsigned long *advance(volatile unsigned long *p, int offset) {
            return p + offset;
        }
        long distance(volatile unsigned long *p, volatile unsigned long *q) {
            return p - q;
        }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "pointer-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Lpointer_entry\n", 1);
    assembly.push_str(
        ".Lpointer_entry:
         addi sp, sp, -16
         sd ra, 0(sp)
         li s0, 77
         li a0, 1
         slli a0, a0, 40
         addi a0, a0, 64
         li a1, -3
         call advance
         mv s2, a0
         li a1, 1
         slli a1, a1, 40
         addi a1, a1, 64
         call distance
         mv s3, a0
         ld ra, 0(sp)
         addi sp, sp, 16
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 3000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(77));
    assert_eq!(machine.core().register(18), Some((1_u64 << 40) + 40));
    assert_eq!(machine.core().register(19), Some((-3_i64) as u64));
}

#[test]
fn compiled_pointer_buffer_copy_reports_checksum_and_exact_memory() {
    let source = include_str!("fixtures/kiuas16_memory.c");
    let (mut machine, output) = execute_with_budget(source, 100000);
    assert_eq!(output, [b'P', 0x80, 0x7f, b'\n']);
    assert_eq!(machine.core().register(10), Some(256));
    let mut source_bytes = [0; 256];
    let mut copied_bytes = [0; 256];
    let mut words = [0; 8];
    machine
        .bus_mut()
        .read(0x80000000, &mut source_bytes)
        .unwrap();
    machine
        .bus_mut()
        .read(0x80000200, &mut copied_bytes)
        .unwrap();
    machine.bus_mut().read(0x80000400, &mut words).unwrap();
    let expected: Vec<u8> = (0..256).map(|i| i as u8).collect();
    assert_eq!(source_bytes.as_slice(), expected);
    assert_eq!(copied_bytes, source_bytes);
    assert_eq!(&words[..4], &32640_u32.to_le_bytes());
    assert_eq!(&words[4..], &256_u32.to_le_bytes());

    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "100000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn invalid_pointer_operations_have_source_located_diagnostics() {
    for body in [
        "return p + p;",
        "return 1 - p;",
        "return p * 2;",
        "return p >> 1;",
        "return p - b;",
        "return p == b;",
        "return p < 1;",
        "return p[ b ];",
        "return 1[2];",
        "p = b; return 0;",
        "p = 1; return 0;",
        "int n = p; return n;",
        "return (p + 1) = p;",
        "return &p;",
    ] {
        let source = format!(
            "int main(void) {{
                volatile int *p = (volatile int *)0x80000000UL;
                volatile unsigned char *b = (volatile unsigned char *)0x80000020UL;
                {body}
            }}"
        );
        let diagnostic = compile_to_assembly(&source, "pointer.c")
            .unwrap_err()
            .to_string();
        assert!(diagnostic.contains("pointer.c:"), "{body}: {diagnostic}");
    }
}

#[test]
fn typed_integer_promotions_conversions_and_signed_comparisons_execute() {
    let cases = [
        ("return -1 < 0;", 1),
        ("return -1L > 0L;", 0),
        ("return 4L >= 4;", 1),
        ("return 5 != 5L;", 0),
        ("return 5 == 5L;", 1),
        ("return ~0;", -1),
        ("return !0L;", 1),
        ("return !7L;", 0),
        ("unsigned char a = 255; return a + 1;", 256),
        ("unsigned char a = 258; return a;", 2),
        ("long a = 2147483647L + 1L; return a > 2147483647L;", 1),
        ("long a = 2147483647L + 1L; return (int)a < 0;", 1),
        ("return (3 & 1) | (4 ^ 2);", 7),
        ("int a = 7; { int a = 8; a = a + 2; } return a;", 7),
        ("int a = 3; return a = 9;", 9),
        ("return +7 * -3;", -21),
        ("if (0) return 1; else return 2;", 2),
        ("int a = 1; while (0) { a = 9; } return a;", 1),
        ("unsigned char a = 255; return -a;", -255),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }
}

#[test]
fn compound_assignments_and_increment_results_execute() {
    let cases = [
        ("int n = 3; return n++ + 10;", 13),
        ("int n = 3; return ++n + 10;", 14),
        ("int n = 3; int a = n--; return a * 10 + n;", 32),
        ("int n = 3; int a = --n; return a * 10 + n;", 22),
        ("unsigned char n = 255; int old = n++; return old + n;", 255),
        ("unsigned char n = 0; return --n;", 255),
        ("unsigned int n = 0xffffffffU; return ++n;", 0),
        ("unsigned char n = 250; return n += 10;", 4),
        ("int n = -17; return n /= 5;", -3),
        ("int n = -17; return n %= 5;", -2),
        ("unsigned int n = 0xffffffffU; return (n >>= 31);", 1),
        ("int n = -4; return n >>= 1UL;", -2),
        ("unsigned char n = 128; return n <<= 1;", 0),
        ("long n = -1; return (n += 2U);", 1),
        (
            "unsigned int n = 0xffffffffU; return (n /= 2UL) == 2147483647U;",
            1,
        ),
        (
            "unsigned long n = 0xffffffffffffffffUL; return (n %= 16UL);",
            15,
        ),
        ("int n = -17; return (n /= 5U) == 858993455U;", 1),
        (
            "int a = 1; int b = 2; a += (b *= 3); return a * 10 + b;",
            76,
        ),
        (
            "int n = 7; n *= 3; n -= 2; n &= 15; n ^= 3; return n |= 8;",
            8,
        ),
        (
            "volatile int *p = (volatile int *)0x80000000UL; volatile int *old = p++; return p - old;",
            1,
        ),
        (
            "volatile long *p = (volatile long *)0x80000010UL; --p; p += 3; p -= 1; return (unsigned long)p == 0x80000018UL;",
            1,
        ),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }
}

#[test]
fn compound_destinations_and_rhs_are_evaluated_once_across_calls() {
    let (mut machine, output) = execute(
        "volatile unsigned char *target(void) {
             *(volatile unsigned char *)0x10000000 = 65;
             return (volatile unsigned char *)0x80000000UL;
         }
         int rhs(void) {
             *(volatile unsigned char *)0x10000000 = 66;
             return 10;
         }
         int main(void) {
             volatile unsigned char *p = (volatile unsigned char *)0x80000000UL;
             *p = 250;
             int a = (*target() += rhs());
             int b = (*target())++;
             int c = ++(*target());
             return a * 100 + b * 10 + c;
         }",
    );
    assert_eq!(output, b"ABAA");
    assert_eq!(machine.core().register(10), Some(446));
    let mut bytes = [0; 2];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(bytes, [6, 0]);
    let (machine, _) = execute(
        "int main(void) {
             volatile int *p = (volatile int *)0x80000000UL;
             p[0] = 4; p[1] = 7;
             int i = 0;
             p[i++] += 3;
             if (i != 1) return 1;
             if (p[0] != 7) return 2;
             *p++ += 5;
             return *p + p[-1];
         }",
    );
    assert_eq!(machine.core().register(10), Some(19));
}

#[test]
fn update_guest_executes_through_production_cli_and_preserves_memory() {
    let source = include_str!("fixtures/kiuas16_updates.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, [138]);
    assert_eq!(machine.core().register(10), Some(138));
    let mut bytes = [0; 16];
    machine.bus_mut().read(0x80000100, &mut bytes).unwrap();
    let mut expected = std::array::from_fn::<_, 16, _>(|i| i as u8);
    expected[3] = 21;
    assert_eq!(bytes, expected);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, [138]);
}

#[test]
fn invalid_update_operands_diagnose() {
    for body in [
        "return 1++;",
        "return ++1;",
        "return (1 + 2) += 3;",
        "return p *= 2;",
        "return p += p;",
        "return p <<= 1;",
        "return p -= p;",
        "return n += p;",
        "return f()++;",
        "return ++f();",
        "return n += f();",
    ] {
        let source = format!(
            "void f(void) {{}} int main(void) {{
                volatile int *p = (volatile int *)0x80000000UL;
                int n = 1; {body}
            }}"
        );
        let diagnostic = compile_to_assembly(&source, "update.c")
            .unwrap_err()
            .to_string();
        assert!(diagnostic.contains("update.c:"), "{body}: {diagnostic}");
    }
}

#[test]
fn volatile_update_reads_uart_data_only_once() {
    let source = "
        int main(void) {
            volatile unsigned char *uart = (volatile unsigned char *)0x10000000;
            *uart += 0;
            *uart = *uart;
            return 0;
        }";
    let assembly = compile_to_assembly(source, "uart-update.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, b"XY", &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(output, b"XY");
}

#[test]
fn nested_and_recursive_stack_calls_execute_through_production_tools() {
    let source = include_str!("fixtures/kiuas16_stack.c");
    let (machine, output) = execute(source);
    assert_eq!(machine.core().register(10), Some(139));
    assert_eq!(output, [139]);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, [139]);
}

#[test]
fn independent_caller_supplies_mixed_width_stack_parameters() {
    let source = "
        long stack(long a, long b, long c, long d, long e, long f, long g, long h,
                   unsigned int i, int j, unsigned char k, volatile long *p) {
            if (a+b+c+d+e+f+g+h != 36L) return 1;
            if (i != 4294967295L) return 2;
            if (j != -17L) return 3;
            if (k != 2) return 4;
            if ((unsigned long)p != 0x10000000000UL) return 5;
            return 77;
        }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "stack-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Lstack_entry\n", 1);
    assembly.push_str(
        ".Lstack_entry:
         addi sp, sp, -48
         sd ra, 32(sp)
         li s0, 123
         li s1, 456
         li t0, -1
         sd t0, 0(sp)
         li t0, -17
         sd t0, 8(sp)
         li t0, 2
         sd t0, 16(sp)
         li t0, 1
         slli t0, t0, 40
         sd t0, 24(sp)
         li a0, 1
         li a1, 2
         li a2, 3
         li a3, 4
         li a4, 5
         li a5, 6
         li a6, 7
         li a7, 8
         call stack
         ld ra, 32(sp)
         addi sp, sp, 48
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    for (register, value) in [(2, 0x80010000), (8, 123), (9, 456), (10, 77)] {
        assert_eq!(machine.core().register(register), Some(value));
    }
}

#[test]
fn independent_callee_checks_stack_offsets_extension_and_alignment() {
    for count in [9, 10, 11, 12, 31, 32] {
        let parameters = (0..count)
            .map(|i| format!("long p{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let arguments = (0..count)
            .map(|i| format!("twice({})", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!(
            "long probe({parameters}) {{ return 0; }}
             long twice(long n) {{ return n * 2; }}
             int main(void) {{ return 3 + probe({arguments}); }}"
        );
        let mut assembly = compile_to_assembly(&source, "stack.c").unwrap().replacen(
            "\nprobe:\n",
            "\n.Lunused_stack_probe:\n",
            1,
        );
        assembly.push_str("probe:\nandi t0, sp, 15\nbne t0, zero, .Lstack_bad\n");
        for i in 0..count {
            assembly.push_str(&format!("li t0, {}\n", (i + 1) * 2));
            if i < 8 {
                assembly.push_str(&format!("bne a{i}, t0, .Lstack_bad\n"));
            } else {
                assembly.push_str(&format!(
                    "ld t1, {}(sp)\nbne t1, t0, .Lstack_bad\n",
                    (i - 8) * 8
                ));
            }
        }
        assembly.push_str("li a0, 99\nret\n.Lstack_bad:\nli a0, -99\nret\n");
        let rom = assemble_boot_rom(&assembly).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0, "{count}");
        assert!(report.waiting > 0, "{count}");
        assert_eq!(machine.core().register(10), Some(102), "{count}");
        assert_eq!(machine.core().register(2), Some(0x80010000), "{count}");
    }

    let source = "
        unsigned int probe(long a, long b, long c, long d, long e, long f, long g, long h, unsigned int i) { return 0; }
        int main(void) { return probe(1,2,3,4,5,6,7,8,0x80000000U) == 0xffffffffU; }";
    let mut assembly = compile_to_assembly(source, "extension.c")
        .unwrap()
        .replacen("\nprobe:\n", "\n.Lunused_extension:\n", 1);
    assembly.push_str("probe:\nld t0, 0(sp)\nli t1, -2147483648\nbne t0, t1, .Lextension_bad\nli a0, -1\nret\n.Lextension_bad:\nli a0, 0\nret\n");
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert_eq!(machine.core().register(10), Some(1));
}

#[test]
fn scalar_parameter_limit_is_explicit_and_maximum_executes() {
    let parameters = (0..32)
        .map(|i| format!("long p{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let arguments = (0..32)
        .map(|i| format!("{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let (machine, _) = execute(&format!(
        "long last({parameters}) {{ return p31 + p8; }}
         int main(void) {{ return last({arguments}); }}"
    ));
    assert_eq!(machine.core().register(10), Some(39));
    let diagnostic = compile_to_assembly(
        &format!(
            "long last({parameters}, long extra) {{ return extra; }}
         int main(void) {{ return 0; }}"
        ),
        "limit.c",
    )
    .unwrap_err()
    .to_string();
    assert!(diagnostic.contains("32 scalar parameters"), "{diagnostic}");
}

#[test]
fn logical_and_conditional_expressions_sequence_and_skip_side_effects() {
    let cases = [
        ("return 0 && (1 / 0);", 0),
        ("return 7 && 9;", 1),
        ("return -1L || 0;", 1),
        ("return 0UL || 0;", 0),
        ("return (volatile int *)0 && *(volatile int *)0;", 0),
        ("return (volatile int *)1 || *(volatile int *)0;", 1),
        ("int n = 0; int a = 0 && (n = 8); return n + a;", 0),
        ("int n = 0; int a = 1 || (n = 8); return n + a;", 1),
        (
            "int n = 0; int a = (n = 2) && (n = n + 3); return n + a;",
            6,
        ),
        ("int n = 0; int a = (n = 0) || (n = 7); return n + a;", 8),
        ("int n = 0; return (0 ? (n = 3) : (n = 9)) + n;", 18),
        ("int n = 0; return (1 ? (n = 3) : (n = 9)) + n;", 6),
        ("int n = 0; int a = (n = n + 1) ? 5 : 6; return n + a;", 6),
        ("return 1 ? 2 : 0 ? 3 : 4;", 2),
        ("return 0 ? 2 : 0 ? 3 : 4;", 4),
        ("return 0 ? 1 : 7 && 3;", 1),
        ("return (1 ? -1 : 1U) > 0;", 1),
        ("return (0 ? 1U : -1L) < 0;", 1),
        (
            "return (1 ? (unsigned char)255 : (unsigned char)1) + 1;",
            256,
        ),
        (
            "return *(0 ? (volatile int *)0 : (volatile int *)0x80000000UL);",
            0,
        ),
        ("return ((volatile int *)0 ? 1 : 4);", 4),
        ("return 3 * (0 || 9) + (1 && 2);", 4),
        ("return 1 + (0 ? 2L : 4294967295U) == 4294967296L;", 1),
        ("int n = 0; return (1 || (0 && (n = 9))) + n;", 1),
        ("int n = 0; return (0 && (1 || (n = 9))) + n;", 0),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }

    let (machine, output) = execute(
        "int mark(int n) { *(volatile unsigned char *)0x10000000 = n; return n; }
         void emit(int n) { *(volatile unsigned char *)0x10000000 = n; }
         int main(void) {
             int a = mark(65) && mark(66);
             int b = mark(67) || mark(68);
             int c = mark(69) ? mark(70) : mark(71);
             0 ? emit(72) : emit(73);
             return a + b + c;
         }",
    );
    assert_eq!(output, b"ABCEFI");
    assert_eq!(machine.core().register(10), Some(72));
}

#[test]
fn structured_loops_target_continue_and_break_and_preserve_scope() {
    let cases = [
        (
            "int n = 0; for (;;) { n = n + 1; if (n == 3) break; } return n;",
            3,
        ),
        ("int n = 0; do { n = n + 1; } while (0); return n;", 1),
        (
            "int n = 0; do { n = n + 1; continue; n = 9; } while (n < 3); return n;",
            3,
        ),
        (
            "int n = 0; while (n < 3) { n = n + 1; continue; n = 9; } return n;",
            3,
        ),
        ("int n = 7; do { break; n = 9; } while (1); return n;", 7),
        (
            "int i; int n = 0; for (i = 0; i < 5; i = i + 1) { if (i == 2) continue; n = n + i; } return n;",
            8,
        ),
        (
            "int n = 0; for (int i = 0; i < 3; i = i + 1) { n = n + i; } return n;",
            3,
        ),
        (
            "int i = 7; for (int i = 0; i < 2; i = i + 1) {} return i;",
            7,
        ),
        (
            "int i = 7; for (int i = 0; i < 2; i = i + 1) { int i = 9; } return i;",
            7,
        ),
        ("int i; for (i = 0; 0; i = 9) {} return i;", 0),
        (
            "int i = 0; int n = 0; for (; i < 3;) { i = i + 1; n = n + i; } return n;",
            6,
        ),
        (
            "int i = 0; int n = 0; while ((i = i + 1) < 4) { for (;;) { n = n + 1; break; } continue; } return n;",
            3,
        ),
        (
            "int n = 0; for (int i = 0; i < 2; i = i + 1) { for (int j = 0; j < 3; j = j + 1) { if (j == 1) continue; n = n + 1; } } return n;",
            4,
        ),
        ("for (;;) { return 12; }", 12),
        (
            "int n = 0; int updates = 0; for (; (n = n + 1) < 4; updates = updates + 1) { continue; } return n * 10 + updates;",
            43,
        ),
        ("int n = 0; for (; 1; n = 9) { break; } return n;", 0),
        (
            "int n = 0; do { continue; } while ((n = n + 1) < 3); return n;",
            3,
        ),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(machine.core().register(10), Some(expected), "{body}");
    }
}

#[test]
fn control_flow_errors_diagnose_even_in_unevaluated_paths() {
    for body in [
        "break;",
        "continue;",
        "if (0) break;",
        "while (0) {} break;",
        "for (int i = 0; i < 1; i = i + 1) {} return i;",
        "return 0 && unknown;",
        "return 1 || unknown;",
        "return 1 ? 2 : unknown;",
        "return 0 ? unknown : 2;",
        "return 1 ? 1 : (volatile int *)0;",
        "return 1 ? (volatile int *)0 : (volatile long *)0;",
        "return f() && 1;",
        "return 1 ? f() : 1;",
        "return f() ? 1 : 2;",
        "for (; f();) {}",
        "do {} while (f());",
        "for (int i = 0; i < 1; unknown()) {}",
        "for (int i = 0; i < 1; hidden = 9) { int hidden = 0; }",
        "while (0) { unknown(); }",
        "for (; 0;) { unknown(); }",
    ] {
        let source = format!("void f(void) {{}} int main(void) {{ {body} }}");
        let diagnostic = compile_to_assembly(&source, "control.c")
            .unwrap_err()
            .to_string();
        assert!(diagnostic.contains("control.c:"), "{body}: {diagnostic}");
    }
}

#[test]
fn compiled_polling_and_nested_loop_guest_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_control.c");
    let (mut machine, output) = execute(source);
    assert_eq!(output, [b'L', 18, b'\n']);
    assert_eq!(machine.core().register(10), Some(18));
    let mut status = [0; 4];
    machine.bus_mut().read(0x80000000, &mut status).unwrap();
    assert_eq!(status, 1_u32.to_le_bytes());
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn const_pointer_reads_preserve_all_integer_widths_and_qualifier_spellings() {
    for (ty, value, expected) in [
        ("char", "255", 255u64),
        ("signed char", "255", u64::MAX),
        ("unsigned char", "255", 255),
        ("short", "65535", u64::MAX),
        ("unsigned short", "65535", 65535),
        ("int", "-7", (-7i64) as u64),
        ("unsigned int", "0x80000001U", 0x80000001),
        ("long", "-4294967297L", (-4294967297i64) as u64),
        ("unsigned long", "0xfedcba9876543210UL", 0xfedcba9876543210),
    ] {
        for qualified in [
            format!("const {ty}"),
            format!("{ty} const"),
            format!("const volatile {ty}"),
            format!("volatile {ty} const"),
        ] {
            let source = format!(
                "{ty} object[] = {{ [1] = {value} }};
                 {qualified} *view = object + 1;
                 unsigned long read({qualified} *p) {{ return *p; }}
                 int main(void) {{
                     {qualified} *local = view;
                     {qualified} *table[] = {{ [1] = local }};
                     if (table[0] || &*local != view || &table[1][0] != local) return 1;
                     if (sizeof(local) != 8 || sizeof(*local) != sizeof({ty})) return 2;
                     *(volatile unsigned long *)0x80000000UL = read(table[1]);
                     *(volatile unsigned long *)0x80000008UL = (unsigned long)object;
                     return local - object;
                 }}"
            );
            let (mut machine, _) = execute(&source);
            assert_eq!(machine.core().register(10), Some(1), "{source}");
            let mut report = [0; 16];
            machine.bus_mut().read(0x80000000, &mut report).unwrap();
            assert_eq!(
                u64::from_le_bytes(report[..8].try_into().unwrap()),
                expected,
                "{source}"
            );
            let size = match ty {
                "char" | "signed char" | "unsigned char" => 1,
                "short" | "unsigned short" => 2,
                "int" | "unsigned int" => 4,
                _ => 8,
            };
            let mut bytes = vec![0; size * 2];
            machine
                .bus_mut()
                .read(
                    u64::from_le_bytes(report[8..].try_into().unwrap()),
                    &mut bytes,
                )
                .unwrap();
            assert_eq!(&bytes[..size], vec![0; size]);
            assert_eq!(&bytes[size..], &expected.to_le_bytes()[..size]);
        }
    }
}

#[test]
fn const_pointer_tables_aliases_and_generic_qualification_unions_execute() {
    let (machine, output) = execute(
        "typedef const char *text;
         typedef text *texts;
         typedef const volatile char *read_only_mmio;
         typedef volatile char *writable_mmio;
         char bytes[] = \"AB\";
         text table[] = { [2] = bytes + 1, [0] = \"X\" };
         text choose(int n, text a, text b) { return n ? a : b; }
         text ninth(int a,int b,int c,int d,int e,int f,int g,int h,text p) { return p; }
         int main(void) {
             texts cursor = table;
             text p = choose(0, 0, ninth(1,2,3,4,5,6,7,8,cursor[2]));
             const void *generic = p;
             const char *q = generic;
             volatile char *writable = bytes;
             read_only_mmio combined = 1 ? p : writable;
             const volatile void *both = 0 ? generic : writable;
             const volatile char *roundtrip = both;
             writable_mmio alias = writable;
             *alias = 'C';
             cursor[1] = q;
             ++cursor;
             if (cursor - table != 1 || &*cursor != &table[1]) return 1;
             if (*cursor != p || combined != q || *roundtrip != 'C') return 2;
             if (choose(1, 0, p) != 0 || (0 ? p : 0) != 0) return 3;
             *(volatile unsigned char *)0x10000000UL = *table[0];
             *(volatile unsigned char *)0x10000000UL = *cursor[0];
             *(volatile unsigned char *)0x10000000UL = *roundtrip;
             char *explicit_view = (char *)p;
             *explicit_view = 'D';
             return *p;
         }",
    );
    assert_eq!(output, b"XBC");
    assert_eq!(machine.core().register(10), Some(b'D' as u64));
}

#[test]
fn const_volatile_views_skip_unevaluated_reads_and_read_uart_once() {
    let source =
        "int calls;
         const volatile unsigned char *uart(void) { ++calls; return (const volatile unsigned char *)0x10000000UL; }
         int main(void) {
             sizeof(*uart());
             0 && *uart();
             1 || *uart();
             const volatile unsigned char *p = 1 ? uart() : 0;
             unsigned char a = *p++;
             --p;
             unsigned char b = *p;
             *(volatile unsigned char *)0x10000000UL = a;
             *(volatile unsigned char *)0x10000000UL = b;
             return calls;
         }";
    let rom = assemble_boot_rom(&compile_to_assembly(source, "const-uart.c").unwrap()).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, b"ABC", &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(output, b"AB");
    assert_eq!(machine.core().register(10), Some(1));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut next = [0; 1];
    machine.bus_mut().read(0x10000000, &mut next).unwrap();
    assert_eq!(next, *b"C");
}

#[test]
fn const_atomic_loads_execute_without_changing_objects_or_neighbor_bytes() {
    for (ty, value, expected, size) in [
        ("int", "0x80000001U", (-2147483647i64) as u64, 4),
        ("unsigned int", "0x80000001U", 0x80000001, 4),
        ("long", "-4294967297L", (-4294967297i64) as u64, 8),
        (
            "unsigned long",
            "0xfedcba9876543210UL",
            0xfedcba9876543210,
            8,
        ),
    ] {
        let source = format!(
            "{ty} object = {value};
             int main(void) {{
                 const volatile {ty} *p = &object;
                 {ty} old = __atomic_load_n(p, 2);
                 if (old != object) return 1;
                 *(volatile unsigned long *)0x80000000UL = old;
                 *(volatile unsigned long *)0x80000008UL = object;
                 {ty} *mutable = &object;
                 __atomic_store_n(mutable, 7, 3);
                 if (__atomic_load_n(p, 5) != 7) return 2;
                 return object;
             }}"
        );
        let rom =
            assemble_boot_rom(&compile_to_assembly(&source, "const-atomic.c").unwrap()).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        machine.bus_mut().write(0x80010000, &[0xa5; 16]).unwrap();
        let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0);
        assert!(report.waiting > 0);
        assert_eq!(machine.core().register(10), Some(7));
        assert_eq!(machine.core().register(2), Some(0x80010000));
        assert_eq!(machine.core().register(8), Some(0));
        let mut bytes = [0; 16];
        machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
        assert_eq!(&bytes[..8], &expected.to_le_bytes());
        assert_eq!(&bytes[8..], &expected.to_le_bytes());
        machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
        assert_eq!(&bytes[..size], &7u64.to_le_bytes()[..size]);
        assert_eq!(&bytes[size..8], vec![0; 8 - size]);
        assert_eq!(&bytes[8..], &[0xa5; 8]);
    }
}

#[test]
fn const_pointer_recursive_stack_views_and_two_level_returns_execute() {
    let (machine, _) = execute(
        "const int **pass(const int **p) { return p; }
         const int *read(const int **p) { return *p; }
         int recurse(int n, const int *parent) {
             int local = n;
             const int *view = &local;
             const int **table = pass(&view);
             int result = *read(table) + (parent ? *parent : 0);
             if (n) result += recurse(n - 1, view);
             if (view != &local || *view != n) return 99;
             return result;
         }
         int main(void) { return recurse(4, 0); }",
    );
    assert_eq!(machine.core().register(10), Some(20));
}

#[test]
fn const_pointer_register_and_stack_abi_interoperate_with_independent_caller() {
    let mut assembly = compile_to_assembly(
        "const int *probe(int a,int b,int c,int d,int e,int f,int g,int h,const int *p) {
             if (a + b + c + d + e + f + g + h != 36 || *p != -17) return 0;
             return p + 1;
         }
         int main(void) { return 0; }",
        "const-abi.c",
    )
    .unwrap()
    .replacen("call main\n", "call .Lconst_entry\n", 1);
    assembly.push_str(
        ".Lconst_entry:
         addi sp, sp, -32
         sd ra, 24(sp)
         li s0, 123
         li s1, 456
         li t0, 1
         slli t0, t0, 31
         sd t0, 0(sp)
         li t1, -17
         sw t1, 0(t0)
         li a0, 1
         li a1, 2
         li a2, 3
         li a3, 4
         li a4, 5
         li a5, 6
         li a6, 7
         li a7, 8
         call probe
         ld ra, 24(sp)
         addi sp, sp, 32
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    for (register, value) in [(2, 0x80010000), (8, 123), (9, 456), (10, 0x80000004)] {
        assert_eq!(machine.core().register(register), Some(value));
    }
}

#[test]
fn const_writes_qualifier_loss_and_unsupported_objects_preserve_existing_rom() {
    for body in [
        "*p = 1;",
        "p[0] = 1;",
        "*p += 1;",
        "++*p;",
        "(*p)++;",
        "p[0]--;",
        "*(&*p) = 1;",
        "*(&p[0]) = 1;",
        "int *q = p;",
        "int *q = 0; q = p;",
        "consume(p);",
        "int *q = 1 ? p : &object;",
        "int *q = 0 ? p : 0;",
        "void *q = p;",
        "const void *v = p; int *q = v;",
        "volatile int *q = p;",
        "const int **q = &mutable;",
        "int **q = &p;",
        "const volatile int **q = &p;",
        "int **q = 1 ? &p : &mutable;",
        "**(&p) = 1;",
        "sizeof((*p)++);",
        "if (0) *p = 1;",
        "__atomic_store_n(p, 1, 3);",
        "__atomic_exchange_n(p, 1, 2);",
        "const int value = 1; value = 2;",
        "const int a[2] = {1}; a[0] = 2;",
        "int *const q = &object;",
        "sizeof(int *const);",
        "(int *const)&object;",
        "typedef int *ip; const ip q = &object;",
        "typedef int *ip; sizeof(const ip);",
        "typedef int *ip; (const ip)&object;",
        "typedef const int *cp; volatile cp q = p;",
        "const void *v = p; *v;",
        "const void *v = p; v++;",
    ] {
        let source = format!(
            "int object;
             int consume(int *p) {{ return *p; }}
             int main(void) {{
                 int *mutable = &object;
                 const int *p = mutable;
                 {body}
                 return 0;
             }}"
        );
        let diagnostic = compile_to_assembly(&source, "const.c")
            .unwrap_err()
            .to_string();
        assert!(diagnostic.contains("const.c:"), "{body}: {diagnostic}");
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{body}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    for source in [
        "int object; const int *p = &object; int *bad(void) { return p; } int main(void) { return 0; }",
        "int object; const int *p = &object; int *q = p; int main(void) { return 0; }",
        "int object; const int *p = &object; int *a[] = { [0] = p }; int main(void) { return 0; }",
        "const int object = 1; int main(void) { object = 2; return 0; }",
        "typedef int *ip; const ip q; int main(void) { return 0; }",
        "typedef const int *cp; typedef const cp bad; int main(void) { return 0; }",
        "const int *p; extern int *p; int main(void) { return 0; }",
        "int f(const int *p); int f(int *p) { return 0; } int main(void) { return 0; }",
    ] {
        assert!(
            compile_to_assembly(source, "const-global.c").is_err(),
            "{source}"
        );
    }
}

#[test]
fn qualified_objects_preserve_widths_sparse_data_bss_and_stack_bytes() {
    for (ty, value, expected, size) in [
        ("_Bool", "256", 1u64, 1),
        ("char", "255", 255, 1),
        ("signed char", "255", u64::MAX, 1),
        ("unsigned char", "128", 128, 1),
        ("short", "65535", u64::MAX, 2),
        ("unsigned short", "65535", 65535, 2),
        ("int", "0x80000001U", (-2147483647i64) as u64, 4),
        ("unsigned int", "0x80000001U", 0x80000001, 4),
        ("long", "-4294967297L", (-4294967297i64) as u64, 8),
        (
            "unsigned long",
            "0xfedcba9876543210UL",
            0xfedcba9876543210,
            8,
        ),
    ] {
        let source = format!(
            "const {ty} frozen[] = {{ [2] = {value}, [0] = {value} }};
             extern const {ty} frozen[3];
             const volatile {ty} zero[2];
             volatile {ty} state;
             int main(void) {{
                 const {ty} local[] = {{ [2] = {value}, [0] = {value} }};
                 volatile {ty} mutable = {value};
                 const volatile {ty} scalar = {value};
                 const volatile {ty} *view = local;
                 volatile {ty} *write = &mutable;
                 state = *write;
                 if (sizeof(frozen) != 3 * sizeof({ty}) || sizeof(local) != sizeof(frozen)) return 1;
                 if (frozen[0] != scalar || frozen[1] || frozen[2] != state) return 2;
                 if (view[0] != scalar || view[1] || view[2] != state || zero[0] || zero[1]) return 3;
                 *(volatile unsigned long *)0x80000000UL = (unsigned long)frozen;
                 *(volatile unsigned long *)0x80000008UL = (unsigned long)zero;
                 *(volatile unsigned long *)0x80000010UL = (unsigned long)&state;
                 *(volatile unsigned long *)0x80000018UL = (unsigned long)local;
                 *(volatile unsigned long *)0x80000020UL = (unsigned long)write;
                 *(volatile unsigned long *)0x80000028UL = (unsigned long)&scalar;
                 *(volatile unsigned long *)0x80000030UL = scalar;
                 return 42;
             }}"
        );
        let rom = assemble_boot_rom(&compile_to_assembly(&source, "qualified.c").unwrap()).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        machine.bus_mut().write(0x80010000, &[0xa5; 128]).unwrap();
        machine.bus_mut().write(0x8000fc00, &[0xa5; 1024]).unwrap();
        let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0, "{ty}: {:?}", report.last_trap);
        assert!(report.waiting > 0, "{ty}");
        assert_eq!(machine.core().register(10), Some(42), "{ty}");
        assert_eq!(machine.core().register(2), Some(0x80010000));
        assert_eq!(machine.core().register(8), Some(0));
        let mut report = [0; 56];
        machine.bus_mut().read(0x80000000, &mut report).unwrap();
        assert_eq!(
            u64::from_le_bytes(report[48..].try_into().unwrap()),
            expected,
            "{ty}"
        );
        let scalar = expected.to_le_bytes()[..size].to_vec();
        let mut array = scalar.clone();
        array.extend(vec![0; size]);
        array.extend(&scalar);
        for (index, address) in report[..48].chunks_exact(8).enumerate() {
            let address = u64::from_le_bytes(address.try_into().unwrap());
            let mut expected = match index {
                0 | 3 => array.clone(),
                1 => vec![0; size * 2],
                _ => scalar.clone(),
            };
            if index < 3 {
                expected.resize(expected.len().div_ceil(8) * 8, 0);
            } else if index == 3 {
                expected.resize(expected.len().div_ceil(8) * 8, 0xa5);
            }
            let mut actual = vec![0; expected.len()];
            machine.bus_mut().read(address, &mut actual).unwrap();
            assert_eq!(actual, expected, "{ty}, object {index}");
        }
        let bytes = (3 * size).div_ceil(8) * 8 + (2 * size).div_ceil(8) * 8 + 8;
        let mut neighbor = [0; 8];
        machine
            .bus_mut()
            .read(0x80010000 + bytes as u64, &mut neighbor)
            .unwrap();
        assert_eq!(neighbor, [0xa5; 8], "{ty}");
    }
}

#[test]
fn qualified_objects_aliases_casts_and_character_array_bounds_execute() {
    let (machine, output) = execute(
        "typedef const unsigned short word;
         typedef word same;
         typedef volatile unsigned int counter;
         static same value = 65535;
         static char const text[] = \"Q\";
         static volatile unsigned char bytes[] = {\"AB\"};
         static const unsigned short *table[] = { &value, 0 };
         const unsigned short *read(void) { return &value; }
         int main(void) {
             const char local[] = \"C\";
             char const trailing[] = \"D\";
             same copy = *read();
             counter count = 0;
             unsigned short const volatile *both = &copy;
             const volatile void *generic = both;
             const volatile unsigned short *roundtrip = generic;
             if (*roundtrip != 65535 || *table[0] != copy || table[1]) return 1;
             if (sizeof(text) != 2 || sizeof(bytes) != 3 || sizeof(trailing) != 2) return 2;
             if (sizeof(same) != 2 || sizeof(const volatile unsigned int) != 4) return 3;
             if ((const unsigned char)256 != 0 || (volatile _Bool)256 != 1) return 4;
             unsigned int *explicit_view = (unsigned int *)&count;
             *explicit_view = 7;
             if (count++ != 7 || ++count != 9 || (count += 256) != 265) return 5;
             bytes[1] += 1;
             *(volatile unsigned char *)0x10000000UL = text[0];
             *(volatile unsigned char *)0x10000000UL = local[0];
             *(volatile unsigned char *)0x10000000UL = trailing[0];
             *(volatile unsigned char *)0x10000000UL = bytes[1];
             return count;
         }",
    );
    assert_eq!(output, b"QCDC");
    assert_eq!(machine.core().register(10), Some(265));
}

#[test]
fn qualified_object_parameters_recurse_and_ignore_top_level_signature_qualifiers() {
    let (machine, _) = execute(
        "int ninth(int a,int b,int c,int d,int e,int f,int g,int h,unsigned int value);
         const int ninth(const int a,int b,int c,int d,int e,int f,int g,int h,volatile unsigned int value) {
             const int *first = &a;
             volatile unsigned int *last = &value;
             if (*first != 1 || value != 0x80000001U) return 100;
             *last += 1;
             return value == 0x80000002U;
         }
         int recurse(const int n, volatile int value) {
             const int fixed = n;
             volatile int *p = &value;
             *p += 2;
             if (!n) return value;
             return fixed + recurse(n - 1, value);
         }
         int main(void) {
             return ninth(1,2,3,4,5,6,7,8,0x80000001U) + recurse(3, 10);
         }",
    );
    assert_eq!(machine.core().register(10), Some(25));
}

#[test]
fn qualified_object_volatile_aliases_updates_and_unevaluated_reads_execute_once() {
    let source = "volatile unsigned char received;
         int calls;
         unsigned char input(void) { ++calls; return *(volatile unsigned char *)0x10000000UL; }
         int main(void) {
             const volatile unsigned char first = input();
             volatile unsigned char second = input();
             volatile unsigned char *p = &second;
             sizeof(first); sizeof(received); sizeof(second++);
             0 && (second = input());
             1 || (received = input());
             received = first;
             *p += 1;
             unsigned char old = second++;
             unsigned char now = ++second;
             *(volatile unsigned char *)0x10000000UL = received;
             *(volatile unsigned char *)0x10000000UL = old;
             *(volatile unsigned char *)0x10000000UL = now;
             *(volatile unsigned char *)0x10000000UL = second;
             return calls;
         }";
    let assembly = compile_to_assembly(source, "qualified-uart.c").unwrap();
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, b"ABC", &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(output, b"ACEE");
    assert_eq!(machine.core().register(10), Some(2));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut next = [0; 1];
    machine.bus_mut().read(0x10000000, &mut next).unwrap();
    assert_eq!(next, *b"C");
}

#[test]
fn qualified_object_register_and_stack_parameters_match_independent_assembly() {
    let mut assembly = compile_to_assembly(
        "unsigned int probe(const signed char first, volatile short second,
                            int c,int d,int e,int f,int g,int h,
                            const volatile unsigned int ninth) {
             const signed char *a = &first;
             volatile short *b = &second;
             const volatile unsigned int *last = &ninth;
             if (*a != -1 || *b != -128 || *last != 0x80000001U) return 99;
             if (c + d + e + f + g + h != 33) return 98;
             *b += 1;
             if (second != -127) return 97;
             return ninth + 1U;
         }
         int main(void) { return 0; }",
        "qualified-abi.c",
    )
    .unwrap()
    .replacen("call main\n", "call .Lqualified_entry\n", 1);
    assembly.push_str(
        ".Lqualified_entry:
         addi sp, sp, -32
         sd ra, 24(sp)
         li s0, 123
         li s1, 456
         li t0, -2147483647
         sd t0, 0(sp)
         li a0, -1
         li a1, -128
         li a2, 3
         li a3, 4
         li a4, 5
         li a5, 6
         li a6, 7
         li a7, 8
         call probe
         andi t0, sp, 15
         bne t0, zero, .Lqualified_fail
         ld t0, 0(sp)
         li t1, -2147483647
         bne t0, t1, .Lqualified_fail
         j .Lqualified_done
         .Lqualified_fail:
         li a0, 96
         .Lqualified_done:
         ld ra, 24(sp)
         addi sp, sp, 32
         ret",
    );
    let mut machine =
        Kiuas16Machine::new_with_ram_size(assemble_boot_rom(&assembly).unwrap(), 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    for (register, value) in [
        (2, 0x80010000),
        (8, 123),
        (9, 456),
        (10, (-2147483646i64) as u64),
    ] {
        assert_eq!(machine.core().register(register), Some(value));
    }
}

#[test]
fn qualified_object_atomic_addresses_preserve_widths_and_const_loads() {
    for (ty, value, size) in [
        ("int", "-17", 4),
        ("unsigned int", "0x80000001U", 4),
        ("long", "-4294967297L", 8),
        ("unsigned long", "0xfedcba9876543210UL", 8),
    ] {
        let source = format!(
            "const volatile {ty} fixed = {value};
             volatile {ty} shared;
             int main(void) {{
                 const {ty} local = {value};
                 volatile {ty} counter = 0;
                 {ty} first = __atomic_load_n(&fixed, 2);
                 if (__atomic_load_n(&local, 5) != first) return 1;
                 __atomic_store_n(&shared, first, 3);
                 if (__atomic_exchange_n(&counter, shared, 4) != 0) return 2;
                 if (__atomic_load_n(&counter, 2) != fixed) return 3;
                 if (__atomic_fetch_add(&counter, 1, 0) != first) return 4;
                 *(volatile unsigned long *)0x80000000UL = (unsigned long)&shared;
                 *(volatile unsigned long *)0x80000008UL = shared;
                 return 42;
             }}"
        );
        let (mut machine, _) = execute(&source);
        assert_eq!(machine.core().register(10), Some(42), "{ty}");
        let mut report = [0; 16];
        machine.bus_mut().read(0x80000000, &mut report).unwrap();
        let address = u64::from_le_bytes(report[..8].try_into().unwrap());
        let mut bytes = [0; 8];
        machine.bus_mut().read(address, &mut bytes).unwrap();
        assert_eq!(&bytes[..size], &report[8..8 + size], "{ty}");
        assert_eq!(&bytes[size..], &vec![0; 8 - size], "{ty}");
    }
}

#[test]
fn qualified_object_const_mutation_and_qualifier_loss_preserve_existing_rom() {
    for body in [
        "frozen = 1;",
        "++frozen;",
        "frozen--;",
        "frozen += 1;",
        "*(&frozen) = 1;",
        "sizeof(frozen++);",
        "if (0) frozen = 1;",
        "local = 1;",
        "--local;",
        "local *= 2;",
        "items[0] = 2;",
        "++items[0];",
        "items[0]--;",
        "*(&items[0]) = 1;",
        "*items = 1;",
        "if (0) items[0] += 1;",
        "int *p = &frozen;",
        "int *p = &local;",
        "int *p = items;",
        "volatile int *p = &frozen;",
        "const int *p = &state;",
        "const int *p = &both;",
        "const int *p = values;",
        "int *p = &state;",
        "consume(&frozen);",
        "consume(&state);",
        "int *p = 1 ? &local : &state;",
        "__atomic_store_n(&frozen, 1, 0);",
        "__atomic_fetch_add(&local, 1, 0);",
        "__atomic_exchange_n(&both, 1, 0);",
    ] {
        let source = format!(
            "const int frozen = 1;
             volatile int state;
             const volatile int both = 2;
             int consume(int *p) {{ return *p; }}
             int main(void) {{
                 const int local = 3;
                 const int items[2] = {{ 1, 2 }};
                 volatile int values[2];
                 {body}
                 return 0;
             }}"
        );
        let diagnostic = compile_to_assembly(&source, "qualified-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("qualified-invalid.c:"),
            "{body}: {diagnostic}"
        );
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{body}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    for source in [
        "const int x; extern int x; int main(void) { return 0; }",
        "volatile int x; extern int x; int main(void) { return 0; }",
        "const int a[2]; extern const volatile int a[2]; int main(void) { return 0; }",
        "int f(int n) { const int n = 1; return n; } int main(void) { return f(0); }",
        "int f(const int n) { return ++n; } int main(void) { return f(0); }",
        "int f(const int a,int b,int c,int d,int e,int f,int g,int h,const int n) { n = 1; return n; } int main(void) { return 0; }",
        "typedef const int number; int main(void) { number n = 1; n = 2; return n; }",
        "const void x; int main(void) { return 0; }",
        "const int *const p; int main(void) { return 0; }",
        "typedef int *pointer; const pointer p; int main(void) { return 0; }",
        "int main(void) { const int a[2][2]; return 0; }",
    ] {
        assert!(
            compile_to_assembly(source, "qualified-invalid.c").is_err(),
            "{source}"
        );
    }
}

#[test]
fn designated_integer_arrays_preserve_widths_sparse_zeroes_and_inferred_bounds() {
    for (ty, size, last) in [
        ("char", 1, 35u64),
        ("signed char", 1, 35),
        ("unsigned char", 1, 35),
        ("short", 2, 291),
        ("unsigned short", 2, 291),
        ("int", 4, 291),
        ("unsigned int", 4, 291),
        ("long", 8, 291),
        ("unsigned long", 8, 291),
    ] {
        let source = format!(
            "{ty} global[] = {{ [4] = 0x123, [1] = {{7}}, 9 }};
             int main(void) {{
                 {ty} local[] = {{ [4] = 0x123, [1] = {{7}}, 9 }};
                 *(volatile unsigned long *)0x80000000UL = (unsigned long)global;
                 *(volatile unsigned long *)0x80000008UL = (unsigned long)local;
                 return sizeof(global) + sizeof(local);
             }}"
        );
        let rom = assemble_boot_rom(&compile_to_assembly(&source, "sparse.c").unwrap()).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        machine.bus_mut().write(0x80010000, &[0xa5; 64]).unwrap();
        machine.bus_mut().write(0x8000fc00, &[0xa5; 1024]).unwrap();
        let mut output = Vec::new();
        let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
        assert_eq!(report.traps, 0, "{ty}: {:?}", report.last_trap);
        assert!(report.waiting > 0);
        assert!(output.is_empty());
        assert_eq!(machine.core().register(10), Some(10 * size as u64));
        assert_eq!(machine.core().register(2), Some(0x80010000));
        assert_eq!(machine.core().register(8), Some(0));
        let mut addresses = [0; 16];
        machine.bus_mut().read(0x80000000, &mut addresses).unwrap();
        let expected: Vec<_> = [0u64, 7, 9, 0, last]
            .iter()
            .flat_map(|number| number.to_le_bytes()[..size].to_vec())
            .collect();
        for address in addresses.chunks_exact(8) {
            let address = u64::from_le_bytes(address.try_into().unwrap());
            let mut actual = vec![0; expected.len()];
            machine.bus_mut().read(address, &mut actual).unwrap();
            assert_eq!(actual, expected, "{ty}");
        }
        let padded = expected.len().div_ceil(8) * 8;
        let mut neighbor = [0; 8];
        machine
            .bus_mut()
            .read(0x80010000 + padded as u64, &mut neighbor)
            .unwrap();
        assert_eq!(neighbor, [0xa5; 8], "{ty}");
    }
}

#[test]
fn designated_pointer_tables_resolve_addresses_null_holes_and_stack_arguments() {
    let (mut machine, output) = execute(
        "static char bytes[] = \"AB\";
         static char *table[] = { [3] = \"X\", [0] = bytes + 1, 0 };
         char *ninth(int a,int b,int c,int d,int e,int f,int g,int h,char *p) { return p; }
         int main(void) {
             char *local[4] = { [2] = ninth(1,2,3,4,5,6,7,8,table[3]), [0] = bytes };
             void *generic[] = { [2] = local[2], [0] = bytes };
             if (sizeof(table) != 32 || sizeof(generic) != 24) return 1;
             if (table[1] || table[2] || local[1] || local[3] || generic[1]) return 2;
             if (generic[0] != bytes || generic[2] != local[2]) return 3;
             *(volatile unsigned long *)0x80000000UL = (unsigned long)table;
             *(volatile unsigned long *)0x80000008UL = (unsigned long)(bytes + 1);
             *(volatile unsigned char *)0x10000000UL = *table[0];
             *(volatile unsigned char *)0x10000000UL = *local[0];
             *(volatile unsigned char *)0x10000000UL = *local[2];
             return 42;
         }",
    );
    assert_eq!(output, b"BAX");
    assert_eq!(machine.core().register(10), Some(42));
    let mut addresses = [0; 16];
    machine.bus_mut().read(0x80000000, &mut addresses).unwrap();
    let mut entries = [0; 32];
    machine
        .bus_mut()
        .read(
            u64::from_le_bytes(addresses[..8].try_into().unwrap()),
            &mut entries,
        )
        .unwrap();
    assert_eq!(&entries[..8], &addresses[8..]);
    assert_eq!(&entries[8..24], &[0; 16]);
    let mut text = [0; 2];
    machine
        .bus_mut()
        .read(
            u64::from_le_bytes(entries[24..].try_into().unwrap()),
            &mut text,
        )
        .unwrap();
    assert_eq!(text, *b"X\0");
}

#[test]
fn designated_initializers_evaluate_once_in_source_order_across_recursive_calls() {
    let (machine, output) = execute(
        "int mark(int n) { *(volatile unsigned char *)0x10000000UL = n; return n; }
         int recurse(int n) {
             int values[3] = { [2] = n, [0] = n ? recurse(n - 1) : 0 };
             return values[0] + values[1] + values[2];
         }
         int main(void) {
             int values[] = { [4] = mark(65), [0] = mark(66), mark(67) };
             int total = 0;
             for (int i = 0; i < 2; ++i) {
                 int again[3] = { [2] = mark(68 + i), [0] = recurse(4) };
                 total += again[0] + again[1] + again[2];
             }
             if (sizeof(values) != 20 || values[2] || values[3]) return 1;
             return total + values[0] + values[1] + values[4];
         }",
    );
    assert_eq!(output, b"ABCDE");
    assert_eq!(machine.core().register(10), Some(355));
}

#[test]
fn designated_indices_use_lp64_constants_aliases_enumerators_and_continuations() {
    let (machine, _) = execute(
        "typedef unsigned long index_t;
         enum { LAST = 7 };
         unsigned char global[] = { [LAST] = 255, [sizeof(long) - 6] = 17, 18,
             [(index_t)-1 + 1] = 19 };
         int main(void) {
             int local[] = { [(unsigned char)258] = 3, [1 ? 4 : 1/0] = 5, 6,
                 [0 && 1/0] = 7 };
             if (sizeof(global) != 8 || sizeof(local) != 24) return 1;
             if (global[1] || global[4] || global[5] || global[6]) return 2;
             if (local[1] || local[3]) return 3;
             return global[0] + global[2] + global[3] + global[7]
                 + local[0] + local[2] + local[4] + local[5];
         }",
    );
    assert_eq!(machine.core().register(10), Some(330));
}

#[test]
fn designated_pointer_arrays_preserve_pointee_widths_and_volatile_qualification() {
    for ty in [
        "char",
        "signed char",
        "unsigned char",
        "short",
        "unsigned short",
        "int",
        "unsigned int",
        "long",
        "unsigned long",
    ] {
        for qualifier in ["", "volatile "] {
            let source = format!(
                "{ty} object[2] = {{ [1] = 42 }};
                 {qualifier}{ty} *table[] = {{ [2] = object + 1 }};
                 int main(void) {{
                     {qualifier}{ty} *local[] = {{ [2] = table[2] }};
                     if (table[0] || table[1] || local[0] || local[1]) return 1;
                     if (sizeof(table) != 24 || sizeof(local) != 24) return 2;
                     *local[2] += 1;
                     return object[1] + *table[2];
                 }}"
            );
            let (machine, _) = execute(&source);
            assert_eq!(machine.core().register(10), Some(86), "{source}");
        }
    }
}

#[test]
fn designated_array_limits_execute_and_invalid_forms_preserve_existing_rom() {
    let (machine, _) = execute_with_budget(
        "unsigned char full[] = { [57343] = 99 };
         unsigned char tail[8192];
         int main(void) { return sizeof(full) == 57344 && full[0] == 0 && full[57343] == 99 && tail[8191] == 0; }",
        1000000,
    );
    assert_eq!(machine.core().register(10), Some(1));
    let (machine, _) = execute(
        "int main(void) { unsigned char full[] = { [959] = 99 }; return sizeof(full) == 960 && full[0] == 0 && full[959] == 99; }",
    );
    assert_eq!(machine.core().register(10), Some(1));
    for source in [
        "int a[3] = { [3] = 1 }; int main(void) { return 0; }",
        "int a[3] = { [2] = 1, 2 }; int main(void) { return 0; }",
        "int a[] = { [-1] = 1 }; int main(void) { return 0; }",
        "int a[] = { [1 / 0] = 1 }; int main(void) { return 0; }",
        "int a[] = { [65536] = 1 }; int main(void) { return 0; }",
        "int a[] = { [0xffffffffffffffffUL] = 1 }; int main(void) { return 0; }",
        "int a[] = { [2] = 1, [2] = 2 }; int main(void) { return 0; }",
        "int a[] = { [2] = 1, [1] = 2, 3 }; int main(void) { return 0; }",
        "int a[] = { [0][1] = 1 }; int main(void) { return 0; }",
        "int a[] = { .field = 1 }; int main(void) { return 0; }",
        "int a[] = { [0] = { [0] = 1 } }; int main(void) { return 0; }",
        "int a[] = { [0] 1 }; int main(void) { return 0; }",
        "int a[] = { [0] = }; int main(void) { return 0; }",
        "int a = { [0] = 1 }; int main(void) { return 0; }",
        "int *a[] = { [0] = 1 }; int main(void) { return 0; }",
        "int **a[] = { [0] = 0 }; int main(void) { return 0; }",
        "int a[1][1] = { [0] = {1} }; int main(void) { return 0; }",
        "int a[] = { [0] = unknown }; int main(void) { return 0; }",
        "int a[] = { [0] = 1, [0] = unknown }; int main(void) { return 0; }",
        "long a[] = { [8192] = 1 }; int main(void) { return 0; }",
        "int main(void) { int n = 0; int a[] = { [n] = 1 }; return 0; }",
        "int f(void) { return 0; } int main(void) { int a[] = { [f()] = 1 }; return 0; }",
        "int f(void) { return 0; } int main(void) { int a[] = { [1 ? 0 : f()] = 1 }; return 0; }",
        "int main(void) { int n = 0; int a[] = { [1 ? 0 : n] = 1 }; return 0; }",
        "int main(void) { int n = 0; int a[] = { [sizeof(n)] = 1 }; return 0; }",
        "int main(void) { unsigned char a[] = { [960] = 1 }; return 0; }",
        "int main(void) { if (0) { int a[1] = { [1] = 1 }; } return 0; }",
        "int main(void) { int a[] = { [0] = 1, [0] = 2 }; return 0; }",
        "int main(void) { volatile int *a[] = { [0] = (volatile long *)0 }; return 0; }",
    ] {
        let diagnostic = compile_to_assembly(source, "designator.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("designator.c:"),
            "{source}: {diagnostic}"
        );
        let files = Files::new(source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(!build.stderr.is_empty(), "{source}");
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    let files = Files::new(
        "unsigned char full[] = { [65535] = 99 }; int main(void) { return full[65535]; }",
    );
    std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
    assert!(!files.compile().status.success());
    assert_eq!(
        std::fs::read(files.0.join("rom.bin")).unwrap(),
        b"previous ROM"
    );
}

#[test]
fn null_pointer_constants_convert_all_supported_pointer_categories() {
    for ty in [
        "char *",
        "signed char *",
        "unsigned char *",
        "short *",
        "unsigned short *",
        "int *",
        "unsigned int *",
        "long *",
        "unsigned long *",
        "void *",
        "volatile int *",
        "volatile void *",
        "int **",
        "void **",
        "volatile int **",
        "const int *",
        "const volatile int *",
        "const void *",
        "const volatile void *",
        "const int **",
        "const volatile int **",
        "_Bool *",
        "volatile _Bool *",
        "const _Bool *",
        "const volatile _Bool *",
        "_Bool **",
        "const _Bool **",
    ] {
        for zero in [
            "0",
            "0L",
            "0UL",
            "1 - 1",
            "sizeof(long) - 8",
            "(int)0x100000000UL",
            "(void *)0",
        ] {
            let source = format!(
                "{ty} global = {zero};
                 {ty} pass({ty} p) {{ return p; }}
                 {ty} empty(void) {{ return {zero}; }}
                 int main(void) {{
                     {ty} p = {zero};
                     if (p != 0 || global != 0 || pass({zero}) != 0 || empty() != 0) return 99;
                     p = {zero};
                     return (p == {zero}) + ({zero} == p) + (p == 0);
                 }}"
            );
            let (machine, _) = execute(&source);
            assert_eq!(machine.core().register(10), Some(3), "{source}");
        }
    }
}

#[test]
fn null_conditionals_preserve_object_pointer_types_and_skip_calls() {
    let (machine, _) = execute(
        "static int calls;
         int *choose(int *p) { ++calls; return p; }
         int main(void) {
             int n = 7;
             int *p = 1 ? &n : 0;
             int *q = 0 ? 0 : &n;
             if (*(1 ? p : (void *)0) != 7) return 99;
             if (*(0 ? (void *)0 : q) != 7) return 98;
             volatile int *v = 1 ? &n : 0UL;
             int **table = 0 ? &p : 0;
             if (table != 0 || *v != 7) return 97;
             p = 0 ? choose(&n) : 0;
             q = 1 ? 0 : choose(&n);
             if (p || q || calls) return 96;
             return sizeof(*(1 ? &n : (void *)0)) + (p == (void *)0);
         }",
    );
    assert_eq!(machine.core().register(10), Some(5));
    let (machine, _) = execute(
        "static int calls;
         int *empty(void) { ++calls; return 0; }
         int accepts(int a,int b,int c,int d,int e,int f,int g,int h,int *p) {
             return a+b+c+d+e+f+g+h + (p == 0);
         }
         int main(void) {
             if (empty() != 0 || 0 != empty()) return 99;
             return accepts(1,2,3,4,5,6,7,8,0) + calls;
         }",
    );
    assert_eq!(machine.core().register(10), Some(39));
}

#[test]
fn explicit_null_table_entries_and_conditional_addresses_survive_poisoned_startup() {
    let source = "
        enum { NIL = 0 };
        int n;
        int *table[6] = { NIL, 1 ? &n : 0, 0 ? &n : 0, (void *)0, 0UL };
        int main(void) {
            int *local[3] = { 0, &n, (void *)0 };
            if (local[0] || local[2] || local[1] != &n) return 99;
            *table[1] = 7;
            return n;
        }";
    let assembly = compile_to_assembly(source, "null-table.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 64]).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(7));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut bytes = [0; 64];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    assert_eq!(&bytes[..8], &[0; 8]);
    assert_eq!(&bytes[8..16], &0x80010030u64.to_le_bytes());
    assert_eq!(&bytes[16..48], &[0; 32]);
    assert_eq!(&bytes[48..56], &[7, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&bytes[56..64], &[0xa5; 8]);
}

#[test]
fn null_pointer_runtime_values_nonzero_constants_and_invalid_uses_preserve_output() {
    let bodies = [
        "int *p = 1;",
        "int *p = -1;",
        "int *p = 0x100000000UL;",
        "int zero = 0; int *p = zero;",
        "int *p = 0; int zero = 0; p = zero;",
        "int *p = 0; return p == 1;",
        "int *p = 0; int zero = 0; return p == zero;",
        "int *p = 0; return p < 0;",
        "int *p = 0; return 0 >= p;",
        "int *p = 0; p += 0; return p;",
        "int *p = 0; return 1 ? p : 1;",
        "int *p = 0; int zero = 0; p = 1 ? p : zero;",
        "int zero = 0; int *p = 0 && zero;",
        "int *p = 0 && main();",
        "int *p = 1 ? 0 : main();",
        "int *p = (int *)0; int value = p;",
        "int *p = 1 / 0;",
        "int *p = 0; int **q = &p; volatile int **r = 1 ? q : 0;",
    ];
    for body in bodies {
        let source = format!("int main(void) {{ {body} return 0; }}");
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&build.stderr).contains("guest.c:"),
            "{source}"
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn goto_forward_backward_jumps_and_recursive_cleanup_execute() {
    let (machine, output) = execute(
        "static int calls;
         static int sum(int n) {
             int result = n;
             if (n == 0) goto cleanup;
             result += sum(n - 1);
             goto cleanup;
             result = 99;
         cleanup:
             ++calls;
             return result;
         }
         int main(void) {
             int i = 0;
             int result = 0;
             goto test;
         again:
             result += ++i;
         test:
             if (i < 4) goto again;
             goto cleanup;
             result = 99;
         cleanup:
             *(volatile unsigned char *)0x10000000UL = result;
             return result + sum(4) + calls;
         }",
    );
    assert_eq!(output, [10]);
    assert_eq!(machine.core().register(10), Some(25));
}

#[test]
fn goto_nested_scopes_switch_loops_and_label_namespace_execute() {
    let (machine, output) = execute(
        "int destination = 7;
         typedef int finish;
         enum { AGAIN = 2 };
         int main(void) {
             int total = 0;
             int destination = 3;
             for (int i = 0; i < 3; ++i) {
                 switch (i) {
                 case 0:
                     goto emit;
                 case 1:
                     goto nested;
                 default:
                     goto finish;
                 }
             emit:
                 *(volatile unsigned char *)0x10000000UL = i;
                 ++total;
                 continue;
             }
         nested:
             { int destination = 11;
               total += destination;
               goto AGAIN;
             }
         finish:
             return 99;
         AGAIN:
             goto destination;
         destination:
             return total + destination;
         }",
    );
    assert_eq!(output, [0]);
    assert_eq!(machine.core().register(10), Some(15));
    let (machine, _) = execute(
        "int main(void) {
             int result = 0;
             goto inner;
             { int n = 99;
             inner:
                 n = 7;
                 result = n;
                 goto end;
             }
         end:
             return result;
         }",
    );
    assert_eq!(machine.core().register(10), Some(7));
}

#[test]
fn goto_reentering_scope_reexecutes_initialization_and_skips_side_effects() {
    let (machine, output) = execute(
        "static int calls;
         int initialize(void) { ++calls; return calls; }
         int main(void) {
             int total = 0;
             int rounds = 0;
         restart:
             {
                 int local = initialize();
                 int array[2] = {local, 3};
                 total += array[0] + array[1];
                 if (++rounds < 3) goto restart;
             }
             goto done;
             initialize();
             *(volatile unsigned char *)0x10000000UL = 99;
         done:
             return total + calls;
         }",
    );
    assert!(output.is_empty());
    assert_eq!(machine.core().register(10), Some(18));
}

#[test]
fn goto_labels_are_function_local_and_cleanup_restores_stack_calls() {
    let (machine, _) = execute(
        "static int f(int a,int b,int c,int d,int e,int f,int g,int h,int i,int j) {
             int result = a+b+c+d+e+f+g+h+i+j;
             goto out;
             result = 99;
         out: return result;
         }
         static int g(int n) {
             int result = f(1,2,3,4,5,6,7,8,9,10);
             if (n) result += g(n - 1);
             goto out;
         out: return result;
         }
         int main(void) { goto out; out: return g(2); }",
    );
    assert_eq!(machine.core().register(10), Some(165));
}

#[test]
fn invalid_goto_labels_scopes_and_unevaluated_paths_preserve_rom() {
    let cases = [
        "int main(void) { goto missing; return 0; }",
        "int main(void) { return 0; goto missing; }",
        "int main(void) { if (0) goto missing; return 0; }",
        "int main(void) { while (0) { goto missing; } return 0; }",
        "int main(void) { switch (0) { case 1: goto missing; } return 0; }",
        "int main(void) { same: ; same: return 0; }",
        "int main(void) { { same: ; } { same: ; } return 0; }",
        "int main(void) { if (0) { same: ; } same: return 0; }",
        "int f(void) { target: return 1; } int main(void) { goto target; }",
        "int main(void) { goto target; target: { int hidden = 7; } return hidden; }",
        "int main(void) { goto target; target: int n = 7; return n; }",
        "int main(void) { goto target; target: break; }",
        "int main(void) { target: continue; }",
        "int main(void) { int n = 2; goto target; int a[n]; target: return 0; }",
        "int main(void) { goto * (int *)0x1000UL; }",
        "int main(void) { return &&target; target: return 0; }",
        "int main(void) { goto 123; }",
        "int main(void) { goto default; }",
        "int main(void) { goto if; }",
        "int main(void) { default: return 0; }",
        "int main(void) { typedef int same; same: ; same: return 0; }",
    ];
    for source in cases {
        let files = Files::new(source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&build.stderr).contains("guest.c:"),
            "{source}"
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn pointer_tables_finalize_symbolic_entries_and_zero_fill_poisoned_ram() {
    let source = "
        int target;
        int later;
        static int *links[4] = { &target, &later, (int *)0 };
        int **cursor = links + 1;
        int **direct = &links[0];
        int *empty[3];
        int target = 7;
        int main(void) {
            if (links[2] || links[3] || empty[0] || empty[2]) return 99;
            if (cursor - links != 1 || direct != links) return 98;
            **cursor = 11;
            **direct += 3;
            volatile unsigned long *report = (volatile unsigned long *)0x80000000UL;
            report[0] = (unsigned long)links;
            report[1] = (unsigned long)empty;
            report[2] = (unsigned long)&target;
            report[3] = (unsigned long)&later;
            return target + later;
        }";
    let assembly = compile_to_assembly(source, "table-layout.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 112]).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(21));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut addresses = [0; 32];
    machine.bus_mut().read(0x80000000, &mut addresses).unwrap();
    let addresses: Vec<_> = addresses
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(addresses, [0x80010010, 0x80010038, 0x80010030, 0x80010050]);
    let mut bytes = [0; 96];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    assert_eq!(&bytes[0..8], &0x80010018u64.to_le_bytes());
    assert_eq!(&bytes[8..16], &0x80010010u64.to_le_bytes());
    assert_eq!(&bytes[16..24], &0x80010030u64.to_le_bytes());
    assert_eq!(&bytes[24..32], &0x80010050u64.to_le_bytes());
    assert_eq!(&bytes[32..48], &[0; 16]);
    assert_eq!(&bytes[48..56], &[10, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&bytes[56..80], &[0; 24]);
    assert_eq!(&bytes[80..88], &[11, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&bytes[88..96], &[0xa5; 8]);
}

#[test]
fn pointer_table_elements_support_all_integer_widths_and_volatile_pointees() {
    for ty in [
        "char",
        "signed char",
        "unsigned char",
        "short",
        "unsigned short",
        "int",
        "unsigned int",
        "long",
        "unsigned long",
    ] {
        for qualifier in ["", "volatile "] {
            let source = format!(
                "int main(void) {{
                    {ty} values[3] = {{ 1, 2, 3 }};
                    {qualifier}{ty} *table[4] = {{ values, values + 1, values + 2 }};
                    {qualifier}{ty} **p = table;
                    **p = 7;
                    ++p;
                    *p += 1;
                    **p = 9;
                    p[-1] = values + 1;
                    if (table[3] || sizeof(table) != 32 || sizeof(*p) != 8) return 99;
                    return values[0] + values[1] + values[2] + (p - table);
                }}"
            );
            let (machine, _) = execute(&source);
            assert_eq!(machine.core().register(10), Some(19), "{source}");
        }
    }
    let (machine, _) = execute(
        "int main(void) {
             int n = 4;
             void *table[3] = { &n, (void *)0 };
             void **p = table;
             int *value = *p;
             *value = 8;
             *p = &n;
             if (p[1] || p[2]) return 99;
             return *(int *)*p;
         }",
    );
    assert_eq!(machine.core().register(10), Some(8));
}

#[test]
fn automatic_pointer_tables_recursive_calls_and_element_updates_execute() {
    let (machine, _) = execute(
        "typedef int *item;
         typedef int **items;
         static int calls;
         int *choose(int *p) { ++calls; return p; }
         int recurse(int n) {
             int value = n;
             item table[3] = { choose(&value), (int *)0 };
             items cursor = table;
             int *old = (*cursor)++;
             --*cursor;
             if (old != &value || table[1] || table[2]) return 99;
             int result = n ? recurse(n - 1) : 0;
             return **cursor + result;
         }
         int main(void) { return recurse(4) + calls; }",
    );
    assert_eq!(machine.core().register(10), Some(15));
    let (machine, _) = execute(
        "int main(void) {
             int n = 1;
             int *p = &n;
             int **q = &p;
             int m = 7;
             *q = &m;
             **q += 2;
             return n + m;
         }",
    );
    assert_eq!(machine.core().register(10), Some(10));
}

#[test]
fn pointer_table_arguments_and_returns_preserve_eight_byte_stack_slots() {
    let (machine, _) = execute(
        "int **pass(int a, int b, int c, int d, int e, int f, int g, int h, int **table) {
             if (a+b+c+d+e+f+g+h != 36) return (int **)0;
             return table + 1;
         }
         int main(void) {
             int values[2] = { 3, 9 };
             int *table[] = { values, values + 1 };
             int **p = pass(1,2,3,4,5,6,7,8,table);
             return **p + (p - table);
         }",
    );
    assert_eq!(machine.core().register(10), Some(10));
    let (machine, _) = execute(
        "int n;
         int *p = &n;
         int **q = &p;
         int n = 3;
         int main(void) {
             int m = 11;
             **q += 4;
             *q = &m;
             return n + *p;
         }",
    );
    assert_eq!(machine.core().register(10), Some(18));
}

#[test]
fn invalid_pointer_table_types_initializers_qualifications_and_capacity_preserve_rom() {
    let (machine, _) = execute(
        "int main(void) {
             int *table[120] = { (int *)0 };
             return sizeof(table) + (table[119] == (int *)0);
         }",
    );
    assert_eq!(machine.core().register(10), Some(961));
    let (machine, _) = execute_with_budget(
        "int *table[8192];
         int main(void) {
             if (table[0] || table[8191]) return 99;
             return sizeof(table);
         }",
        400000,
    );
    assert_eq!(machine.core().register(10), Some(65536));
    let sources = [
        "int *table[2] = {1}; int main(void) { return 0; }",
        "int n; unsigned int *table[] = {&n}; int main(void) { return 0; }",
        "int n; int *p = &n; int *table[] = {p}; int main(void) { return 0; }",
        "int *table[] = {&later}; int later; int main(void) { return 0; }",
        "int *table[1] = {(int *)0, (int *)0}; int main(void) { return 0; }",
        "int *table[2] = \"AB\"; int main(void) { return 0; }",
        "int *table[2] = {}; int main(void) { return 0; }",
        "int ***table; int main(void) { return 0; }",
        "int **table[2]; int main(void) { return 0; }",
        "int *table[8193]; int main(void) { return 0; }",
        "int main(void) { int *table[121]; return 0; }",
        "int main(void) { int n; int *table[120]; return 0; }",
        "int main(void) { int *table[2]; table++; return 0; }",
        "int main(void) { int *table[2]; return sizeof(&table); }",
        "int main(void) { int *table[2]; int *other[2]; table = other; return 0; }",
        "int main(void) { int n; int *p = &n; volatile int **q = &p; return 0; }",
        "int main(void) { volatile int *p = (volatile int *)0; int **q = &p; return 0; }",
        "int main(void) { int n; int *p = &n; void **q = &p; return 0; }",
        "int main(void) { int **p = (int **)0; int *q = p; return 0; }",
        "int main(void) { int **p = (int **)0; return sizeof(&p); }",
    ];
    for source in sources {
        let files = Files::new(source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&build.stderr).contains("guest.c:"),
            "{source}"
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn boolean_conversions_promotions_and_scalar_updates_execute() {
    for (body, expected) in [
        ("_Bool b = 256; return b;", 1u64),
        ("_Bool b = -1; return b;", 1),
        ("_Bool b = 0x8000000000000000UL; return b;", 1),
        ("_Bool b = 0; return b;", 0),
        ("_Bool b = (int *)0; return b;", 0),
        ("_Bool b = (int *)0x80000000UL; return b;", 1),
        ("int n = 0; _Bool b = &n; return b;", 1),
        ("_Bool b = 1; return b + b;", 2),
        ("_Bool b = 1; return -b;", u64::MAX),
        ("_Bool b = 1; return sizeof(b + b);", 4),
        (
            "_Bool b = 1; return sizeof(b) + sizeof(_Bool) + sizeof(_Bool *);",
            10,
        ),
        ("_Bool b = 1; return (1 ? b : 5) + (0 ? b : 7U);", 8),
        ("_Bool b = 0; return b++ + b;", 1),
        ("_Bool b = 1; return b++ + b;", 2),
        ("_Bool b = 1; return b-- + b;", 1),
        ("_Bool b = 0; return --b;", 1),
        ("_Bool b = 1; b += 256; return b;", 1),
        ("_Bool b = 1; b -= 1; return b;", 0),
        ("_Bool b = 1; b ^= 1; return b;", 0),
        ("_Bool b = 1; b <<= 8; return b;", 1),
        ("_Bool b = 1; b >>= 1; return b;", 0),
        ("_Bool b = 1; b *= 0; return b;", 0),
        ("_Bool b = 1; b %= 1; return b;", 0),
        ("_Bool b = 1; b = 512; return b;", 1),
        ("_Bool b = 1; return ~b;", (-2i64) as u64),
        ("return (_Bool)(unsigned char)256;", 0),
        ("return (_Bool)256;", 1),
        ("return (_Bool)0xffffffffffffffffUL == 1;", 1),
    ] {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(machine.core().register(10), Some(expected), "{body}");
    }
}

#[test]
fn boolean_arrays_globals_and_qualified_views_preserve_exact_byte_storage() {
    let source = "typedef _Bool flag;
         flag initialized[] = { [4] = 256, [0] = -1, 0, 0x8000000000000000UL };
         flag zero[5];
         const flag *view = initialized;
         int main(void) {
             flag local[] = { [4] = 256, [0] = -1, 0, 0x8000000000000000UL };
             volatile flag *p = (volatile flag *)0x80000020UL;
             p[0] = 256; p[1] = 0; p[2] = -1; p[3] = 1;
             if (p[0]++ != 1 || p[1]-- != 0) return 1;
             p[2] -= 1;
             p[3] <<= 8;
             const volatile flag *read = p;
             if (read[0] != 1 || read[1] != 1 || read[2] != 0 || read[3] != 1) return 2;
             flag *table[] = { [1] = local };
             const flag **cursor = &view;
             if (**cursor != 1 || table[1][4] != 1 || read - p != 0) return 3;
             if (sizeof(initialized) != 5 || sizeof(local) != 5 || sizeof(flag) != 1) return 4;
             *(volatile unsigned long *)0x80000000UL = (unsigned long)initialized;
             *(volatile unsigned long *)0x80000008UL = (unsigned long)zero;
             *(volatile unsigned long *)0x80000010UL = (unsigned long)local;
             return local[0] + local[1] + local[2] + local[3] + local[4];
         }";
    let rom = assemble_boot_rom(&compile_to_assembly(source, "bool-bytes.c").unwrap()).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 64]).unwrap();
    machine.bus_mut().write(0x80000020, &[0xa5; 8]).unwrap();
    machine.bus_mut().write(0x8000fc00, &[0xa5; 1024]).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(3));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut addresses = [0; 24];
    machine.bus_mut().read(0x80000000, &mut addresses).unwrap();
    for (address, expected) in addresses.chunks_exact(8).zip([
        &[1u8, 0, 1, 0, 1][..],
        &[0u8; 5][..],
        &[1u8, 0, 1, 0, 1][..],
    ]) {
        let mut bytes = [0; 5];
        machine
            .bus_mut()
            .read(u64::from_le_bytes(address.try_into().unwrap()), &mut bytes)
            .unwrap();
        assert_eq!(bytes, expected);
    }
    let mut bytes = [0; 8];
    machine.bus_mut().read(0x80000020, &mut bytes).unwrap();
    assert_eq!(bytes, [1, 1, 0, 1, 0xa5, 0xa5, 0xa5, 0xa5]);
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    assert_eq!(bytes, [1, 0, 1, 0, 1, 0, 0, 0]);
    machine.bus_mut().read(0x80010010, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 8]);
    machine.bus_mut().read(0x80010018, &mut bytes).unwrap();
    assert_eq!(bytes, [0xa5; 8]);
}

#[test]
fn boolean_volatile_input_conversion_reads_each_uart_byte_once() {
    let source =
        "static int calls;
         volatile unsigned char *address(void) { ++calls; return (volatile unsigned char *)0x10000000UL; }
         int main(void) {
             sizeof((_Bool)*address());
             0 && (_Bool)*address();
             _Bool a = *address();
             _Bool b = *address();
             *(volatile unsigned char *)0x10000000UL = a;
             *(volatile unsigned char *)0x10000000UL = b;
             return calls;
         }";
    let rom = assemble_boot_rom(&compile_to_assembly(source, "bool-uart.c").unwrap()).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[0, 128, 255], &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(output, [0, 1]);
    assert_eq!(machine.core().register(10), Some(2));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut next = [0; 1];
    machine.bus_mut().read(0x10000000, &mut next).unwrap();
    assert_eq!(next, [255]);
}

#[test]
fn boolean_pointer_conversion_calls_recursion_and_side_effects_execute_once() {
    let (machine, output) = execute(
        "static int calls;
         static int object;
         int *address(void) { ++calls; *(volatile unsigned char *)0x10000000UL = 'P'; return &object; }
         _Bool accept(_Bool flag) { return flag; }
         _Bool recurse(int n, _Bool value) {
             if (n) return recurse(n - 1, value);
             return value;
         }
         _Bool ninth(int a,int b,int c,int d,int e,int f,int g,int h,_Bool value) {
             return value && a + b + c + d + e + f + g + h == 36;
         }
         _Bool pointer_result(void) { return address(); }
         int main(void) {
             _Bool value = accept(address());
             if (!value || !pointer_result() || calls != 2) return 1;
             if (!recurse(4, 0x8000000000000000UL)) return 2;
             if (!ninth(1,2,3,4,5,6,7,8,address()) || calls != 3) return 3;
             if (accept((const volatile void *)0)) return 4;
             sizeof((_Bool)address());
             0 && accept(address());
             if (calls != 3) return 5;
             value = 0;
             value += pointer_result();
             return value + calls;
         }"
    );
    assert_eq!(output, b"PPPP");
    assert_eq!(machine.core().register(10), Some(5));
}

#[test]
fn boolean_constant_indices_enumerators_and_switch_promotions_execute() {
    let (machine, _) = execute(
        "enum { TRUE_VALUE = (_Bool)0x100000000UL, FALSE_VALUE = (_Bool)0 };
         _Bool global = (_Bool)-1;
         int numbers[(_Bool)256 + 1] = { [(_Bool)256] = 42 };
         int main(void) {
             _Bool flag = global;
             switch (flag) { case 0: return 99; case 1: break; default: return 98; }
             if (TRUE_VALUE != 1 || FALSE_VALUE != 0 || sizeof(numbers) != 8) return 97;
             _Bool flags[] = { 0, 256, -2 };
             int total = 0;
             for (int i = 0; i < 3; ++i) total += flags[i];
             return numbers[TRUE_VALUE] + total;
         }",
    );
    assert_eq!(machine.core().register(10), Some(44));
}

#[test]
fn boolean_scalar_register_stack_abi_interoperates_with_independent_assembly() {
    let mut assembly = compile_to_assembly(
        "_Bool probe(_Bool a,int b,int c,int d,int e,int f,int g,int h,_Bool last) {
             return a && last && b + c + d + e + f + g + h == 35;
         }
         int main(void) { return probe(256,2,3,4,5,6,7,8,0x100000000UL); }",
        "bool-abi.c",
    )
    .unwrap();
    assembly = assembly.replacen("\nprobe:\n", "\n.Lunused_bool_probe:\n", 1);
    assembly.push_str(
        "probe:
         andi t0, sp, 15
         bne t0, zero, .Lbool_bad
         li t0, 1
         bne a0, t0, .Lbool_bad
         ld t1, 0(sp)
         bne t1, t0, .Lbool_bad
         li a0, 1
         ret
         .Lbool_bad:
         li a0, 0
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(1));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut assembly = compile_to_assembly(
        "_Bool probe(_Bool a,int b,int c,int d,int e,int f,int g,int h,_Bool last) {
             return a && last && b + c + d + e + f + g + h == 35;
         }
         int main(void) { return 0; }",
        "bool-caller.c",
    )
    .unwrap()
    .replacen("call main\n", "call .Lbool_entry\n", 1);
    assembly.push_str(
        ".Lbool_entry:
         addi sp, sp, -32
         sd ra, 24(sp)
         li s0, 123
         li s1, 456
         li t0, 1
         sd t0, 0(sp)
         li a0, 1
         li a1, 2
         li a2, 3
         li a3, 4
         li a4, 5
         li a5, 6
         li a6, 7
         li a7, 8
         call probe
         ld ra, 24(sp)
         addi sp, sp, 32
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    for (register, value) in [(2, 0x80010000), (8, 123), (9, 456), (10, 1)] {
        assert_eq!(machine.core().register(register), Some(value));
    }
}

#[test]
fn boolean_invalid_types_qualifications_and_atomic_widths_preserve_rom() {
    for source in [
        "unsigned _Bool object; int main(void) { return 0; }",
        "signed _Bool object; int main(void) { return 0; }",
        "long _Bool object; int main(void) { return 0; }",
        "const _Bool object = 1; int main(void) { object = 0; return 0; }",
        "volatile _Bool object; int main(void) { _Bool *p = &object; return 0; }",
        "int main(void) { _Bool a[2] = \"AB\"; return 0; }",
        "int main(void) { _Bool *p = (int *)0; return 0; }",
        "int main(void) { int *p = (_Bool *)0; return 0; }",
        "int main(void) { _Bool *p = (const _Bool *)0; return 0; }",
        "int main(void) { const _Bool *p = (const _Bool *)0; *p = 1; return 0; }",
        "int main(void) { const _Bool *p = (const _Bool *)0; ++*p; return 0; }",
        "int main(void) { _Bool *p = (_Bool *)0; const _Bool **q = &p; return 0; }",
        "int main(void) { _Bool b = 1; int *p = b; return 0; }",
        "void f(void) {} int main(void) { _Bool b = f(); return 0; }",
        "int main(void) { _Bool b = 0; __atomic_load_n(&b,0); return 0; }",
        "int main(void) { _Bool b = 0; __atomic_exchange_n(&b,1,0); return 0; }",
        "int main(void) { _Bool b = 0; __atomic_fetch_or(&b,1,0); return 0; }",
        "int main(void) { _Bool b = 0; __atomic_fetch_add(&b,1,0); return 0; }",
        "int main(void) { _Bool b = 0; switch(b) { case 1: break; case (_Bool)256: break; } return 0; }",
    ] {
        let diagnostic = compile_to_assembly(source, "bool-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(
            diagnostic.contains("bool-invalid.c:"),
            "{source}: {diagnostic}"
        );
        let files = Files::new(source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn compiled_atomic_bitmap_allocator_executes_through_production_cli() {
    let source = include_str!("fixtures/kiuas16_bitmap.c");
    let rom = assemble_boot_rom(&compile_to_assembly(source, "bitmap.c").unwrap()).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 64]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(59));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    assert_eq!(output, b"B\x09\x09\n");
    let mut bytes = [0; 28];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    let expected: Vec<_> = 9u64
        .to_le_bytes()
        .into_iter()
        .chain(0x8000000000000000u64.to_le_bytes())
        .chain(9u32.to_le_bytes())
        .chain(17u32.to_le_bytes())
        .chain(42u32.to_le_bytes())
        .collect();
    assert_eq!(bytes.as_slice(), expected);
    let files = Files::new(source);
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "10000", "--deny-traps"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, output);
}

#[test]
fn atomic_fetch_arithmetic_widths_wrapping_old_values_and_neighbors_execute() {
    for (ty, width, signed) in [
        ("int", 4, true),
        ("unsigned int", 4, false),
        ("long", 8, true),
        ("unsigned long", 8, false),
    ] {
        let mask = if width == 4 {
            u32::MAX as u64
        } else {
            u64::MAX
        };
        for old in [mask, mask >> 1, 1u64 << (width * 8 - 1)] {
            for (operation, operand) in [("add", 1u64), ("sub", 1), ("sub", 1u64 << 63)] {
                for order in 0..=5 {
                    let expected = if operation == "add" {
                        old.wrapping_add(operand) & mask
                    } else {
                        old.wrapping_sub(operand) & mask
                    };
                    let returned = if width == 4 && signed {
                        old as u32 as i32 as i64 as u64
                    } else {
                        old
                    };
                    let source = format!(
                        "int main(void) {{
                             volatile {ty} *p = (volatile {ty} *)0x80000008UL;
                             *p = 0x{old:x}UL;
                             {ty} prior = __atomic_fetch_{operation}(p, 0x{operand:x}UL, {order});
                             *(volatile unsigned long *)0x80000020UL = prior;
                             return 7;
                         }}"
                    );
                    let rom = assemble_boot_rom(&compile_to_assembly(&source, "fetch.c").unwrap())
                        .unwrap();
                    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
                    machine.bus_mut().write(0x80000000, &[0xa5; 48]).unwrap();
                    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
                    assert_eq!(report.traps, 0, "{source}: {:?}", report.last_trap);
                    assert!(report.waiting > 0);
                    assert_eq!(machine.core().register(10), Some(7));
                    assert_eq!(machine.core().register(2), Some(0x80010000));
                    assert_eq!(machine.core().register(8), Some(0));
                    let mut bytes = [0; 24];
                    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
                    assert_eq!(&bytes[..8], &[0xa5; 8]);
                    assert_eq!(
                        &bytes[8..8 + width],
                        &expected.to_le_bytes()[..width],
                        "{source}"
                    );
                    assert!(bytes[8 + width..].iter().all(|byte| *byte == 0xa5));
                    let mut actual = [0; 8];
                    machine.bus_mut().read(0x80000020, &mut actual).unwrap();
                    assert_eq!(u64::from_le_bytes(actual), returned, "{source}");
                }
            }
        }
    }
}

#[test]
fn atomic_fetch_arithmetic_emits_exact_width_ordering_and_fence_words() {
    for width in [4, 8] {
        for operation in ["add", "sub"] {
            for order in 0..=5 {
                let ty = if width == 4 {
                    "unsigned int"
                } else {
                    "unsigned long"
                };
                let source = format!(
                    "enum {{ ORDER = {order} }};
                     int main(void) {{ {ty} object = 7;
                         {ty} old = __atomic_fetch_{operation}(&object, 9, ORDER);
                         return old == 7 && object == ({ty})(7 - 9 + {offset});
                     }}",
                    offset = if operation == "add" { 18 } else { 0 }
                );
                let rom = assemble_boot_rom(&compile_to_assembly(&source, "fetch-bits.c").unwrap())
                    .unwrap();
                let words: Vec<_> = rom
                    .chunks_exact(4)
                    .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
                    .collect();
                let atomics: Vec<_> = words
                    .iter()
                    .enumerate()
                    .filter(|(_, word)| **word & 0x7f == 0x2f)
                    .collect();
                assert_eq!(atomics.len(), 1, "{source}");
                let (index, word) = atomics[0];
                assert_eq!(word >> 27, 0);
                assert_eq!((word >> 12) & 7, if width == 4 { 2 } else { 3 });
                assert_eq!((word >> 7) & 31, 10);
                assert_eq!((word >> 20) & 31, 10);
                assert_eq!(
                    (word >> 25) & 3,
                    match order {
                        0 => 0,
                        1 | 2 => 2,
                        3 => 1,
                        _ => 3,
                    }
                );
                assert_eq!(
                    words.iter().filter(|word| **word == 0x0ff0000f).count(),
                    if order == 5 { 2 } else { 0 }
                );
                if order == 5 {
                    assert_eq!(words[index - 1], 0x0ff0000f);
                    assert_eq!(words[index + 1], 0x0ff0000f);
                }
                let (machine, _) = execute(&source);
                assert_eq!(machine.core().register(10), Some(1));
            }
        }
    }
}

#[test]
fn atomic_fetch_arithmetic_evaluates_operands_once_across_nested_stack_calls() {
    let (mut machine, output) = execute(
        "static unsigned long counter;
         static int calls;
         unsigned long *address(void) {
             ++calls;
             *(volatile unsigned char *)0x10000000UL = 'P';
             return &counter;
         }
         unsigned long ninth(long a,long b,long c,long d,long e,long f,long g,long h,unsigned long value) {
             ++calls;
             *(volatile unsigned char *)0x10000000UL = 'V';
             return value;
         }
         unsigned long operand(void) { return ninth(1,2,3,4,5,6,7,8,5); }
         unsigned long recurse(int n) {
             unsigned long old = __atomic_fetch_add(&counter, 1, 2);
             if (n) return old + recurse(n - 1);
             return old;
         }
         int main(void) {
             if (__atomic_fetch_add(address(), operand(), 4) != 0) return 1;
             if (__atomic_fetch_sub(address(), operand(), 5) != 5) return 2;
             if (counter || calls != 4) return 3;
             if (recurse(4) != 10 || counter != 5) return 4;
             unsigned int local = 0x80000001U;
             unsigned int *table[] = { [1] = &local };
             unsigned int old = __atomic_fetch_add(table[1], (signed char)-1, 0);
             if (old != 0x80000001U || local != 0x80000000U) return 5;
             *(volatile unsigned long *)0x80000000UL = counter;
             return __atomic_fetch_sub(&local, 0x100000000UL, 3) == 0x80000000U;
         }"
    );
    assert_eq!(output, b"PVPV");
    assert_eq!(machine.core().register(10), Some(1));
    let mut bytes = [0; 8];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(bytes, 5u64.to_le_bytes());
}

#[test]
fn atomic_fetch_ticket_counter_uses_old_values_for_slots_and_publication() {
    let (machine, _) = execute(
        "static unsigned int next;
         static unsigned int published;
         static unsigned int entries[4];
         int publish(unsigned int value) {
             unsigned int ticket = __atomic_fetch_add(&next, 1, 0);
             if (ticket >= 4) {
                 __atomic_fetch_sub(&next, 1, 0);
                 return 0;
             }
             entries[ticket] = value;
             __atomic_fetch_add(&published, 1, 3);
             return 1;
         }
         int main(void) {
             if (!publish(11) || !publish(22) || !publish(33) || !publish(44)) return 1;
             if (publish(99) || next != 4) return 2;
             if (__atomic_load_n(&published, 2) != 4) return 3;
             if (entries[0] != 11 || entries[1] != 22 || entries[2] != 33 || entries[3] != 44) return 4;
             return __atomic_fetch_sub(&published, 4, 4);
         }"
    );
    assert_eq!(machine.core().register(10), Some(4));
}

#[test]
fn atomic_fetch_bitwise_widths_orders_old_values_and_neighbor_bytes_execute() {
    for (ty, size, signed) in [
        ("int", 4, true),
        ("unsigned int", 4, false),
        ("long", 8, true),
        ("unsigned long", 8, false),
    ] {
        let mask = if size == 4 { u32::MAX as u64 } else { u64::MAX };
        for (operation, funct5) in [("and", 12u32), ("or", 8), ("xor", 4)] {
            for (old, operand) in [
                (mask, 0u64),
                (0, mask),
                (0xfedcba9880000001, 0x1234567887654321),
                (0x8000000000000000, u64::MAX),
            ] {
                for order in 0..=5 {
                    let old = old & mask;
                    let expected = match operation {
                        "and" => old & operand,
                        "or" => old | operand,
                        _ => old ^ operand,
                    } & mask;
                    let returned = if size == 4 && signed {
                        old as u32 as i32 as i64 as u64
                    } else {
                        old
                    };
                    let source = format!(
                        "int main(void) {{
                             volatile {ty} *p = (volatile {ty} *)0x80000008UL;
                             *p = 0x{old:x}UL;
                             {ty} prior = __atomic_fetch_{operation}(p, 0x{operand:x}UL, {order});
                             *(volatile unsigned long *)0x80000020UL = prior;
                             return 7;
                         }}"
                    );
                    let rom = assemble_boot_rom(&compile_to_assembly(&source, "bits.c").unwrap())
                        .unwrap();
                    let words: Vec<_> = rom
                        .chunks_exact(4)
                        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
                        .collect();
                    let atomics: Vec<_> = words
                        .iter()
                        .enumerate()
                        .filter(|(_, word)| **word & 0x7f == 0x2f)
                        .collect();
                    assert_eq!(atomics.len(), 1);
                    let (index, word) = atomics[0];
                    assert_eq!(word >> 27, funct5);
                    assert_eq!((word >> 12) & 7, if size == 4 { 2 } else { 3 });
                    assert_eq!((word >> 7) & 31, 10);
                    assert_eq!((word >> 20) & 31, 10);
                    assert_eq!(
                        (word >> 25) & 3,
                        match order {
                            0 => 0,
                            1 | 2 => 2,
                            3 => 1,
                            _ => 3,
                        }
                    );
                    assert_eq!(
                        words.iter().filter(|word| **word == 0x0ff0000f).count(),
                        if order == 5 { 2 } else { 0 }
                    );
                    if order == 5 {
                        assert_eq!(words[index - 1], 0x0ff0000f);
                        assert_eq!(words[index + 1], 0x0ff0000f);
                    }
                    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
                    machine.bus_mut().write(0x80000000, &[0xa5; 48]).unwrap();
                    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
                    assert_eq!(report.traps, 0, "{source}: {:?}", report.last_trap);
                    assert!(report.waiting > 0);
                    assert_eq!(machine.core().register(10), Some(7));
                    assert_eq!(machine.core().register(2), Some(0x80010000));
                    assert_eq!(machine.core().register(8), Some(0));
                    let mut bytes = [0; 24];
                    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
                    assert_eq!(&bytes[..8], &[0xa5; 8]);
                    assert_eq!(
                        &bytes[8..8 + size],
                        &expected.to_le_bytes()[..size],
                        "{source}"
                    );
                    assert!(bytes[8 + size..].iter().all(|byte| *byte == 0xa5));
                    let mut actual = [0; 8];
                    machine.bus_mut().read(0x80000020, &mut actual).unwrap();
                    assert_eq!(u64::from_le_bytes(actual), returned, "{source}");
                }
            }
        }
    }
}

#[test]
fn atomic_fetch_bitwise_nested_stack_operands_tables_and_recursive_updates_execute() {
    let (machine, output) = execute(
        "typedef unsigned long word;
         static word object;
         static int calls;
         word *address(void) {
             ++calls; *(volatile unsigned char *)0x10000000UL = 'P'; return &object;
         }
         word ninth(long a,long b,long c,long d,long e,long f,long g,long h,word value) {
             ++calls; *(volatile unsigned char *)0x10000000UL = 'V'; return value;
         }
         word operand(void) { return ninth(1,2,3,4,5,6,7,8,0x8000000000000001UL); }
         word recurse(int n) {
             word old = __atomic_fetch_or(&object, 1UL << n, 2);
             if (n) return old ^ recurse(n - 1);
             return old;
         }
         int main(void) {
             if (__atomic_fetch_or(address(), operand(), 4) != 0) return 1;
             if (__atomic_fetch_xor(address(), operand(), 5) != 0x8000000000000001UL) return 2;
             if (__atomic_fetch_and(address(), operand(), 0) != 0 || calls != 6) return 3;
             if (recurse(3) != 10 || object != 15) return 4;
             unsigned int local = 0x80000001U;
             unsigned int *table[] = { [1] = &local };
             if (__atomic_fetch_and(table[1], (signed char)-2, 3) != 0x80000001U) return 5;
             if (local != 0x80000000U) return 6;
             if (__atomic_fetch_or(table[1], 0x100000001UL, 1) != 0x80000000U) return 7;
             return __atomic_fetch_xor(table[1], 0x80000000U, 2) == 0x80000001U && local == 1;
         }",
    );
    assert_eq!(output, b"PVPVPV");
    assert_eq!(machine.core().register(10), Some(1));
}

#[test]
fn atomic_fetch_invalid_inputs_and_reserved_names_preserve_rom() {
    for operation in ["add", "sub", "and", "or", "xor"] {
        let builtin = format!("__atomic_fetch_{operation}");
        let mut sources: Vec<_> = [
            format!("{builtin}((char *)0, 1, 0);"),
            format!("{builtin}((short *)0, 1, 0);"),
            format!("{builtin}((void *)0, 1, 0);"),
            format!("{builtin}((int **)0, 1, 0);"),
            format!("{builtin}((const int *)0, 1, 0);"),
            format!("{builtin}((const volatile long *)0, 1, 0);"),
            format!("{builtin}(1, 1, 0);"),
            format!("{builtin}((int *)0, (void *)0, 0);"),
            format!("{builtin}((int *)0, 1, 6);"),
            format!("{builtin}((int *)0, 1, -1);"),
            format!("int order = 0; {builtin}((int *)0, 1, order);"),
            format!("{builtin}((int *)0, 1, 1 / 0);"),
            format!("{builtin}((int *)0, 1);"),
            format!("{builtin}((int *)0, 1, 0, 0);"),
            format!("if (0) {builtin}((const int *)0, 1, 0);"),
            format!("int {builtin} = 0;"),
        ]
        .iter()
        .map(|body| format!("int main(void) {{ {body} return 0; }}"))
        .collect();
        sources.extend([
            format!("int {builtin}; int main(void) {{ return 0; }}"),
            format!("int {builtin}(void) {{ return 0; }} int main(void) {{ return 0; }}"),
            format!("typedef int {builtin}; int main(void) {{ return 0; }}"),
            format!("enum {{ {builtin} = 0 }}; int main(void) {{ return 0; }}"),
            format!("int f(int {builtin}) {{ return 0; }} int main(void) {{ return 0; }}"),
        ]);
        for source in sources {
            let diagnostic = compile_to_assembly(&source, "fetch-invalid.c")
                .unwrap_err()
                .to_string();
            assert!(
                diagnostic.contains("fetch-invalid.c:"),
                "{source}: {diagnostic}"
            );
            let files = Files::new(&source);
            std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
            let build = files.compile();
            assert!(!build.status.success(), "{source}");
            assert!(!build.stderr.is_empty());
            assert_eq!(
                std::fs::read(files.0.join("rom.bin")).unwrap(),
                b"previous ROM"
            );
        }
    }
}

#[test]
fn atomic_integer_widths_orders_and_exact_neighbor_bytes_execute() {
    for ty in ["int", "unsigned int", "long", "unsigned long"] {
        for order in 0..=5 {
            let source = format!(
                "int main(void) {{
                    volatile {ty} *p = (volatile {ty} *)0x80000008UL;
                    *p = -1;
                    {ty} old = __atomic_exchange_n(p, 7, {order});
                    if (old != ({ty})-1 || *p != 7) return 99;
                    __atomic_store_n(p, -2, 3);
                    if (__atomic_load_n(p, 2) != ({ty})-2) return 98;
                    return 7;
                }}"
            );
            let assembly = compile_to_assembly(&source, "atomic.c").unwrap();
            let rom = assemble_boot_rom(&assembly).unwrap();
            let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
            machine.bus_mut().write(0x80000000, &[0xa5; 24]).unwrap();
            let mut output = Vec::new();
            let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
            assert_eq!(report.traps, 0, "{source}: {:?}", report.last_trap);
            assert!(report.waiting > 0);
            assert_eq!(machine.core().register(10), Some(7), "{source}");
            assert_eq!(machine.core().register(2), Some(0x80010000));
            assert_eq!(machine.core().register(8), Some(0));
            let mut bytes = [0; 24];
            machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
            let width = if ty.contains("long") { 8 } else { 4 };
            assert_eq!(&bytes[..8], &[0xa5; 8]);
            assert_eq!(&bytes[8..8 + width], &(-2i64).to_le_bytes()[..width]);
            assert!(bytes[8 + width..].iter().all(|byte| *byte == 0xa5));
        }
    }
}

#[test]
fn atomic_operands_nested_calls_and_load_store_orders_execute() {
    let source = "
        static unsigned long object;
        static int calls;
        unsigned long *address(void) { ++calls; return &object; }
        unsigned long value(void) { ++calls; return 0xfedcba9876543210UL; }
        unsigned long exchange(void) { return __atomic_exchange_n(address(), value(), 4); }
        int main(void) {
            if (exchange() || calls != 2) return 99;
            if (__atomic_load_n(&object, 1) != 0xfedcba9876543210UL) return 98;
            __atomic_store_n(address(), value(), 0);
            if (calls != 4) return 97;
            __atomic_store_n(&object, 7, 5);
            if (__atomic_load_n(&object, 5) != 7) return 96;
            __atomic_store_n(&object, 8, 3);
            return __atomic_load_n(&object, 0);
        }";
    let (machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(8));
    let assembly = compile_to_assembly(source, "orders.c").unwrap();
    assert!(assembly.contains("amoswap.d.aqrl a0, a0, (t0)"));
    assert!(assembly.contains("lr.d.aq a0, (t0)"));
    assert!(assembly.contains("amoswap.d zero, a0, (t0)"));
    assert!(assembly.contains("amoswap.d.rl zero, a0, (t0)"));
    assert!(assembly.contains("fence\nlr.d.aq a0, (t0)\nfence"));
    assert!(assembly.contains("fence\namoswap.d.aqrl zero, a0, (t0)\nfence"));
    let (machine, _) = execute(
        "int main(void) {
             int local = -1;
             if (__atomic_exchange_n(&local, 5, 2) != -1) return 99;
             return __atomic_load_n(&local, 0);
         }",
    );
    assert_eq!(machine.core().register(10), Some(5));
    let (machine, _) = execute(
        "static unsigned int lock = 1;
         static unsigned long payload;
         int publish(unsigned long value) {
             if (__atomic_exchange_n(&lock, 1, 2)) return 0;
             payload = value;
             __atomic_store_n(&lock, 0, 3);
             return 1;
         }
         int main(void) {
             if (publish(99) || payload) return 99;
             __atomic_store_n(&lock, 0, 3);
             if (!publish(42)) return 98;
             if (__atomic_load_n(&lock, 2)) return 97;
             return payload;
         }",
    );
    assert_eq!(machine.core().register(10), Some(42));
}

#[test]
fn atomic_memory_orders_emit_exact_rv64_ordering_bits_and_fences() {
    for (operation, orders) in [
        ("load", &[0u32, 1, 2, 5][..]),
        ("store", &[0u32, 3, 5][..]),
        ("exchange", &[0u32, 1, 2, 3, 4, 5][..]),
    ] {
        for order in orders {
            let argument = if operation == "load" { "" } else { "9," };
            let source = format!(
                "enum {{ ORDER = {order} }};
                 int main(void) {{
                     unsigned long object = 7;
                     __atomic_{operation}_n(&object, {argument} ORDER);
                     return object;
                 }}"
            );
            let assembly = compile_to_assembly(&source, "atomic-bits.c").unwrap();
            let rom = assemble_boot_rom(&assembly).unwrap();
            let words: Vec<_> = rom
                .chunks_exact(4)
                .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
                .collect();
            let atomics: Vec<_> = words
                .iter()
                .enumerate()
                .filter(|(_, word)| **word & 0x7f == 0x2f)
                .collect();
            assert_eq!(atomics.len(), 1, "{source}");
            let (index, word) = atomics[0];
            assert_eq!(word >> 27, if operation == "load" { 2 } else { 1 });
            assert_eq!((word >> 12) & 7, 3);
            let expected_order = match order {
                0 => 0,
                1 | 2 => 2,
                3 => 1,
                _ if operation == "load" => 2,
                _ => 3,
            };
            assert_eq!((word >> 25) & 3, expected_order, "{source}");
            let fences = words.iter().filter(|word| **word == 0x0ff0000f).count();
            assert_eq!(fences, if *order == 5 { 2 } else { 0 }, "{source}");
            if *order == 5 {
                assert_eq!(words[index - 1], 0x0ff0000f);
                assert_eq!(words[index + 1], 0x0ff0000f);
            }
            let (machine, _) = execute(&source);
            assert_eq!(
                machine.core().register(10),
                Some(if operation == "load" { 7 } else { 9 })
            );
        }
    }
}

#[test]
fn atomic_misaligned_and_nonram_addresses_trap_in_guest() {
    for expression in [
        "__atomic_load_n((int *)0x80000001UL, 0)",
        "__atomic_exchange_n((long *)0x80000004UL, 1, 2)",
        "__atomic_store_n((int *)0x80000001UL, 1, 3)",
        "__atomic_load_n((int *)0x10000000UL, 0)",
        "__atomic_exchange_n((long *)0x1000UL, 1, 0)",
        "__atomic_fetch_add((int *)0x80000001UL, 1, 0)",
        "__atomic_fetch_sub((long *)0x80000004UL, 1, 0)",
        "__atomic_fetch_add((int *)0x10000000UL, 1, 0)",
        "__atomic_fetch_sub((long *)0x1000UL, 1, 0)",
        "__atomic_fetch_and((int *)0x80000001UL, 1, 0)",
        "__atomic_fetch_or((long *)0x80000004UL, 1, 0)",
        "__atomic_fetch_xor((int *)0x10000000UL, 1, 0)",
        "__atomic_fetch_and((long *)0x1000UL, 1, 0)",
    ] {
        let source = format!("int main(void) {{ {expression}; return 0; }}");
        let assembly = compile_to_assembly(&source, "trap.c").unwrap();
        let rom = assemble_boot_rom(&assembly).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        let report = run_headless(&mut machine, 100, &[], &mut Vec::new()).unwrap();
        assert!(report.traps > 0, "{source}");
        assert!(report.last_trap.is_some());
    }
}

#[test]
fn invalid_atomic_types_orders_arguments_and_reserved_names_preserve_rom() {
    let bodies = [
        "__atomic_load_n((char *)0x80000000UL, 0);",
        "__atomic_store_n((short *)0x80000000UL, 1, 0);",
        "__atomic_exchange_n((void *)0x80000000UL, 1, 0);",
        "__atomic_load_n(1, 0);",
        "__atomic_load_n((int *)0x80000000UL, 3);",
        "__atomic_load_n((int *)0x80000000UL, 4);",
        "__atomic_store_n((int *)0x80000000UL, 1, 1);",
        "__atomic_store_n((int *)0x80000000UL, 1, 2);",
        "__atomic_store_n((int *)0x80000000UL, 1, 4);",
        "__atomic_exchange_n((int *)0x80000000UL, 1, 6);",
        "__atomic_exchange_n((int *)0x80000000UL, 1, -1);",
        "int order = 0; __atomic_load_n((int *)0x80000000UL, order);",
        "__atomic_load_n((int *)0x80000000UL, 1 / 0);",
        "__atomic_load_n((int *)0x80000000UL, (void *)0);",
        "__atomic_store_n((int *)0x80000000UL, (void *)0, 0);",
        "return __atomic_store_n((int *)0x80000000UL, 1, 0);",
        "__atomic_load_n((int *)0x80000000UL);",
        "__atomic_exchange_n((int *)0x80000000UL, 0);",
        "__atomic_store_n((int *)0x80000000UL, 0, 0, 0);",
        "int __atomic_load_n = 0;",
    ];
    let mut sources: Vec<_> = bodies
        .iter()
        .map(|body| format!("int main(void) {{ {body} return 0; }}"))
        .collect();
    sources.extend([
        "int __atomic_load_n; int main(void) { return 0; }".to_owned(),
        "int __atomic_store_n(void) { return 0; } int main(void) { return 0; }".to_owned(),
        "typedef int __atomic_load_n; int main(void) { return 0; }".to_owned(),
        "enum { __atomic_exchange_n = 0 }; int main(void) { return 0; }".to_owned(),
    ]);
    for source in sources {
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&build.stderr).contains("guest.c:"),
            "{source}"
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn file_linkage_forward_declarations_private_recursion_and_poisoned_storage_execute() {
    let source = "
        typedef unsigned short u16;
        static u16 private[3] = { 1, 2, 3 };
        extern u16 private[3];
        static unsigned long visits;
        extern unsigned long visits;
        static u16 *cursor = private + 1;
        extern u16 *cursor;
        extern unsigned long shared;
        static int step(int n);
        extern int step(int n);
        int main(void) {
            if (visits || shared != 7) return 99;
            int sum = step(3);
            volatile unsigned long *report = (volatile unsigned long *)0x80000000UL;
            report[0] = (unsigned long)private;
            report[1] = (unsigned long)&visits;
            report[2] = (unsigned long)cursor;
            report[3] = (unsigned long)&shared;
            return sum + *cursor + visits + shared;
        }
        int step(int n) {
            if (!n) return 0;
            ++visits;
            *cursor += n;
            return n + step(n - 1);
        }
        static int step(int n);
        extern unsigned long shared = 7;
        unsigned long shared;
        extern unsigned long shared;
        static unsigned long visits;
    ";
    let assembly = compile_to_assembly(source, "linkage.c").unwrap();
    assert!(assembly.contains(".Lrv64_internal_step:"));
    assert!(assembly.contains("call .Lrv64_internal_step"));
    assert!(!assembly.lines().any(|line| line == "step:"));
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 64]).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert!(output.is_empty());
    assert_eq!(machine.core().register(10), Some(24));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut addresses = [0; 32];
    machine.bus_mut().read(0x80000000, &mut addresses).unwrap();
    let addresses: Vec<_> = addresses
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(addresses, [0x80010008, 0x80010018, 0x8001000a, 0x80010010]);
    let mut bytes = [0; 40];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    assert_eq!(&bytes[0..8], &0x8001000au64.to_le_bytes());
    assert_eq!(&bytes[8..16], &[1, 0, 8, 0, 3, 0, 0, 0]);
    assert_eq!(&bytes[16..24], &7u64.to_le_bytes());
    assert_eq!(&bytes[24..32], &3u64.to_le_bytes());
    assert_eq!(&bytes[32..40], &[0xa5; 8]);
}

#[test]
fn file_linkage_multi_declarators_tentative_data_and_aliases_execute() {
    let (machine, _) = execute(
        "typedef unsigned long word;
         extern word left, right;
         static unsigned char first = 255, second;
         extern unsigned char first, second;
         static word twice(word n);
         word twice(word n);
         word twice(word n) { return n + n; }
         int main(void) {
             if (left != 4 || right || second) return 99;
             right = twice(left);
             second = first;
             return right + second;
         }
         word left = 4, right;
         word left;
         extern word right;
         static unsigned char second;
         extern word twice(word n);",
    );
    assert_eq!(machine.core().register(10), Some(263));
}

#[test]
fn extern_declarations_do_not_allocate_duplicate_global_storage() {
    let (mut machine, _) = execute_with_budget(
        "extern unsigned char data[65536];
         extern unsigned char data[65536];
         int main(void) {
             data[0] = 17;
             data[65535] = 29;
             return sizeof(data) + data[0] + data[65535];
         }
         unsigned char data[65536];
         extern unsigned char data[65536];",
        400000,
    );
    assert_eq!(machine.core().register(10), Some(65582));
    let mut first = [0; 2];
    let mut last = [0; 2];
    machine.bus_mut().read(0x80010000, &mut first).unwrap();
    machine.bus_mut().read(0x8001fffe, &mut last).unwrap();
    assert_eq!(first, [17, 0]);
    assert_eq!(last, [0, 29]);
}

#[test]
fn file_linkage_external_function_symbols_remain_abi_callable() {
    let (machine, _) = execute(
        "extern unsigned int identity(unsigned int value);
         unsigned int identity(unsigned int value) { return value; }
         extern unsigned int identity(unsigned int value);
         extern int main(void);
         int main(void) { return identity(4294967295U) == 4294967295U; }",
    );
    assert_eq!(machine.core().register(10), Some(1));
    let assembly = compile_to_assembly(
        "extern int f(void); int f(void) { return 3; }
         int main(void) { return f(); }",
        "external.c",
    )
    .unwrap();
    assert!(assembly.lines().any(|line| line == "f:"));
    assert!(assembly.lines().any(|line| line == "call f"));
}

#[test]
fn invalid_file_linkage_declarations_visibility_and_definitions_preserve_output() {
    let cases = [
        "extern int missing; int main(void) { return 0; }",
        "extern int missing; int main(void) { return missing; }",
        "static int missing(void); int main(void) { return 0; }",
        "extern int missing(void); int main(void) { return missing(); }",
        "extern int x; static int x; int main(void) { return 0; }",
        "int x; static int x; int main(void) { return 0; }",
        "static int x; int x; int main(void) { return 0; }",
        "static int x; extern int x; int x; int main(void) { return 0; }",
        "extern int f(void); static int f(void) { return 0; } int main(void) { return f(); }",
        "int f(void); static int f(void) { return 0; } int main(void) { return f(); }",
        "static int x; extern long x; int main(void) { return 0; }",
        "extern int x; long x; int main(void) { return 0; }",
        "extern int a[2]; int a[3]; int main(void) { return 0; }",
        "extern int x = 1; int x = 2; int main(void) { return 0; }",
        "static int f(void); extern long f(void); int main(void) { return 0; }",
        "static int main(void) { return 0; }",
        "extern int main(void); static int main(void) { return 0; }",
        "static int x; extern int x(void); int main(void) { return 0; }",
        "extern int x; int main(void) { return 0; } int x(void) { return 1; }",
        "typedef int number; extern int number; int main(void) { return 0; }",
        "enum { VALUE = 1 }; extern int VALUE; int main(void) { return 0; }",
        "static int; int main(void) { return 0; }",
        "extern int; int main(void) { return 0; }",
        "static extern int x; int main(void) { return 0; }",
        "static static int x; int main(void) { return 0; }",
        "extern int a[]; int a[2]; int main(void) { return 0; }",
        "int main(void) { static int count; return 0; }",
        "int main(void) { extern int x; return 0; } int x;",
        "int f(static int n) { return n; } int main(void) { return 0; }",
        "int main(void) { return x; } extern int x; int x;",
        "int main(void) { return f(); } static int f(void) { return 1; }",
    ];
    for source in cases {
        let files = Files::new(source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&build.stderr).contains("guest.c:"),
            "{source}: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM",
            "{source}"
        );
    }
}

#[test]
fn invalid_and_unsupported_c_is_not_silently_compiled() {
    let cases = [
        "#include <stdio.h>\nint main(void) { return 0; }",
        "#define VALUE(a) a\nint main(void) { return VALUE(1, 2); }",
        "int main() { return 0; }",
        "int main(void) { unsigned long long a; return 0; }",
        "int main(void) { static int a; return 0; }",
        "int main(void) { const int a = 1; ++a; return 0; }",
        "int main(void) { return unknown; }",
        "int main(void) { int a; int a; return 0; }",
        "int main(void) { int a[2][3]; return 0; }",
        "int main(void) { return 1, 2; }",
        "int main(void) { return 1ULL; }",
        "int main(void) { return 18446744073709551616UL; }",
        "int main(void) { return; }",
        "void f(void) { return 1; } int main(void) { f(); return 0; }",
        "int f(int a) { return a; } int main(void) { return f(); }",
        "int f(void); int main(void) { return f(); }",
        "int main(void) { return f(); } int f(void) { return 1; }",
        "int f(int a); long f(int a) { return a; } int main(void) { return 0; }",
        "int main(void) { return 0; } int main(void) { return 1; }",
        "int main(void) { int f = 1; return f(); }",
        "int main(void) { return 0; unknown(); }",
        "int ***a; int main(void) { return 0; }",
        "int main(void) { volatile unsigned char *p = 1; return 0; }",
        "int main(void) { volatile unsigned char *p = (volatile unsigned char *)1; return p + p; }",
        "int main(void) { switch ((void *)0) { case 1: return 0; } }",
        "int main(void) { return 2 = 1; }",
        "int main(void) { int a = 1; volatile int *p = (volatile int *)0x10000000; p = a; return 0; }",
    ];
    for source in cases {
        let error = compile_to_assembly(source, "invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid.c"), "{source}: {error}");
    }
}

#[test]
fn switch_promotions_lp64_case_conversion_and_constant_expressions_execute() {
    let cases = [
        ("signed char n = -1;", "255", "-1"),
        ("unsigned char n = 255;", "-1", "255"),
        ("char n = 255;", "-1", "255"),
        ("short n = -32768;", "32768", "-32768"),
        ("unsigned short n = 65535;", "-1", "65535"),
        ("int n = -2147483647 - 1;", "2147483647", "0x80000000U"),
        ("unsigned int n = 4294967295U;", "0", "-1"),
        ("long n = -1;", "4294967295U", "18446744073709551615UL"),
        ("unsigned long n = 0x8000000000000000UL;", "0", "1UL << 63"),
        ("int n = 0;", "1", "0x100000000UL"),
        ("unsigned int n = 0;", "1", "0x100000000UL"),
        ("long n = 0x100000000L;", "0", "0x100000000UL"),
    ];
    for (declaration, miss, hit) in cases {
        let source = format!(
            "int main(void) {{ {declaration}
                switch (n) {{ case {miss}: return 1; case {hit}: return 7; default: return 9; }}
            }}"
        );
        let (machine, _) = execute(&source);
        assert_eq!(machine.core().register(10), Some(7), "{source}");
    }
    let (machine, _) = execute(
        "typedef unsigned long u64;
         enum { BASE = 3, COMMAND = BASE + 1 };
         int main(void) {
             switch ((u64)COMMAND) {
             case 0 && (1 / 0): return 1;
             case sizeof(u64) / 2: return 7;
             default: return 9;
             }
         }",
    );
    assert_eq!(machine.core().register(10), Some(7));
}

#[test]
fn switch_nested_break_continue_and_fallthrough_execute() {
    let (machine, output) = execute(
        "void put(unsigned char c) { *(volatile unsigned char *)0x10000000 = c; }
         int main(void) {
             int tails = 0;
             for (int i = 0; i < 6; ++i) {
                 switch (i) {
                 case 0: continue;
                 case 1:
                     while (1) {
                         switch (9) { default: put('A'); break; }
                         break;
                     }
                     break;
                 case 2:
                     switch (i) { case 2: put('B'); break; default: return 99; }
                 case 3: put('C'); continue;
                 default: put('D'); break;
                 }
                 put('E');
                 ++tails;
             }
             return tails;
         }",
    );
    assert_eq!(output, b"AEBCCDEDE");
    assert_eq!(machine.core().register(10), Some(3));
}

#[test]
fn switch_selector_side_effects_defaults_empty_cases_and_scopes_execute() {
    let (machine, output) = execute(
        "int calls;
         int select(void) { ++calls; return 2; }
         int main(void) {
             int n = 10;
             switch (select()) {
             case 1:
             case 2: { int n = 5; *(volatile unsigned char *)0x10000000 = n; }
             default: n += 3;
             case 4: n += 4; break;
             }
             switch (n++) {}
             switch (99) { case 1: n = 0; }
             switch (2) { default: n += 2; case 1: n += 3; break; case 2: n += 7; }
             switch (99) { default: n += 2; case 1: n += 3; break; case 2: n = 0; }
             volatile unsigned short *p = (volatile unsigned short *)0x80000000;
             *p = 65535;
             switch ((*p)++) { case 65535: break; default: return 99; }
             return n + calls + *p;
         }",
    );
    assert_eq!(output, [5]);
    assert_eq!(machine.core().register(10), Some(31));
    let mut bytes = [0; 2];
    let mut machine = machine;
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(bytes, [0, 0]);
    let (machine, output) = execute(
        "int main(void) {
             int n = 10;
             switch (0) {
             case 0: ; int n = 3;
             case 1: n += 4; *(volatile unsigned char *)0x10000000 = n; break;
             }
             return n;
         }",
    );
    assert_eq!(output, [7]);
    assert_eq!(machine.core().register(10), Some(10));
}

#[test]
fn switch_volatile_uart_selector_is_read_once_before_dispatch() {
    let assembly = compile_to_assembly(
        "int main(void) {
             volatile unsigned char *uart = (volatile unsigned char *)0x10000000;
             switch (*uart) {
             case 0: return 99;
             case 'X': *uart = 'A'; break;
             default: return 98;
             }
             *uart = *uart;
             return 7;
         }",
        "uart-switch.c",
    )
    .unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 10000, b"XY", &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(output, b"AY");
    assert_eq!(machine.core().register(10), Some(7));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn switch_recursive_returns_and_loop_continue_targets_execute() {
    let (machine, _) = execute(
        "int factorial(int n) {
             switch (n) { case 0: case 1: return 1; default: return n * factorial(n - 1); }
         }
         int main(void) {
             int i = 0;
             int count = 0;
             do {
                 ++i;
                 switch (i) { case 1: continue; default: ++count; }
             } while (i < 3);
             while (i < 5) {
                 ++i;
                 switch (i) { case 4: continue; default: ++count; }
             }
             return factorial(5) + count;
         }",
    );
    assert_eq!(machine.core().register(10), Some(123));
}

#[test]
fn invalid_switch_cases_selectors_scopes_and_control_targets_diagnose() {
    let cases = [
        "switch ((void *)0) {}",
        "switch (1) { case (int *)0: break; }",
        "int n = 1; switch (n) { case n: break; }",
        "int n = 1; switch (n) { case n++: break; }",
        "switch (1) { case main(): break; }",
        "switch (1) { case 1 / 0: break; }",
        "switch (1) { case 1 << 32: break; }",
        "switch (1) { case 2147483647 + 1: break; }",
        "switch (1) { case 1: break; case 1: break; }",
        "switch (0U) { case -1: break; case 4294967295U: break; }",
        "switch (0) { case 0: break; case 0x100000000UL: break; }",
        "switch (0L) { case -1: break; case 18446744073709551615UL: break; }",
        "switch (1) { default: break; default: break; }",
        "switch (1) { case 1: continue; }",
        "break;",
        "continue;",
        "case 1: return 0;",
        "default: return 0;",
        "switch (1) { case 1: { int hidden = 1; } case 2: return hidden; }",
        "switch (1) { case 1: { int hidden = 1; } } return hidden;",
        "switch (1) { case 1: ; int hidden = 1; } return hidden;",
        "switch (1) { case 1: ; int n = 1; case 2: ; int n = 2; }",
        "switch (1) { case 1: if (1) { case 2: break; } }",
    ];
    for body in cases {
        let source = format!("int main(void) {{ {body} return 0; }}");
        let files = Files::new(&source);
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.compile();
        assert!(!build.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&build.stderr).contains("guest.c:"),
            "{source}: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM",
            "{source}"
        );
    }
}

#[test]
fn lp64_unsigned_literals_conversions_and_arithmetic_execute() {
    let cases = [
        ("return 2147483648 > 0;", 1),
        ("return 4294967295 == 4294967295L;", 1),
        ("return 4294967295U > 2147483647;", 1),
        ("return 0xffffffff == -1;", 1),
        ("return 037777777777 == 4294967295U;", 1),
        ("return 0x8000000000000000 > 0;", 1),
        ("return 18446744073709551615UL > 0L;", 1),
        ("return 18446744073709551615U == -1L;", 1),
        ("return 18446744073709551615 == -1L;", 1),
        ("return 0xffffffffffffffffL == -1L;", 1),
        ("return 0XfEdCbA9876543210uL == 18364758544493064720UL;", 1),
        ("return (unsigned int)-1L == 4294967295U;", 1),
        ("return (unsigned long)-1 == 18446744073709551615UL;", 1),
        ("return (long)4294967295U == 4294967295L;", 1),
        ("return -1L < 1U;", 1),
        ("return -1 < 1U;", 0),
        ("return -1L < 1UL;", 0),
        ("return 4294967295U + 1U == 0U;", 1),
        ("return 0UL - 1UL == 18446744073709551615UL;", 1),
        ("return -4294967295U == 1U;", 1),
        ("return ~0U == 4294967295U;", 1),
        ("return ~0UL == 18446744073709551615UL;", 1),
        ("return 0x80000000U * 2U == 0;", 1),
        ("return (unsigned char)0xffffffffffffffffUL;", 255),
        ("return (int)0xffffffffUL;", -1),
        (
            "return 0x8000000000000000UL / 2UL == 0x4000000000000000UL;",
            1,
        ),
        ("return 0xffffffffffffffffUL % 16UL;", 15),
        ("return 4294967295U / 2U == 2147483647;", 1),
        ("return 4294967295U % 2U;", 1),
        ("return -17 / 5;", -3),
        ("return -17 % 5;", -2),
        ("return -17L / 5U;", -3),
        ("return -17L % 5U;", -2),
        ("return 1U << 31 == 0x80000000U;", 1),
        ("return 0x80000000U >> 31;", 1),
        ("return -4 >> 1;", -2),
        ("return -4L >> 1;", -2),
        ("return 0xffffffffffffffffUL >> 63;", 1),
        ("return 1UL << 63 == 0x8000000000000000UL;", 1),
        ("return 1U << 31UL == 0x80000000U;", 1),
        ("unsigned char a = 255; return a << 1;", 510),
        ("return -1 < 4294967295U;", 0),
        ("return -1 <= 4294967295U;", 1),
        ("return 0xffffffffffffffffUL >= 1;", 1),
        ("return 0xffffffffffffffffUL != 0;", 1),
    ];
    for (body, expected) in cases {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }
}

#[test]
fn unsigned_volatile_words_doublewords_and_nested_calls_execute() {
    let (mut machine, output) = execute(
        "unsigned long combine(unsigned int low, unsigned long high) {
                 return high | low;
             }
             unsigned int narrow(unsigned long value) { return value; }
             int main(void) {
                 volatile unsigned int *word = (volatile unsigned int *)0x80000000UL;
                 volatile unsigned long *wide = (volatile unsigned long *)0x80000008UL;
                 *word = 0xffffffffU;
                 *wide = combine(narrow(0x12345678ffffffffUL), 0xabcdef0100000000UL);
                 if (*word != 4294967295L) return 1;
                 if (*wide != 0xabcdef01ffffffffUL) return 2;
                 *(volatile unsigned char *)0x10000000 = (*wide >> 32) & 255U;
                 return *word / 0x10000000U;
             }",
    );
    assert_eq!(output, [1]);
    assert_eq!(machine.core().register(10), Some(15));
    let mut bytes = [0; 16];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(&bytes[..4], &u32::MAX.to_le_bytes());
    assert_eq!(&bytes[4..8], &[0; 4]);
    assert_eq!(&bytes[8..], &0xabcdef01ffffffff_u64.to_le_bytes());
}

#[test]
fn wide_literal_materialization_matches_independent_host_values() {
    let cases = [
        ("0UL", 0),
        ("2147483647L", 0x7fffffff),
        ("2147483648", 0x80000000),
        ("0xffffffffU", 0xffffffff),
        ("0x100000000UL", 0x100000000),
        ("0x1234567887654321UL", 0x1234567887654321),
        ("0x7ffffffffffff7ffUL", 0x7ffffffffffff7ff),
        ("0x7ffffffffffff800UL", 0x7ffffffffffff800),
        ("9223372036854775807L", 0x7fffffffffffffff),
        ("9223372036854775808", 0x8000000000000000),
        ("0x8000000000000800UL", 0x8000000000000800),
        ("0xaaaaaaaaaaaaaaaaUL", 0xaaaaaaaaaaaaaaaa),
        ("0xffffffff7fffffffUL", 0xffffffff7fffffff),
        ("0xffffffff80000000UL", 0xffffffff80000000),
        ("0xffffffffffffffffUL", u64::MAX),
    ];
    for (literal, expected) in cases {
        let source = format!(
            "unsigned long wide(void) {{ return {literal}; }}
             int main(void) {{ return 0; }}"
        );
        let assembly = compile_to_assembly(&source, "literal.c").unwrap().replacen(
            "call main\n",
            "call wide\n",
            1,
        );
        let rom = assemble_boot_rom(&assembly).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
        let report = run_headless(&mut machine, 1000, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0, "{literal}");
        assert!(report.waiting > 0, "{literal}");
        assert_eq!(machine.core().register(10), Some(expected), "{literal}");
    }
}

#[test]
fn unsigned_integer_abi_interoperates_with_independent_assembly() {
    let source = "
            unsigned int echo(unsigned int value) { return value; }
            unsigned int probe(unsigned int a, unsigned int b) { return 0; }
            unsigned int call_probe(void) { return probe(0x80000000U, echo(0xffffffffU)); }
            int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "unsigned-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Lunsigned_entry\n", 1)
        .replacen("\nprobe:\n", "\n.Lunused_probe:\n", 1);
    assembly.push_str(
        ".Lunsigned_entry:
             addi sp, sp, -16
             sd ra, 0(sp)
             li a0, -1
             call echo
             mv s2, a0
             call call_probe
             mv s3, a0
             ld ra, 0(sp)
             addi sp, sp, 16
             ret
             probe:
             andi t0, sp, 15
             bne t0, zero, .Lunsigned_bad
             li t0, -2147483648
             bne a0, t0, .Lunsigned_bad
             li t0, -1
             bne a1, t0, .Lunsigned_bad
             li a0, -2
             ret
             .Lunsigned_bad:
             li a0, 7
             ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(18), Some(u64::MAX));
    assert_eq!(machine.core().register(19), Some(u64::MAX - 1));
    assert_eq!(machine.core().register(2), Some(0x80010000));
}

#[test]
fn invalid_lp64_integer_literals_diagnose_without_truncation() {
    for literal in [
        "18446744073709551616UL",
        "0x10000000000000000",
        "1ULL",
        "1UU",
        "1LL",
        "09",
    ] {
        let source = format!("int main(void) {{ return {literal}; }}");
        let error = compile_to_assembly(&source, "literal.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("literal.c"), "{literal}: {error}");
    }
}
#[test]
fn declared_forward_calls_and_eight_byte_local_slot_limits_are_explicit() {
    let (machine, _) = execute(
        "long next(long value);
             int main(void) { return (int)next(4L); }
             long next(long value) { return value + 1L; }",
    );
    assert_eq!(machine.core().register(10), Some(5));
    let declarations: String = (0..120)
        .map(|i| format!("long value{i} = {i}L;\n"))
        .collect();
    let (machine, _) = execute(&format!(
        "int main(void) {{ {declarations} return (int)value119; }}"
    ));
    assert_eq!(machine.core().register(10), Some(119));
    let error = compile_to_assembly(
        &format!("int main(void) {{ {declarations} long extra; return 0; }}"),
        "limits.c",
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("120 local/parameter slots"), "{error}");
}

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Files(PathBuf);

impl Files {
    fn new(source: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "kiuas16-compiler-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("guest.c"), source).unwrap();
        Self(directory)
    }

    fn compile(&self) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_kiuas16-cc"))
            .arg(self.0.join("guest.c"))
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
fn enum_constants_use_lp64_evaluation_and_signed_int_runtime_types() {
    let (machine, _) = execute(
        "typedef unsigned short u16;
         enum {
             ZERO,
             ONE,
             MINIMUM = -2147483647 - 1,
             AFTER_MINIMUM,
             MAXIMUM = 2147483647,
             WRAPPED = (0xffffffffU + 1U),
             WIDTH = sizeof(unsigned long),
             SHIFT = 1UL << 20,
             MASK = SHIFT | 7,
             NEGATIVE = -7,
             NARROW = (u16)65538,
             SHORT_CIRCUIT = 1 ? NARROW : 1 / 0,
             LOGICAL = 0 && (1 / 0),
         };
         enum { NEXT = ONE + 2, FINAL };
         int main(void) {
             if (ZERO || ONE != 1 || NEXT != 3 || FINAL != 4) return 1;
             if (MINIMUM != (-2147483647 - 1) || AFTER_MINIMUM != -2147483647) return 2;
             if (MAXIMUM != 2147483647 || WRAPPED || WIDTH != 8) return 3;
             if (MASK != 1048583 || NEGATIVE != -7 || NARROW != 2 || SHORT_CIRCUIT != 2) return 4;
             if (LOGICAL || sizeof(MINIMUM) != 4 || sizeof(WIDTH) != 4 || sizeof(MAXIMUM) != 4) return 5;
             if (MINIMUM < 0L == 0 || MINIMUM < 1U) return 6;
             return 73;
         }",
    );
    assert_eq!(machine.core().register(10), Some(73));
}

#[test]
fn enum_bounds_initializers_and_pointer_offsets_execute_without_storage_objects() {
    let source = "
        enum { LENGTH = 4, FIRST = -1, SECOND = 32768, OFFSET = 2 };
        unsigned short data[LENGTH] = {FIRST, SECOND, LENGTH};
        unsigned short *cursor = data + OFFSET;
        int values[LENGTH] = {FIRST, SECOND};
        int sum(int n) { if (n) return LENGTH + sum(n - 1); return OFFSET; }
        int main(void) {
            unsigned char local[LENGTH] = {LENGTH, OFFSET};
            if (sizeof(data) != 8 || sizeof(values) != 16 || sizeof(local) != 4) return 1;
            if (data[0] != 65535 || data[1] != 32768 || *cursor != 4 || data[3]) return 2;
            if (values[0] != -1 || values[1] != 32768 || local[1] != 2 || local[3]) return 3;
            *(volatile unsigned long *)0x80000000UL = (unsigned long)data;
            *(volatile unsigned long *)0x80000008UL = (unsigned long)cursor;
            return sum(3);
        }";
    let (mut machine, _) = execute(source);
    assert_eq!(machine.core().register(10), Some(14));
    let mut bytes = [0; 16];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(&bytes[..8], &0x80010008_u64.to_le_bytes());
    assert_eq!(&bytes[8..], &0x8001000c_u64.to_le_bytes());
    let mut data = [0; 8];
    machine.bus_mut().read(0x80010008, &mut data).unwrap();
    assert_eq!(data, [255, 255, 0, 128, 4, 0, 0, 0]);
}

#[test]
fn unsupported_enum_forms_ranges_visibility_and_name_collisions_diagnose() {
    for source in [
        "enum { BAD = 2147483648 }; int main(void) { return 0; }",
        "enum { BAD = 0xffffffffU }; int main(void) { return 0; }",
        "enum { BAD = -2147483649L }; int main(void) { return 0; }",
        "enum { MAX = 2147483647, BAD }; int main(void) { return 0; }",
        "enum { BAD = 1 / 0 }; int main(void) { return 0; }",
        "enum { BAD = 2147483647 + 1 }; int main(void) { return 0; }",
        "enum { BAD = 1 << 32 }; int main(void) { return 0; }",
        "enum { BAD = (void *)0 }; int main(void) { return 0; }",
        "enum { BAD = FUTURE, FUTURE = 7 }; int main(void) { return 0; }",
        "int n; enum { BAD = n }; int main(void) { return 0; }",
        "int f(void) { return 1; } enum { BAD = f() }; int main(void) { return 0; }",
        "enum { }; int main(void) { return 0; }",
        "enum { SAME, SAME }; int main(void) { return 0; }",
        "enum tagged { VALUE }; int main(void) { return 0; }",
        "int main(void) { return sizeof(enum missing); }",
        "int main(void) { return (enum missing)1; }",
        "int f(enum missing n) { return n; } int main(void) { return 0; }",
        "enum { VALUE } object; int main(void) { return 0; }",
        "typedef enum { VALUE } name; int main(void) { return 0; }",
        "int main(void) { enum { VALUE }; return 0; }",
        "enum { VALUE }; int VALUE; int main(void) { return 0; }",
        "int VALUE; enum { VALUE }; int main(void) { return 0; }",
        "enum { VALUE }; int VALUE(void) { return 1; } int main(void) { return 0; }",
        "typedef int VALUE; enum { VALUE }; int main(void) { return 0; }",
        "enum { VALUE }; int main(void) { int VALUE = 1; return VALUE; }",
        "enum { VALUE }; int f(int VALUE) { return VALUE; } int main(void) { return 0; }",
        "int main(void) { return FUTURE; } enum { FUTURE = 7 };",
        "enum { VALUE }; int main(void) { VALUE = 7; return 0; }",
        "enum { VALUE }; int main(void) { return (long)&VALUE; }",
    ] {
        let error = compile_to_assembly(source, "enum-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("enum-invalid.c"), "{source}: {error}");
    }
    let files = Files::new("enum { BAD = 0xffffffffU }; int main(void) { return 0; }");
    std::fs::write(files.0.join("rom.bin"), b"previous").unwrap();
    assert!(!files.compile().status.success());
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"previous");
}

#[test]
fn scalar_typedefs_preserve_lp64_widths_promotions_and_array_bounds() {
    let (machine, _) = execute(
        "typedef unsigned char u8;
         typedef unsigned short u16;
         typedef unsigned int u32;
         typedef unsigned long u64;
         typedef signed short i16;
         typedef u64 address;
         typedef void nothing;
         typedef void *buffer;
         typedef u16 *half_pointer;
         u16 seed[] = {65535, 7};
         half_pointer cursor = seed;
         address high = 0xfedcba9876543210UL;
         nothing set(half_pointer p, u16 value) { *p = value; }
         i16 recurse(i16 n, u8 depth) {
             if (depth) return recurse(n + 1, depth - 1);
             return n;
         }
         int main(void) {
             u8 a = 255; u16 b = 65535; u32 c = 0xffffffffU;
             address d = high; i16 negative = 65535;
             u16 local[sizeof(address)] = {7};
             buffer opaque = local;
             half_pointer back = opaque;
             if (sizeof(u8) != 1 || sizeof(u16) != 2 || sizeof(u32) != 4) return 1;
             if (sizeof(address) != 8 || sizeof(buffer) != 8 || sizeof(local) != 16) return 2;
             if (a + 1 != 256 || b + 1 != 65536 || c + 1U || negative != -1) return 3;
             if (d != 0xfedcba9876543210UL || back != local || back[7]) return 4;
             set(cursor, (u16)65538);
             if (seed[0] != 2 || seed[1] != 7 || recurse(32766, 3) != -32767) return 5;
             volatile u16 *v = back;
             *(volatile u16 *)v = 11;
             if (back[0] != 11 || sizeof(volatile u16 *) != 8) return 6;
             return 67;
         }",
    );
    assert_eq!(machine.core().register(10), Some(67));
}

#[test]
fn alias_and_underlying_function_signatures_and_global_declarations_are_compatible() {
    let (machine, _) = execute(
        "typedef unsigned long word;
         typedef word *word_pointer;
         word total;
         unsigned long total = 9;
         word sum(word a, word b);
         unsigned long sum(unsigned long a, unsigned long b) { return a + b; }
         word_pointer address(word_pointer p) { return p; }
         int main(void) {
             word local = sum(total, 4UL);
             word_pointer p = address(&local);
             if (*p != 13 || sizeof(*p) != 8) return 1;
             return 71;
         }",
    );
    assert_eq!(machine.core().register(10), Some(71));
}

#[test]
fn unsupported_typedef_targets_and_namespace_conflicts_diagnose() {
    for source in [
        "typedef float real; int main(void) { return 0; }",
        "typedef double real; int main(void) { return 0; }",
        "typedef unsigned long long wide; int main(void) { return 0; }",
        "typedef int ***nested; int main(void) { return 0; }",
        "typedef int array[2]; int main(void) { return 0; }",
        "typedef int initialized = 7; int main(void) { return 0; }",
        "typedef int (*callback)(void,...); int main(void) { return 0; }",
        "typedef int word; typedef long word; int main(void) { return 0; }",
        "typedef int word; int word; int main(void) { return 0; }",
        "int word; typedef int word; int main(void) { return 0; }",
        "typedef int word; int main(void) { int word = 7; return word; }",
        "typedef int word; int f(int word) { return word; } int main(void) { return 0; }",
        "typedef int word; int word(void) { return 1; } int main(void) { return 0; }",
        "int word(void) { return 1; } typedef int word; int main(void) { return 0; }",
        "typedef volatile int object; int main(void) { object n; int *p = &n; return 0; }",
        "typedef int *pointer; int main(void) { volatile pointer p; return 0; }",
        "typedef int *pointer; int main(void) { sizeof(volatile pointer); return 0; }",
        "int main(void) { typedef int word; return 0; }",
        "int main(void) { word n; return 0; } typedef int word;",
        "typedef int word; int main(void) { word *p = (long *)0; return 0; }",
    ] {
        let error = compile_to_assembly(source, "aliases-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("aliases-invalid.c"), "{source}: {error}");
    }
    let files = Files::new("typedef float hidden; int main(void) { return 0; }");
    std::fs::write(files.0.join("rom.bin"), b"previous").unwrap();
    assert!(!files.compile().status.success());
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"previous");
}

#[test]
fn generic_object_pointers_round_trip_supported_types_and_qualifications() {
    for ty in [
        "char",
        "signed char",
        "unsigned char",
        "short",
        "unsigned short",
        "int",
        "unsigned int",
        "long",
        "unsigned long",
    ] {
        let source = format!(
            "void *echo(void *p) {{ return p; }}
             volatile void *qualified(volatile void *p) {{ return p; }}
             int main(void) {{
                 {ty} value = 7;
                 {ty} *p = &value;
                 void *opaque = p;
                 {ty} *back = echo(opaque);
                 volatile {ty} *v = qualified(p);
                 volatile void *q = v;
                 volatile {ty} *again = q;
                 if (opaque != p || p != opaque || back != p || again != v) return 1;
                 if (q != opaque || opaque != q || !q || !opaque) return 2;
                 *back = 9; *again += 2;
                 if (value != 11 || sizeof(opaque) != 8 || sizeof(q) != 8) return 3;
                 {ty} *explicit = ({ty} *)(void *)v;
                 if (explicit != p || *explicit != 11) return 4;
                 return 59;
             }}"
        );
        let (machine, _) = execute(&source);
        assert_eq!(machine.core().register(10), Some(59), "{ty}");
    }
}

#[test]
fn generic_global_addresses_resolve_final_layout_and_preserve_pointer_bytes() {
    let source = "
        unsigned short data[3];
        void *a = &data[1];
        volatile void *b = &data[2];
        void *c = (void *)0xfedcba9876543210UL;
        void *d = (void *)0;
        void *e;
        void *f = 1 ? &data[0] : (void *)0;
        unsigned short data[3] = {7, 11, 13};
        void *echo(void *p) { return p; }
        int main(void) {
            unsigned short *p = a;
            volatile unsigned short *q = b;
            if (*p != 11 || *q != 13 || d || e) return 1;
            if (a != &data[1] || b != &data[2] || f != data) return 2;
            if ((unsigned long)c != 0xfedcba9876543210UL) return 3;
            e = echo(data + 2);
            q = e;
            *q += 1;
            if (data[2] != 14) return 4;
            *(volatile unsigned long *)0x80000000UL = (unsigned long)data;
            return 61;
        }";
    let assembly = compile_to_assembly(source, "generic-globals.c").unwrap();
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    machine.bus_mut().write(0x80010000, &[0xa5; 56]).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(61));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
    let mut bytes = [0; 56];
    machine.bus_mut().read(0x80010000, &mut bytes).unwrap();
    let expected: Vec<u8> = [
        0x80010022_u64,
        0x80010024,
        0xfedcba9876543210,
        0,
        u64::from_le_bytes([7, 0, 11, 0, 14, 0, 0, 0]),
        0x80010020,
        0x80010024,
    ]
    .iter()
    .flat_map(|n| n.to_le_bytes())
    .collect();
    assert_eq!(bytes.as_slice(), expected);
}

#[test]
fn generic_conditionals_merge_qualifiers_and_skip_unselected_calls() {
    let (machine, output) = execute(
        "int n = 7;
         short s = 11;
         int calls;
         void *pick(void *p) { ++calls; return p; }
         int main(void) {
             int *p = &n;
             volatile int *v = p;
             void *opaque = p;
             volatile void *a = 1 ? opaque : v;
             volatile void *b = 0 ? v : opaque;
             if (a != p || b != p) return 1;
             volatile int *back = a;
             if (*back != 7) return 2;
             void *chosen = 1 ? pick(&s) : pick(p);
             short *q = chosen;
             if (*q != 11 || calls != 1) return 3;
             if ((0 ? opaque : p) != p || (1 ? p : opaque) != p) return 4;
             volatile void *qualified = v;
             if ((0 ? opaque : qualified) != p) return 5;
             *(volatile unsigned char *)0x10000000UL = calls;
             return 63;
         }",
    );
    assert_eq!(machine.core().register(10), Some(63));
    assert_eq!(output, [1]);
}

#[test]
fn generic_pointer_abi_interoperates_with_independent_register_and_stack_calls() {
    let source = "
        void *echo(void *p) { return p; }
        volatile void *probe(void *a, int b, int c, int d, int e, int f,
                             int g, int h, volatile void *i) { return i; }
        volatile void *caller(void) {
            return probe((void *)0xfedcba9876543210UL, 0, 0, 0, 0, 0, 0, 0,
                         echo((void *)0x80000000UL));
        }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "generic-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Lgeneric_entry\n", 1)
        .replacen("\nprobe:\n", "\n.Lunused_probe:\n", 1);
    assembly.push_str(
        ".Lgeneric_entry:
         addi sp, sp, -16
         sd ra, 0(sp)
         li a0, -1
         call echo
         mv s2, a0
         call caller
         mv s3, a0
         ld ra, 0(sp)
         addi sp, sp, 16
         ret
         probe:
         andi t0, sp, 15
         bne t0, zero, .Lgeneric_bad
         srli t0, a0, 32
         li t1, -19088744
         slli t1, t1, 32
         srli t1, t1, 32
         bne t0, t1, .Lgeneric_bad
         slli t0, a0, 32
         srli t0, t0, 32
         li t1, 1985229328
         bne t0, t1, .Lgeneric_bad
         ld t0, 0(sp)
         li t1, 1
         slli t1, t1, 31
         bne t0, t1, .Lgeneric_bad
         mv a0, t0
         ret
         .Lgeneric_bad:
         li a0, 7
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(18), Some(u64::MAX));
    assert_eq!(machine.core().register(19), Some(0x80000000));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn invalid_generic_pointer_operations_and_qualification_removal_diagnose() {
    for body in [
        "void *p = (void *)0; p++;",
        "void *p = (void *)0; --p;",
        "void *p = (void *)0; p += 1;",
        "void *p = (void *)0; p -= 1;",
        "void *p = (void *)0; p + 1;",
        "void *p = (void *)0; 1 + p;",
        "void *p = (void *)0; p - p;",
        "void *p = (void *)0; p[0];",
        "void *p = (void *)0; *p;",
        "void *p = (void *)0; sizeof(*p);",
        "void *p = (void *)0; p < p;",
        "void *p = (void *)0; p >= p;",
        "void *p = (void *)0; p();",
        "void **p = (void **)0; &p;",
        "void ***p;",
        "void **a[2];",
        "void *p = 1;",
        "volatile int *p = (volatile int *)0; void *q = p;",
        "volatile void *p = (volatile void *)0; int *q = p;",
        "volatile void *p = (volatile void *)0; void *q = p;",
        "int *p = (int *)0; volatile void *v = p; void *q = 1 ? p : v;",
        "void *p = (void *)0; volatile int *v = (volatile int *)0; void *q = 0 ? p : v;",
        "int *p = (int *)0; long *q = (long *)0; void *v = 1 ? p : q;",
        "void *p = (void *)0; int *q = (int *)0; p < q;",
        "void *p = (void *)0; return p;",
    ] {
        let source = format!("int main(void) {{ {body} return 0; }}");
        let error = compile_to_assembly(&source, "generic-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("generic-invalid.c"), "{body}: {error}");
    }
}

#[test]
fn halfword_storage_guest_preserves_descriptor_neighbors_and_ring_entries() {
    let source = include_str!("fixtures/kiuas16_storage.c");
    let rom = assemble_boot_rom(&compile_to_assembly(source, "storage.c").unwrap()).unwrap();
    let files = Files::new(source);
    let mut disk = vec![0; 1024];
    disk[512..].fill(b'R');
    let path = files.0.join("disk.raw");
    for writable in [false, true] {
        std::fs::write(&path, &disk).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom.clone(), 0x20000).unwrap();
        machine
            .bus_mut()
            .attach_root_block(Box::new(FileSectorMedia::open(&path, !writable).unwrap()))
            .unwrap();
        machine.bus_mut().write(0x80000000, &[0xa5; 64]).unwrap();
        machine.bus_mut().write(0x80000100, &[0xa5; 20]).unwrap();
        let mut output = Vec::new();
        let report = run_headless(&mut machine, 20000, &[], &mut output).unwrap();
        assert_eq!(report.traps, 0, "{:?}", report.last_trap);
        assert!(report.waiting > 0);
        assert_eq!(output, [b'C', 0, b'R', u8::from(!writable), 0, 1, 3, 0]);
        assert_eq!(machine.core().register(10), Some(0));
        assert_eq!(machine.core().register(2), Some(0x80010000));
        assert_eq!(machine.core().register(8), Some(0));
        let mut descriptors = [0; 64];
        machine
            .bus_mut()
            .read(0x80000000, &mut descriptors)
            .unwrap();
        let mut expected = Vec::new();
        for (address, length, flags, next) in [
            (0x80000300_u64, 16_u32, 1_u16, 2_u16),
            (0x80000400, 512, 1, 2),
            (0x80000600, 1, 2, 0),
        ] {
            expected.extend_from_slice(&address.to_le_bytes());
            expected.extend_from_slice(&length.to_le_bytes());
            expected.extend_from_slice(&flags.to_le_bytes());
            expected.extend_from_slice(&next.to_le_bytes());
        }
        expected.extend_from_slice(&[0xa5; 16]);
        assert_eq!(descriptors.as_slice(), expected);
        let mut available = [0; 20];
        machine.bus_mut().read(0x80000100, &mut available).unwrap();
        assert_eq!(&available[..10], &[0, 0, 3, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&available[10..], &[0xa5; 10]);
        let mut used = [0; 4];
        machine.bus_mut().read(0x80000200, &mut used).unwrap();
        assert_eq!(used, [0, 0, 3, 0]);
        let mut copy_addresses = [0; 16];
        machine
            .bus_mut()
            .read(0x80000700, &mut copy_addresses)
            .unwrap();
        let snapshot = u64::from_le_bytes(copy_addresses[..8].try_into().unwrap());
        assert_eq!(
            u64::from_le_bytes(copy_addresses[8..].try_into().unwrap()),
            0x80000400
        );
        let mut copied = [0; 16];
        machine.bus_mut().read(snapshot, &mut copied).unwrap();
        assert_eq!(&copied[..1], b"W");
        assert_eq!(&copied[1..], &[b'R'; 15]);
        let mut buffer = [0; 512];
        machine.bus_mut().read(0x80000400, &mut buffer).unwrap();
        assert_eq!(buffer[0], b'W');
        assert_eq!(&buffer[1..], &[b'R'; 511]);
        drop(machine);
        let mut expected_disk = disk.clone();
        if writable {
            expected_disk[512] = b'W';
        }
        assert_eq!(std::fs::read(&path).unwrap(), expected_disk);
    }
}

#[test]
fn halfword_promotions_narrowing_aliases_and_updates_execute() {
    for (body, expected) in [
        ("short n = 65535; return n;", -1),
        ("unsigned short n = -1; return n + 1;", 65536),
        ("short n = 32768; return n;", -32768),
        ("short n = -32768; return n / 2;", -16384),
        ("short n = -1; return n < 1U;", 0),
        ("short n = -2; return n >> 1;", -1),
        ("unsigned short n = 32768; return n >> 1;", 16384),
        ("unsigned short n = 256; return n * n == 65536;", 1),
        ("short n = 32767; int old = n++; return old + n;", -1),
        ("short n = -32768; return --n;", 32767),
        ("short n = 32767; return n += 1;", -32768),
        ("unsigned short n = 65535; return ++n;", 0),
        ("unsigned short n = 32768; return n <<= 1;", 0),
        ("short n = 0; short *p = &n; *p = 65535; return n;", -1),
        (
            "unsigned short n = 0; unsigned short *p = &n; *p = -1; return n;",
            65535,
        ),
        (
            "return sizeof(short) + sizeof(unsigned short) + sizeof(short *);",
            12,
        ),
        ("unsigned short n = 1; return sizeof(n + n);", 4),
    ] {
        let (machine, _) = execute(&format!("int main(void) {{ {body} }}"));
        assert_eq!(
            machine.core().register(10),
            Some(expected as i64 as u64),
            "{body}"
        );
    }
}

#[test]
fn halfword_arrays_globals_static_addresses_and_volatile_bytes_execute() {
    let (mut machine, _) = execute(
        "short signed_values[] = {32768, 65535, 32767};
         unsigned short unsigned_values[4] = {65536, -1, 0x1234};
         short *signed_cursor = signed_values + 1;
         volatile unsigned short *cursor = &unsigned_values[2];
         int main(void) {
             short local[3] = {65535, 32768};
             short *p = local + 2;
             if (sizeof(signed_values) != 6 || sizeof(unsigned_values) != 8) return 1;
             if (signed_values[0] != -32768 || *signed_cursor != -1) return 2;
             if (unsigned_values[0] || unsigned_values[1] != 65535 || unsigned_values[3]) return 3;
             if (p - local != 2 || p[-1] != -32768 || *p || local[0] != -1) return 4;
             if ((unsigned long)cursor != (unsigned long)unsigned_values + 4UL) return 5;
             *cursor += 1;
             volatile short *s = (volatile short *)0x80000002UL;
             volatile unsigned short *u = (volatile unsigned short *)0x80000004UL;
             *(volatile unsigned long *)0x80000000UL = 0xa5a5a5a5a5a5a5a5UL;
             *s = 0x8000; *u = 0xffff;
             if (*s != -32768 || *u != 65535) return 6;
             *(volatile unsigned long *)0x80000008UL = (unsigned long)signed_values;
             *(volatile unsigned long *)0x80000010UL = (unsigned long)unsigned_values;
             return 53;
         }",
    );
    assert_eq!(machine.core().register(10), Some(53));
    let mut bytes = [0; 24];
    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
    assert_eq!(&bytes[..8], &[0xa5, 0xa5, 0, 0x80, 0xff, 0xff, 0xa5, 0xa5]);
    for (address, expected) in bytes[8..].chunks_exact(8).zip([
        [0, 0x80, 0xff, 0xff, 0xff, 0x7f, 0, 0],
        [0, 0, 0xff, 0xff, 0x35, 0x12, 0, 0],
    ]) {
        let mut data = [0; 8];
        machine
            .bus_mut()
            .read(u64::from_le_bytes(address.try_into().unwrap()), &mut data)
            .unwrap();
        assert_eq!(data, expected);
    }
    for bound in ["(unsigned short)65538", "sizeof(short)", "sizeof(\"X\")"] {
        let (machine, _) = execute(&format!(
            "int main(void) {{ short a[{bound}] = {{7}}; return sizeof(a); }}"
        ));
        assert_eq!(machine.core().register(10), Some(4));
    }
}

#[test]
fn halfword_updates_survive_nested_calls_and_qualification_conversions() {
    let (machine, output) = execute(
        "short values[3] = {32767, -32768, 7};
         int calls;
         short *target(void) { ++calls; return values; }
         int operand(void) { ++calls; return 1; }
         short recur(short n, unsigned short depth) {
             if (depth) return recur(n + 1, depth - 1);
             return n;
         }
         int main(void) {
             volatile short *v = values;
             short *p = (short *)v;
             if (p != v || (1 ? p : v) != p || v - p) return 1;
             short old = (*target())++;
             if (old != 32767 || values[0] != -32768 || calls != 1) return 2;
             *target() += operand();
             if (values[0] != -32767 || calls != 3) return 3;
             if (recur(32766, 3) != -32767) return 4;
             signed short int s = -1;
             unsigned short int u = 65535;
             volatile signed short int *sv = &s;
             volatile unsigned short int *uv = &u;
             if (*sv != -1 || *uv != 65535) return 5;
             *(volatile unsigned char *)0x10000000UL = calls;
             return 57;
         }",
    );
    assert_eq!(machine.core().register(10), Some(57));
    assert_eq!(output, [3]);
}

#[test]
fn halfword_abi_matches_independent_register_and_stack_partners() {
    let source = "
        short echo(short n) { return n; }
        unsigned short echo_unsigned(unsigned short n) { return n; }
        short probe(short a, unsigned short b, int c, int d, int e, int f,
                    int g, int h, short i, unsigned short j) { return 0; }
        short caller(void) {
            return probe(-32768, 65535, 0, 0, 0, 0, 0, 0, echo(-1), 32768);
        }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "halfword-abi.c")
        .unwrap()
        .replacen("call main\n", "call .Lhalfword_entry\n", 1)
        .replacen("\nprobe:\n", "\n.Lunused_probe:\n", 1);
    assembly.push_str(
        ".Lhalfword_entry:
         addi sp, sp, -16
         sd ra, 0(sp)
         li a0, 65535
         call echo
         mv s2, a0
         li a0, -1
         call echo_unsigned
         mv s3, a0
         call caller
         mv s4, a0
         ld ra, 0(sp)
         addi sp, sp, 16
         ret
         probe:
         andi t0, sp, 15
         bne t0, zero, .Lhalfword_bad
         li t0, -32768
         bne a0, t0, .Lhalfword_bad
         li t0, 65535
         bne a1, t0, .Lhalfword_bad
         ld t1, 0(sp)
         li t0, -1
         bne t1, t0, .Lhalfword_bad
         ld t1, 8(sp)
         li t0, 32768
         bne t1, t0, .Lhalfword_bad
         li a0, -2
         ret
         .Lhalfword_bad:
         li a0, 7
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 10000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(18), Some(u64::MAX));
    assert_eq!(machine.core().register(19), Some(65535));
    assert_eq!(machine.core().register(20), Some(u64::MAX - 1));
    assert_eq!(machine.core().register(2), Some(0x80010000));
    assert_eq!(machine.core().register(8), Some(0));
}

#[test]
fn invalid_halfword_types_initializers_and_qualifications_diagnose() {
    for source in [
        "int main(void) { unsigned short *p = (short *)0; return 0; }",
        "int main(void) { short *p = (int *)0; return 0; }",
        "int main(void) { short *p = 1; return 0; }",
        "int main(void) { volatile short *v = (volatile short *)0; short *p = v; return 0; }",
        "int main(void) { volatile unsigned short *v = (volatile unsigned short *)0; unsigned short *p = v; return 0; }",
        "int main(void) { short ***p; return 0; }",
        "int main(void) { short **a[2]; return 0; }",
        "int main(void) { short a[481]; return 0; }",
        "short a[32769]; int main(void) { return 0; }",
        "short a; unsigned short a; int main(void) { return 0; }",
        "int main(void) { volatile short a; short *p = &a; return 0; }",
        "int main(void) { short a[2] = \"AB\"; return 0; }",
    ] {
        let error = compile_to_assembly(source, "halfword-invalid.c")
            .unwrap_err()
            .to_string();
        assert!(error.contains("halfword-invalid.c"), "{source}: {error}");
    }
}

#[test]
fn c_source_to_production_rom_cli_executes_guest_storage_without_host_queue_setup() {
    let files = Files::new(include_str!("fixtures/kiuas16_storage.c"));
    let build = files.compile();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(build.stdout.is_empty());
    let mut disk = vec![0; 1024];
    disk[512..].fill(b'R');
    std::fs::write(files.0.join("disk.raw"), &disk).unwrap();
    for writable in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"));
        command
            .arg("--rom")
            .arg(files.0.join("rom.bin"))
            .arg("--root")
            .arg(files.0.join("disk.raw"))
            .args(["--steps", "20000", "--deny-traps"]);
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
            [b'C', 0, b'R', u8::from(!writable), 0, 1, 3, 0]
        );
        let mut expected = disk.clone();
        if writable {
            expected[512] = b'W';
        }
        assert_eq!(std::fs::read(files.0.join("disk.raw")).unwrap(), expected);
    }
}

#[test]
fn compiler_cli_preserves_source_and_existing_output_on_invalid_input() {
    let files = Files::new("int main(void) { return missing; }");
    let source = files.0.join("guest.c");
    let original = std::fs::read(&source).unwrap();
    std::fs::write(files.0.join("rom.bin"), b"keep").unwrap();
    let result = files.compile();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("guest.c:1"));
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"keep");
    let result = Command::new(env!("CARGO_BIN_EXE_kiuas16-cc"))
        .arg(&source)
        .arg(&source)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("overwrite"));
    assert_eq!(std::fs::read(&source).unwrap(), original);

    let oversized = format!("int main(void) {{ {} return 0; }}", "1 + 2;\n".repeat(3000));
    std::fs::write(&source, oversized).unwrap();
    let result = files.compile();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("exceeds boot ROM size"));
    assert_eq!(std::fs::read(files.0.join("rom.bin")).unwrap(), b"keep");
}

#[test]
fn volatile_loads_store_widths_and_assignment_results_execute() {
    let (machine, _) = execute(
        "long ram(void) { return 2147483647L + 1L; }
             int main(void) {
                 volatile long *wide = (volatile long *)ram();
                 volatile int *word = (volatile int *)(ram() + 8L);
                 volatile unsigned char *byte = (volatile unsigned char *)(ram() + 12L);
                 *wide = 2147483647L + 2L;
                 *word = -17;
                 *byte = 258;
                 if (*wide != 2147483647L + 2L) return 1;
                 if (*word != -17) return 2;
                 if (*byte != 2) return 3;
                 return *byte = 259;
             }",
    );
    assert_eq!(machine.core().register(10), Some(3));
}

#[test]
fn independently_assembled_caller_checks_compiled_parameter_and_saved_register_rules() {
    let source = "
        long weighted(long a, long b, long c, long d, long e, long f, long g, long h) {
            return a + b*2L + c*3L + d*4L + e*5L + f*6L + g*7L + h*8L;
        }
        long sign(int value) { return value; }
        unsigned char clip(int value) { return value; }
        int main(void) { return 0; }";
    let mut assembly = compile_to_assembly(source, "abi.c").unwrap().replacen(
        "call main\n",
        "call .Labi_entry\n",
        1,
    );
    assembly.push_str(
        ".Labi_entry:
         addi sp, sp, -16
         sd ra, 0(sp)
         li s0, 77
         li s1, 88
         li a0, 1
         li a1, 2
         li a2, 3
         li a3, 4
         li a4, 5
         li a5, 6
         li a6, 7
         li a7, 8
         call weighted
         mv s2, a0
         li a0, -17
         call sign
         mv s3, a0
         li a0, 258
         call clip
         mv s4, a0
         ld ra, 0(sp)
         addi sp, sp, 16
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 3000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    for (register, value) in [
        (2, 0x80010000),
        (8, 77),
        (9, 88),
        (18, 204),
        (19, (-17_i64) as u64),
        (20, 2),
    ] {
        assert_eq!(machine.core().register(register), Some(value));
    }
}

#[test]
fn independently_assembled_callee_checks_compiled_call_alignment_and_arguments() {
    let source = "
        long probe(long a, long b, long c, long d, long e, long f, long g, long h) { return 0L; }
        long twice(long a) { return a*2L; }
        int main(void) { return (int)probe(1L, 2L, 3L, 4L, 5L, 6L, 7L, twice(4L)); }";
    let mut assembly = compile_to_assembly(source, "abi.c").unwrap().replacen(
        "\nprobe:\n",
        "\n.Lunused_probe:\n",
        1,
    );
    assembly.push_str(
        "probe:
         andi t0, sp, 15
         bne t0, zero, .Lbad_alignment
         add a0, a0, a1
         add a0, a0, a2
         add a0, a0, a3
         add a0, a0, a4
         add a0, a0, a5
         add a0, a0, a6
         add a0, a0, a7
         ret
         .Lbad_alignment:
         li a0, -1
         ret",
    );
    let rom = assemble_boot_rom(&assembly).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let report = run_headless(&mut machine, 3000, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert_eq!(machine.core().register(10), Some(36));
    assert_eq!(machine.core().register(2), Some(0x80010000));
}
