use std::collections::BTreeMap;

use super::{AssemblyError, error, parse_number};
use crate::kiuas16::{BOOT_ROM, BOOT_ROM_BYTES};

struct Statement<'a> {
    line: usize,
    offset: usize,
    operation: &'a str,
    operands: Vec<&'a str>,
    size: usize,
}

fn symbol(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '.')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

fn register(text: &str, line: usize) -> Result<u32, AssemblyError> {
    let aliases = [
        "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
        "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
        "t5", "t6",
    ];
    if text == "fp" {
        return Ok(8);
    }
    if let Some(index) = aliases.iter().position(|alias| *alias == text) {
        return Ok(index as u32);
    }
    if let Some(digits) = text.strip_prefix('x') {
        if let Ok(index) = digits.parse::<u32>() {
            if index < 32 && text == format!("x{index}") {
                return Ok(index);
            }
        }
    }
    Err(error(line, format!("invalid RV64 register {text:?}")))
}

fn signed(value: i64, bits: u32, line: usize) -> Result<u32, AssemblyError> {
    if !(-(1_i64 << (bits - 1))..(1_i64 << (bits - 1))).contains(&value) {
        return Err(error(
            line,
            format!("{bits}-bit signed immediate out of range: {value}"),
        ));
    }
    Ok(value as u32 & ((1_u32 << bits) - 1))
}

fn immediate(opcode: u32, rd: u32, rs: u32, function: u32, value: u32) -> u32 {
    value << 20 | rs << 15 | function << 12 | rd << 7 | opcode
}

fn memory(text: &str, line: usize) -> Result<(u32, u32), AssemblyError> {
    let (offset, base) = text
        .split_once('(')
        .and_then(|(offset, rest)| rest.strip_suffix(')').map(|base| (offset, base)))
        .ok_or_else(|| error(line, "expected offset(register)"))?;
    let offset = if offset.trim().is_empty() {
        0
    } else {
        parse_number(offset, line)?
    };
    Ok((register(base.trim(), line)?, signed(offset, 12, line)?))
}

