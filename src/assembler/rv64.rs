use std::collections::BTreeMap;

use super::{AssemblyError, error, parse_number};
use crate::kiuas16::{BOOT_ROM, BOOT_ROM_BYTES};

struct Statement<'a> {
    line: usize,
    position: usize,
    offset: usize,
    operation: &'a str,
    operands: Vec<&'a str>,
    size: usize,
}

#[derive(Default)]
struct Labels<'a> {
    named: BTreeMap<&'a str, usize>,
    numeric: BTreeMap<&'a str, Vec<(usize, usize)>>,
}

fn numeric_label(text: &str) -> Option<&str> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let number = text.trim_start_matches('0');
    Some(if number.is_empty() { "0" } else { number })
}

impl Labels<'_> {
    fn resolve(
        &self,
        text: &str,
        statement: &Statement<'_>,
    ) -> Result<Option<usize>, AssemblyError> {
        if let Some(offset) = self.named.get(text) {
            return Ok(Some(*offset));
        }
        let Some((number, backward)) = text
            .strip_suffix('b')
            .map(|number| (number, true))
            .or_else(|| text.strip_suffix('f').map(|number| (number, false)))
            .and_then(|(number, backward)| numeric_label(number).map(|n| (n, backward)))
        else {
            return Ok(None);
        };
        let target = self.numeric.get(number).and_then(|definitions| {
            if backward {
                definitions
                    .iter()
                    .rev()
                    .find(|(position, _)| *position <= statement.position)
            } else {
                definitions
                    .iter()
                    .find(|(position, _)| *position > statement.position)
            }
        });
        target.map(|(_, offset)| Some(*offset)).ok_or_else(|| {
            error(
                statement.line,
                format!("undefined numeric local label {text:?}"),
            )
        })
    }
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

fn csr(text: &str, line: usize) -> Result<u32, AssemblyError> {
    let value = match text {
        "sstatus" => 0x100,
        "sie" => 0x104,
        "stvec" => 0x105,
        "scounteren" => 0x106,
        "sscratch" => 0x140,
        "sepc" => 0x141,
        "scause" => 0x142,
        "stval" => 0x143,
        "sip" => 0x144,
        "stimecmp" => 0x14d,
        "satp" => 0x180,
        "mstatus" => 0x300,
        "misa" => 0x301,
        "medeleg" => 0x302,
        "mideleg" => 0x303,
        "mie" => 0x304,
        "mtvec" => 0x305,
        "mcounteren" => 0x306,
        "menvcfg" => 0x30a,
        "mscratch" => 0x340,
        "mepc" => 0x341,
        "mcause" => 0x342,
        "mtval" => 0x343,
        "mip" => 0x344,
        "pmpcfg0" => 0x3a0,
        "pmpaddr0" => 0x3b0,
        "mcycle" => 0xb00,
        "minstret" => 0xb02,
        "cycle" => 0xc00,
        "time" => 0xc01,
        "instret" => 0xc02,
        "mhartid" => 0xf14,
        _ => parse_number(text, line)?,
    };
    if !(0..=0xfff).contains(&value) {
        return Err(error(line, "CSR address must be an unsigned 12-bit number"));
    }
    Ok(value as u32)
}

fn immediate(opcode: u32, rd: u32, rs: u32, function: u32, value: u32) -> u32 {
    value << 20 | rs << 15 | function << 12 | rd << 7 | opcode
}

fn load_immediate(operands: &[&str], line: usize) -> Result<Vec<u32>, AssemblyError> {
    let [destination, literal] = operands else {
        return Err(error(line, "li expects 2 operands"));
    };
    let rd = register(destination, line)?;
    let mut value = parse_number(literal, line)?;
    let mut tails = Vec::new();
    while i32::try_from(value).is_err() {
        let low = (value << 52) >> 52;
        tails.push(low);
        value = ((i128::from(value) - i128::from(low)) >> 12) as i64;
    }
    let low = (value << 52) >> 52;
    let upper = ((value - low) as u32) & 0xfffff000;
    let mut words = vec![
        upper | rd << 7 | 0x37,
        immediate(0x1b, rd, rd, 0, signed(low, 12, line)?),
    ];
    for low in tails.into_iter().rev() {
        words.push(immediate(0x13, rd, rd, 1, 12));
        words.push(immediate(0x13, rd, rd, 0, signed(low, 12, line)?));
    }
    Ok(words)
}

