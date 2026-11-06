use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use suomicpu16::assembler::rv64::assemble_boot_rom;
use suomicpu16::kiuas16::runtime::run_headless;
use suomicpu16::kiuas16::{BOOT_ROM_BYTES, Kiuas16Machine};

#[test]
fn local_address_addends_match_independent_pcrel_encodings() {
    for (with_label, direct) in [
        ("la a0, target + 4\nnop\ntarget: nop", "la a0, 16\nnop\nnop"),
        ("la a0, . + 8\nnop", "la a0, 8\nnop"),
        ("la a0, 1f - 4\nnop\n1: nop", "la a0, 8\nnop\nnop"),
        ("j target+4\nnop\ntarget: nop", "j 12\nnop\nnop"),
        ("j .+8\nnop", "j 8\nnop"),
        ("j 1f+4\nnop\n1: nop\nnop", "j 12\nnop\nnop\nnop"),
        (
            "beqz a0, target+4\nnop\ntarget: nop",
            "beqz a0, 12\nnop\nnop",
        ),
        ("call target - 4\ntarget: nop", "call 0\nnop"),
        ("tail target+4\ntarget: nop", "tail 12\nnop"),
    ] {
        assert_eq!(
            assemble_boot_rom(with_label).unwrap(),
            assemble_boot_rom(direct).unwrap(),
            "{with_label}"
        );
    }
    let rom = assemble_boot_rom("la a0, .").unwrap();
    assert_eq!(&rom[..4], &0x00000517u32.to_le_bytes());
    assert_eq!(&rom[4..8], &0x00050513u32.to_le_bytes());
}

#[test]
fn local_address_addends_execute_calls_branches_and_jump_skips() {
    let rom = assemble_boot_rom(
        "li a0, 19\nbeqz zero, 1f+4\n\
         1: addi a0, a0, -100\naddi a0, a0, 23\ncall 2f+4\nj 3f+4\n\
         2: addi a0, a0, -99\naddi a0, a0, 0\nret\n\
         3: addi a0, a0, -98\nwfi",
    )
    .unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let report = run_headless(&mut machine, 40, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(42));
    assert_eq!(machine.core().register(1), Some(0x1018));
}

#[test]
fn local_address_addends_reject_invalid_targets_without_changing_other_operands() {
    for source in [
        "j missing+4",
        "la a0, missing-4",
        "beqz a0, missing + 4",
        "j target+4+8\ntarget: nop",
        "la a0, target-4-8\ntarget: nop",
        "j target+",
        "la a0, .-",
        "j target+9223372036854775807\ntarget: nop",
        "j target--9223372036854775808\ntarget: nop",
        "j target+1\ntarget: nop",
        "beqz a0, target-1\ntarget: nop",
        "tail target+2\ntarget: nop",
        "addi a0, zero, target+4",
        "ld a0, target+4(t0)",
    ] {
        let diagnostic = assemble_boot_rom(source).unwrap_err().to_string();
        assert!(diagnostic.starts_with("line "), "{source}: {diagnostic}");
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
fn rom_origin_matches_offsets_and_every_checked_fill_byte() {
    for fill in -128i64..=255 {
        let rom = assemble_boot_rom(&format!(".byte 19\n.org 8, {fill}\n.byte 23")).unwrap();
        assert_eq!(rom[0], 19);
        assert!(rom[1..8].iter().all(|byte| *byte == fill as u8));
        assert_eq!(rom[8], 23);
        assert!(rom[9..].iter().all(|byte| *byte == 0));
    }
    for target in [0usize, 1, 7, 4095, 4096, 4097, 61439] {
        for literal in [
            target.to_string(),
            format!("0x{target:x}"),
            format!("0o{target:o}"),
            format!("0b{target:b}"),
        ] {
            let rom = assemble_boot_rom(&format!(".org {literal}\n.byte 42")).unwrap();
            assert!(rom[..target].iter().all(|byte| *byte == 0));
            assert_eq!(rom[target], 42);
            assert!(rom[target + 1..].iter().all(|byte| *byte == 0));
        }
    }
    let default = assemble_boot_rom(".byte 19\n.org 8\n.byte 23").unwrap();
    assert!(default == assemble_boot_rom(".byte 19\n.org 8, 0\n.byte 23").unwrap());
    assert!(default == assemble_boot_rom(".byte 19\n.zero 7\n.byte 23").unwrap());
}

#[test]
fn rom_origin_preserves_labels_locations_alignment_and_rom_limits() {
    let rom = assemble_boot_rom(
        "first: .org 7, 42\n1: .byte 19\n.org 8, -1\n\
         .org 8, 128\n.org 16, 23\n2: .quad first, 1b, 2b, ., 2b - 1b\nwfi",
    )
    .unwrap();
    assert_eq!(&rom[..7], &[42; 7]);
    assert_eq!(rom[7], 19);
    assert_eq!(&rom[8..16], &[23; 8]);
    for (bytes, expected) in rom[16..56]
        .chunks_exact(8)
        .zip([0x1000u64, 0x1007, 0x1010, 0x1028, 9])
    {
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), expected);
    }
    assert_eq!(&rom[56..60], &0x10500073u32.to_le_bytes());
    let rom = assemble_boot_rom(".org 61439, 42\n.byte 23\n.org 61440, 255").unwrap();
    assert!(rom[..61439].iter().all(|byte| *byte == 42));
    assert_eq!(rom[61439], 23);
    assert!(
        assemble_boot_rom(".org 61440\n.byte 1")
            .unwrap_err()
            .to_string()
            .contains("exceeds")
    );
    let rom = assemble_boot_rom(".org 61440, -1").unwrap();
    assert_eq!(rom.len(), BOOT_ROM_BYTES);
    assert!(rom.iter().all(|byte| *byte == 255));
    let rom = assemble_boot_rom(".org 4097\n.balign 8, 42\n.quad .").unwrap();
    assert_eq!(&rom[4097..4104], &[42; 7]);
    assert_eq!(&rom[4104..4112], &0x2008u64.to_le_bytes());
}

#[test]
fn rom_origin_executes_fixed_entry_and_returning_helpers() {
    let rom = assemble_boot_rom(
        "j entry\n.org 256, 42\n\
         helper: addi a0, a0, 23\nret\n\
         .org 4096, 128\nentry: li a0, 19\ncall helper\n\
         la t0, entry\nlbu a1, -1(t0)\nwfi",
    )
    .unwrap();
    assert_eq!(&rom[256..260], &0x01750513u32.to_le_bytes());
    assert!(rom[264..4096].iter().all(|byte| *byte == 128));
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 24, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert!(output.is_empty());
    assert_eq!(machine.core().register(10), Some(42));
    assert_eq!(machine.core().register(11), Some(128));
    assert_eq!(machine.core().register(1), Some(0x200c));
}