fn encode(
    statement: &Statement<'_>,
    labels: &BTreeMap<&str, usize>,
) -> Result<Vec<u32>, AssemblyError> {
    let Statement {
        line,
        offset,
        operation,
        operands,
        ..
    } = statement;
    let line = *line;
    let op = *operation;
    let args = operands.as_slice();
    let count = |expected| {
        if args.len() == expected {
            Ok(())
        } else {
            Err(error(line, format!("{op} expects {expected} operands")))
        }
    };
    let reg = |i: usize| register(args[i], line);
    let number = |i: usize| parse_number(args[i], line);
    let relative = |text: &str| -> Result<i64, AssemblyError> {
        if let Some(target) = labels.get(text) {
            Ok(*target as i64 - *offset as i64)
        } else if symbol(text) {
            Err(error(line, format!("undefined label {text:?}")))
        } else {
            parse_number(text, line)
        }
    };
    let word = match op {
        op if op.starts_with("amoswap.") || op.starts_with("lr.") => {
            let parts: Vec<_> = op.split('.').collect();
            let width = match parts.get(1).copied() {
                Some("w") => 2,
                Some("d") => 3,
                _ => return Err(error(line, "atomic width must be w or d")),
            };
            let ordering = match &parts[2..] {
                [] => 0,
                ["aq"] => 2,
                ["rl"] => 1,
                ["aqrl"] => 3,
                _ => return Err(error(line, "invalid atomic ordering suffix")),
            };
            let load = parts[0] == "lr";
            count(if load { 2 } else { 3 })?;
            let (base, offset) = memory(args[if load { 1 } else { 2 }], line)?;
            if offset != 0 {
                return Err(error(line, "atomic address requires zero offset"));
            }
            let source = if load { 0 } else { reg(1)? };
            (if load { 2 } else { 1 }) << 27
                | ordering << 25
                | source << 20
                | base << 15
                | width << 12
                | reg(0)? << 7
                | 0x2f
        }
        "nop" | "ret" | "wfi" | "ecall" | "ebreak" | "mret" | "sret" | "fence" | "fence.i" => {
            count(0)?;
            match op {
                "nop" => 0x00000013,
                "ret" => 0x00008067,
                "wfi" => 0x10500073,
                "ecall" => 0x00000073,
                "ebreak" => 0x00100073,
                "mret" => 0x30200073,
                "sret" => 0x10200073,
                "fence" => 0x0ff0000f,
                _ => 0x0000100f,
            }
        }
        "mv" => {
            count(2)?;
            immediate(0x13, reg(0)?, reg(1)?, 0, 0)
        }
        "li" => {
            count(2)?;
            let value = number(1)?;
            if i32::try_from(value).is_err() {
                return Err(error(
                    line,
                    "li currently requires a signed 32-bit constant",
                ));
            }
            let low = (value << 52) >> 52;
            let upper = ((value - low) as u32) & 0xfffff000;
            return Ok(vec![
                upper | reg(0)? << 7 | 0x37,
                immediate(0x1b, reg(0)?, reg(0)?, 0, signed(low, 12, line)?),
            ]);
        }
        "la" => {
            count(2)?;
            let delta = relative(args[1])?;
            if i32::try_from(delta).is_err() {
                return Err(error(
                    line,
                    "la currently requires a signed 32-bit displacement",
                ));
            }
            let low = (delta << 52) >> 52;
            let high = (delta - low) >> 12;
            let upper = signed(high, 20, line)?;
            return Ok(vec![
                upper << 12 | reg(0)? << 7 | 0x17,
                immediate(0x13, reg(0)?, reg(0)?, 0, signed(low, 12, line)?),
            ]);
        }
        "lui" | "auipc" => {
            count(2)?;
            let value = number(1)?;
            if !(-524288..=1048575).contains(&value) {
                return Err(error(line, "upper immediate out of range"));
            }
            (value as u32 & 0xfffff) << 12 | reg(0)? << 7 | if op == "lui" { 0x37 } else { 0x17 }
        }
        "addi" | "addiw" | "slti" | "sltiu" | "xori" | "ori" | "andi" => {
            count(3)?;
            let function = match op {
                "slti" => 2,
                "sltiu" => 3,
                "xori" => 4,
                "ori" => 6,
                "andi" => 7,
                _ => 0,
            };
            immediate(
                if op == "addiw" { 0x1b } else { 0x13 },
                reg(0)?,
                reg(1)?,
                function,
                signed(number(2)?, 12, line)?,
            )
        }
        "slli" | "srli" | "srai" | "slliw" | "srliw" | "sraiw" => {
            count(3)?;
            let word = op.ends_with('w');
            let shift = number(2)?;
            if !(0..if word { 32 } else { 64 }).contains(&shift) {
                return Err(error(line, "shift amount out of range"));
            }
            immediate(
                if word { 0x1b } else { 0x13 },
                reg(0)?,
                reg(1)?,
                if op.starts_with("sll") { 1 } else { 5 },
                shift as u32 | if op.starts_with("sra") { 0x400 } else { 0 },
            )
        }
        "add" | "sub" | "sll" | "slt" | "sltu" | "xor" | "srl" | "sra" | "or" | "and" | "mul"
        | "div" | "divu" | "rem" | "remu" | "addw" | "subw" => {
            count(3)?;
            let (function, upper) = match op {
                "sub" | "subw" => (0, 0x20),
                "sll" => (1, 0),
                "slt" => (2, 0),
                "sltu" => (3, 0),
                "xor" => (4, 0),
                "srl" => (5, 0),
                "sra" => (5, 0x20),
                "or" => (6, 0),
                "and" => (7, 0),
                "mul" => (0, 1),
                "div" => (4, 1),
                "divu" => (5, 1),
                "rem" => (6, 1),
                "remu" => (7, 1),
                _ => (0, 0),
            };
            upper << 25
                | reg(2)? << 20
                | reg(1)? << 15
                | function << 12
                | reg(0)? << 7
                | if op.ends_with('w') { 0x3b } else { 0x33 }
        }
        "lb" | "lh" | "lw" | "ld" | "lbu" | "lhu" | "lwu" => {
            count(2)?;
            let (base, offset) = memory(args[1], line)?;
            let function = match op {
                "lb" => 0,
                "lh" => 1,
                "lw" => 2,
                "ld" => 3,
                "lbu" => 4,
                "lhu" => 5,
                _ => 6,
            };
            immediate(0x03, reg(0)?, base, function, offset)
        }
        "sb" | "sh" | "sw" | "sd" => {
            count(2)?;
            let (base, offset) = memory(args[1], line)?;
            let function = match op {
                "sb" => 0,
                "sh" => 1,
                "sw" => 2,
                _ => 3,
            };
            (offset >> 5) << 25
                | reg(0)? << 20
                | base << 15
                | function << 12
                | (offset & 31) << 7
                | 0x23
        }
        "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" => {
            count(3)?;
            let delta = relative(args[2])?;
            if delta % 4 != 0 {
                return Err(error(line, "branch target must be four-byte aligned"));
            }
            let value = signed(delta, 13, line)?;
            let function = match op {
                "beq" => 0,
                "bne" => 1,
                "blt" => 4,
                "bge" => 5,
                "bltu" => 6,
                _ => 7,
            };
            (value >> 12) << 31
                | ((value >> 5) & 63) << 25
                | reg(1)? << 20
                | reg(0)? << 15
                | function << 12
                | ((value >> 1) & 15) << 8
                | ((value >> 11) & 1) << 7
                | 0x63
        }
        "j" | "call" | "jal" => {
            count(if op == "jal" { 2 } else { 1 })?;
            let delta = relative(args[args.len() - 1])?;
            if delta % 4 != 0 {
                return Err(error(line, "jump target must be four-byte aligned"));
            }
            let value = signed(delta, 21, line)?;
            let rd = match op {
                "j" => 0,
                "call" => 1,
                _ => reg(0)?,
            };
            (value >> 20) << 31
                | ((value >> 1) & 1023) << 21
                | ((value >> 11) & 1) << 20
                | ((value >> 12) & 255) << 12
                | rd << 7
                | 0x6f
        }
        "jalr" => {
            count(2)?;
            let (base, offset) = memory(args[1], line)?;
            immediate(0x67, reg(0)?, base, 0, offset)
        }
        _ => return Err(error(line, format!("unsupported RV64 instruction {op:?}"))),
    };
    Ok(vec![word])
}

