use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use suomicpu16::assembler::rv64::assemble_boot_rom;
use suomicpu16::kiuas16::runtime::run_headless;
use suomicpu16::kiuas16::{BOOT_ROM_BYTES, Kiuas16Machine};

#[test]
fn rv64m_remaining_operations_match_independent_instruction_words() {
    for (operation, expected) in [
        ("mulh", 0x02c59533u32),
        ("mulhsu", 0x02c5a533),
        ("mulhu", 0x02c5b533),
        ("mulw", 0x02c5853b),
        ("divw", 0x02c5c53b),
        ("divuw", 0x02c5d53b),
        ("remw", 0x02c5e53b),
        ("remuw", 0x02c5f53b),
    ] {
        let rom = assemble_boot_rom(&format!("{operation} a0, a1, a2")).unwrap();
        assert_eq!(&rom[..4], &expected.to_le_bytes(), "{operation}");
        let numeric = assemble_boot_rom(&format!("{operation} x10, x11, x12")).unwrap();
        assert_eq!(rom, numeric);
    }
}

#[test]
fn rv64m_high_products_execute_signed_unsigned_edges_and_register_aliases() {
    let values = [
        0u64,
        1,
        2,
        u64::MAX,
        i64::MIN as u64,
        i64::MAX as u64,
        0x0123456789abcdef,
    ];
    for operation in ["mulh", "mulhsu", "mulhu"] {
        for destination in ["a0", "a1", "a2", "zero"] {
            let rom = assemble_boot_rom(&format!(
                "la t0, values
                 ld a1, 0(t0)
                 ld a2, 8(t0)
                 {operation} {destination}, a1, a2
                 wfi
                 .balign 8
                 values: .dword 0, 0"
            ))
            .unwrap();
            for left in values {
                for right in values {
                    let expected = match operation {
                        "mulh" => (((left as i64 as i128) * (right as i64 as i128)) >> 64) as u64,
                        "mulhsu" => (((left as i64 as i128) * i128::from(right)) >> 64) as u64,
                        _ => ((u128::from(left) * u128::from(right)) >> 64) as u64,
                    };
                    let mut image = rom.clone();
                    image[24..32].copy_from_slice(&left.to_le_bytes());
                    image[32..40].copy_from_slice(&right.to_le_bytes());
                    let mut machine = Kiuas16Machine::new_with_ram_size(image, 4096).unwrap();
                    let report = run_headless(&mut machine, 12, &[], &mut Vec::new()).unwrap();
                    assert_eq!(report.traps, 0);
                    assert!(report.waiting > 0);
                    assert_eq!(machine.core().register(0), Some(0));
                    for (name, register, original) in
                        [("a0", 10, 0), ("a1", 11, left), ("a2", 12, right)]
                    {
                        assert_eq!(
                            machine.core().register(register),
                            Some(if destination == name {
                                expected
                            } else {
                                original
                            }),
                            "{operation} {destination}: {left:#x}, {right:#x}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn rv64m_word_operations_execute_truncation_overflow_zero_divisors_and_sign_extension() {
    let values = [
        0u64,
        1,
        2,
        7,
        (-7i64) as u64,
        u64::MAX,
        0x80000000,
        0x7fffffff,
        0x1234567800000000,
        0xfedcba9880000001,
        0xabcdef01ffffffff,
    ];
    for operation in ["mulw", "divw", "divuw", "remw", "remuw"] {
        for destination in ["a0", "a1", "a2", "zero"] {
            let rom = assemble_boot_rom(&format!(
                "la t0, values
                 ld a1, 0(t0)
                 ld a2, 8(t0)
                 {operation} {destination}, a1, a2
                 wfi
                 .balign 8
                 values: .dword 0, 0"
            ))
            .unwrap();
            for left in values {
                for right in values {
                    let a = left as u32;
                    let b = right as u32;
                    let signed_a = i64::from(a as i32);
                    let signed_b = i64::from(b as i32);
                    let word = match operation {
                        "mulw" => (u64::from(a) * u64::from(b)) as u32,
                        "divw" | "divuw" if b == 0 => u32::MAX,
                        "remw" | "remuw" if b == 0 => a,
                        "divw" => (signed_a / signed_b) as u32,
                        "divuw" => a / b,
                        "remw" => (signed_a % signed_b) as u32,
                        _ => a % b,
                    };
                    let expected = word as i32 as i64 as u64;
                    let mut image = rom.clone();
                    image[24..32].copy_from_slice(&left.to_le_bytes());
                    image[32..40].copy_from_slice(&right.to_le_bytes());
                    let mut machine = Kiuas16Machine::new_with_ram_size(image, 4096).unwrap();
                    let report = run_headless(&mut machine, 12, &[], &mut Vec::new()).unwrap();
                    assert_eq!(report.traps, 0);
                    assert!(report.waiting > 0);
                    assert_eq!(machine.core().register(0), Some(0));
                    for (name, register, original) in
                        [("a0", 10, 0), ("a1", 11, left), ("a2", 12, right)]
                    {
                        assert_eq!(
                            machine.core().register(register),
                            Some(if destination == name {
                                expected
                            } else {
                                original
                            }),
                            "{operation} {destination}: {left:#x}, {right:#x}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn rv64m_invalid_operands_preserve_existing_rom() {
    for operation in [
        "mulh", "mulhsu", "mulhu", "mulw", "divw", "divuw", "remw", "remuw",
    ] {
        for operands in [
            "",
            "a0, a1",
            "a0, a1, a2, a3",
            "x32, a1, a2",
            "a0, x32, a2",
            "a0, a1, 2",
        ] {
            let source = format!("{operation} {operands}");
            let diagnostic = assemble_boot_rom(&source).unwrap_err().to_string();
            assert!(diagnostic.starts_with("line 1:"), "{diagnostic}");
            let files = Files::new();
            std::fs::write(files.0.join("guest.S"), source).unwrap();
            std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
            let build = files.assemble();
            assert!(!build.status.success());
            assert!(!build.stderr.is_empty());
            assert_eq!(
                std::fs::read(files.0.join("rom.bin")).unwrap(),
                b"previous ROM"
            );
        }
    }
}

#[test]
fn branch_aliases_match_independent_bytes_and_base_branch_ranges() {
    for (alias, base, expected) in [
        ("beqz a0", "beq a0, zero", 0x00050463u32),
        ("bnez a0", "bne a0, zero", 0x00051463),
        ("blez a0", "bge zero, a0", 0x00a05463),
        ("bgez a0", "bge a0, zero", 0x00055463),
        ("bltz a0", "blt a0, zero", 0x00054463),
        ("bgtz a0", "blt zero, a0", 0x00a04463),
        ("bgt a0, a1", "blt a1, a0", 0x00a5c463),
        ("ble a0, a1", "bge a1, a0", 0x00a5d463),
        ("bgtu a0, a1", "bltu a1, a0", 0x00a5e463),
        ("bleu a0, a1", "bgeu a1, a0", 0x00a5f463),
    ] {
        let rom = assemble_boot_rom(&format!("{alias}, 8")).unwrap();
        assert_eq!(&rom[..4], &expected.to_le_bytes(), "{alias}");
        for target in ["-4096", "-4", "0", "4092", "1b", "1f", "end"] {
            let alias_source = format!("1: nop\n{alias}, {target}\n1: end: wfi");
            let base_source = format!("1: nop\n{base}, {target}\n1: end: wfi");
            assert_eq!(
                assemble_boot_rom(&alias_source).unwrap(),
                assemble_boot_rom(&base_source).unwrap(),
                "{alias}, {target}"
            );
        }
    }
}

#[test]
fn branch_aliases_execute_full_width_signed_and_unsigned_truth_tables() {
    for alias in [
        "beqz", "bnez", "blez", "bgez", "bltz", "bgtz", "bgt", "ble", "bgtu", "bleu",
    ] {
        let zero_test = alias.ends_with('z');
        let operands = if zero_test { "a0" } else { "a0, a1" };
        let rom = assemble_boot_rom(&format!(
            "la t0, values
             ld a0, 0(t0)
             ld a1, 8(t0)
             addi a2, zero, 0
             {alias} {operands}, 1f
             j 2f
             1: addi a2, zero, 1
             2: wfi
             .balign 8
             values: .dword 0, 0"
        ))
        .unwrap();
        for left in [0u64, 1, u64::MAX, i64::MIN as u64, i64::MAX as u64] {
            for right in [0u64, 1, u64::MAX, i64::MIN as u64, i64::MAX as u64] {
                let expected = match alias {
                    "beqz" => left == 0,
                    "bnez" => left != 0,
                    "blez" => (left as i64) <= 0,
                    "bgez" => (left as i64) >= 0,
                    "bltz" => (left as i64) < 0,
                    "bgtz" => (left as i64) > 0,
                    "bgt" => (left as i64) > (right as i64),
                    "ble" => (left as i64) <= (right as i64),
                    "bgtu" => left > right,
                    _ => left <= right,
                };
                let mut image = rom.clone();
                image[40..48].copy_from_slice(&left.to_le_bytes());
                image[48..56].copy_from_slice(&right.to_le_bytes());
                let mut machine = Kiuas16Machine::new_with_ram_size(image, 4096).unwrap();
                let report = run_headless(&mut machine, 20, &[], &mut Vec::new()).unwrap();
                assert_eq!(report.traps, 0, "{alias}: {:?}", report.last_trap);
                assert!(report.waiting > 0);
                assert_eq!(machine.core().register(10), Some(left));
                assert_eq!(machine.core().register(11), Some(right));
                assert_eq!(
                    machine.core().register(12),
                    Some(u64::from(expected)),
                    "{alias}: {left:#x}, {right:#x}"
                );
            }
        }
    }
}

#[test]
fn branch_aliases_invalid_operands_preserve_existing_rom() {
    for (operation, operands) in [
        ("beqz", "a0"),
        ("bnez", "a0"),
        ("blez", "a0"),
        ("bgez", "a0"),
        ("bltz", "a0"),
        ("bgtz", "a0"),
        ("bgt", "a0, a1"),
        ("ble", "a0, a1"),
        ("bgtu", "a0, a1"),
        ("bleu", "a0, a1"),
    ] {
        for (tail, message) in [
            ("4096", "out of range"),
            ("-4100", "out of range"),
            ("2", "aligned"),
            ("missing", "undefined label"),
            ("1f", "undefined numeric local label"),
            ("8, a2", "operands"),
        ] {
            let source = format!("{operation} {operands}, {tail}");
            let diagnostic = assemble_boot_rom(&source).unwrap_err().to_string();
            assert!(diagnostic.starts_with("line 1:"), "{diagnostic}");
            assert!(diagnostic.contains(message), "{diagnostic}");
            let files = Files::new();
            std::fs::write(files.0.join("guest.S"), source).unwrap();
            std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
            let build = files.assemble();
            assert!(!build.status.success());
            assert!(!build.stderr.is_empty());
            assert_eq!(
                std::fs::read(files.0.join("rom.bin")).unwrap(),
                b"previous ROM"
            );
        }
        for source in [
            format!("{operation}"),
            format!("{operation} {operands}"),
            format!(
                "{operation} x32, {}",
                if operands.contains(',') { "a1, 8" } else { "8" }
            ),
        ] {
            assert!(assemble_boot_rom(&source).is_err(), "{source}");
        }
    }
}

#[test]
fn numeric_local_labels_resolve_nearest_source_definitions() {
    let numeric = assemble_boot_rom(
        "0: 12: 0012: .zero 0
         la a0, 12b
         la a1, 12f
         12: .zero 0
         la a2, 12b
         la a3, 0b
         .dword 12b, 12f, 0b
         12: .word 12b, 12f
         12: .byte 0xff
         .balign 4
         .dword 12b",
    )
    .unwrap();
    let named = assemble_boot_rom(
        "start: .zero 0
         la a0, start
         la a1, middle
         middle: .zero 0
         la a2, middle
         la a3, start
         .dword middle, next, start
         next: .word next, end
         end: .byte 0xff
         .balign 4
         .dword end",
    )
    .unwrap();
    assert_eq!(numeric, named);
    for (source, expected) in [
        ("1: beq zero, zero, 1b", 0x00000063u32),
        ("beq zero, zero, 1f\n1:", 0x00000263),
        ("1: j 1f\n1:", 0x0040006f),
        ("12: jal ra, 12b", 0x000000ef),
        ("call 1f\n1: ret", 0x004000ef),
    ] {
        let rom = assemble_boot_rom(source).unwrap();
        assert_eq!(&rom[..4], &expected.to_le_bytes(), "{source}");
    }
    for operation in ["beq", "bne", "blt", "bge", "bltu", "bgeu"] {
        let numeric = format!("1: nop\n{operation} a0, a1, 1b\n{operation} a0, a1, 1f\n1: wfi");
        let named = format!(
            "before: nop\n{operation} a0, a1, before\n{operation} a0, a1, after\nafter: wfi"
        );
        assert_eq!(
            assemble_boot_rom(&numeric).unwrap(),
            assemble_boot_rom(&named).unwrap()
        );
    }
}

#[test]
fn numeric_local_labels_distinguish_direction_at_equal_addresses() {
    let rom = assemble_boot_rom(
        "1: .dword 1b, 1f
         1: .zero 0
         .dword 1b, 1f
         .zero 0
         1: 1: .dword 1b
         la a0, 1b
         1:",
    )
    .unwrap();
    let values: Vec<_> = rom[..40]
        .chunks_exact(8)
        .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(values, [0x1000, 0x1010, 0x1010, 0x1020, 0x1020]);
    let zero = assemble_boot_rom("1: .zero 0\nla a0, 1f\n1:").unwrap();
    let named = assemble_boot_rom("past: .zero 0\nla a0, future\nfuture:").unwrap();
    assert_eq!(zero, named);
}

#[test]
fn numeric_local_label_calls_and_loops_execute_through_production_cli() {
    let source = "li sp, -2147418112
        li a0, 0
        call 1f
        call 2f
        li t0, 0x10000000
        sb a0, 0(t0)
        wfi
        1: addi a0, a0, 21
        ret
        2: li t1, 3
        1: addi t1, t1, -1
        bne t1, zero, 1b
        addi a0, a0, 21
        ret";
    let rom = assemble_boot_rom(source).unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 100, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert_eq!(output, b"*");
    assert_eq!(machine.core().register(10), Some(42));
    let files = Files::new();
    std::fs::write(files.0.join("guest.S"), source).unwrap();
    let build = files.assemble();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(env!("CARGO_BIN_EXE_kiuas16-run"))
        .arg("--rom")
        .arg(files.0.join("rom.bin"))
        .args(["--steps", "100", "--deny-traps"])
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
fn numeric_local_label_errors_preserve_existing_rom() {
    for (source, line, message) in [
        ("j 1f\n1: j 1f", 2, "undefined numeric local label"),
        ("1: j 2b\n2:", 1, "undefined numeric local label"),
        ("1: j 1f", 1, "undefined numeric local label"),
        ("j 1b\n1:", 1, "undefined numeric local label"),
        ("la a0, 9f", 1, "undefined numeric local label"),
        (".dword 9b", 1, "undefined numeric local label"),
        (".word 9f", 1, "undefined numeric local label"),
        (".byte 9f", 1, "undefined numeric local label"),
        (".globl 1\n1: wfi", 1, "global symbol"),
        ("1f: wfi", 1, "invalid label"),
        ("-1: wfi", 1, "invalid label"),
        ("j 1f\n.byte 0\n1:", 1, "aligned"),
        ("beq zero, zero, 1f\n.zero 4092\n1:", 1, "out of range"),
    ] {
        let diagnostic = assemble_boot_rom(source).unwrap_err().to_string();
        assert!(
            diagnostic.starts_with(&format!("line {line}:")),
            "{diagnostic}"
        );
        assert!(diagnostic.contains(message), "{diagnostic}");
        let files = Files::new();
        std::fs::write(files.0.join("guest.S"), source).unwrap();
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.assemble();
        assert!(!build.status.success(), "{source}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

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
        ("csrrw a0, mscratch, a1", 0x34059573),
        ("csrrs a0, 0x340, a1", 0x3405a573),
        ("csrrc a0, mscratch, a1", 0x3405b573),
        ("csrrwi a0, mscratch, 31", 0x340fd573),
        ("csrrsi a0, mscratch, 0", 0x34006573),
        ("csrrci a0, mscratch, 1", 0x3400f573),
        ("csrr a0, mhartid", 0xf1402573),
        ("csrw mscratch, a1", 0x34059073),
        ("csrs mscratch, a1", 0x3405a073),
        ("csrc mscratch, a1", 0x3405b073),
        ("csrwi mscratch, 31", 0x340fd073),
        ("csrsi mscratch, 0", 0x34006073),
        ("csrci mscratch, 1", 0x3400f073),
        ("csrr a0, 0", 0x00002573),
        ("csrr a0, 4095", 0xfff02573),
        ("lr.w a0, (t0)", 0x1002a52f),
        ("lr.d.aq a0, (t0)", 0x1402b52f),
        ("amoswap.w a0, a1, (t0)", 0x08b2a52f),
        ("amoswap.d.rl zero, a0, (t0)", 0x0aa2b02f),
        ("amoswap.w.aqrl a0, a1, (t0)", 0x0eb2a52f),
        ("amoadd.w a0, a1, (t0)", 0x00b2a52f),
        ("amoadd.d.aq a0, a1, (t0)", 0x04b2b52f),
        ("amoadd.w.rl zero, a0, (t0)", 0x02a2a02f),
        ("amoadd.d.aqrl a0, a1, (t0)", 0x06b2b52f),
        ("amoxor.w a0, a1, (t0)", 0x20b2a52f),
        ("amoor.d.aq a0, a1, (t0)", 0x44b2b52f),
        ("amoand.w.rl zero, a0, (t0)", 0x62a2a02f),
        ("amoxor.d.aqrl a0, a1, (t0)", 0x26b2b52f),
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
fn assembled_csr_register_and_immediate_operations_execute() {
    let rom = assemble_boot_rom(
        "li t0, -1
         csrrw a0, mscratch, t0
         csrrs a1, mscratch, zero
         li t0, 0x15
         csrrc a2, mscratch, t0
         csrrs a3, mscratch, t0
         csrrwi a4, mscratch, 16
         csrrsi a5, mscratch, 3
         csrrci a6, mscratch, 1
         csrr a7, mscratch
         csrwi sscratch, 31
         csrci sscratch, 15
         csrsi sscratch, 1
         csrr s1, sscratch
         csrw sscratch, t0
         csrc sscratch, t0
         csrs sscratch, t0
         csrr s2, sscratch
         csrr s3, mhartid
         csrrsi s4, mhartid, 0
         csrrc s5, mhartid, zero
         wfi",
    )
    .unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let report = run_headless(&mut machine, 100, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    for (register, value) in [
        (10, 0),
        (11, u64::MAX),
        (12, u64::MAX),
        (13, !0x15u64),
        (14, u64::MAX),
        (15, 16),
        (16, 19),
        (17, 18),
        (9, 17),
        (18, 21),
        (19, 0),
        (20, 0),
        (21, 0),
    ] {
        assert_eq!(
            machine.core().register(register),
            Some(value),
            "x{register}"
        );
    }
}

#[test]
fn assembler_csr_names_match_machine_register_addresses() {
    for (name, address) in [
        ("sstatus", 0x100u32),
        ("sie", 0x104),
        ("stvec", 0x105),
        ("scounteren", 0x106),
        ("sscratch", 0x140),
        ("sepc", 0x141),
        ("scause", 0x142),
        ("stval", 0x143),
        ("sip", 0x144),
        ("stimecmp", 0x14d),
        ("satp", 0x180),
        ("mstatus", 0x300),
        ("misa", 0x301),
        ("medeleg", 0x302),
        ("mideleg", 0x303),
        ("mie", 0x304),
        ("mtvec", 0x305),
        ("mcounteren", 0x306),
        ("menvcfg", 0x30a),
        ("mscratch", 0x340),
        ("mepc", 0x341),
        ("mcause", 0x342),
        ("mtval", 0x343),
        ("mip", 0x344),
        ("pmpcfg0", 0x3a0),
        ("pmpaddr0", 0x3b0),
        ("mcycle", 0xb00),
        ("minstret", 0xb02),
        ("cycle", 0xc00),
        ("time", 0xc01),
        ("instret", 0xc02),
        ("mhartid", 0xf14),
    ] {
        let rom = assemble_boot_rom(&format!("csrr a0, {name}")).unwrap();
        let expected = address << 20 | 0x2573;
        assert_eq!(&rom[..4], &expected.to_le_bytes(), "{name}");
    }
}

#[test]
fn invalid_csr_operands_preserve_existing_rom() {
    for source in [
        "csrr a0, unknown",
        "csrr a0, -1",
        "csrr a0, 4096",
        "csrrw a0, mscratch",
        "csrr a0, mscratch, zero",
        "csrrwi a0, mscratch, 32",
        "csrwi mscratch, -1",
        "csrsi mscratch, a0",
        "csrrci a0, mscratch, 0x20",
        "csrrw a0, mscratch, 1",
        "csrw mscratch, x32",
        "csrci mscratch",
        "csrs mscratch, zero, a0",
    ] {
        let diagnostic = assemble_boot_rom(source).unwrap_err().to_string();
        assert!(diagnostic.starts_with("line 1:"), "{diagnostic}");
        let files = Files::new();
        std::fs::write(files.0.join("guest.S"), source).unwrap();
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let result = files.assemble();
        assert!(!result.status.success(), "{source}");
        assert!(!result.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn remaining_atomics_match_independent_instruction_encodings() {
    for (operation, function) in [
        ("sc", 3u32),
        ("amomin", 16),
        ("amomax", 20),
        ("amominu", 24),
        ("amomaxu", 28),
    ] {
        for (width, code) in [("w", 2u32), ("d", 3)] {
            for (suffix, ordering) in [("", 0u32), (".aq", 2), (".rl", 1), (".aqrl", 3)] {
                for (destination, register) in [("a0", 10u32), ("zero", 0)] {
                    let source = format!("{operation}.{width}{suffix} {destination}, a1, (t0)");
                    let expected = function << 27
                        | ordering << 25
                        | 11 << 20
                        | 5 << 15
                        | code << 12
                        | register << 7
                        | 0x2f;
                    let rom = assemble_boot_rom(&source).unwrap();
                    assert_eq!(&rom[..4], &expected.to_le_bytes(), "{source}");
                }
            }
        }
    }
}

#[test]
fn remaining_atomic_extrema_execute_signed_unsigned_edges_and_preserve_neighbors() {
    for operation in ["amomin", "amomax", "amominu", "amomaxu"] {
        for (width, size) in [("w", 4), ("d", 8)] {
            for suffix in ["", ".aq", ".rl", ".aqrl"] {
                for (old, operand) in [
                    (0x8000000080000000u64, 1u64),
                    (1, 0x8000000080000000),
                    (u64::MAX, 0),
                    (0, u64::MAX),
                    (42, 42),
                ] {
                    let rom = assemble_boot_rom(&format!(
                        "li t0, 1
                         slli t0, t0, 31
                         addi t0, t0, 8
                         ld a1, 8(t0)
                         {operation}.{width}{suffix} a0, a1, (t0)
                         wfi"
                    ))
                    .unwrap();
                    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
                    machine.bus_mut().write(0x80000000, &[0xa5; 32]).unwrap();
                    machine
                        .bus_mut()
                        .write(0x80000008, &old.to_le_bytes()[..size])
                        .unwrap();
                    machine
                        .bus_mut()
                        .write(0x80000010, &operand.to_le_bytes())
                        .unwrap();
                    let report = run_headless(&mut machine, 20, &[], &mut Vec::new()).unwrap();
                    assert_eq!(report.traps, 0, "{operation}.{width}{suffix}");
                    assert!(report.waiting > 0);
                    let signed_old = if size == 4 {
                        old as i32 as i64
                    } else {
                        old as i64
                    };
                    let signed_operand = if size == 4 {
                        operand as i32 as i64
                    } else {
                        operand as i64
                    };
                    let unsigned_old = if size == 4 { old as u32 as u64 } else { old };
                    let unsigned_operand = if size == 4 {
                        operand as u32 as u64
                    } else {
                        operand
                    };
                    let expected = match operation {
                        "amomin" => signed_old.min(signed_operand) as u64,
                        "amomax" => signed_old.max(signed_operand) as u64,
                        "amominu" => unsigned_old.min(unsigned_operand),
                        _ => unsigned_old.max(unsigned_operand),
                    };
                    assert_eq!(machine.core().register(10), Some(signed_old as u64));
                    let mut bytes = [0; 32];
                    machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
                    let mut expected_bytes = [0xa5; 32];
                    expected_bytes[8..8 + size].copy_from_slice(&expected.to_le_bytes()[..size]);
                    expected_bytes[16..24].copy_from_slice(&operand.to_le_bytes());
                    assert_eq!(bytes, expected_bytes, "{operation}.{width}{suffix}");
                }
            }
        }
    }
}

#[test]
fn remaining_atomic_store_conditionals_observe_reservations_and_width() {
    for (width, size, other_width) in [("w", 4, "d"), ("d", 8, "w")] {
        for suffix in ["", ".aq", ".rl", ".aqrl"] {
            for (prelude, address_offset, success) in [
                (format!("lr.{width} a0, (t0)"), 0, true),
                (String::new(), 0, false),
                (format!("lr.{width} a0, (t0)\nsw zero, 16(t0)"), 0, false),
                (format!("lr.{width} a0, (t0)"), 8, false),
                (format!("lr.{other_width} a0, (t0)"), 0, false),
            ] {
                let rom = assemble_boot_rom(&format!(
                    "li t0, 1
                     slli t0, t0, 31
                     addi t0, t0, 8
                     li a1, 42
                     {prelude}
                     addi t1, t0, {address_offset}
                     sc.{width}{suffix} a2, a1, (t1)
                     sc.{width}{suffix} a3, a1, (t1)
                     wfi"
                ))
                .unwrap();
                let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
                machine.bus_mut().write(0x80000000, &[0xa5; 40]).unwrap();
                let report = run_headless(&mut machine, 30, &[], &mut Vec::new()).unwrap();
                assert_eq!(report.traps, 0);
                assert!(report.waiting > 0);
                assert_eq!(machine.core().register(12), Some(u64::from(!success)));
                assert_eq!(machine.core().register(13), Some(1));
                if prelude.starts_with(&format!("lr.{width} ")) {
                    let old = if size == 4 {
                        0xffffffffa5a5a5a5
                    } else {
                        0xa5a5a5a5a5a5a5a5
                    };
                    assert_eq!(machine.core().register(10), Some(old));
                }
                let mut bytes = [0; 40];
                machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
                let mut expected = [0xa5; 40];
                if success {
                    expected[8..8 + size].copy_from_slice(&42u64.to_le_bytes()[..size]);
                }
                if prelude.contains("sw zero") {
                    expected[24..28].fill(0);
                }
                assert_eq!(bytes, expected, "{width}{suffix}: {prelude}");
            }
        }
    }
}

#[test]
fn remaining_atomics_zero_destinations_and_aliased_sc_operands_execute() {
    for (width, size) in [("w", 4), ("d", 8)] {
        let rom = assemble_boot_rom(&format!(
            "li t0, 1
             slli t0, t0, 31
             li a0, 42
             lr.{width} zero, (t0)
             sc.{width} a0, a0, 0(t0)
             lr.{width} zero, (t0)
             li a1, 21
             sc.{width} zero, a1, (t0)
             amomin.{width} zero, zero, (t0)
             amomax.{width} zero, a1, (t0)
             amominu.{width} zero, zero, (t0)
             amomaxu.{width} zero, a1, (t0)
             wfi"
        ))
        .unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
        machine.bus_mut().write(0x80000000, &[0xa5; 16]).unwrap();
        let report = run_headless(&mut machine, 40, &[], &mut Vec::new()).unwrap();
        assert_eq!(report.traps, 0);
        assert!(report.waiting > 0);
        assert_eq!(machine.core().register(0), Some(0));
        assert_eq!(machine.core().register(10), Some(0));
        let mut bytes = [0; 16];
        machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
        let mut expected = [0xa5; 16];
        expected[..size].copy_from_slice(&21u64.to_le_bytes()[..size]);
        assert_eq!(bytes, expected);
    }
}

#[test]
fn remaining_atomics_invalid_guest_addresses_trap_without_writes() {
    use suomicpu16::kiuas16::CoreTrapCause;

    for operation in ["sc", "amomin", "amomax", "amominu", "amomaxu"] {
        for width in ["w", "d"] {
            for (setup, address, cause) in [
                (
                    "li t0, 1\nslli t0, t0, 31\naddi t0, t0, 1",
                    0x80000001,
                    CoreTrapCause::StoreAddressMisaligned,
                ),
                (
                    "li t0, 0x10000000",
                    0x10000000,
                    CoreTrapCause::StoreAccessFault,
                ),
                ("li t0, 0x1000", 0x1000, CoreTrapCause::StoreAccessFault),
            ] {
                let rom =
                    assemble_boot_rom(&format!(                    "la t1, fault\ncsrw mtvec, t1\n{setup}\n{operation}.{width} a0, a1, (t0)\nwfi\nfault: wfi"))
                        .unwrap();
                let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
                machine.bus_mut().write(0x80000000, &[0xa5; 16]).unwrap();
                let mut output = Vec::new();
                let report = run_headless(&mut machine, 20, &[], &mut output).unwrap();
                assert_eq!(report.traps, 1);
                assert!(report.waiting > 0);
                let trap = report.last_trap.unwrap();
                assert_eq!(trap.cause, cause, "{operation}.{width}");
                assert_eq!(trap.value, address);
                assert!(output.is_empty());
                let mut bytes = [0; 16];
                machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
                assert_eq!(bytes, [0xa5; 16]);
            }
        }
    }
}

#[test]
fn remaining_atomic_invalid_operands_preserve_existing_rom() {
    for operation in ["sc", "amomin", "amomax", "amominu", "amomaxu"] {
        for (operands, message) in [
            ("b a0, a1, (t0)", "width"),
            ("d.aq.rl a0, a1, (t0)", "suffix"),
            ("w.acquire a0, a1, (t0)", "suffix"),
            ("w a0, a1, 4(t0)", "zero offset"),
            ("w a0, (t0)", "operands"),
            ("w a0, a1, (t0), a2", "operands"),
            ("d a0, a1, (x32)", "register"),
            ("d a0, 1, (t0)", "register"),
        ] {
            let source = format!("{operation}.{operands}");
            let diagnostic = assemble_boot_rom(&source).unwrap_err().to_string();
            assert!(diagnostic.starts_with("line 1:"), "{diagnostic}");
            assert!(diagnostic.contains(message), "{diagnostic}");
            let files = Files::new();
            std::fs::write(files.0.join("guest.S"), source).unwrap();
            std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
            let build = files.assemble();
            assert!(!build.status.success());
            assert!(!build.stderr.is_empty());
            assert_eq!(
                std::fs::read(files.0.join("rom.bin")).unwrap(),
                b"previous ROM"
            );
        }
    }
}

#[test]
fn assembled_atomic_add_returns_old_bits_and_updates_only_the_addressed_width() {
    for (width, old, operand) in [
        ("w", 0x7fffffff_u64, 1i64),
        ("w", 0x80000000, -1),
        ("d", u64::MAX, 1),
        ("d", 0x8000000000000000, -1),
    ] {
        for suffix in ["", ".aq", ".rl", ".aqrl"] {
            let rom = assemble_boot_rom(&format!(
                "li t0, 1
                 slli t0, t0, 31
                 addi t0, t0, 8
                 li a1, {operand}
                 amoadd.{width}{suffix} a0, a1, (t0)
                 wfi"
            ))
            .unwrap();
            let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
            machine.bus_mut().write(0x80000000, &[0xa5; 24]).unwrap();
            let size = if width == "w" { 4 } else { 8 };
            machine
                .bus_mut()
                .write(0x80000008, &old.to_le_bytes()[..size])
                .unwrap();
            let report = run_headless(&mut machine, 20, &[], &mut Vec::new()).unwrap();
            assert_eq!(report.traps, 0);
            assert!(report.waiting > 0);
            let expected_old = if size == 4 {
                old as u32 as i32 as i64 as u64
            } else {
                old
            };
            assert_eq!(machine.core().register(10), Some(expected_old));
            let mut bytes = [0; 24];
            machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
            assert_eq!(&bytes[..8], &[0xa5; 8]);
            assert_eq!(
                &bytes[8..8 + size],
                &old.wrapping_add(operand as u64).to_le_bytes()[..size]
            );
            assert!(bytes[8 + size..].iter().all(|byte| *byte == 0xa5));
        }
    }
}

#[test]
fn assembled_bitwise_atomics_match_independent_bytes_and_memory_results() {
    for (op, funct5) in [("amoand", 12u32), ("amoor", 8), ("amoxor", 4)] {
        for (width, funct3, size) in [("w", 2u32, 4), ("d", 3, 8)] {
            for (suffix, order) in [("", 0u32), (".aq", 2), (".rl", 1), (".aqrl", 3)] {
                let instruction = format!("{op}.{width}{suffix} a0, a1, (t0)");
                let rom = assemble_boot_rom(&instruction).unwrap();
                let expected =
                    funct5 << 27 | order << 25 | 11 << 20 | 5 << 15 | funct3 << 12 | 10 << 7 | 0x2f;
                assert_eq!(&rom[..4], &expected.to_le_bytes(), "{instruction}");
                let rom = assemble_boot_rom(&format!(
                    "li t0, 1
                     slli t0, t0, 31
                     addi t0, t0, 8
                     li a1, -2
                     {instruction}
                     wfi"
                ))
                .unwrap();
                let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
                machine.bus_mut().write(0x80000000, &[0xa5; 24]).unwrap();
                let old = 0xfedcba9880000001u64;
                machine
                    .bus_mut()
                    .write(0x80000008, &old.to_le_bytes()[..size])
                    .unwrap();
                let report = run_headless(&mut machine, 20, &[], &mut Vec::new()).unwrap();
                assert_eq!(report.traps, 0);
                assert!(report.waiting > 0);
                let returned = if size == 4 {
                    old as u32 as i32 as i64 as u64
                } else {
                    old
                };
                assert_eq!(machine.core().register(10), Some(returned));
                let operand = (-2i64) as u64;
                let expected = match op {
                    "amoand" => old & operand,
                    "amoor" => old | operand,
                    _ => old ^ operand,
                };
                let mut bytes = [0; 24];
                machine.bus_mut().read(0x80000000, &mut bytes).unwrap();
                assert_eq!(&bytes[..8], &[0xa5; 8]);
                assert_eq!(&bytes[8..8 + size], &expected.to_le_bytes()[..size]);
                assert!(bytes[8 + size..].iter().all(|byte| *byte == 0xa5));
            }
        }
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
        ("amoadd.b a0, a1, (t0)", "width"),
        ("amoadd.d.aq.rl a0, a1, (t0)", "suffix"),
        ("amoadd.w a0, a1, 4(t0)", "zero offset"),
        ("amoadd.d a0, (t0)", "operands"),
        ("amoand.b a0, a1, (t0)", "width"),
        ("amoor.d.aq.rl a0, a1, (t0)", "suffix"),
        ("amoxor.w a0, a1, 4(t0)", "zero offset"),
        ("amoand.d a0, (t0)", "operands"),
        ("amoor.w a0, a1, (x32)", "register"),
        ("slli a0, a0, 64", "shift amount"),
        ("slliw a0, a0, 32", "shift amount"),
        ("j missing", "undefined label"),
        ("j 2", "aligned"),
        ("beq a0, a1, 4096", "out of range"),
        ("jal ra, 1048576", "out of range"),
        ("la a0, 9223372036854775807", "displacement"),
        ("li a0, 2147483648", "32-bit"),
        ("a: nop\na: nop", "duplicate label"),
        ("1a: nop", "invalid label"),
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