fn fence_set(text: &str, line: usize) -> Result<u32, AssemblyError> {
    if text == "0" {
        return Ok(0);
    }
    let mut mask = 0;
    for flag in text.chars() {
        let bit = match flag {
            'i' => 8,
            'o' => 4,
            'r' => 2,
            'w' => 1,
            _ => return Err(error(line, "fence set requires unique i/o/r/w flags or 0")),
        };
        if mask & bit != 0 {
            return Err(error(line, "duplicate fence flag"));
        }
        mask |= bit;
    }
    if mask == 0 {
        return Err(error(line, "empty fence set; use 0 for an empty mask"));
    }
    Ok(mask)
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

fn encode(statement: &Statement<'_>, labels: &Labels<'_>) -> Result<Vec<u32>, AssemblyError> {
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
        if let Some(target) = labels.resolve(text, statement)? {
            Ok(target as i64 - *offset as i64)
        } else if symbol(text) {
            Err(error(line, format!("undefined label {text:?}")))
        } else {
            parse_number(text, line)
        }
    };
    let word = match op {
        "rdcycle" | "rdtime" | "rdinstret" => {
            count(1)?;
            let address = match op {
                "rdcycle" => 0xc00,
                "rdtime" => 0xc01,
                _ => 0xc02,
            };
            immediate(0x73, reg(0)?, 0, 2, address)
        }
        "csrrw" | "csrrs" | "csrrc" | "csrrwi" | "csrrsi" | "csrrci" | "csrr" | "csrw" | "csrs"
        | "csrc" | "csrwi" | "csrsi" | "csrci" => {
            let full = op.starts_with("csrr") && op != "csrr";
            let read = op == "csrr";
            count(if full { 3 } else { 2 })?;
            let rd = if full || read { reg(0)? } else { 0 };
            let address = csr(args[if full || read { 1 } else { 0 }], line)?;
            let immediate_source = op.ends_with('i');
            let source = if read {
                0
            } else {
                let index = if full { 2 } else { 1 };
                if immediate_source {
                    let value = number(index)?;
                    if !(0..=31).contains(&value) {
                        return Err(error(
                            line,
                            "CSR immediate must be an unsigned 5-bit number",
                        ));
                    }
                    value as u32
                } else {
                    reg(index)?
                }
            };
            let function = match op {
                "csrrw" | "csrw" | "csrrwi" | "csrwi" => 1,
                "csrrs" | "csrr" | "csrs" | "csrrsi" | "csrsi" => 2,
                _ => 3,
            } | if immediate_source { 4 } else { 0 };
            immediate(0x73, rd, source, function, address)
        }
        op if [
            "amoswap.", "amoadd.", "amoand.", "amoor.", "amoxor.", "amomin.", "amomax.",
            "amominu.", "amomaxu.", "lr.", "sc.",
        ]
        .iter()
        .any(|prefix| op.starts_with(prefix)) =>
        {
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
            (match parts[0] {
                "lr" => 2,
                "sc" => 3,
                "amoswap" => 1,
                "amoadd" => 0,
                "amoxor" => 4,
                "amoor" => 8,
                "amoand" => 12,
                "amomin" => 16,
                "amomax" => 20,
                "amominu" => 24,
                "amomaxu" => 28,
                _ => return Err(error(line, "unsupported atomic instruction")),
            }) << 27
                | ordering << 25
                | source << 20
                | base << 15
                | width << 12
                | reg(0)? << 7
                | 0x2f
        }
        "sfence.vma" => {
            if args.len() > 2 {
                return Err(error(line, "sfence.vma expects 0, 1, or 2 operands"));
            }
            let address = if args.is_empty() { 0 } else { reg(0)? };
            let asid = if args.len() < 2 { 0 } else { reg(1)? };
            0x12000073 | address << 15 | asid << 20
        }
        "fence" => {
            let (predecessor, successor) = if args.is_empty() {
                (15, 15)
            } else {
                count(2)?;
                (fence_set(args[0], line)?, fence_set(args[1], line)?)
            };
            immediate(0x0f, 0, 0, 0, predecessor << 4 | successor)
        }
        "nop" | "ret" | "wfi" | "ecall" | "ebreak" | "mret" | "sret" | "fence.i" => {
            count(0)?;
            match op {
                "nop" => 0x00000013,
                "ret" => 0x00008067,
                "wfi" => 0x10500073,
                "ecall" => 0x00000073,
                "ebreak" => 0x00100073,
                "mret" => 0x30200073,
                "sret" => 0x10200073,
                _ => 0x0000100f,
            }
        }
        "mv" => {
            count(2)?;
            immediate(0x13, reg(0)?, reg(1)?, 0, 0)
        }
        "not" | "sext.w" | "seqz" => {
            count(2)?;
            let (opcode, function, value) = match op {
                "not" => (0x13, 4, 0xfff),
                "sext.w" => (0x1b, 0, 0),
                _ => (0x13, 3, 1),
            };
            immediate(opcode, reg(0)?, reg(1)?, function, value)
        }
        "neg" | "negw" | "snez" | "sltz" | "sgtz" => {
            count(2)?;
            let source = reg(1)?;
            let (left, right) = if op == "sltz" {
                (source, 0)
            } else {
                (0, source)
            };
            let function = match op {
                "snez" => 3,
                "sltz" | "sgtz" => 2,
                _ => 0,
            };
            let upper = if matches!(op, "neg" | "negw") {
                0x20
            } else {
                0
            };
            upper << 25
                | right << 20
                | left << 15
                | function << 12
                | reg(0)? << 7
                | if op == "negw" { 0x3b } else { 0x33 }
        }
        "li" => {
            return load_immediate(args, line);
        }
        "la" | "tail" => {
            let tail = op == "tail";
            count(if tail { 1 } else { 2 })?;
            let delta = relative(args[if tail { 0 } else { 1 }])?;
            if tail && delta % 4 != 0 {
                return Err(error(line, "tail target must be four-byte aligned"));
            }
            if i32::try_from(delta).is_err() {
                return Err(error(
                    line,
                    format!("{op} currently requires a signed 32-bit displacement"),
                ));
            }
            let low = (delta << 52) >> 52;
            let high = (delta - low) >> 12;
            let upper = signed(high, 20, line)?;
            let base = if tail { 6 } else { reg(0)? };
            return Ok(vec![
                upper << 12 | base << 7 | 0x17,
                immediate(
                    if tail { 0x67 } else { 0x13 },
                    if tail { 0 } else { base },
                    base,
                    0,
                    signed(low, 12, line)?,
                ),
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
        | "div" | "divu" | "rem" | "remu" | "addw" | "subw" | "mulh" | "mulhsu" | "mulhu"
        | "mulw" | "divw" | "divuw" | "remw" | "remuw" | "sllw" | "srlw" | "sraw" => {
            count(3)?;
            let (function, upper) = match op {
                "sub" | "subw" => (0, 0x20),
                "sll" | "sllw" => (1, 0),
                "slt" => (2, 0),
                "sltu" => (3, 0),
                "xor" => (4, 0),
                "srl" | "srlw" => (5, 0),
                "sra" | "sraw" => (5, 0x20),
                "or" => (6, 0),
                "and" => (7, 0),
                "mul" | "mulw" => (0, 1),
                "mulh" => (1, 1),
                "mulhsu" => (2, 1),
                "mulhu" => (3, 1),
                "div" | "divw" => (4, 1),
                "divu" | "divuw" => (5, 1),
                "rem" | "remw" => (6, 1),
                "remu" | "remuw" => (7, 1),
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
        "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" | "beqz" | "bnez" | "blez" | "bgez"
        | "bltz" | "bgtz" | "bgt" | "ble" | "bgtu" | "bleu" => {
            let zero_test = matches!(op, "beqz" | "bnez" | "blez" | "bgez" | "bltz" | "bgtz");
            count(if zero_test { 2 } else { 3 })?;
            let left = reg(0)?;
            let right = if zero_test { 0 } else { reg(1)? };
            let (left, right) = if matches!(op, "blez" | "bgtz" | "bgt" | "ble" | "bgtu" | "bleu") {
                (right, left)
            } else {
                (left, right)
            };
            let delta = relative(args[args.len() - 1])?;
            if delta % 4 != 0 {
                return Err(error(line, "branch target must be four-byte aligned"));
            }
            let value = signed(delta, 13, line)?;
            let function = match op {
                "beq" | "beqz" => 0,
                "bne" | "bnez" => 1,
                "blt" | "bltz" | "bgtz" | "bgt" => 4,
                "bge" | "bgez" | "blez" | "ble" => 5,
                "bltu" | "bgtu" => 6,
                _ => 7,
            };
            (value >> 12) << 31
                | ((value >> 5) & 63) << 25
                | right << 20
                | left << 15
                | function << 12
                | ((value >> 1) & 15) << 8
                | ((value >> 11) & 1) << 7
                | 0x63
        }
        "j" | "call" | "jal" => {
            if op == "jal" {
                if !matches!(args.len(), 1 | 2) {
                    return Err(error(line, "jal expects 1 or 2 operands"));
                }
            } else {
                count(1)?;
            }
            let delta = relative(args[args.len() - 1])?;
            if delta % 4 != 0 {
                return Err(error(line, "jump target must be four-byte aligned"));
            }
            let value = signed(delta, 21, line)?;
            let rd = match op {
                "j" => 0,
                "call" => 1,
                "jal" if args.len() == 1 => 1,
                _ => reg(0)?,
            };
            (value >> 20) << 31
                | ((value >> 1) & 1023) << 21
                | ((value >> 11) & 1) << 20
                | ((value >> 12) & 255) << 12
                | rd << 7
                | 0x6f
        }
        "jr" | "jalr" => {
            let (rd, base, offset) = if op == "jr" {
                count(1)?;
                (0, reg(0)?, 0)
            } else {
                match args.len() {
                    1 => (1, reg(0)?, 0),
                    2 => {
                        let (base, offset) = memory(args[1], line)?;
                        (reg(0)?, base, offset)
                    }
                    3 => (reg(0)?, reg(1)?, signed(number(2)?, 12, line)?),
                    _ => return Err(error(line, "jalr expects 1, 2, or 3 operands")),
                }
            };
            immediate(0x67, rd, base, 0, offset)
        }
        _ => return Err(error(line, format!("unsupported RV64 instruction {op:?}"))),
    };
    Ok(vec![word])
}

pub fn assemble_boot_rom(source: &str) -> Result<Vec<u8>, AssemblyError> {
    let mut labels = Labels::default();
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
            if let Some(number) = numeric_label(name) {
                labels
                    .numeric
                    .entry(number)
                    .or_default()
                    .push((statements.len(), offset));
            } else if !symbol(name) {
                return Err(error(line, format!("invalid label {name:?}")));
            } else if labels.named.insert(name, offset).is_some() {
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
            ".byte" | ".half" | ".short" | ".2byte" | ".word" | ".dword" => {
                if operands.is_empty() {
                    return Err(error(line, "data directive requires at least one value"));
                }
                operands.len()
                    * match operation {
                        ".byte" => 1,
                        ".half" | ".short" | ".2byte" => 2,
                        ".word" => 4,
                        _ => 8,
                    }
            }
            ".zero" | ".balign" | ".p2align" | ".align" => {
                if operands.len() != 1 {
                    return Err(error(line, "expected one size operand"));
                }
                let value = parse_number(operands[0], line)?;
                let value = if matches!(operation, ".p2align" | ".align") {
                    if !(0..=15).contains(&value) {
                        return Err(error(line, "alignment exponent must be between 0 and 15"));
                    }
                    1_i64 << value
                } else {
                    value
                };
                let value = usize::try_from(value)
                    .ok()
                    .filter(|value| *value <= BOOT_ROM_BYTES)
                    .ok_or_else(|| error(line, "size outside ROM bounds"))?;
                if operation != ".zero" {
                    if !value.is_power_of_two() {
                        return Err(error(line, "alignment must be a positive power of two"));
                    }
                    (value - (BOOT_ROM.start as usize + offset) % value) % value
                } else {
                    value
                }
            }
            "li" => load_immediate(&operands, line)?.len() * 4,
            "la" | "tail" => 8,
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
            position: statements.len(),
            offset,
            operation,
            operands,
            size,
        });
        offset += size;
    }
    for statement in &statements {
        if matches!(statement.operation, ".globl" | ".global")
            && !labels.named.contains_key(statement.operands[0])
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
            ..
        } = statement;
        match operation {
            ".text" | ".globl" | ".global" | ".zero" | ".balign" | ".p2align" | ".align" => {}
            ".byte" | ".half" | ".short" | ".2byte" | ".word" | ".dword" => {
                let width = match operation {
                    ".byte" => 1,
                    ".half" | ".short" | ".2byte" => 2,
                    ".word" => 4,
                    _ => 8,
                };
                for (index, operand) in operands.iter().enumerate() {
                    let value = if let Some(target) = labels.resolve(operand, &statement)? {
                        BOOT_ROM.start as i64 + target as i64
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