pub fn assemble_boot_rom(source: &str) -> Result<Vec<u8>, AssemblyError> {
    let mut labels = BTreeMap::new();
    let mut statements = Vec::new();
    let mut offset = 0_usize;
    for (index, raw) in source.lines().enumerate() {
        let line = index + 1;
        if let Some(rest) = raw.trim_start().strip_prefix('#') {
            let directive = rest.split_whitespace().next().unwrap_or("");
            if matches!(
                directive,
                "include"
                    | "define"
                    | "undef"
                    | "if"
                    | "ifdef"
                    | "ifndef"
                    | "elif"
                    | "else"
                    | "endif"
                    | "error"
                    | "pragma"
                    | "line"
            ) || directive.starts_with(|c: char| c.is_ascii_digit())
            {
                return Err(error(line, "preprocessor directives are not supported"));
            }
        }
        let mut text = raw.split('#').next().unwrap_or("").trim();
        while let Some((name, rest)) = text.split_once(':') {
            let name = name.trim();
            if !symbol(name) {
                return Err(error(line, format!("invalid label {name:?}")));
            }
            if labels.insert(name, offset).is_some() {
                return Err(error(line, format!("duplicate label {name:?}")));
            }
            text = rest.trim();
        }
        if text.is_empty() {
            continue;
        }
        let (operation, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
        let operands: Vec<_> = if rest.trim().is_empty() {
            Vec::new()
        } else {
            rest.split(',').map(str::trim).collect()
        };
        let size = match operation {
            ".text" => {
                if !operands.is_empty() {
                    return Err(error(line, ".text expects no operands"));
                }
                0
            }
            ".globl" | ".global" => {
                if operands.len() != 1 || !symbol(operands[0]) {
                    return Err(error(line, "expected one global symbol"));
                }
                0
            }
            ".byte" | ".word" | ".dword" => {
                if operands.is_empty() {
                    return Err(error(line, "data directive requires at least one value"));
                }
                operands.len()
                    * match operation {
                        ".byte" => 1,
                        ".word" => 4,
                        _ => 8,
                    }
            }
            ".zero" | ".balign" => {
                if operands.len() != 1 {
                    return Err(error(line, "expected one size operand"));
                }
                let value = parse_number(operands[0], line)?;
                let value = usize::try_from(value)
                    .ok()
                    .filter(|value| *value <= BOOT_ROM_BYTES)
                    .ok_or_else(|| error(line, "size outside ROM bounds"))?;
                if operation == ".balign" {
                    if !value.is_power_of_two() {
                        return Err(error(line, "alignment must be a positive power of two"));
                    }
                    (value - offset % value) % value
                } else {
                    value
                }
            }
            "li" | "la" => 8,
            _ => 4,
        };
        if !operation.starts_with('.') && offset % 4 != 0 {
            return Err(error(line, "instruction must be four-byte aligned"));
        }
        if offset
            .checked_add(size)
            .is_none_or(|end| end > BOOT_ROM_BYTES)
        {
            return Err(error(line, "source exceeds boot ROM size"));
        }
        statements.push(Statement {
            line,
            offset,
            operation,
            operands,
            size,
        });
        offset += size;
    }
    for statement in &statements {
        if matches!(statement.operation, ".globl" | ".global")
            && !labels.contains_key(statement.operands[0])
        {
            return Err(error(statement.line, "global symbol is undefined"));
        }
    }
    let mut rom = vec![0; BOOT_ROM_BYTES];
    for statement in statements {
        let Statement {
            line,
            offset,
            operation,
            ref operands,
            size,
        } = statement;
        match operation {
            ".text" | ".globl" | ".global" | ".zero" | ".balign" => {}
            ".byte" | ".word" | ".dword" => {
                let width = match operation {
                    ".byte" => 1,
                    ".word" => 4,
                    _ => 8,
                };
                for (index, operand) in operands.iter().enumerate() {
                    let value = if let Some(target) = labels.get(operand) {
                        BOOT_ROM.start as i64 + *target as i64
                    } else {
                        parse_number(operand, line)?
                    };
                    let bits = width * 8;
                    if width < 8 && !(-(1_i64 << (bits - 1))..(1_i64 << bits)).contains(&value) {
                        return Err(error(line, "data value out of range"));
                    }
                    let start = offset + index * width;
                    rom[start..start + width].copy_from_slice(&value.to_le_bytes()[..width]);
                }
            }
            _ if operation.starts_with('.') => {
                return Err(error(line, format!("unsupported directive {operation:?}")));
            }
            _ => {
                let words = encode(&statement, &labels)?;
                if words.len() * 4 != size {
                    return Err(error(line, "internal instruction size mismatch"));
                }
                for (index, word) in words.iter().enumerate() {
                    let start = offset + index * 4;
                    rom[start..start + 4].copy_from_slice(&word.to_le_bytes());
                }
            }
        }
    }
    Ok(rom)
}