#[test]
fn rom_origin_invalid_operands_preserve_existing_rom() {
    for source in [
        ".org",
        ".org -1",
        ".org 61441",
        ".org 18446744073709551615",
        ".org missing",
        ".org 1+1",
        ".org .",
        ".org 1,",
        ".org ,42",
        ".org 8, 256",
        ".org 0, 256",
        ".org 0, -129",
        ".org 8, missing",
        ".org 8, 0xffffffffffffffff",
        ".org 8, 1, 2",
        ".org 0x__ff",
        ".org 4\n.org 3",
        ".byte 42\n.org 0",
        ".org 1\nwfi",
        ".org 8\n.org 4\n.byte 42",
    ] {
        let source = format!("# first line\n{source}");
        let diagnostic = assemble_boot_rom(&source).unwrap_err().to_string();
        assert!(
            diagnostic.starts_with("line 2:") || diagnostic.starts_with("line 3:"),
            "{diagnostic}"
        );
        let files = Files::new();
        std::fs::write(files.0.join("guest.S"), &source).unwrap();
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.assemble();
        assert!(!build.status.success(), "{source}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("guest.S")).unwrap(),
            source.as_bytes()
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn data_location_matches_each_value_address_for_every_integer_width() {
    for (directive, width) in [
        (".byte", 1),
        (".half", 2),
        (".short", 2),
        (".2byte", 2),
        (".word", 4),
        (".long", 4),
        (".int", 4),
        (".4byte", 4),
        (".dword", 8),
        (".quad", 8),
        (".8byte", 8),
    ] {
        for offset in [0usize, 1, 7, 4095, 4096, 4097] {
            let rom = assemble_boot_rom(&format!(
                ".zero {offset}\nbase: {directive} . - base, .-base, .-base, .-base"
            ))
            .unwrap();
            for index in 0..4 {
                let expected = (index * width) as u64;
                let start = offset + index * width;
                assert_eq!(
                    &rom[start..start + width],
                    &expected.to_le_bytes()[..width],
                    "{directive} at {offset}, value {index}"
                );
            }
        }
    }
    let rom = assemble_boot_rom(".word ., .\n.quad ., .").unwrap();
    assert_eq!(&rom[..4], &0x1000u32.to_le_bytes());
    assert_eq!(&rom[4..8], &0x1004u32.to_le_bytes());
    assert_eq!(&rom[8..16], &0x1008u64.to_le_bytes());
    assert_eq!(&rom[16..24], &0x1010u64.to_le_bytes());
}

#[test]
fn data_location_preserves_numeric_labels_alignment_and_rom_limits() {
    let rom = assemble_boot_rom(
        "1: .byte 19\n.balign 8, 42\n\
         .half 2f - ., . - 1b\n\
         .word . - 1b, 2f - .\n2: .quad ., . + 8, - . + 1b\nwfi",
    )
    .unwrap();
    assert_eq!(&rom[8..12], &[12, 0, 10, 0]);
    assert_eq!(&rom[12..16], &12u32.to_le_bytes());
    assert_eq!(&rom[16..20], &4u32.to_le_bytes());
    assert_eq!(&rom[20..28], &0x1014u64.to_le_bytes());
    assert_eq!(&rom[28..36], &0x1024u64.to_le_bytes());
    assert_eq!(&rom[36..44], &(-36i64).to_le_bytes());
    assert_eq!(&rom[44..48], &0x10500073u32.to_le_bytes());
    for (directive, width) in [(".byte", 1), (".half", 2), (".word", 4), (".quad", 8)] {
        let padding = BOOT_ROM_BYTES - width;
        let rom =
            assemble_boot_rom(&format!(".zero {padding}\n{directive} end - .\nend:")).unwrap();
        assert_eq!(&rom[padding..], &(width as u64).to_le_bytes()[..width]);
        assert!(
            assemble_boot_rom(&format!(".zero {}\n{directive} . - .", padding + 1))
                .unwrap_err()
                .to_string()
                .contains("exceeds")
        );
    }
    let rom = assemble_boot_rom(".zero 61432\n.half ., .\n.word .").unwrap();
    assert_eq!(&rom[61432..61434], &0xfff8u16.to_le_bytes());
    assert_eq!(&rom[61434..61436], &0xfffau16.to_le_bytes());
    assert_eq!(&rom[61436..], &0xfffcu32.to_le_bytes());
    assert!(assemble_boot_rom(".zero 61440\n.byte .").is_err());
}

#[test]
fn data_location_executes_self_relative_dispatch_and_pointer_entries() {
    let rom = assemble_boot_rom(
        "la t0, table\nlw t1, 0(t0)\nadd t1, t0, t1\n\
         jalr ra, 0(t1)\nla t0, table\nld t2, 8(t0)\n\
         lbu a1, 0(t2)\nwfi\n\
         target: li a0, 42\nret\n.balign 8\n\
         table: .word target - ., payload - .\n\
         .quad . + 8\npayload: .byte 19, 23",
    )
    .unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 32, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert!(output.is_empty());
    assert_eq!(machine.core().register(10), Some(42));
    assert_eq!(machine.core().register(11), Some(19));
}

#[test]
fn data_location_invalid_inputs_preserve_outputs_and_nondata_rejections() {
    for source in [
        ".byte .",
        ".half . + 65536",
        ".word . - 2147487745",
        ".quad . + 18446744073709551615",
        ".byte . - . - 129",
        ".word . + missing",
        ".word . + 1f",
        ".word . + 1b",
        ".word . +",
        ".word . * 2",
        ".word (.)",
        ".word ..",
        ".word . = 42",
        ".: .word 19",
        ".globl .\n.word 19",
        "li a0, .",
        ".space .",
        ".balign .",
        ".zero 1, .",
        ".align 2, .",
    ] {
        let source = format!("# first line\n{source}");
        let diagnostic = assemble_boot_rom(&source).unwrap_err().to_string();
        assert!(diagnostic.starts_with("line 2:"), "{diagnostic}");
        let files = Files::new();
        std::fs::write(files.0.join("guest.S"), &source).unwrap();
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.assemble();
        assert!(!build.status.success(), "{source}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("guest.S")).unwrap(),
            source.as_bytes()
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
    let rom = assemble_boot_rom(".Ldot: .word .Ldot - ., . - .Ldot").unwrap();
    assert_eq!(&rom[..8], &[0, 0, 0, 0, 4, 0, 0, 0]);
}

#[test]
fn data_expressions_match_independent_bytes_and_checked_final_ranges() {
    let rom = assemble_boot_rom(
        ".byte 19 + 23, 0xff - 1, -128 + 0, 1 - -2, + 7, 9 + -2\n\
         .half 0x8000 - 1, -32768, 65535 - 1\n\
         .word 0x100000000 - 1, -2147483648, 0xffffffff + 1 - 2\n\
         .quad 18446744073709551615, 0xffffffffffffffff + 1 - 1",
    )
    .unwrap();
    let expected = [
        42, 254, 128, 3, 7, 7, 255, 127, 0, 128, 254, 255, 255, 255, 255, 255, 0, 0, 0, 128, 254,
        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        255,
    ];
    assert_eq!(&rom[..expected.len()], &expected);
    for (aliases, expression, expected) in [
        (&[".byte"][..], "256-1", vec![255]),
        (
            &[".half", ".short", ".2byte"][..],
            "65536 - 1",
            vec![255; 2],
        ),
        (
            &[".word", ".long", ".int", ".4byte"][..],
            "4294967296-1",
            vec![255; 4],
        ),
        (
            &[".dword", ".quad", ".8byte"][..],
            "0xffffffffffffffff + 1 - 1",
            vec![255; 8],
        ),
    ] {
        for alias in aliases {
            let rom = assemble_boot_rom(&format!("{alias} {expression}")).unwrap();
            assert_eq!(&rom[..expected.len()], &expected, "{alias}");
            assert!(rom[expected.len()..].iter().all(|byte| *byte == 0));
        }
    }
}

#[test]
fn data_expressions_resolve_forward_backward_labels_and_fixed_layout() {
    let rom = assemble_boot_rom(
        "1: base: .byte 2f - 1b, end - base\n\
         .half 1b + 2, -base + end\n\
         2: .word end-base, base-end\n\
         .balign 8\nend: .quad base + 8, 2b - 1b + 19 + 23 - 6\n\
         .dword 0xffffffffffffffff - base + base\nwfi",
    )
    .unwrap();
    assert_eq!(&rom[..6], &[6, 16, 2, 16, 16, 0]);
    assert_eq!(&rom[6..10], &16i32.to_le_bytes());
    assert_eq!(&rom[10..14], &(-16i32).to_le_bytes());
    assert_eq!(&rom[16..24], &0x1008u64.to_le_bytes());
    assert_eq!(&rom[24..32], &42u64.to_le_bytes());
    assert_eq!(&rom[32..40], &u64::MAX.to_le_bytes());
    assert_eq!(&rom[40..44], &0x10500073u32.to_le_bytes());
    let rom = assemble_boot_rom(".zero 61432\n.quad 65536 - 1\nend:").unwrap();
    assert_eq!(&rom[61432..], &65535u64.to_le_bytes());
    assert!(
        assemble_boot_rom(".zero 61436\n.quad 1 + 1")
            .unwrap_err()
            .to_string()
            .contains("exceeds")
    );
    let rom = assemble_boot_rom("base: .half end - base\n.zero 61438\nend:").unwrap();
    assert_eq!(&rom[..2], &61440u16.to_le_bytes());
}

#[test]
fn data_expressions_execute_relative_pointer_table_dispatch() {
    let rom = assemble_boot_rom(
        "la t0, table\nlw t1, 0(t0)\nadd t1, t0, t1\n\
         jalr ra, 0(t1)\nla t0, table\nld t2, 8(t0)\n\
         lbu a1, 0(t2)\nwfi\n\
         target: li a0, 42\nret\n.balign 8\n\
         table: .word target - table, table - target\n\
         .quad payload + 1\npayload: .byte 19, 23",
    )
    .unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 32, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert!(output.is_empty());
    assert_eq!(machine.core().register(10), Some(42));
    assert_eq!(machine.core().register(11), Some(23));
}

#[test]
fn data_expressions_invalid_inputs_preserve_outputs_and_other_operand_limits() {
    for source in [
        ".byte 255 + 1",
        ".byte -128 - 1",
        ".half 65535 + 1",
        ".short -32768 - 1",
        ".word 4294967295 + 1",
        ".long -2147483648-1",
        ".quad 18446744073709551615 + 1",
        ".dword -9223372036854775808 - 1",
        ".quad 18446744073709551616 - 1",
        ".word missing+4",
        ".word 1f-1b",
        ".byte 1+",
        ".byte +",
        ".byte 1--",
        ".byte 1+++2",
        ".word (19 + 23)",
        ".word 6*7",
        ".word 1 << 2",
        ".word 42UL",
        ".word 01 + 1",
        ".word 0x__ff + 1",
        "li a0, 19 + 23",
        ".space 1+1",
        ".balign 4+4",
        ".zero 1, 19+23",
    ] {
        let source = format!("# first line\n{source}");
        let diagnostic = assemble_boot_rom(&source).unwrap_err().to_string();
        assert!(diagnostic.starts_with("line 2:"), "{diagnostic}");
        let files = Files::new();
        std::fs::write(files.0.join("guest.S"), &source).unwrap();
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.assemble();
        assert!(!build.status.success(), "{source}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("guest.S")).unwrap(),
            source.as_bytes()
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn bounded_alignment_matches_thresholds_and_mapped_addresses() {
    for exponent in 0..=15 {
        let alignment = 1usize << exponent;
        for offset in [0, 1, 3, 4, 17, 4095, 4096, 4097] {
            let padding = (alignment - (0x1000 + offset) % alignment) % alignment;
            for maximum in [0, padding.saturating_sub(1), padding, padding + 1] {
                let expected = if maximum != 0 && padding > maximum {
                    0
                } else {
                    padding
                };
                for (directive, operand) in [
                    (".balign", alignment),
                    (".p2align", exponent),
                    (".align", exponent),
                ] {
                    let rom = assemble_boot_rom(&format!(
                        ".zero {offset}\n{directive} {operand}, 42, {maximum}\n.byte 19"
                    ))
                    .unwrap();
                    assert!(rom[..offset].iter().all(|byte| *byte == 0));
                    assert!(
                        rom[offset..offset + expected]
                            .iter()
                            .all(|byte| *byte == 42)
                    );
                    assert_eq!(
                        rom[offset + expected],
                        19,
                        "{directive} {operand} at {offset}, max {maximum}"
                    );
                    assert!(rom[offset + expected + 1..].iter().all(|byte| *byte == 0));
                }
            }
        }
    }
}

#[test]
fn bounded_alignment_fills_all_byte_patterns_and_keeps_default_zeros() {
    for directive in [".balign 8", ".p2align 3", ".align 3"] {
        for fill in -128i64..=255 {
            let rom =
                assemble_boot_rom(&format!(".byte 19\n{directive}, {fill}\n.byte 23")).unwrap();
            assert_eq!(rom[0], 19);
            assert!(rom[1..8].iter().all(|byte| *byte == fill as u8));
            assert_eq!(rom[8], 23);
        }
        let original = assemble_boot_rom(&format!(".byte 19\n{directive}\n.byte 23")).unwrap();
        for suffix in [", 0", ",,7", ",,0", ", 0, 7", ", 0, 9223372036854775807"] {
            assert!(
                original
                    == assemble_boot_rom(&format!(".byte 19\n{directive}{suffix}\n.byte 23"))
                        .unwrap()
            );
        }
        let suppressed =
            assemble_boot_rom(&format!(".byte 19\n{directive}, -1, 6\n.byte 23")).unwrap();
        assert_eq!(&suppressed[..3], &[19, 23, 0]);
        let signed = assemble_boot_rom(&format!(".byte 19\n{directive}, -1, 7\n.byte 23")).unwrap();
        let hex =
            assemble_boot_rom(&format!(".byte 19\n{directive}, 0xff, 0b111\n.byte 23")).unwrap();
        assert!(signed == hex);
    }
}

#[test]
fn bounded_alignment_preserves_labels_rom_limits_and_instruction_checks() {
    let rom = assemble_boot_rom(
        ".byte 19\n.balign 8, 42, 7\n1: aligned: .byte 23\n\
         .p2align 4, 255, 6\n2: skipped: .byte 5\n.align 2,,2\n\
         entry: wfi\n.dword 1b, 2b, aligned, skipped, entry",
    )
    .unwrap();
    assert_eq!(&rom[..12], &[19, 42, 42, 42, 42, 42, 42, 42, 23, 5, 0, 0]);
    assert_eq!(&rom[12..16], &0x10500073u32.to_le_bytes());
    for (bytes, value) in rom[16..56]
        .chunks_exact(8)
        .zip([0x1008u64, 0x1009, 0x1008, 0x1009, 0x100c])
    {
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), value);
    }
    for directive in [".balign 4", ".p2align 2", ".align 2"] {
        assert!(
            assemble_boot_rom(&format!(".byte 19\n{directive}, 42, 2\nwfi"))
                .unwrap_err()
                .to_string()
                .contains("aligned")
        );
        let rom = assemble_boot_rom(&format!(".zero 61437\n{directive}, 42, 3")).unwrap();
        assert_eq!(&rom[61437..], &[42; 3]);
        assert!(
            assemble_boot_rom(&format!(".zero 61437\n{directive}, 42, 3\n.byte 1"))
                .unwrap_err()
                .to_string()
                .contains("exceeds")
        );
        let rom =
            assemble_boot_rom(&format!(".zero 61437\n{directive}, 42, 2\n.byte 19,23,5")).unwrap();
        assert_eq!(&rom[61437..], &[19, 23, 5]);
        assert!(assemble_boot_rom(&format!(".zero 61440\n{directive}, 255, 1")).is_ok());
    }
}

#[test]
fn bounded_alignment_executes_skipped_padding_and_reads_fill_bytes() {
    let rom = assemble_boot_rom(
        "j entry\n.byte 19\n.balign 8, 42, 3\n1: .byte 23\n\
         .p2align 4, 255, 6\n2: .byte 5\n.align 2,,2\n\
         entry: la t0, 1b\nlbu a0, -1(t0)\nla t1, 2b\n\
         lbu a1, 0(t1)\nsub a2, t1, t0\nwfi",
    )
    .unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 24, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert!(output.is_empty());
    assert_eq!(machine.core().register(10), Some(42));
    assert_eq!(machine.core().register(11), Some(5));
    assert_eq!(machine.core().register(12), Some(1));
}

#[test]
fn bounded_alignment_invalid_operands_preserve_existing_rom() {
    for directive in [".balign 8", ".p2align 3", ".align 3"] {
        for suffix in [
            ",",
            ", 256",
            ", -129",
            ", missing",
            ", 0xffffffffffffffff",
            ", 42,",
            ",,missing",
            ", 42, -1",
            ", 42, 18446744073709551615",
            ", 42, 2, 1",
            ", 42, 1 << 2",
            ",,",
        ] {
            for prefix in ["", ".byte 19\n"] {
                let source = format!("{prefix}{directive}{suffix}");
                let diagnostic = assemble_boot_rom(&source).unwrap_err().to_string();
                let line = if prefix.is_empty() { 1 } else { 2 };
                assert!(
                    diagnostic.starts_with(&format!("line {line}:")),
                    "{diagnostic}"
                );
                let files = Files::new();
                std::fs::write(files.0.join("guest.S"), &source).unwrap();
                std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
                let build = files.assemble();
                assert!(!build.status.success(), "{source}");
                assert!(!build.stderr.is_empty());
                assert_eq!(
                    std::fs::read(files.0.join("guest.S")).unwrap(),
                    source.as_bytes()
                );
                assert_eq!(
                    std::fs::read(files.0.join("rom.bin")).unwrap(),
                    b"previous ROM"
                );
            }
        }
    }
    assert!(assemble_boot_rom(".balign 8, 256, 1").is_err());
    assert!(assemble_boot_rom(".byte 1\n.balign 8, 256, 1").is_err());
}

#[test]
fn data_aliases_match_independent_bytes_and_canonical_directives() {
    for (aliases, canonical, values, expected) in [
        (
            &[".long", ".int", ".4byte"][..],
            ".word",
            "-2147483648, 2147483647, 4294967295, 0x12345678",
            &[
                0, 0, 0, 128, 255, 255, 255, 127, 255, 255, 255, 255, 120, 86, 52, 18,
            ][..],
        ),
        (
            &[".quad", ".8byte"][..],
            ".dword",
            "-9223372036854775808, 18446744073709551615, 0x0123456789abcdef",
            &[
                0, 0, 0, 0, 0, 0, 0, 128, 255, 255, 255, 255, 255, 255, 255, 255, 239, 205, 171,
                137, 103, 69, 35, 1,
            ][..],
        ),
    ] {
        let canonical = assemble_boot_rom(&format!("{canonical} {values}")).unwrap();
        for alias in aliases {
            let rom = assemble_boot_rom(&format!("{alias} {values}")).unwrap();
            assert_eq!(&rom[..expected.len()], expected, "{alias}");
            assert!(rom == canonical, "{alias}");
        }
    }
}

#[test]
fn data_aliases_preserve_unaligned_labels_widths_and_rom_limits() {
    for (alias, width) in [
        (".long", 4),
        (".int", 4),
        (".4byte", 4),
        (".quad", 8),
        (".8byte", 8),
    ] {
        let rom = assemble_boot_rom(&format!(
            ".byte 42\n1: first: {alias} 1f, 1b, first\n1: .byte 19\n\
             .balign 4\nentry: wfi\n.dword entry"
        ))
        .unwrap();
        let end = 1 + width * 3;
        assert_eq!(rom[0], 42);
        for (index, address) in [0x1000u64 + end as u64, 0x1001, 0x1001]
            .into_iter()
            .enumerate()
        {
            let offset = 1 + index * width;
            assert_eq!(
                &rom[offset..offset + width],
                &address.to_le_bytes()[..width],
                "{alias}"
            );
        }
        assert_eq!(rom[end], 19);
        let entry = (end + 4) & !3;
        assert_eq!(&rom[entry..entry + 4], &0x10500073u32.to_le_bytes());
        assert_eq!(
            &rom[entry + 4..entry + 12],
            &(0x1000 + entry as u64).to_le_bytes()
        );
        let padding = BOOT_ROM_BYTES - width;
        let rom = assemble_boot_rom(&format!(".zero {padding}\n{alias} 42")).unwrap();
        assert_eq!(rom[padding], 42);
        assert!(
            assemble_boot_rom(&format!(".zero {}\n{alias} 42", padding + 1))
                .unwrap_err()
                .to_string()
                .contains("exceeds")
        );
        assert!(
            assemble_boot_rom(&format!(".byte 1\n{alias} 42\nwfi"))
                .unwrap_err()
                .to_string()
                .contains("aligned")
        );
    }
}

#[test]
fn data_aliases_execute_signed_unsigned_and_pointer_table_loads() {
    let rom = assemble_boot_rom(
        "la t0, 1f\nlw a0, 0(t0)\nlwu t1, 4(t0)\nadd a0, a0, t1\n\
         lw a1, 8(t0)\nlwu a2, 12(t0)\nld a3, 16(t0)\nld a4, 24(t0)\nwfi\n\
         .balign 8\n1: .long 19\n.int 23\n.4byte -2147483648, 4294967295\n\
         .quad 18446744073709551615\n.8byte 1b",
    )
    .unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 24, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert!(output.is_empty());
    for (register, value) in [
        (10, 42),
        (11, (-2147483648i64) as u64),
        (12, 4294967295),
        (13, u64::MAX),
        (14, 0x1028),
    ] {
        assert_eq!(machine.core().register(register), Some(value));
    }
}

#[test]
fn data_aliases_invalid_values_preserve_source_and_existing_rom() {
    for (alias, overflow, underflow) in [
        (".long", "4294967296", "-2147483649"),
        (".int", "4294967296", "-2147483649"),
        (".4byte", "4294967296", "-2147483649"),
        (".quad", "18446744073709551616", "-9223372036854775809"),
        (".8byte", "18446744073709551616", "-9223372036854775809"),
    ] {
        for operand in [
            "",
            overflow,
            underflow,
            "missing",
            "1f",
            "1b",
            "1,",
            ",1",
            "1,,2",
            "0x__ff",
            "42UL",
            "label + 4",
        ] {
            let source = format!("# first line\n{alias} {operand}");
            let diagnostic = assemble_boot_rom(&source).unwrap_err().to_string();
            assert!(diagnostic.starts_with("line 2:"), "{diagnostic}");
            let files = Files::new();
            std::fs::write(files.0.join("guest.S"), &source).unwrap();
            std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
            let build = files.assemble();
            assert!(!build.status.success(), "{source}");
            assert!(!build.stderr.is_empty());
            assert_eq!(
                std::fs::read(files.0.join("guest.S")).unwrap(),
                source.as_bytes()
            );
            assert_eq!(
                std::fs::read(files.0.join("rom.bin")).unwrap(),
                b"previous ROM"
            );
        }
    }
}

#[test]
fn filled_space_matches_every_byte_and_preserves_implicit_zero_fills() {
    for operation in [".zero", ".space", ".skip"] {
        for fill in -128i64..=255 {
            let rom =
                assemble_boot_rom(&format!(".byte 19\n{operation} 3, {fill}\n.byte 23")).unwrap();
            assert_eq!(&rom[..5], &[19, fill as u8, fill as u8, fill as u8, 23]);
            assert!(rom[5..].iter().all(|byte| *byte == 0));
        }
        let rom = assemble_boot_rom(&format!(".byte 19\n{operation} 3\n.byte 23")).unwrap();
        assert_eq!(&rom[..5], &[19, 0, 0, 0, 23]);
        let explicit = assemble_boot_rom(&format!(".byte 19\n{operation} 3, 0\n.byte 23")).unwrap();
        assert!(rom == explicit);
        for fill in ["0x_FF", "0o377", "0B1111_1111", "-0x80", "+42"] {
            let rom = assemble_boot_rom(&format!("{operation} 2, {fill}")).unwrap();
            let expected = if fill == "-0x80" {
                128
            } else if fill == "+42" {
                42
            } else {
                255
            };
            assert_eq!(&rom[..2], &[expected, expected]);
        }
    }
}

#[test]
fn filled_space_preserves_labels_alignment_and_exact_rom_limits() {
    let rom = assemble_boot_rom(
        "first: .space 0, -1\n1: .skip 3, 42\n\
         second: .zero 0, 128\n.zero 2, 255\n.balign 4\n\
         entry: wfi\n.dword first, 1b, second, entry",
    )
    .unwrap();
    assert_eq!(&rom[..8], &[42, 42, 42, 255, 255, 0, 0, 0]);
    assert_eq!(&rom[8..12], &0x10500073u32.to_le_bytes());
    for (bytes, address) in rom[12..44]
        .chunks_exact(8)
        .zip([0x1000u64, 0x1000, 0x1003, 0x1008])
    {
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), address);
    }
    for operation in [".zero", ".space", ".skip"] {
        let rom = assemble_boot_rom(&format!("{operation} 61440, 42\n{operation} 0, 255")).unwrap();
        assert_eq!(rom.len(), BOOT_ROM_BYTES);
        assert!(rom.iter().all(|byte| *byte == 42));
        assert!(assemble_boot_rom(&format!(".byte 1\n{operation} 61439, -1")).is_ok());
        assert!(
            assemble_boot_rom(&format!(".byte 1\n{operation} 61440, -1"))
                .unwrap_err()
                .to_string()
                .contains("exceeds")
        );
        assert!(
            assemble_boot_rom(&format!("{operation} 1, 0\nwfi"))
                .unwrap_err()
                .to_string()
                .contains("aligned")
        );
    }
}

#[test]
fn filled_space_executes_guest_owned_table_scan() {
    let rom = assemble_boot_rom(
        "la t0, 1f\nli t1, 4\nli a0, 0\n\
         2: lbu t2, 0(t0)\nadd a0, a0, t2\naddi t0, t0, 1\n\
         addi t1, t1, -1\nbnez t1, 2b\nwfi\n\
         1: .space 2, 19\n.skip 1, 4\n.zero 1, 0",
    )
    .unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 40, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert!(output.is_empty());
    assert_eq!(machine.core().register(10), Some(42));
}

#[test]
fn filled_space_invalid_operands_preserve_existing_rom() {
    for operation in [".zero", ".space", ".skip"] {
        for operands in [
            "",
            "1, 2, 3",
            "-1",
            "61441",
            "18446744073709551615",
            "1,",
            ",1",
            "1, -129",
            "1, 256",
            "0, 256",
            "0, missing",
            "1, 0xffffffffffffffff",
            "1, 1 << 2",
            "1, 0x__ff",
            "missing, 42",
        ] {
            let source = format!("# first line\n{operation} {operands}");
            let diagnostic = assemble_boot_rom(&source).unwrap_err().to_string();
            assert!(diagnostic.starts_with("line 2:"), "{diagnostic}");
            let files = Files::new();
            std::fs::write(files.0.join("guest.S"), &source).unwrap();
            std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
            let build = files.assemble();
            assert!(!build.status.success(), "{source}");
            assert!(!build.stderr.is_empty());
            assert_eq!(
                std::fs::read(files.0.join("guest.S")).unwrap(),
                source.as_bytes()
            );
            assert_eq!(
                std::fs::read(files.0.join("rom.bin")).unwrap(),
                b"previous ROM"
            );
        }
    }
}

#[test]
fn unsigned_literals_match_signed_encodings_and_independent_data_bytes() {
    for (literal, value) in [
        ("18446744073709551615", u64::MAX),
        ("+0X_FFFF_FFFF_FFFF_FFFF", u64::MAX),
        ("0o1777777777777777777777", u64::MAX),
        (
            "0B1111111111111111111111111111111111111111111111111111111111111111",
            u64::MAX,
        ),
        ("9223372036854775808", 1u64 << 63),
        ("0x8000_0000_0000_0001", (1u64 << 63) + 1),
        ("0xfedcba9876543210", 0xfedcba9876543210),
        ("0xffff_ffff_8000_0000", (-2147483648i64) as u64),
    ] {
        let signed = i64::from_le_bytes(value.to_le_bytes());
        let unsigned_rom = assemble_boot_rom(&format!("li a0, {literal}")).unwrap();
        let signed_rom = assemble_boot_rom(&format!("li a0, {signed}")).unwrap();
        assert_eq!(
            unsigned_rom
                .iter()
                .zip(signed_rom)
                .position(|(a, b)| *a != b),
            None,
            "{literal}"
        );
        let data = assemble_boot_rom(&format!(".dword {literal}, {signed}")).unwrap();
        assert_eq!(&data[..8], &value.to_le_bytes());
        assert_eq!(&data[8..16], &value.to_le_bytes());
    }
    let rom = assemble_boot_rom("li a0, 0xffffffffffffffff").unwrap();
    assert_eq!(&rom[..4], &0x00000537u32.to_le_bytes());
    assert_eq!(&rom[4..8], &0xfff5051bu32.to_le_bytes());
    assert!(rom[8..].iter().all(|byte| *byte == 0));
}

#[test]
fn unsigned_literals_execute_all_bases_boundaries_and_register_destinations() {
    let mut values = vec![u64::MAX, 1u64 << 63, (1u64 << 63) + 1, 0xfedcba9876543210];
    for bit in 0..64 {
        values.push(1u64 << bit);
        values.push(!(1u64 << bit));
    }
    let mut pattern = 0xfedcba9876543210u64;
    for _ in 0..64 {
        pattern ^= pattern << 13;
        pattern ^= pattern >> 7;
        pattern ^= pattern << 17;
        values.push(pattern);
    }
    for value in values {
        for literal in [
            value.to_string(),
            format!("0x{value:x}"),
            format!("0o{value:o}"),
            format!("0b{value:b}"),
        ] {
            for destination in ["a0", "zero"] {
                let rom = assemble_boot_rom(&format!("li {destination}, {literal}\nwfi")).unwrap();
                let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
                let mut output = Vec::new();
                let report = run_headless(&mut machine, 12, &[], &mut output).unwrap();
                assert_eq!(report.traps, 0, "{literal}");
                assert!(report.waiting > 0, "{literal}");
                assert!(output.is_empty());
                for register in 0..32 {
                    let expected = if destination == "a0" && register == 10 {
                        value
                    } else {
                        0
                    };
                    assert_eq!(
                        machine.core().register(register),
                        Some(expected),
                        "{literal}: x{register}"
                    );
                }
            }
        }
    }
}

#[test]
fn unsigned_literals_preserve_layout_and_exact_rom_limits() {
    let rom = assemble_boot_rom(
        "li a0, 0xffffffffffffffff\nli a1, 0x8000000000000000\n\
         la a2, 1f\nld a3, 0(a2)\nwfi\n.balign 8\n\
         1: .dword 0xffffffffffffffff, 1b",
    )
    .unwrap();
    assert_eq!(&rom[56..64], &u64::MAX.to_le_bytes());
    assert_eq!(&rom[64..72], &0x1038u64.to_le_bytes());
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let report = run_headless(&mut machine, 24, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(u64::MAX));
    assert_eq!(machine.core().register(11), Some(1u64 << 63));
    assert_eq!(machine.core().register(12), Some(0x1038));
    assert_eq!(machine.core().register(13), Some(u64::MAX));
    for (padding, operation) in [
        (61432, "li a0, 0xffffffffffffffff"),
        (61408, "li a0, 0x8000000000000000"),
        (61432, ".dword 18446744073709551615"),
    ] {
        assert_eq!(
            assemble_boot_rom(&format!(".zero {padding}\n{operation}"))
                .unwrap()
                .len(),
            BOOT_ROM_BYTES
        );
        assert!(
            assemble_boot_rom(&format!(".zero {}\n{operation}", padding + 4))
                .unwrap_err()
                .to_string()
                .contains("exceeds")
        );
    }
}

#[test]
fn unsigned_literals_invalid_inputs_preserve_outputs_and_signed_immediate_rules() {
    let literals = [
        "18446744073709551616",
        "0x10000000000000000",
        "-9223372036854775809",
        "-0xffffffffffffffff",
        "+0x",
        "0x__ff",
        "0xffff_",
        "1__0",
        "018446744073709551615",
        "18446744073709551615UL",
        "0xffffffffffffffff + 1",
        "170141183460469231731687303715884105728",
    ];
    let mut sources = Vec::new();
    for literal in literals {
        sources.push(format!("li a0, {literal}"));
        sources.push(format!(".dword {literal}"));
    }
    for source in [
        "addi a0, zero, 0xffffffffffffffff",
        "ld a0, 0xffffffffffffffff(zero)",
        "csrr a0, 0xffffffffffffffff",
        "slli a0, a0, 0xffffffffffffffff",
        "j 0xffffffffffffffff",
        "la a0, 0xffffffffffffffff",
        ".byte 0xffffffffffffffff",
        ".half 0xffffffffffffffff",
        ".word 0xffffffffffffffff",
        ".zero 0xffffffffffffffff",
        ".balign 0xffffffffffffffff",
    ] {
        sources.push(source.to_string());
    }
    for source in sources {
        let diagnostic = assemble_boot_rom(&source).unwrap_err().to_string();
        assert!(diagnostic.starts_with("line 1:"), "{diagnostic}");
        let files = Files::new();
        std::fs::write(files.0.join("guest.S"), &source).unwrap();
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.assemble();
        assert!(!build.status.success(), "{source}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("guest.S")).unwrap(),
            source.as_bytes()
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn string_data_preserves_quoted_delimiters_and_checked_byte_escapes() {
    let rom = assemble_boot_rom(
        r##"first: .ascii "a#,:", "", "\"\\", "z" # ignored ":,
           .asciz "", "b#,:"
           .string "c", ""
           .ascii "\a\b\f\n\r\t\v\'", "\0\07\101\377", "\x0\x7f\x80\xff"
           .ascii "\0007", "\x00002a", "\1234"
           .ascii "end\\", "#include: x,y"
           .ascii "" # empty data
        "##,
    )
    .unwrap();
    let expected = b"a#,:\"\\z\0b#,:\0c\0\0\x07\x08\x0c\n\r\t\x0b'\0\x07A\xff\0\x7f\x80\xff\07*S4end\\#include: x,y";
    assert_eq!(&rom[..expected.len()], expected);
    assert!(rom[expected.len()..].iter().all(|byte| *byte == 0));
    let utf8 = assemble_boot_rom(".ascii \"\u{e4}\u{1f600}\"").unwrap();
    assert_eq!(&utf8[..6], "\u{e4}\u{1f600}".as_bytes());
    for value in 0..=255u8 {
        for escape in [format!("\\{value:03o}"), format!("\\x{value:02x}")] {
            let rom = assemble_boot_rom(&format!(".ascii \"{escape}\"")).unwrap();
            assert_eq!(rom[0], value, "{escape}");
            assert!(rom[1..].iter().all(|byte| *byte == 0));
        }
    }
}

#[test]
fn string_data_sizes_labels_alignment_and_rom_limits_exactly() {
    let rom = assemble_boot_rom(
        "1: .ascii \"\"\nfirst: .ascii \"a,b:#\"\n\
         2: .asciz \"\", \"x\"\n.string \"yz\"\n.ascii \"\"\n\
         .balign 4\nentry: wfi\n.dword first, 1b, 2b, entry",
    )
    .unwrap();
    assert_eq!(&rom[..12], b"a,b:#\0x\0yz\0\0");
    assert_eq!(&rom[12..16], &0x10500073u32.to_le_bytes());
    for (bytes, address) in rom[16..48]
        .chunks_exact(8)
        .zip([0x1000u64, 0x1000, 0x1005, 0x100c])
    {
        assert_eq!(u64::from_le_bytes(bytes.try_into().unwrap()), address);
    }
    for directive in [".ascii", ".asciz", ".string"] {
        let padding = if directive == ".ascii" { 61439 } else { 61438 };
        let rom = assemble_boot_rom(&format!(".zero {padding}\n{directive} \"x\"")).unwrap();
        assert_eq!(rom[padding], b'x');
        assert_eq!(rom.len(), BOOT_ROM_BYTES);
        assert!(
            assemble_boot_rom(&format!(".zero {}\n{directive} \"x\"", padding + 1))
                .unwrap_err()
                .to_string()
                .contains("exceeds")
        );
    }
    assert!(assemble_boot_rom(".zero 61440\n.ascii \"\"").is_ok());
    assert!(assemble_boot_rom(".zero 61440\n.asciz \"\"").is_err());
    assert!(
        assemble_boot_rom(".ascii \"x\"\nwfi")
            .unwrap_err()
            .to_string()
            .contains("aligned")
    );
}

#[test]
fn string_data_guest_executes_bytes_and_numeric_label_addresses() {
    let rom = assemble_boot_rom(
        "li t0, 0x10000000\nla t1, 1f\nli a0, 0\n\
         2: lbu t2, 0(t1)\nbeqz t2, 3f\nsb t2, 0(t0)\n\
         addi a0, a0, 1\naddi t1, t1, 1\nj 2b\n\
         3: wfi\n1: .string \"S#,:*\\n\"",
    )
    .unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 100, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(output, b"S#,:*\n");
    assert_eq!(machine.core().register(10), Some(6));
}

#[test]
fn string_data_invalid_operands_preserve_existing_rom() {
    for source in [
        ".ascii",
        ".asciz",
        ".string",
        ".ascii 42",
        ".ascii 'x'",
        ".ascii \"x\",",
        ".ascii ,\"x\"",
        ".ascii \"x\",,\"y\"",
        ".ascii \"x\" \"y\"",
        ".ascii \"x",
        ".ascii \"x\\",
        ".ascii \"\\q\"",
        ".ascii \"\\u0041\"",
        ".ascii \"\\x\"",
        ".ascii \"\\x100\"",
        ".ascii \"\\400\"",
        ".ascii \"\\777\"",
        ".ascii \"\\8\"",
        ".ascii \"x\"junk",
        ".ascii \"x\"#\nwfi",
    ] {
        let source = format!("# first line\n{source}");
        let diagnostic = assemble_boot_rom(&source).unwrap_err().to_string();
        assert!(
            diagnostic.starts_with("line 2:") || diagnostic.starts_with("line 3:"),
            "{diagnostic}"
        );
        let files = Files::new();
        std::fs::write(files.0.join("guest.S"), &source).unwrap();
        std::fs::write(files.0.join("rom.bin"), b"previous ROM").unwrap();
        let build = files.assemble();
        assert!(!build.status.success(), "{source}");
        assert!(!build.stderr.is_empty());
        assert_eq!(
            std::fs::read(files.0.join("guest.S")).unwrap(),
            source.as_bytes()
        );
        assert_eq!(
            std::fs::read(files.0.join("rom.bin")).unwrap(),
            b"previous ROM"
        );
    }
}

#[test]
fn tail_calls_match_independent_words_and_displacement_boundaries() {
    for (delta, upper, lower) in [
        (0, 0x00000317u32, 0x00030067u32),
        (8, 0x00000317, 0x00830067),
        (-4, 0x00000317, 0xffc30067),
        (2048, 0x00001317, 0x80030067),
        (-2048, 0x00000317, 0x80030067),
        (1048576, 0x00100317, 0x00030067),
        (-2147483648i64, 0x80000317, 0x00030067),
        (2147481596, 0x7ffff317, 0x7fc30067),
    ] {
        let rom = assemble_boot_rom(&format!("tail {delta}")).unwrap();
        assert_eq!(&rom[..4], &upper.to_le_bytes(), "{delta}");
        assert_eq!(&rom[4..8], &lower.to_le_bytes(), "{delta}");
        assert!(rom[8..].iter().all(|byte| *byte == 0));
    }
}

#[test]
fn tail_calls_execute_forward_backward_chains_and_preserve_links() {
    for (source, target, scratch) in [
        (
            "la ra, done\nli a0, 19\ntail target\naddi a0, zero, -99\n\
             target: addi a0, a0, 23\nret\ndone: wfi",
            0x1024u64,
            0x1010u64,
        ),
        (
            "j start\n1: addi a0, a0, 23\nret\n\
             start: la ra, done\nli a0, 19\ntail 1b\naddi a0, zero, -99\ndone: wfi",
            0x1028,
            0x101c,
        ),
    ] {
        let rom = assemble_boot_rom(source).unwrap();
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
        let mut output = Vec::new();
        let report = run_headless(&mut machine, 24, &[], &mut output).unwrap();
        assert_eq!(report.traps, 0);
        assert!(report.waiting > 0);
        assert!(output.is_empty());
        for register in 0..32 {
            let expected = match register {
                1 => target,
                6 => scratch,
                10 => 42,
                _ => 0,
            };
            assert_eq!(
                machine.core().register(register),
                Some(expected),
                "x{register}"
            );
        }
    }
    let rom = assemble_boot_rom(
        "la ra, done\nli a0, 19\ntail 1f\naddi a0, zero, -99\n\
         1: tail 2f\naddi a0, zero, -98\n\
         .balign 16\n2: addi a0, a0, 23\nret\ndone: wfi\n\
         .balign 8\n.dword 1b, 2b, done",
    )
    .unwrap();
    assert_eq!(&rom[64..72], &0x101cu64.to_le_bytes());
    assert_eq!(&rom[72..80], &0x1030u64.to_le_bytes());
    assert_eq!(&rom[80..88], &0x1038u64.to_le_bytes());
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let report = run_headless(&mut machine, 24, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(42));
    assert_eq!(machine.core().register(1), Some(0x1038));
    assert_eq!(
        assemble_boot_rom(".zero 61432\ntail -4").unwrap().len(),
        BOOT_ROM_BYTES
    );
    assert!(
        assemble_boot_rom(".zero 61436\ntail -4")
            .unwrap_err()
            .to_string()
            .contains("exceeds")
    );
}

#[test]
fn tail_calls_invalid_operands_preserve_existing_rom() {
    for source in [
        "tail",
        "tail ra, target",
        "tail missing",
        "tail 1f",
        "tail 1b",
        "tail 2",
        "tail -2",
        "tail 2147483648",
        "tail -2147483652",
        "tail 2147483644",
        "tail 0(ra)",
    ] {
        let diagnostic = assemble_boot_rom(source).unwrap_err().to_string();
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

#[test]
fn counter_reads_match_independent_instruction_words_and_csr_forms() {
    for (operation, csr, expected) in [
        ("rdcycle", "cycle", 0xc0002573u32),
        ("rdtime", "time", 0xc0102573),
        ("rdinstret", "instret", 0xc0202573),
    ] {
        let rom = assemble_boot_rom(&format!("{operation} a0")).unwrap();
        assert_eq!(&rom[..4], &expected.to_le_bytes(), "{operation}");
        for register in 0..32 {
            assert!(
                assemble_boot_rom(&format!("{operation} x{register}")).unwrap()
                    == assemble_boot_rom(&format!("csrrs x{register}, {csr}, zero")).unwrap(),
                "{operation} x{register}"
            );
        }
    }
}

#[test]
fn counter_reads_execute_full_width_progression_and_zero_destinations() {
    for (operation, writable, bias) in [("rdcycle", "mcycle", 1u64), ("rdinstret", "minstret", 1)] {
        for value in [
            0u64,
            0xfffffffE,
            0x1234567887654321,
            i64::MAX as u64,
            u64::MAX,
        ] {
            let rom = assemble_boot_rom(&format!(
                "li a1, {}
                 csrw {writable}, a1
                 {operation} a0
                 {operation} zero
                 {operation} a2
                 wfi",
                value as i64
            ))
            .unwrap();
            let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
            let mut output = Vec::new();
            let report = run_headless(&mut machine, 24, &[], &mut output).unwrap();
            assert_eq!(report.traps, 0);
            assert!(report.waiting > 0);
            assert!(output.is_empty());
            for register in 0..32 {
                let expected = match register {
                    10 => value.wrapping_add(bias),
                    11 => value,
                    12 => value.wrapping_add(bias + 2),
                    _ => 0,
                };
                assert_eq!(
                    machine.core().register(register),
                    Some(expected),
                    "{operation}, initial {value:#x}, x{register}"
                );
            }
        }
    }
    let rom = assemble_boot_rom("rdtime a0\nrdtime zero\nrdtime a1\nwfi").unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let report = run_headless(&mut machine, 12, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(
        machine.core().register(11).unwrap() - machine.core().register(10).unwrap(),
        2
    );
}

#[test]
fn counter_reads_enforce_independent_machine_and_supervisor_enable_bits() {
    use suomicpu16::kiuas16::{CoreTrapCause, PrivilegeMode};

    for (operation, bit, word) in [
        ("rdcycle", 1u64, 0xc0002573u64),
        ("rdtime", 2, 0xc0102573),
        ("rdinstret", 4, 0xc0202573),
    ] {
        for (mode, mpp) in [
            (PrivilegeMode::Machine, 3u64),
            (PrivilegeMode::Supervisor, 1),
            (PrivilegeMode::User, 0),
        ] {
            for machine_enabled in [false, true] {
                for supervisor_enabled in [false, true] {
                    let machine_mask = if machine_enabled { bit } else { 7 ^ bit };
                    let supervisor_mask = if supervisor_enabled { bit } else { 7 ^ bit };
                    let rom = assemble_boot_rom(&format!(
                        "la t0, fault
                         csrw mtvec, t0
                         li t0, -1
                         csrw pmpaddr0, t0
                         li t0, 15
                         csrw pmpcfg0, t0
                         li t0, {machine_mask}
                         csrw mcounteren, t0
                         li t0, {supervisor_mask}
                         csrw scounteren, t0
                         la a4, entry
                         csrw mepc, a4
                         li t0, {}
                         csrw mstatus, t0
                         li a0, -1
                         mret
                         entry: {operation} a0
                         addi a1, zero, 42
                         wfi
                         fault: wfi",
                        mpp << 11
                    ))
                    .unwrap();
                    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
                    let mut output = Vec::new();
                    let report = run_headless(&mut machine, 48, &[], &mut output).unwrap();
                    let allowed = mode == PrivilegeMode::Machine
                        || (machine_enabled
                            && (mode == PrivilegeMode::Supervisor || supervisor_enabled));
                    assert_eq!(
                        report.traps,
                        u64::from(!allowed),
                        "{operation} {mode:?} M:{machine_enabled} S:{supervisor_enabled}"
                    );
                    assert!(report.waiting > 0);
                    assert!(output.is_empty());
                    assert_eq!(
                        machine.core().register(11),
                        Some(if allowed { 42 } else { 0 })
                    );
                    if allowed {
                        assert_ne!(machine.core().register(10), Some(u64::MAX));
                        assert_eq!(machine.core().privilege(), mode);
                    } else {
                        let trap = report.last_trap.unwrap();
                        assert_eq!(trap.cause, CoreTrapCause::IllegalInstruction);
                        assert_eq!(trap.value, word);
                        assert_eq!(Some(trap.pc), machine.core().register(14));
                        assert_eq!(machine.core().register(10), Some(u64::MAX));
                    }
                }
            }
        }
    }
}

#[test]
fn counter_reads_invalid_operands_preserve_existing_rom() {
    for operation in ["rdcycle", "rdtime", "rdinstret"] {
        for operands in ["", "a0, a1", "x32", "1", "(a0)"] {
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
    for operation in ["rdcycleh", "rdtimeh", "rdinstreth"] {
        assert!(
            assemble_boot_rom(&format!("{operation} a0"))
                .unwrap_err()
                .to_string()
                .contains("unsupported RV64 instruction")
        );
    }
}

#[test]
fn translation_fence_matches_independent_instruction_words() {
    for (source, expected) in [
        ("sfence.vma", 0x12000073u32),
        ("sfence.vma zero", 0x12000073),
        ("sfence.vma zero, zero", 0x12000073),
        ("sfence.vma a0", 0x12050073),
        ("sfence.vma a0, a1", 0x12b50073),
        ("sfence.vma zero, a1", 0x12b00073),
        ("sfence.vma x31, x31", 0x13ff8073),
    ] {
        let rom = assemble_boot_rom(source).unwrap();
        assert_eq!(&rom[..4], &expected.to_le_bytes(), "{source}");
    }
    for address in 0..32 {
        for asid in 0..32 {
            let rom = assemble_boot_rom(&format!("sfence.vma x{address}, x{asid}")).unwrap();
            let expected = 0x12000073u32 | address << 15 | asid << 20;
            assert_eq!(&rom[..4], &expected.to_le_bytes());
        }
    }
}

#[test]
fn translation_fence_enforces_privilege_and_tvm_without_operand_writes() {
    use suomicpu16::kiuas16::{CoreTrapCause, PrivilegeMode};

    for (mode, mpp) in [
        (PrivilegeMode::Machine, 3u64),
        (PrivilegeMode::Supervisor, 1),
        (PrivilegeMode::User, 0),
    ] {
        for tvm in [false, true] {
            let status = (mpp << 11) | if tvm { 1 << 20 } else { 0 };
            let rom = assemble_boot_rom(&format!(
                "la t0, fault
                 csrw mtvec, t0
                 li t0, -1
                 csrw pmpaddr0, t0
                 li t0, 15
                 csrw pmpcfg0, t0
                 li a0, 42
                 li a1, 7
                 la a4, entry
                 csrw mepc, a4
                 li t0, {status}
                 csrw mstatus, t0
                 mret
                 entry: sfence.vma a0, a1
                 addi a2, zero, 42
                 wfi
                 fault: wfi"
            ))
            .unwrap();
            let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
            machine.bus_mut().write(0x80000000, &[0xa5; 16]).unwrap();
            let mut output = Vec::new();
            let report = run_headless(&mut machine, 40, &[], &mut output).unwrap();
            let illegal = mode == PrivilegeMode::User || (mode == PrivilegeMode::Supervisor && tvm);
            assert_eq!(report.traps, u64::from(illegal), "{mode:?}, TVM {tvm}");
            assert!(report.waiting > 0);
            assert!(output.is_empty());
            assert_eq!(machine.core().register(10), Some(42));
            assert_eq!(machine.core().register(11), Some(7));
            assert_eq!(
                machine.core().register(12),
                Some(if illegal { 0 } else { 42 })
            );
            assert_eq!(
                machine.core().privilege(),
                if illegal {
                    PrivilegeMode::Machine
                } else {
                    mode
                }
            );
            if illegal {
                let trap = report.last_trap.unwrap();
                assert_eq!(trap.cause, CoreTrapCause::IllegalInstruction);
                assert_eq!(trap.value, 0x12b50073);
                assert_eq!(Some(trap.pc), machine.core().register(14));
            }
            let mut memory = [0; 16];
            machine.bus_mut().read(0x80000000, &mut memory).unwrap();
            assert_eq!(memory, [0xa5; 16]);
        }
    }
}

#[test]
fn translation_fence_tvm_is_guest_writable_and_gates_supervisor_satp() {
    use suomicpu16::kiuas16::CoreTrapCause;

    for instruction in ["csrr a0, satp", "csrw satp, a1", "sfence.vma"] {
        for tvm in [false, true] {
            let status = 2048 | if tvm { 1 << 20 } else { 0 };
            let rom = assemble_boot_rom(&format!(
                "la t0, fault
                 csrw mtvec, t0
                 li t0, -1
                 csrw pmpaddr0, t0
                 li t0, 15
                 csrw pmpcfg0, t0
                 la a4, entry
                 csrw mepc, a4
                 li t0, {status}
                 csrw mstatus, t0
                 csrr a3, mstatus
                 csrr a5, sstatus
                 csrw sstatus, zero
                 csrr a6, mstatus
                 li a0, 99
                 li a1, 0
                 mret
                 entry: {instruction}
                 addi a2, zero, 42
                 wfi
                 fault: wfi"
            ))
            .unwrap();
            let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
            let report = run_headless(&mut machine, 48, &[], &mut Vec::new()).unwrap();
            assert_eq!(report.traps, u64::from(tvm), "{instruction}, TVM {tvm}");
            assert!(report.waiting > 0);
            assert_eq!(machine.core().register(13), Some(status));
            assert_eq!(machine.core().register(15), Some(0));
            assert_eq!(machine.core().register(16), Some(status));
            assert_eq!(machine.core().register(12), Some(if tvm { 0 } else { 42 }));
            assert_eq!(
                machine.core().register(10),
                Some(if !tvm && instruction == "csrr a0, satp" {
                    0
                } else {
                    99
                })
            );
            if tvm {
                let trap = report.last_trap.unwrap();
                assert_eq!(trap.cause, CoreTrapCause::IllegalInstruction);
                assert_eq!(Some(trap.pc), machine.core().register(14));
            }
        }
    }
    let rom = assemble_boot_rom(
        "li t0, 1048576
         csrs mstatus, t0
         csrr a0, mstatus
         csrc mstatus, t0
         csrr a1, mstatus
         wfi",
    )
    .unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let report = run_headless(&mut machine, 16, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(1 << 20));
    assert_eq!(machine.core().register(11), Some(0));
}

#[test]
fn translation_fence_executes_guest_owned_sv39_page_remap() {
    use suomicpu16::kiuas16::PrivilegeMode;

    let rom = assemble_boot_rom(
        "li t0, -1
         csrw pmpaddr0, t0
         li t0, 15
         csrw pmpcfg0, t0
         li t0, 2147483648
         li t1, 207
         sd t1, 0(t0)
         li t1, 0x20000401
         sd t1, 8(t0)
         li t1, 0x200000cf
         sd t1, 16(t0)
         li t2, 2147487744
         li t1, 0x20000801
         sd t1, 0(t2)
         li t2, 2147491840
         li t1, 0x20000cc7
         sd t1, 0(t2)
         li t3, 2147495936
         li t1, 19
         sd t1, 0(t3)
         li t3, 2147500032
         li t1, 42
         sd t1, 0(t3)
         li t1, 8
         slli t1, t1, 60
         li t3, 0x80000
         or t1, t1, t3
         csrw satp, t1
         sfence.vma
         la t3, supervisor
         csrw mepc, t3
         li t3, 2048
         csrw mstatus, t3
         mret
         supervisor: li t4, 0x40000000
         ld a0, 0(t4)
         li t3, 0x200010c7
         sd t3, 0(t2)
         li t5, 0
         sfence.vma t4, t5
         ld a1, 0(t4)
         csrr a2, satp
         wfi",
    )
    .unwrap();
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 0x20000).unwrap();
    let mut output = Vec::new();
    let report = run_headless(&mut machine, 160, &[], &mut output).unwrap();
    assert_eq!(report.traps, 0, "{:?}", report.last_trap);
    assert!(report.waiting > 0);
    assert!(output.is_empty());
    assert_eq!(machine.core().privilege(), PrivilegeMode::Supervisor);
    assert_eq!(machine.core().register(10), Some(19));
    assert_eq!(machine.core().register(11), Some(42));
    assert_eq!(machine.core().register(12), Some(0x8000000000080000));
    for (address, expected) in [
        (0x80002000, 0x200010c7u64),
        (0x80003000, 19),
        (0x80004000, 42),
    ] {
        let mut bytes = [0; 8];
        machine.bus_mut().read(address, &mut bytes).unwrap();
        assert_eq!(u64::from_le_bytes(bytes), expected);
    }
}

#[test]
fn translation_fence_invalid_operands_preserve_existing_rom() {
    for operands in [
        "a0, a1, a2",
        "x32",
        "a0, x32",
        "1",
        "a0, 7",
        "(a0)",
        ",a1",
        "a0,",
    ] {
        let source = format!("sfence.vma {operands}");
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

#[test]
fn exponent_alignment_matches_mapped_addresses_and_byte_alignment() {
    for exponent in 0..=15 {
        let alignment = 1usize << exponent;
        for offset in [0usize, 1, 3, 4, 4095, 4096, 4097] {
            let address = (0x1000 + offset).next_multiple_of(alignment);
            for directive in [".p2align", ".align", ".balign"] {
                let operand = if directive == ".balign" {
                    alignment
                } else {
                    exponent
                };
                let rom = assemble_boot_rom(&format!(
                    ".zero {offset}\n{directive} {operand}\nvalue: .dword value"
                ))
                .unwrap();
                let index = address - 0x1000;
                assert!(rom[..index].iter().all(|byte| *byte == 0));
                assert_eq!(
                    &rom[index..index + 8],
                    &(address as u64).to_le_bytes(),
                    "{directive} {operand}, offset {offset}"
                );
                assert!(rom[index + 8..].iter().all(|byte| *byte == 0));
            }
        }
    }
}

#[test]
fn exponent_alignment_executes_aligned_labels_and_zero_length_directives() {
    for directive in [".p2align", ".align"] {
        let rom = assemble_boot_rom(&format!(
            "j 2f
             .byte 0xa5
             {directive} 13
             1: .dword 42
             .byte 0x5a
             {directive} 4
             2: la a1, 1b
             la a2, 2b
             ld a0, 0(a1)
             wfi
             3: {directive} 0
             .dword 3b, 3f
             3:"
        ))
        .unwrap();
        assert_eq!(&rom[4096..4104], &42u64.to_le_bytes());
        assert!(rom[5..4096].iter().all(|byte| *byte == 0));
        assert_eq!(&rom[4136..4144], &0x2028u64.to_le_bytes());
        assert_eq!(&rom[4144..4152], &0x2038u64.to_le_bytes());
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
        let mut output = Vec::new();
        let report = run_headless(&mut machine, 24, &[], &mut output).unwrap();
        assert_eq!(report.traps, 0);
        assert!(report.waiting > 0);
        assert!(output.is_empty());
        assert_eq!(machine.core().register(10), Some(42));
        assert_eq!(machine.core().register(11), Some(0x2000));
        assert_eq!(machine.core().register(12), Some(0x2010));
        assert_eq!(
            assemble_boot_rom(&format!(".zero 61439\n{directive} 15"))
                .unwrap()
                .len(),
            BOOT_ROM_BYTES
        );
        assert!(
            assemble_boot_rom(&format!(".zero 61439\n{directive} 15\n.byte 1"))
                .unwrap_err()
                .to_string()
                .contains("exceeds")
        );
    }
}

#[test]
fn exponent_alignment_invalid_operands_preserve_existing_rom() {
    for directive in [".p2align", ".align"] {
        for operand in [
            "",
            "-1",
            "16",
            "63",
            "64",
            "9223372036854775807",
            "missing",
            "2, 256",
            "2, 0, -1",
        ] {
            let source = format!("{directive} {operand}");
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
        assert!(
            assemble_boot_rom(&format!(".byte 1\n{directive} 0\nnop"))
                .unwrap_err()
                .to_string()
                .contains("aligned")
        );
    }
}

#[test]
fn halfword_data_matches_independent_bytes_and_aliases() {
    let expected = [
        0x00, 0x80, 0xff, 0xff, 0x00, 0x00, 0x01, 0x00, 0xff, 0x7f, 0x00, 0x80, 0xff, 0xff,
    ];
    for directive in [".half", ".short", ".2byte"] {
        let rom = assemble_boot_rom(&format!(
            ".byte 0xa5\n{directive} -32768, -1, 0, 1, 32767, 32768, 65535\n.byte 0x5a"
        ))
        .unwrap();
        assert_eq!(rom[0], 0xa5);
        assert_eq!(&rom[1..15], &expected, "{directive}");
        assert_eq!(rom[15], 0x5a);
        assert!(rom[16..].iter().all(|byte| *byte == 0));
    }
}

#[test]
fn halfword_data_executes_signed_unsigned_loads_and_label_layout() {
    for directive in [".half", ".short", ".2byte"] {
        let rom = assemble_boot_rom(&format!(
            "j read
             1: {directive} -32768, 65535, 42, 1b, 2f
             .balign 4
             2: read: la t0, 1b
             lh a0, 0(t0)
             lhu a1, 0(t0)
             lh a2, 2(t0)
             lhu a3, 2(t0)
             lhu a4, 4(t0)
             lhu a5, 6(t0)
             lhu a6, 8(t0)
             wfi"
        ))
        .unwrap();
        assert_eq!(&rom[4..14], &[0, 128, 255, 255, 42, 0, 4, 16, 16, 16]);
        let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
        let mut output = Vec::new();
        let report = run_headless(&mut machine, 24, &[], &mut output).unwrap();
        assert_eq!(report.traps, 0);
        assert!(report.waiting > 0);
        assert!(output.is_empty());
        for (register, expected) in [
            (10, (-32768i64) as u64),
            (11, 32768),
            (12, u64::MAX),
            (13, 65535),
            (14, 42),
            (15, 0x1004),
            (16, 0x1010),
        ] {
            assert_eq!(
                machine.core().register(register),
                Some(expected),
                "{directive}"
            );
        }
        let rom =
            assemble_boot_rom(&format!("j end\n.zero 61432\nend: {directive} end, 65535")).unwrap();
        assert_eq!(&rom[61436..], &[0xfc, 0xff, 0xff, 0xff]);
        assert!(
            assemble_boot_rom(&format!(".zero 61439\n{directive} 0"))
                .unwrap_err()
                .to_string()
                .contains("exceeds")
        );
    }
}

#[test]
fn halfword_data_invalid_operands_preserve_existing_rom() {
    for directive in [".half", ".short", ".2byte"] {
        for operand in [
            "", "-32769", "65536", "missing", "1f", "1b", "1,", ",1", "1,,2",
        ] {
            let source = format!("{directive} {operand}");
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
        for source in [
            format!("{directive} 0\nnop"),
            format!(".zero 61440\nend:\n{directive} end"),
            format!("{directive} end\n.zero 61438\nend:"),
        ] {
            let diagnostic = assemble_boot_rom(&source).unwrap_err().to_string();
            assert!(
                diagnostic.contains("aligned")
                    || diagnostic.contains("exceeds")
                    || diagnostic.contains("out of range"),
                "{diagnostic}"
            );
        }
    }
}

#[test]
fn jump_forms_match_independent_words_and_existing_encodings() {
    for (source, canonical, expected) in [
        ("jal 8", "jal ra, 8", 0x008000efu32),
        ("jalr a1", "jalr ra, 0(a1)", 0x000580e7),
        ("jr a1", "jalr zero, 0(a1)", 0x00058067),
        ("jr ra", "ret", 0x00008067),
        ("jalr a0, a1, -2048", "jalr a0, -2048(a1)", 0x80058567),
        ("jalr a0, a1, 2047", "jalr a0, 2047(a1)", 0x7ff58567),
        ("jalr a1, a1, 0", "jalr a1, 0(a1)", 0x000585e7),
        ("jr x11", "jalr x0, 0(x11)", 0x00058067),
        ("jalr x11", "jalr x1, 0(x11)", 0x000580e7),
    ] {
        let rom = assemble_boot_rom(source).unwrap();
        assert_eq!(&rom[..4], &expected.to_le_bytes(), "{source}");
        assert!(
            rom == assemble_boot_rom(canonical).unwrap(),
            "{source} != {canonical}"
        );
    }
    for target in ["-1048576", "-4", "0", "1048572", "1b", "1f", "end"] {
        assert!(
            assemble_boot_rom(&format!("1: nop\njal {target}\n1: end: wfi")).unwrap()
                == assemble_boot_rom(&format!("1: nop\njal ra, {target}\n1: end: wfi")).unwrap(),
            "{target}"
        );
    }
}

#[test]
fn jump_forms_execute_indirect_links_aliases_offsets_and_low_bit_clearing() {
    let mut cases = vec![("jr a1".to_string(), 0, 0), ("jalr a1".to_string(), 1, 0)];
    for offset in [-2048i64, -1, 0, 1, 2047] {
        for (destination, register) in [("zero", 0), ("ra", 1), ("a1", 11), ("a2", 12)] {
            cases.push((
                format!("jalr {destination}, {offset}(a1)"),
                register,
                offset,
            ));
            cases.push((
                format!("jalr {destination}, a1, {offset}"),
                register,
                offset,
            ));
        }
    }
    for (instruction, destination, offset) in cases {
        for low_bit in [0, 1] {
            let base = 0x1014 - offset + low_bit;
            let rom = assemble_boot_rom(&format!(
                "li a1, {base}
                 {instruction}
                 addi a0, zero, -99
                 wfi
                 target: addi a0, zero, 42
                 wfi"
            ))
            .unwrap();
            let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
            let mut output = Vec::new();
            let report = run_headless(&mut machine, 12, &[], &mut output).unwrap();
            assert_eq!(report.traps, 0, "{instruction}, low bit {low_bit}");
            assert!(report.waiting > 0);
            assert!(output.is_empty());
            for register in 0..32 {
                let expected = if register != 0 && register == destination {
                    0x100c
                } else {
                    match register {
                        10 => 42,
                        11 => base as u64,
                        _ => 0,
                    }
                };
                assert_eq!(
                    machine.core().register(register),
                    Some(expected),
                    "{instruction}, low bit {low_bit}, x{register}"
                );
            }
        }
    }
}

#[test]
fn jump_forms_invalid_operands_preserve_existing_rom() {
    for source in [
        "jal",
        "jal ra, 4, 8",
        "jal x32, 4",
        "jal missing",
        "jal 2",
        "jal 1048576",
        "jal -1048580",
        "jr",
        "jr ra, 0",
        "jr 0(ra)",
        "jr x32",
        "jalr",
        "jalr x32",
        "jalr ra, x32, 0",
        "jalr x32, ra, 0",
        "jalr ra, ra, x1",
        "jalr ra, ra, -2049",
        "jalr ra, ra, 2048",
        "jalr ra, ra, 0, zero",
        "jalr 0(ra)",
    ] {
        let diagnostic = assemble_boot_rom(source).unwrap_err().to_string();
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

#[test]
fn wide_immediates_match_independent_expansions_and_preserve_32_bit_bytes() {
    for (literal, canonical) in [
        (
            "2147483648",
            "lui a0, 128\naddiw a0, a0, 0\nslli a0, a0, 12\naddi a0, a0, 0",
        ),
        (
            "9223372036854775807",
            "lui a0, 32768\naddiw a0, a0, 0\nslli a0, a0, 12\naddi a0, a0, 0\nslli a0, a0, 12\naddi a0, a0, 0\nslli a0, a0, 12\naddi a0, a0, -1",
        ),
        (
            "-9223372036854775808",
            "lui a0, -32768\naddiw a0, a0, 0\nslli a0, a0, 12\naddi a0, a0, 0\nslli a0, a0, 12\naddi a0, a0, 0\nslli a0, a0, 12\naddi a0, a0, 0",
        ),
    ] {
        assert_eq!(
            assemble_boot_rom(&format!("li a0, {literal}"))
                .unwrap()
                .iter()
                .zip(assemble_boot_rom(canonical).unwrap())
                .position(|(actual, expected)| *actual != expected),
            None,
            "{literal}"
        );
    }
    for (literal, upper, lower) in [
        ("0", 0x00000537u32, 0x0005051b_u32),
        ("2048", 0x00001537, 0x8005051b),
        ("-1", 0x00000537, 0xfff5051b),
        ("2147483647", 0x80000537, 0xfff5051b),
        ("-2147483648", 0x80000537, 0x0005051b),
    ] {
        let rom = assemble_boot_rom(&format!("li a0, {literal}")).unwrap();
        assert_eq!(&rom[..4], &upper.to_le_bytes());
        assert_eq!(&rom[4..8], &lower.to_le_bytes());
        assert!(rom[8..].iter().all(|byte| *byte == 0));
    }
    for literal in ["-9223372036854775808", "-0x8000_0000_0000_0000"] {
        let rom = assemble_boot_rom(&format!(".dword {literal}")).unwrap();
        assert_eq!(&rom[..8], &i64::MIN.to_le_bytes());
    }
}

#[test]
fn wide_immediates_execute_boundaries_patterns_and_zero_destinations() {
    let mut values = vec![i64::MIN, i64::MAX, 0x123456789abcdef, -0x123456789abcdef];
    for bit in 0..63 {
        let power = 1i64 << bit;
        for delta in [-1, 0, 1] {
            values.push(power.saturating_add(delta));
            values.push((-power).saturating_add(delta));
        }
    }
    let mut bits = 0x123456789abcdefu64;
    for _ in 0..128 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        values.push(bits as i64);
    }
    for destination in ["a0", "x11", "zero"] {
        for &value in &values {
            let rom = assemble_boot_rom(&format!("li {destination}, {value}\nwfi")).unwrap();
            let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
            let mut output = Vec::new();
            let report = run_headless(&mut machine, 12, &[], &mut output).unwrap();
            assert_eq!(report.traps, 0, "{value}");
            assert!(report.waiting > 0);
            assert!(output.is_empty());
            for register in 0..32 {
                let expected = if (destination == "a0" && register == 10)
                    || (destination == "x11" && register == 11)
                {
                    value as u64
                } else {
                    0
                };
                assert_eq!(
                    machine.core().register(register),
                    Some(expected),
                    "li {destination}, {value}; x{register}"
                );
            }
        }
    }
}

#[test]
fn wide_immediates_preserve_label_alignment_and_rom_size_checks() {
    let rom = assemble_boot_rom(
        "1: li a0, 9223372036854775807
         la a1, 1b
         la a2, 2f
         j 2f
         li a0, -9223372036854775808
         .balign 16
         2: wfi
         .balign 8
         .dword 1b, 2b",
    )
    .unwrap();
    assert_eq!(&rom[104..112], &0x1000u64.to_le_bytes());
    assert_eq!(&rom[112..120], &0x1060u64.to_le_bytes());
    let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
    let report = run_headless(&mut machine, 24, &[], &mut Vec::new()).unwrap();
    assert_eq!(report.traps, 0);
    assert!(report.waiting > 0);
    assert_eq!(machine.core().register(10), Some(i64::MAX as u64));
    assert_eq!(machine.core().register(11), Some(0x1000));
    assert_eq!(machine.core().register(12), Some(0x1060));
    let source = ".zero 61408\nli a0, 9223372036854775807";
    assert_eq!(assemble_boot_rom(source).unwrap().len(), BOOT_ROM_BYTES);
    assert!(
        assemble_boot_rom(&format!("{source}\nnop"))
            .unwrap_err()
            .to_string()
            .contains("exceeds")
    );
    assert!(
        assemble_boot_rom(".zero 61412\nli a0, 9223372036854775807")
            .unwrap_err()
            .to_string()
            .contains("exceeds")
    );
}

#[test]
fn wide_immediates_invalid_operands_preserve_existing_rom() {
    for operands in [
        "",
        "a0",
        "a0, 1, 2",
        "x32, 1",
        "a0, missing",
        "a0, 18446744073709551616",
        "a0, -9223372036854775809",
        "a0, 0x10000000000000000",
        "a0, 0x1ffffffffffffffff",
        "a0, 1 << 40",
        "a0, 0x1__0000",
    ] {
        let source = format!("li {operands}");
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

#[test]
fn explicit_fences_match_independent_instruction_words() {
    for (source, expected) in [
        ("fence", 0x0ff0000fu32),
        ("fence iorw, iorw", 0x0ff0000f),
        ("fence rw, rw", 0x0330000f),
        ("fence w, w", 0x0110000f),
        ("fence r, rw", 0x0230000f),
        ("fence o, o", 0x0440000f),
        ("fence io, rw", 0x0c30000f),
        ("fence 0, w", 0x0010000f),
        ("fence r, 0", 0x0200000f),
        ("fence 0, 0", 0x0000000f),
        ("fence.i", 0x0000100f),
    ] {
        let rom = assemble_boot_rom(source).unwrap();
        assert_eq!(&rom[..4], &expected.to_le_bytes(), "{source}");
    }
    for flags in [
        "iorw", "iowr", "irow", "irwo", "iwor", "iwro", "oirw", "oiwr", "oriw", "orwi", "owir",
        "owri", "riow", "riwo", "roiw", "rowi", "rwio", "rwoi", "wior", "wiro", "woir", "wori",
        "wrio", "wroi",
    ] {
        assert_eq!(
            assemble_boot_rom(&format!("fence {flags}, {flags}")).unwrap(),
            assemble_boot_rom("fence").unwrap(),
            "{flags}"
        );
    }
}

#[test]
fn explicit_fences_execute_every_mask_pair_without_register_or_memory_changes() {
    let sets = [
        "0", "w", "r", "rw", "o", "ow", "or", "orw", "i", "iw", "ir", "irw", "io", "iow", "ior",
        "iorw",
    ];
    for (predecessor, left) in sets.iter().enumerate() {
        for (successor, right) in sets.iter().enumerate() {
            let rom = assemble_boot_rom(&format!(
                "li a0, 42
                 li a1, -1
                 fence {left}, {right}
                 wfi"
            ))
            .unwrap();
            let expected = 0x0000000fu32 | (predecessor as u32) << 24 | (successor as u32) << 20;
            assert_eq!(&rom[16..20], &expected.to_le_bytes());
            let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
            let sentinel = [0xa5; 32];
            machine.bus_mut().write(0x80000000, &sentinel).unwrap();
            let mut output = Vec::new();
            let report = run_headless(&mut machine, 12, &[], &mut output).unwrap();
            assert_eq!(report.traps, 0, "fence {left}, {right}");
            assert!(report.waiting > 0);
            assert!(output.is_empty());
            for register in 0..32 {
                let expected = match register {
                    10 => 42,
                    11 => u64::MAX,
                    _ => 0,
                };
                assert_eq!(machine.core().register(register), Some(expected));
            }
            let mut memory = [0; 32];
            machine.bus_mut().read(0x80000000, &mut memory).unwrap();
            assert_eq!(memory, sentinel);
        }
    }
}

#[test]
fn explicit_fences_invalid_operands_preserve_existing_rom() {
    for source in [
        "fence rw",
        "fence r, w, i",
        "fence , w",
        "fence r,",
        "fence ,",
        "fence rr, w",
        "fence r, ww",
        "fence RW, rw",
        "fence r, x",
        "fence 1, rw",
        "fence rw, 15",
        "fence 0r, w",
        "fence r, w0",
        "fence r w, rw",
        "fence x0, x1",
        "fence.i r, w",
        "fence.tso",
    ] {
        let diagnostic = assemble_boot_rom(source).unwrap_err().to_string();
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

#[test]
fn scalar_pseudos_match_independent_words_and_canonical_instructions() {
    for (operation, canonical, expected) in [
        ("not", "xori a0, a1, -1", 0xfff5c513u32),
        ("neg", "sub a0, zero, a1", 0x40b00533),
        ("negw", "subw a0, zero, a1", 0x40b0053b),
        ("sext.w", "addiw a0, a1, 0", 0x0005851b),
        ("seqz", "sltiu a0, a1, 1", 0x0015b513),
        ("snez", "sltu a0, zero, a1", 0x00b03533),
        ("sltz", "slt a0, a1, zero", 0x0005a533),
        ("sgtz", "slt a0, zero, a1", 0x00b02533),
    ] {
        let rom = assemble_boot_rom(&format!("{operation} a0, a1")).unwrap();
        assert_eq!(&rom[..4], &expected.to_le_bytes(), "{operation}");
        assert_eq!(rom, assemble_boot_rom(canonical).unwrap());
        assert_eq!(
            rom,
            assemble_boot_rom(&format!("{operation} x10, x11")).unwrap()
        );
    }
}

#[test]
fn scalar_pseudos_execute_full_width_edges_word_sign_extension_and_aliases() {
    let values = [
        0u64,
        1,
        42,
        u64::MAX,
        i64::MIN as u64,
        i64::MAX as u64,
        0x80000000,
        0x7fffffff,
        0x100000000,
        0x123456788000002a,
        0xabcdef01ffffffff,
    ];
    for operation in [
        "not", "neg", "negw", "sext.w", "seqz", "snez", "sltz", "sgtz",
    ] {
        for (destination, source) in [
            ("a0", "a1"),
            ("a1", "a1"),
            ("zero", "a1"),
            ("a0", "zero"),
            ("zero", "zero"),
        ] {
            let rom = assemble_boot_rom(&format!(
                "la t0, value
                 ld a1, 0(t0)
                 {operation} {destination}, {source}
                 wfi
                 .balign 8
                 value: .dword 0"
            ))
            .unwrap();
            for value in values {
                let input = if source == "zero" { 0 } else { value };
                let expected = match operation {
                    "not" => !input,
                    "neg" => input.wrapping_neg(),
                    "negw" => (-(i64::from(input as i32))) as i32 as i64 as u64,
                    "sext.w" => input as i32 as i64 as u64,
                    "seqz" => u64::from(input == 0),
                    "snez" => u64::from(input != 0),
                    "sltz" => u64::from((input as i64) < 0),
                    _ => u64::from((input as i64) > 0),
                };
                let mut image = rom.clone();
                image[24..32].copy_from_slice(&value.to_le_bytes());
                let mut machine = Kiuas16Machine::new_with_ram_size(image, 4096).unwrap();
                let report = run_headless(&mut machine, 12, &[], &mut Vec::new()).unwrap();
                assert_eq!(report.traps, 0);
                assert!(report.waiting > 0);
                assert_eq!(machine.core().register(0), Some(0));
                for (name, register, original) in [("a0", 10, 0), ("a1", 11, value)] {
                    assert_eq!(
                        machine.core().register(register),
                        Some(if destination == name {
                            expected
                        } else {
                            original
                        }),
                        "{operation} {destination}, {source}: {value:#x}"
                    );
                }
            }
        }
    }
}

#[test]
fn scalar_pseudos_invalid_operands_preserve_existing_rom() {
    for operation in [
        "not", "neg", "negw", "sext.w", "seqz", "snez", "sltz", "sgtz",
    ] {
        for operands in ["", "a0", "a0, a1, a2", "x32, a1", "a0, x32", "a0, 1"] {
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
fn word_shifts_match_independent_instruction_words() {
    for (operation, expected) in [
        ("sllw", 0x00c5953bu32),
        ("srlw", 0x00c5d53b),
        ("sraw", 0x40c5d53b),
    ] {
        let rom = assemble_boot_rom(&format!("{operation} a0, a1, a2")).unwrap();
        assert_eq!(&rom[..4], &expected.to_le_bytes(), "{operation}");
        assert_eq!(
            rom,
            assemble_boot_rom(&format!("{operation} x10, x11, x12")).unwrap()
        );
    }
}

#[test]
fn word_shifts_execute_masked_counts_sign_extension_and_register_aliases() {
    let values = [
        0u64,
        1,
        u64::MAX,
        0x80000000,
        0x7fffffff,
        0x1234567800000000,
        0xfedcba9880000001,
        0xabcdef01ffffffff,
    ];
    let counts = [0u64, 1, 7, 31, 32, 33, 63, 64, 65, u64::MAX, 0x100000009];
    for operation in ["sllw", "srlw", "sraw"] {
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
                for right in counts {
                    let shift = right & 31;
                    let word = match operation {
                        "sllw" => (u64::from(left as u32) << shift) as u32,
                        "srlw" => (u64::from(left as u32) >> shift) as u32,
                        _ => (i64::from(left as i32) >> shift) as u32,
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
        for value in values {
            let rom = assemble_boot_rom(&format!(
                "la t0, value
                 ld a1, 0(t0)
                 {operation} a1, a1, a1
                 wfi
                 .balign 8
                 value: .dword {}",
                value as i64
            ))
            .unwrap();
            let mut machine = Kiuas16Machine::new_with_ram_size(rom, 4096).unwrap();
            let report = run_headless(&mut machine, 12, &[], &mut Vec::new()).unwrap();
            let shift = value & 31;
            let word = match operation {
                "sllw" => (u64::from(value as u32) << shift) as u32,
                "srlw" => (u64::from(value as u32) >> shift) as u32,
                _ => (i64::from(value as i32) >> shift) as u32,
            };
            assert_eq!(report.traps, 0);
            assert!(report.waiting > 0);
            assert_eq!(
                machine.core().register(11),
                Some(word as i32 as i64 as u64),
                "{operation} a1, a1, a1: {value:#x}"
            );
        }
    }
}

#[test]
fn word_shifts_invalid_operands_preserve_existing_rom() {
    for operation in ["sllw", "srlw", "sraw"] {
        for operands in [
            "",
            "a0, a1",
            "a0, a1, a2, a3",
            "x32, a1, a2",
            "a0, x32, a2",
            "a0, a1, x32",
            "a0, a1, 1",
            "a0, a1, -1",
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
        ("li a0, 18446744073709551616", "out of range"),
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
