use crate::MemorySegment;
use crate::image::ProgramImage;
use crate::isa::{
    ALU_IMMEDIATE_FLAG, ALU_MISC_OPERATION, ALU_OPERATIONS, ALU_PREFIX, BASE_OPCODES,
    CLASSIC_EXTENSIONS, FAR_BRANCH_PREFIX, FAR_CONTROL_OPERATIONS, FAR_CONTROL_PREFIX,
    FLOAT_OPERATIONS, FLOAT_PREFIX, MISC_OPERATIONS, MULDIV_IMMEDIATE_FLAG, MULDIV_OPERATIONS,
    MULDIV_PREFIX, OP_LDI_H, OP_SHR, WIDE_IMMEDIATE_FLAG, WIDE_OPERATIONS, WIDE_PREFIX,
    branch_condition, lookup,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MEMORY_SIZE: u32 = 1 << 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssemblyError(pub String);

impl std::fmt::Display for AssemblyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AssemblyError {}

#[derive(Debug)]
struct Statement {
    line: usize,
    text: String,
    address: u32,
    data: Option<Vec<u8>>,
}

fn error(line: usize, message: impl std::fmt::Display) -> AssemblyError {
    AssemblyError(format!("line {line}: {message}"))
}

fn strip_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' && quote.is_some() {
            escaped = true;
        } else if quote == Some(character) {
            quote = None;
        } else if quote.is_none() && (character == ';' || character == '#') {
            return &line[..index];
        } else if quote.is_none() && (character == '\'' || character == '"') {
            quote = Some(character);
        }
    }
    line
}

fn parse_number(text: &str, line: usize) -> Result<i64, AssemblyError> {
    let value = text.trim();
    let (negative, unsigned) = if let Some(rest) = value.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = value.strip_prefix('+') {
        (false, rest)
    } else {
        (false, value)
    };
    let (base, digits, prefixed) = if let Some(rest) = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))
    {
        (16, rest, true)
    } else if let Some(rest) = unsigned
        .strip_prefix("0o")
        .or_else(|| unsigned.strip_prefix("0O"))
    {
        (8, rest, true)
    } else if let Some(rest) = unsigned
        .strip_prefix("0b")
        .or_else(|| unsigned.strip_prefix("0B"))
    {
        (2, rest, true)
    } else {
        (10, unsigned, false)
    };
    let digits = if prefixed {
        digits.strip_prefix('_').unwrap_or(digits)
    } else {
        digits
    };
    let mut normalized = String::with_capacity(digits.len());
    let mut previous_was_digit = false;
    let mut has_digit = false;
    let mut remaining = digits;
    while let Some(character) = remaining.chars().next() {
        remaining = &remaining[character.len_utf8()..];
        if character == '_' {
            let next_is_digit = remaining
                .chars()
                .next()
                .and_then(|next| next.to_digit(base))
                .is_some();
            if !previous_was_digit || !next_is_digit {
                return Err(error(
                    line,
                    format!("expected a numeric value, got {text:?}"),
                ));
            }
            previous_was_digit = false;
            continue;
        }
        if character.to_digit(base).is_none() {
            return Err(error(
                line,
                format!("expected a numeric value, got {text:?}"),
            ));
        }
        normalized.push(character);
        previous_was_digit = true;
        has_digit = true;
    }
    if !has_digit
        || !previous_was_digit
        || !prefixed
            && base == 10
            && normalized.len() > 1
            && normalized.starts_with('0')
            && normalized.bytes().any(|digit| digit != b'0')
    {
        return Err(error(
            line,
            format!("expected a numeric value, got {text:?}"),
        ));
    }
    let parsed = i64::from_str_radix(&normalized, base)
        .map_err(|_| error(line, format!("expected a numeric value, got {text:?}")))?;
    Ok(if negative { -parsed } else { parsed })
}

fn register(text: &str, line: usize) -> Result<u16, AssemblyError> {
    let text = text.trim();
    let digits = text
        .strip_prefix('R')
        .or_else(|| text.strip_prefix('r'))
        .ok_or_else(|| error(line, format!("invalid register {text:?}; expected R0-R7")))?;
    if digits.len() != 1 {
        return Err(error(
            line,
            format!("invalid register {text:?}; expected R0-R7"),
        ));
    }
    digits
        .parse::<u16>()
        .ok()
        .filter(|value| *value < 8)
        .ok_or_else(|| error(line, format!("invalid register {text:?}; expected R0-R7")))
}

fn split_operands(text: &str) -> Vec<&str> {
    if text.trim().is_empty() {
        Vec::new()
    } else {
        text.split(',').map(str::trim).collect()
    }
}

fn parse_data(text: &str, line: usize) -> Result<Vec<u8>, AssemblyError> {
    let (format, values) = text
        .split_once(char::is_whitespace)
        .ok_or_else(|| error(line, ".data expects a format and values"))?;
    let values = values.trim();
    match format.to_ascii_lowercase().as_str() {
        "hex" | "oct" | "bin" => {
            if values.is_empty() {
                return Err(error(
                    line,
                    format!(".data {format} requires at least one value"),
                ));
            }
            let (base, prefixes): (u32, &[&str]) = match format.to_ascii_lowercase().as_str() {
                "hex" => (16, &["0x", "0X"]),
                "oct" => (8, &["0o", "0O"]),
                _ => (2, &["0b", "0B"]),
            };
            let mut tokens = values
                .split(|character: char| character.is_whitespace() || character == ',')
                .filter(|token| !token.is_empty())
                .peekable();
            if tokens.peek().is_none() {
                return Err(error(
                    line,
                    format!(".data {format} requires at least one value"),
                ));
            }
            tokens
                .map(|token| {
                    let digits = prefixes
                        .iter()
                        .find_map(|prefix| token.strip_prefix(prefix))
                        .unwrap_or(token);
                    let value = u16::from_str_radix(digits, base).map_err(|_| {
                        error(line, format!("invalid {format} data value {token:?}"))
                    })?;
                    u8::try_from(value)
                        .map_err(|_| error(line, ".data values must fit in one byte"))
                })
                .collect()
        }
        "string" => parse_strings(values, line),
        _ => Err(error(line, format!("unsupported .data format {format:?}"))),
    }
}

fn parse_strings(mut text: &str, line: usize) -> Result<Vec<u8>, AssemblyError> {
    let mut result = Vec::new();
    let mut found = false;
    while !text.trim_start().is_empty() {
        text = text.trim_start();
        if let Some(rest) = text.strip_prefix(',') {
            text = rest;
            continue;
        }
        let quote = text
            .chars()
            .next()
            .filter(|character| *character == '\'' || *character == '"')
            .ok_or_else(|| error(line, ".data string expects quoted string values"))?;
        text = &text[quote.len_utf8()..];
        let mut value = String::new();
        let mut closed = false;
        while let Some(character) = text.chars().next() {
            text = &text[character.len_utf8()..];
            if character == quote {
                closed = true;
                break;
            }
            if character == '\\' {
                let (escaped, rest) = parse_escape(text, line)?;
                text = rest;
                if let Some(escaped) = escaped {
                    value.push(escaped);
                }
            } else {
                value.push(character);
            }
        }
        if !closed {
            return Err(error(line, "unterminated string in .data"));
        }
        found = true;
        result.extend_from_slice(value.as_bytes());
        if text
            .chars()
            .next()
            .is_some_and(|character| !character.is_whitespace() && character != ',')
        {
            return Err(error(line, "expected a separator between strings"));
        }
    }
    if found {
        Ok(result)
    } else {
        Err(error(line, ".data string requires at least one string"))
    }
}

fn parse_escape(text: &str, line: usize) -> Result<(Option<char>, &str), AssemblyError> {
    let Some(escape) = text.chars().next() else {
        return Err(error(line, "unterminated string in .data"));
    };
    let remainder = &text[escape.len_utf8()..];
    let simple = match escape {
        'a' => Some('\u{7}'),
        'b' => Some('\u{8}'),
        'f' => Some('\u{c}'),
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        'v' => Some('\u{b}'),
        '\\' => Some('\\'),
        '\'' => Some('\''),
        '"' => Some('"'),
        '\n' => return Ok((None, remainder)),
        _ => None,
    };
    if let Some(character) = simple {
        return Ok((Some(character), remainder));
    }
    if escape == 'x' || escape == 'u' || escape == 'U' {
        let digits_count = match escape {
            'x' => 2,
            'u' => 4,
            _ => 8,
        };
        if remainder.len() < digits_count || !remainder.is_char_boundary(digits_count) {
            return Err(error(
                line,
                format!("invalid \\{escape} escape in .data string"),
            ));
        }
        let digits = &remainder[..digits_count];
        let value = u32::from_str_radix(digits, 16)
            .map_err(|_| error(line, format!("invalid \\{escape} escape in .data string")))?;
        let character = char::from_u32(value)
            .ok_or_else(|| error(line, "invalid Unicode code point in .data string"))?;
        return Ok((Some(character), &remainder[digits_count..]));
    }
    if ('0'..='7').contains(&escape) {
        let mut digits = String::from(escape);
        let mut rest = remainder;
        while digits.len() < 3 {
            let Some(next) = rest.chars().next() else {
                break;
            };
            if !('0'..='7').contains(&next) {
                break;
            }
            digits.push(next);
            rest = &rest[next.len_utf8()..];
        }
        let value = u32::from_str_radix(&digits, 8)
            .map_err(|_| error(line, "invalid octal escape in .data string"))?;
        return Ok((
            Some(
                char::from_u32(value)
                    .ok_or_else(|| error(line, "invalid Unicode code point in .data string"))?,
            ),
            rest,
        ));
    }
    Ok((Some('\\'), text))
}

fn label_delimiter(text: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' && quote.is_some() {
            escaped = true;
        } else if quote == Some(character) {
            quote = None;
        } else if quote.is_none() && (character == '\'' || character == '"') {
            quote = Some(character);
        } else if quote.is_none() && character == ':' {
            return Some(index);
        }
    }
    None
}

fn expand_includes(
    source: &str,
    base_directory: &Path,
    include_stack: &mut Vec<PathBuf>,
) -> Result<String, AssemblyError> {
    let mut expanded = Vec::new();
    for (index, source_line) in source.lines().enumerate() {
        let line = index + 1;
        let directive = strip_comment(source_line).trim();
        if !directive.to_ascii_lowercase().starts_with(".include") {
            expanded.push(source_line.to_owned());
            continue;
        }
        let Some((name, path_text)) = directive.split_once(char::is_whitespace) else {
            return Err(error(line, ".include expects one quoted file path"));
        };
        if !name.eq_ignore_ascii_case(".include") {
            return Err(error(line, ".include expects one quoted file path"));
        }
        let path_text = path_text.trim();
        let Some(quote @ ('\'' | '"')) = path_text.chars().next() else {
            return Err(error(line, ".include expects one quoted file path"));
        };
        if path_text.len() < 2 || !path_text.ends_with(quote) {
            return Err(error(line, ".include expects one quoted file path"));
        }
        let include_name = &path_text[quote.len_utf8()..path_text.len() - quote.len_utf8()];
        if include_name.is_empty() {
            return Err(error(
                line,
                ".include expects one non-empty quoted file path",
            ));
        }
        let mut include_path = base_directory.join(include_name);
        if !include_path.is_file() {
            let file_name = Path::new(include_name).file_name();
            let mut ancestor = Some(base_directory);
            for _ in 0..5 {
                let Some(current) = ancestor else { break };
                let found = ["lib", "fonts"]
                    .iter()
                    .filter_map(|folder| file_name.map(|name| current.join(folder).join(name)))
                    .find(|candidate| candidate.is_file());
                if let Some(found) = found {
                    include_path = found;
                    break;
                }
                ancestor = current.parent();
            }
        }
        let resolved = include_path.canonicalize().map_err(|io_error| {
            error(
                line,
                format!("cannot include {}: {io_error}", include_path.display()),
            )
        })?;
        if include_stack.contains(&resolved) {
            let mut cycle: Vec<String> = include_stack
                .iter()
                .map(|item| item.display().to_string())
                .collect();
            cycle.push(resolved.display().to_string());
            return Err(error(
                line,
                format!("circular .include detected: {}", cycle.join(" -> ")),
            ));
        }
        let included = fs::read_to_string(&resolved).map_err(|io_error| {
            error(
                line,
                format!("cannot include {}: {io_error}", resolved.display()),
            )
        })?;
        let included = included.trim_start_matches('\u{feff}');
        include_stack.push(resolved.clone());
        let result = expand_includes(
            included,
            resolved.parent().unwrap_or(base_directory),
            include_stack,
        );
        include_stack.pop();
        expanded.push(result?);
    }
    Ok(expanded.join("\n"))
}

fn is_register(text: &str) -> bool {
    let text = text.trim();
    text.len() == 2
        && matches!(text.as_bytes()[0], b'R' | b'r')
        && matches!(text.as_bytes()[1], b'0'..=b'7')
}

fn extended_instruction(
    mnemonic: &str,
    operands: &[&str],
    line: usize,
    labels: &BTreeMap<String, u32>,
) -> Result<Option<Vec<u16>>, AssemblyError> {
    let opcode = lookup(FAR_CONTROL_OPERATIONS, mnemonic);
    let arguments = 1;
    if let Some(operation) = opcode {
        if operands.len() != arguments {
            return Err(error(
                line,
                format!("{mnemonic} expects one address or label"),
            ));
        }
        let target = labels.get(operands[0]).map_or_else(
            || parse_number(operands[0], line),
            |value| Ok(i64::from(*value)),
        )?;
        if !(0..i64::from(MEMORY_SIZE)).contains(&target) {
            return Err(error(
                line,
                format!("{mnemonic} target is outside available memory"),
            ));
        }
        return Ok(Some(vec![
            FAR_CONTROL_PREFIX | (operation << 9),
            ((target as u32 >> 16) & 0x3fff) as u16,
            target as u16,
        ]));
    }

    let branch = branch_condition(mnemonic);
    if let Some(condition) = branch {
        if operands.len() != 1 {
            return Err(error(
                line,
                format!("{mnemonic} expects one address or label"),
            ));
        }
        let target = labels.get(operands[0]).map_or_else(
            || parse_number(operands[0], line),
            |value| Ok(i64::from(*value)),
        )?;
        if !(0..i64::from(MEMORY_SIZE)).contains(&target) {
            return Err(error(
                line,
                format!("{mnemonic} target is outside available memory"),
            ));
        }
        return Ok(Some(vec![
            FAR_BRANCH_PREFIX | (condition << 7),
            ((target as u32 >> 16) & 0x3fff) as u16,
            target as u16,
        ]));
    }

    let wide = lookup(WIDE_OPERATIONS, mnemonic);
    if let Some(operation) = wide {
        let unary = mnemonic == "NEG32";
        if operands.len() != if unary { 1 } else { 2 } {
            return Err(error(
                line,
                format!(
                    "{mnemonic} expects {} operand(s)",
                    if unary { 1 } else { 2 }
                ),
            ));
        }
        let destination = register(operands[0], line)?;
        if destination & 1 != 0 {
            return Err(error(
                line,
                format!("{mnemonic} destination must be an even register"),
            ));
        }
        let mut control = operation << 12;
        let source = if unary {
            0
        } else if matches!(mnemonic, "LD32" | "ST32") {
            let address = register(operands[1], line)?;
            if address & 1 != 0 {
                return Err(error(
                    line,
                    format!("{mnemonic} address must be an even register"),
                ));
            }
            address
        } else if matches!(mnemonic, "SHL32" | "LSR32" | "ASR32") && !is_register(operands[1]) {
            let shift = parse_number(operands[1], line)?;
            if !(0..=31).contains(&shift) {
                return Err(error(
                    line,
                    format!("{mnemonic} shift count must be between 0 and 31"),
                ));
            }
            control |= WIDE_IMMEDIATE_FLAG;
            shift as u16
        } else {
            let source = register(operands[1], line)?;
            if !matches!(mnemonic, "SHL32" | "LSR32" | "ASR32") && source & 1 != 0 {
                return Err(error(
                    line,
                    format!("{mnemonic} source must be an even register"),
                ));
            }
            if matches!(mnemonic, "DIV32S" | "MOD32S") {
                control |= WIDE_IMMEDIATE_FLAG;
            }
            source
        };
        return Ok(Some(vec![
            0xf800 | (destination << 8) | WIDE_PREFIX,
            control | source,
        ]));
    }

    let muldiv = lookup(MULDIV_OPERATIONS, mnemonic).or_else(|| (mnemonic == "LDA").then_some(7));
    if let Some(operation) = muldiv {
        if operands.len() != 2 {
            return Err(error(
                line,
                format!("{mnemonic} expects a register and a register or immediate"),
            ));
        }
        let destination = register(operands[0], line)?;
        let first = 0xf800 | (destination << 8) | MULDIV_PREFIX;
        let control = operation << 12;
        if operation == 7 {
            let target = labels.get(operands[1]).map_or_else(
                || parse_number(operands[1], line),
                |value| Ok(i64::from(*value)),
            )?;
            if !(0..i64::from(MEMORY_SIZE)).contains(&target) {
                return Err(error(
                    line,
                    format!("{mnemonic} address is outside available memory"),
                ));
            }
            return Ok(Some(vec![
                first,
                control,
                ((target as u32 >> 16) & 0x3fff) as u16,
                target as u16,
            ]));
        }
        if matches!(mnemonic, "LDP" | "STP" | "SPLIT30" | "JOIN30") {
            if !is_register(operands[1]) {
                return Err(error(line, format!("{mnemonic} expects two registers")));
            }
            return Ok(Some(vec![first, control | register(operands[1], line)?]));
        }
        if is_register(operands[1]) {
            return Ok(Some(vec![first, control | register(operands[1], line)?]));
        }
        let immediate = parse_number(operands[1], line)?;
        if !(-0x8000..=0xffff).contains(&immediate) {
            return Err(error(
                line,
                format!("{mnemonic} immediate must fit in 16 bits"),
            ));
        }
        return Ok(Some(vec![
            first,
            control | MULDIV_IMMEDIATE_FLAG,
            immediate as u16,
        ]));
    }

    let float = lookup(FLOAT_OPERATIONS, mnemonic);
    if let Some(action) = float {
        let unary = (20..=23).contains(&action);
        if operands.len() != if unary { 1 } else { 2 } {
            return Err(error(
                line,
                format!(
                    "{mnemonic} expects {} operand(s)",
                    if unary { 1 } else { 2 }
                ),
            ));
        }
        let destination = register(operands[0], line)?;
        let source = if unary {
            destination
        } else {
            register(operands[1], line)?
        };
        let pair = |value: u16, binary64: bool| -> Result<(), AssemblyError> {
            if value & 1 != 0 || binary64 && !matches!(value, 0 | 4) {
                return Err(error(
                    line,
                    format!(
                        "{mnemonic} register must be {}",
                        if binary64 {
                            "R0 or R4"
                        } else {
                            "an even register"
                        }
                    ),
                ));
            }
            Ok(())
        };
        if action <= 9 || unary {
            let binary64 = (5..=9).contains(&action) || matches!(action, 21 | 23);
            pair(destination, binary64)?;
            pair(source, binary64)?;
        } else if (10..=17).contains(&action) {
            let binary64 = action >= 14;
            pair(destination, binary64)?;
            if matches!(action, 12 | 13 | 16 | 17) {
                pair(source, false)?;
            }
        } else if action == 18 {
            pair(destination, true)?;
            pair(source, false)?;
        } else if action == 19 {
            pair(destination, false)?;
            pair(source, true)?;
        } else {
            let binary64 = action >= 28;
            pair(source, binary64)?;
            if matches!(action, 26 | 27 | 30 | 31) {
                pair(destination, false)?;
            }
        }
        return Ok(Some(vec![
            0xf800 | (destination << 8) | FLOAT_PREFIX,
            (action << 11) | source,
        ]));
    }

    let classic_extension = CLASSIC_EXTENSIONS
        .iter()
        .find_map(|(name, value)| (*name == mnemonic).then_some(*value));
    if let Some((operation, binary)) = classic_extension {
        if binary && operands.len() != 2 || !binary && operands.len() != 1 {
            return Err(error(
                line,
                format!(
                    "{mnemonic} expects {} operand(s)",
                    if binary { 2 } else { 1 }
                ),
            ));
        }
        let destination = register(operands[0], line)?;
        let word = if mnemonic == "MOVSP" {
            (OP_SHR << 11) | (destination << 8) | (7 << 2)
        } else if matches!(mnemonic, "ITOF" | "FTOI") {
            (OP_SHR << 11) | (operation << 5) | (destination << 8)
        } else {
            (OP_SHR << 11)
                | (destination << 8)
                | (operation << 5)
                | (register(operands[1], line)? << 2)
        };
        return Ok(Some(vec![word]));
    }

    let (alu_name, operands) = if mnemonic == "SHR" && operands.len() == 2 {
        ("LSR", operands)
    } else {
        (mnemonic, operands)
    };
    let alu_operation = lookup(ALU_OPERATIONS, alu_name);
    if let Some(operation) = alu_operation
        && (matches!(
            alu_name,
            "SHL" | "LSR" | "ASR" | "ROL" | "ROR" | "ADC" | "SBC" | "NEG" | "TEST"
        ) || operands.len() == 2 && !is_register(operands[1]))
    {
        if operands.len() != if alu_name == "NEG" { 1 } else { 2 } {
            return Err(error(
                line,
                format!(
                    "{alu_name} expects {} operand(s)",
                    if alu_name == "NEG" { 1 } else { 2 }
                ),
            ));
        }
        let destination = register(operands[0], line)?;
        let mut control = operation << 12;
        let mut words = vec![0xf800 | (destination << 8) | ALU_PREFIX];
        if alu_name == "NEG" {
            words.push(control);
        } else if matches!(
            alu_name,
            "SWAP"
                | "SEXT"
                | "JMPR"
                | "CALLR"
                | "PUSHF"
                | "POPF"
                | "SETC"
                | "CLC"
                | "SETSP"
                | "CLZ"
                | "MOVSPH"
                | "CALLRP"
                | "JMPRP"
                | "SETSPH"
        ) {
            return Err(error(line, "invalid ALU instruction"));
        } else if is_register(operands[1]) {
            words.push(control | register(operands[1], line)?);
        } else {
            let immediate = parse_number(operands[1], line)?;
            if matches!(alu_name, "SHL" | "LSR" | "ASR" | "ROL" | "ROR") {
                if !(0..=15).contains(&immediate) {
                    return Err(error(
                        line,
                        format!("{alu_name} shift count must be between 0 and 15"),
                    ));
                }
                words.push(control | ALU_IMMEDIATE_FLAG | immediate as u16);
            } else {
                if !(-0x8000..=0xffff).contains(&immediate) {
                    return Err(error(
                        line,
                        format!("{alu_name} immediate must fit in 16 bits"),
                    ));
                }
                control |= ALU_IMMEDIATE_FLAG;
                words.push(control);
                words.push(immediate as u16);
            }
        }
        return Ok(Some(words));
    }

    let misc = lookup(MISC_OPERATIONS, mnemonic);
    if let Some(operation) = misc {
        let pair_call = matches!(mnemonic, "CALLRP" | "JMPRP");
        let expected = if pair_call {
            2
        } else if matches!(mnemonic, "PUSHF" | "POPF" | "SETC" | "CLC") {
            0
        } else {
            1
        };
        if operands.len() != expected {
            return Err(error(
                line,
                format!("{mnemonic} expects {expected} operand(s)"),
            ));
        }
        let destination = if expected == 0 {
            0
        } else {
            register(operands[0], line)?
        };
        let mut control = (ALU_MISC_OPERATION << 12) | operation;
        if pair_call {
            control |= register(operands[1], line)? << 9;
        }
        return Ok(Some(vec![
            0xf800 | (destination << 8) | ALU_PREFIX,
            control,
        ]));
    }
    Ok(None)
}

fn instruction(
    text: &str,
    line: usize,
    labels: &BTreeMap<String, u32>,
) -> Result<Vec<u16>, AssemblyError> {
    let (mnemonic, operands) = text
        .split_once(char::is_whitespace)
        .map_or((text, ""), |(name, args)| (name, args));
    let mnemonic = mnemonic.to_ascii_uppercase();
    let operands = split_operands(operands);
    if let Some(words) = extended_instruction(&mnemonic, &operands, line, labels)? {
        return Ok(words);
    }
    let opcode = lookup(BASE_OPCODES, &mnemonic)
        .filter(|opcode| *opcode != OP_SHR || mnemonic == "SHR")
        .ok_or_else(|| error(line, format!("unknown instruction {mnemonic:?}")))?;
    let bad_count = |expected: &str| error(line, format!("{mnemonic} expects {expected}"));
    let imm = |text: &str, maximum: i64| -> Result<u16, AssemblyError> {
        let value = parse_number(text, line)?;
        if !(0..=maximum).contains(&value) {
            return Err(error(
                line,
                format!("{mnemonic} operand must be between 0 and {maximum:#x}"),
            ));
        }
        Ok(value as u16)
    };
    let target = |text: &str| -> Result<i64, AssemblyError> {
        if let Some(address) = labels.get(text) {
            Ok(i64::from(*address))
        } else {
            parse_number(text, line)
        }
    };
    let one = || {
        if operands.len() == 1 {
            Ok(())
        } else {
            Err(bad_count("one operand"))
        }
    };
    let two = || {
        if operands.len() == 2 {
            Ok(())
        } else {
            Err(bad_count("two operands"))
        }
    };
    match mnemonic.as_str() {
        "HALT" | "EI" | "DI" | "RTI" | "RET" | "NOP" => {
            if !operands.is_empty() {
                return Err(bad_count("no operands"));
            }
            Ok(vec![opcode << 11])
        }
        "LDI" => {
            two()?;
            let rd = register(operands[0], line)?;
            let value = imm(operands[1], 0xffff)?;
            let mut words = vec![(opcode << 11) | (rd << 8) | (value & 0xff)];
            if value > 0xff {
                words.push((OP_LDI_H << 11) | (rd << 8) | (value >> 8));
            }
            Ok(words)
        }
        "LDI_H" => {
            two()?;
            Ok(vec![
                (opcode << 11) | (register(operands[0], line)? << 8) | imm(operands[1], 0xff)?,
            ])
        }
        "LD" | "LDW" => {
            two()?;
            Ok(vec![
                (opcode << 11)
                    | (register(operands[0], line)? << 8)
                    | (register(operands[1], line)? << 5),
            ])
        }
        "ST" | "STW" => {
            two()?;
            let address = register(operands[0], line)?;
            let value = register(operands[1], line)?;
            Ok(vec![(opcode << 11) | (value << 8) | (address << 5)])
        }
        "LDWS" | "STWS" => {
            two()?;
            Ok(vec![
                (opcode << 11) | (register(operands[0], line)? << 8) | imm(operands[1], 0xff)?,
            ])
        }
        "ADJSP" => {
            one()?;
            let value = parse_number(operands[0], line)?;
            if !(-128..=127).contains(&value) {
                return Err(error(line, "ADJSP offset must be between -128 and 127"));
            }
            Ok(vec![(opcode << 11) | (value as u16 & 0xff)])
        }
        "MOV" => {
            two()?;
            Ok(vec![
                (opcode << 11)
                    | (register(operands[0], line)? << 8)
                    | (register(operands[1], line)? << 5),
            ])
        }
        "ADD" | "SUB" | "AND" | "OR" | "XOR" | "CMP" => {
            two()?;
            let rd = register(operands[0], line)?;
            Ok(vec![
                (opcode << 11) | (rd << 8) | (rd << 5) | (register(operands[1], line)? << 2),
            ])
        }
        "NOT" => {
            if !(1..=2).contains(&operands.len()) {
                return Err(bad_count("a destination and optional source"));
            }
            let rd = register(operands[0], line)?;
            let rs = if operands.len() == 2 {
                register(operands[1], line)?
            } else {
                rd
            };
            Ok(vec![(opcode << 11) | (rd << 8) | (rs << 5)])
        }
        "INC" | "DEC" | "PUSH" | "POP" | "SHR" => {
            one()?;
            Ok(vec![(opcode << 11) | (register(operands[0], line)? << 8)])
        }
        "JMP" | "CALL" | "BZ" | "BC" => {
            one()?;
            let address = target(operands[0])?;
            if !(0..=0x7ff).contains(&address) {
                return Err(error(
                    line,
                    format!("{mnemonic} target must be between 0 and 0x7ff"),
                ));
            }
            Ok(vec![(opcode << 11) | address as u16])
        }
        "JZ" => {
            two()?;
            let rd = register(operands[0], line)?;
            let address = target(operands[1])?;
            if !(0..=0xff).contains(&address) {
                return Err(error(line, "JZ target must be between 0 and 0xff"));
            }
            Ok(vec![(opcode << 11) | (rd << 8) | address as u16])
        }
        _ => Err(error(line, format!("unsupported instruction {mnemonic:?}"))),
    }
}

fn instruction_size(text: &str, line: usize) -> Result<u32, AssemblyError> {
    let (mnemonic, arguments) = text
        .split_once(char::is_whitespace)
        .map_or((text, ""), |(name, args)| (name, args));
    let mnemonic = mnemonic.to_ascii_uppercase();
    let operands = split_operands(arguments);
    let size = match mnemonic.as_str() {
        "JMPX" | "CALLX" | "BZX" | "BCX" | "BNZX" | "BNCX" | "BNX" | "BNNX" | "BVX" | "BNVX"
        | "BLTX" | "BGEX" | "BGTX" | "BLEX" | "BHIX" | "BLSX" | "BNZ" | "BNC" | "BN" | "BNN"
        | "BV" | "BNV" | "BLT" | "BGE" | "BGT" | "BLE" | "BHI" | "BLS" => 6,
        "LDI19" | "LDI30" | "LDA" => 8,
        "F32ADD" | "F32SUB" | "F32MUL" | "F32DIV" | "F32CMP" | "F64ADD" | "F64SUB" | "F64MUL"
        | "F64DIV" | "F64CMP" | "I16TOF32" | "U16TOF32" | "I32TOF32" | "U32TOF32" | "I16TOF64"
        | "U16TOF64" | "I32TOF64" | "U32TOF64" | "F32TOF64" | "F64TOF32" | "F32NEG" | "F64NEG"
        | "F32SQRT" | "F64SQRT" | "F32TOI16" | "F32TOU16" | "F32TOI32" | "F32TOU32"
        | "F64TOI16" | "F64TOU16" | "F64TOI32" | "F64TOU32" | "ADD32" | "SUB32" | "CMP32"
        | "AND32" | "OR32" | "XOR32" | "SHL32" | "LSR32" | "ASR32" | "NEG32" | "MOV32" | "LD32"
        | "ST32" | "MUL32" | "DIV32" | "DIV32S" | "MOD32" | "MOD32S" => 4,
        "MUL" | "MULHU" | "MULHS" | "DIV" | "DIVS" | "MOD" | "MODS" => {
            if operands.get(1).is_some_and(|operand| !is_register(operand)) {
                6
            } else {
                4
            }
        }
        "LDP" | "STP" | "SPLIT30" | "JOIN30" => 4,
        "SHR" if operands.len() == 2 => 4,
        "SHL" | "LSR" | "ASR" | "ROL" | "ROR" | "ADC" | "SBC" | "NEG" | "TEST" => {
            if matches!(mnemonic.as_str(), "ADC" | "SBC" | "TEST")
                && operands.get(1).is_some_and(|operand| !is_register(operand))
            {
                6
            } else {
                4
            }
        }
        "SWAP" | "SEXT" | "JMPR" | "CALLR" | "PUSHF" | "POPF" | "SETC" | "CLC" | "SETSP"
        | "CLZ" | "MOVSPH" | "CALLRP" | "JMPRP" | "SETSPH" => 4,
        "ADD" | "SUB" | "AND" | "OR" | "XOR" | "CMP" => {
            if operands.len() == 2 && !is_register(operands[1]) {
                6
            } else {
                2
            }
        }
        "LDI" => {
            if operands.len() != 2 {
                return Err(error(line, "LDI expects a register and an immediate"));
            }
            let value = parse_number(operands[1], line)?;
            if !(0..=0xffff).contains(&value) {
                return Err(error(line, "LDI immediate must be between 0 and 65535"));
            }
            if value > 0xff { 4 } else { 2 }
        }
        _ => 2,
    };
    Ok(size)
}

fn assemble_expanded(source: &str, origin: u32) -> Result<ProgramImage, AssemblyError> {
    if origin >= MEMORY_SIZE {
        return Err(AssemblyError(format!(
            "invalid assembly origin {origin:#x}"
        )));
    }
    let mut parsed = Vec::new();
    for (index, source_line) in source.lines().enumerate() {
        let mut text = strip_comment(source_line).trim();
        if text.is_empty() {
            continue;
        }
        if let Some(delimiter) = label_delimiter(text) {
            let (label, rest) = text.split_at(delimiter);
            if label.is_empty()
                || !label.chars().enumerate().all(|(position, character)| {
                    character == '_'
                        || character.is_ascii_alphanumeric()
                            && (position > 0 || !character.is_ascii_digit())
                })
            {
                return Err(error(index + 1, "invalid label"));
            }
            parsed.push((index + 1, format!("{label}:")));
            text = rest[1..].trim();
            if text.is_empty() {
                continue;
            }
        }
        parsed.push((index + 1, text.to_owned()));
    }

    let mut labels = BTreeMap::new();
    let mut statements = Vec::new();
    let mut address = origin;
    let mut entry_instruction = None;
    let mut first_emitted = None;
    for (line, text) in parsed {
        if let Some(label) = text.strip_suffix(':') {
            if labels.insert(label.to_owned(), address).is_some() {
                return Err(error(line, format!("duplicate label {label:?}")));
            }
            continue;
        }
        if text.to_ascii_lowercase().starts_with(".address") {
            let (directive, value) = text.split_once(char::is_whitespace).unwrap_or((&text, ""));
            if !directive.eq_ignore_ascii_case(".address") || value.trim().is_empty() {
                return Err(error(line, ".address expects one numeric address"));
            }
            let target = parse_number(value, line)?;
            if !(0..i64::from(MEMORY_SIZE)).contains(&target) {
                return Err(error(line, ".address is outside available memory"));
            }
            address = target as u32;
            statements.push(Statement {
                line,
                text,
                address,
                data: None,
            });
            continue;
        }
        if text.to_ascii_lowercase().starts_with(".data") {
            let (directive, values) = text.split_once(char::is_whitespace).unwrap_or((&text, ""));
            if !directive.eq_ignore_ascii_case(".data") {
                return Err(error(line, ".data expects a format and values"));
            }
            let bytes = parse_data(values.trim(), line)?;
            statements.push(Statement {
                line,
                text,
                address,
                data: Some(bytes.clone()),
            });
            if !bytes.is_empty() && first_emitted.is_none() {
                first_emitted = Some(address);
            }
            address = address
                .checked_add(bytes.len() as u32)
                .ok_or_else(|| error(line, "assembled program exceeds available memory"))?;
        } else {
            let size = instruction_size(&text, line)?;
            statements.push(Statement {
                line,
                text,
                address,
                data: None,
            });
            entry_instruction.get_or_insert(address);
            first_emitted.get_or_insert(address);
            address = address
                .checked_add(size)
                .ok_or_else(|| error(line, "assembled program exceeds available memory"))?;
        }
        if address > MEMORY_SIZE {
            return Err(error(line, "assembled program exceeds available memory"));
        }
    }

    let mut bytes = BTreeMap::new();
    for statement in statements {
        if statement.text.to_ascii_lowercase().starts_with(".address") {
            continue;
        }
        let emitted = if let Some(data) = statement.data {
            data
        } else {
            instruction(&statement.text, statement.line, &labels)?
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect()
        };
        for (offset, byte) in emitted.into_iter().enumerate() {
            let location = statement.address + offset as u32;
            if bytes.insert(location, byte).is_some() {
                return Err(error(
                    statement.line,
                    format!("output overlaps previously emitted data at {location:#x}"),
                ));
            }
        }
    }
    let mut segments: Vec<MemorySegment> = Vec::new();
    for (address, byte) in bytes {
        if let Some(segment) = segments.last_mut()
            && segment.address + segment.data.len() as u32 == address
        {
            segment.data.push(byte);
            continue;
        }
        segments.push(MemorySegment {
            address,
            data: vec![byte],
        });
    }
    Ok(ProgramImage {
        entry_point: entry_instruction.or(first_emitted).unwrap_or(origin),
        segments,
    })
}

pub fn assemble_with_base_directory(
    source: &str,
    origin: u32,
    base_directory: &Path,
) -> Result<ProgramImage, AssemblyError> {
    if origin >= MEMORY_SIZE {
        return Err(AssemblyError(format!(
            "invalid assembly origin {origin:#x}"
        )));
    }
    let mut include_stack = Vec::new();
    let expanded = expand_includes(source, base_directory, &mut include_stack)?;
    assemble_expanded(&expanded, origin)
}

pub fn assemble(source: &str, origin: u32) -> Result<ProgramImage, AssemblyError> {
    let directory = std::env::current_dir().map_err(|io_error| {
        AssemblyError(format!("cannot resolve current directory: {io_error}"))
    })?;
    assemble_with_base_directory(source, origin, &directory)
}

pub fn assemble_file(path: &Path, origin: u32) -> Result<ProgramImage, AssemblyError> {
    let canonical_path = path
        .canonicalize()
        .map_err(|io_error| AssemblyError(format!("cannot read {}: {io_error}", path.display())))?;
    let source = fs::read_to_string(&canonical_path).map_err(|io_error| {
        AssemblyError(format!(
            "cannot read {}: {io_error}",
            canonical_path.display()
        ))
    })?;
    let source = source.trim_start_matches('\u{feff}');
    let mut include_stack = vec![canonical_path.clone()];
    let expanded = expand_includes(
        source,
        canonical_path.parent().unwrap_or(Path::new(".")),
        &mut include_stack,
    )
    .map_err(|assembly_error| {
        AssemblyError(format!("{}: {assembly_error}", canonical_path.display()))
    })?;
    assemble_expanded(&expanded, origin).map_err(|assembly_error| {
        AssemblyError(format!("{}: {assembly_error}", canonical_path.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_forward_labels_and_wide_load_immediates() {
        let image = assemble("JMP finish\nLDI R1, 315\nfinish: HALT\n", 0).unwrap();
        assert_eq!(image.entry_point, 0);
        assert_eq!(image.segments[0].data, b"\x58\x06\x09\x3b\x81\x01\x00\x00");
    }

    #[test]
    fn sizes_two_operand_shr_as_an_extended_shift_before_resolving_labels() {
        let image = assemble("SHR R0, 3\nnext: JMP next\n", 0).unwrap();
        assert_eq!(image.segments[0].data, b"\xf8\x1f\x18\x03\x58\x04");
    }

    #[test]
    fn parses_python_style_numeric_separators_and_rejects_ambiguous_octal() {
        assert_eq!(
            assemble("LDI R0, 1_000\n", 0).unwrap().segments[0].data,
            b"\x08\xe8\x80\x03"
        );
        assert_eq!(
            assemble("LDI R0, 0x_FF\n", 0).unwrap().segments[0].data,
            b"\x08\xff"
        );
        assert!(assemble("LDI R0, 01", 0).is_err());
        assert!(assemble("LDI R0, 1__0", 0).is_err());
    }

    #[test]
    fn builds_sorted_segments_and_keeps_first_instruction_entry() {
        let image = assemble(
            ".address 0x100\nstart: JMP end\n.address 0x20\nend: HALT\n",
            0,
        )
        .unwrap();
        assert_eq!(image.entry_point, 0x100);
        assert_eq!(
            image
                .segments
                .iter()
                .map(|segment| (segment.address, segment.data.as_slice()))
                .collect::<Vec<_>>(),
            [(0x20, &[0x00, 0x00][..]), (0x100, &[0x58, 0x20][..])]
        );
    }

    #[test]
    fn emits_data_formats_and_utf8_strings() {
        let image = assemble(
            ".data hex 0x41, 42\n.data oct 101 102\n.data bin 01000011, 01000100\n.data string \"Hi\\n\"\n",
            0,
        )
        .unwrap();
        assert_eq!(image.segments[0].data, b"ABABCDHi\n");
        assert_eq!(
            assemble(".data string \"ä\"", 0).unwrap().segments[0].data,
            "ä".as_bytes()
        );
        assert_eq!(
            assemble(".data string \"\\x41\\101\\u00e4\\U0001F642\"", 0)
                .unwrap()
                .segments[0]
                .data,
            "AAä🙂".as_bytes()
        );
        assert_eq!(
            assemble(".data string \"key:value\"; ignored", 0)
                .unwrap()
                .segments[0]
                .data,
            b"key:value"
        );
        assert_eq!(
            assemble(".data bin 0b1", 0).unwrap().segments[0].data,
            &[0x01]
        );
    }

    #[test]
    fn preserves_python_assembler_encoding_and_sparse_address_regressions() {
        let image = assemble("JMP finish\nLDI R1, 315\nfinish: HALT\n", 0).unwrap();
        assert_eq!(image.entry_point, 0);
        assert_eq!(image.segments[0].data, b"\x58\x06\x09\x3b\x81\x01\x00\x00");

        let image = assemble(
            ".address 0x100\nstart: JMP end\n.address 0x20\nend: HALT\n",
            0,
        )
        .unwrap();
        assert_eq!(image.entry_point, 0x100);
        assert_eq!(
            image
                .segments
                .iter()
                .map(|segment| (segment.address, segment.data.as_slice()))
                .collect::<Vec<_>>(),
            [(0x20, &[0x00, 0x00][..]), (0x100, &[0x58, 0x20][..])]
        );

        let image = assemble(".address 0x30000\n.data hex 19", 0).unwrap();
        assert_eq!(image.segments[0].address, 0x30000);
        assert_eq!(image.segments[0].data, [0x19]);

        let image = assemble("LDA R2, 0x3F51234\nHALT", 0).unwrap();
        assert_eq!(
            image.segments[0].data,
            b"\xfa\x1e\x70\x00\x03\xf5\x12\x34\x00\x00"
        );
        assert_eq!(
            assemble("LDI30 R2, 0x3F51234\nHALT", 0).unwrap().segments[0].data,
            image.segments[0].data
        );

        let image = assemble(
            ".data hex 0x41, 42\n.data oct 101 102\n.data bin 01000011, 01000100\n.data string \"Hi\\n\"\n",
            0,
        )
        .unwrap();
        assert_eq!(image.segments[0].data, b"ABABCDHi\n");

        assert_eq!(
            assemble("ST R0, R1", 0).unwrap().segments[0].data,
            b"\x19\x00"
        );
        let image = assemble(
            "CALL routine\nHALT\n.address 0x400\nroutine: RET\nbranch: BZ routine\n",
            0,
        )
        .unwrap();
        let segments = image
            .segments
            .iter()
            .map(|segment| (segment.address, segment.data.as_slice()))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(segments[&0], b"\xb4\x00\x00\x00");
        assert_eq!(segments[&0x400], b"\xb8\x00\xc4\x00");

        let image = assemble("JMPX 0x12345\nCALLX 0x12345\nBZX 0x12345\nBCX 0x12345\n", 0).unwrap();
        assert_eq!(
            image.segments[0].data,
            b"\xf8\x3e\x00\x01\x23\x45"
                .iter()
                .chain(b"\xfa\x3e\x00\x01\x23\x45")
                .chain(b"\xfc\x3e\x00\x01\x23\x45")
                .chain(b"\xfe\x3e\x00\x01\x23\x45")
                .copied()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            assemble("BNZX 0x12345\nBLTX 0x23456\n", 0)
                .unwrap()
                .segments[0]
                .data,
            b"\xf8\x0d\x00\x01\x23\x45\xfb\x0d\x00\x02\x34\x56"
        );
        assert_eq!(
            assemble("CALLRP R0, R1\nJMPRP R2, R3", 0).unwrap().segments[0].data,
            b"\xf8\x1f\xf2\x0b\xfa\x1f\xf6\x0c"
        );
        assert_eq!(
            assemble(
                "LDW R2, R3\nSTW R2, R3\nLDWS R2, 4\nSTWS R2, 4\nADJSP -2\nBC 0x100\nSHR R3\n",
                0,
            )
            .unwrap()
            .segments[0]
                .data,
            b"\xca\x60\xd3\x40\xda\x04\xe2\x04\xe8\xfe\xf1\x00\xfb\x00"
        );
    }

    #[test]
    fn encodes_base_register_forms_for_every_register_combination() {
        for destination in 0..8u16 {
            for source in 0..8u16 {
                for mnemonic in ["MOV", "LD", "LDW"] {
                    let opcode = lookup(BASE_OPCODES, mnemonic).unwrap();
                    let image =
                        assemble(&format!("{mnemonic} R{destination}, R{source}"), 0).unwrap();
                    let expected = (opcode << 11) | (destination << 8) | (source << 5);
                    assert_eq!(
                        image.segments[0].data,
                        expected.to_be_bytes(),
                        "{mnemonic} R{destination}, R{source}"
                    );
                }

                for mnemonic in ["ST", "STW"] {
                    let opcode = lookup(BASE_OPCODES, mnemonic).unwrap();
                    let image =
                        assemble(&format!("{mnemonic} R{destination}, R{source}"), 0).unwrap();
                    let expected = (opcode << 11) | (source << 8) | (destination << 5);
                    assert_eq!(
                        image.segments[0].data,
                        expected.to_be_bytes(),
                        "{mnemonic} R{destination}, R{source}"
                    );
                }

                for mnemonic in ["ADD", "SUB", "AND", "OR", "XOR", "CMP"] {
                    let opcode = lookup(BASE_OPCODES, mnemonic).unwrap();
                    let image =
                        assemble(&format!("{mnemonic} R{destination}, R{source}"), 0).unwrap();
                    let expected =
                        (opcode << 11) | (destination << 8) | (destination << 5) | (source << 2);
                    assert_eq!(
                        image.segments[0].data,
                        expected.to_be_bytes(),
                        "{mnemonic} R{destination}, R{source}"
                    );
                }

                let image = assemble(&format!("NOT R{destination}, R{source}"), 0).unwrap();
                let expected = (lookup(BASE_OPCODES, "NOT").unwrap() << 11)
                    | (destination << 8)
                    | (source << 5);
                assert_eq!(
                    image.segments[0].data,
                    expected.to_be_bytes(),
                    "NOT R{destination}, R{source}"
                );
            }
        }
    }

    #[test]
    fn validates_memory_boundaries_and_entry_point_defaults() {
        assert_eq!(assemble("", 0).unwrap().entry_point, 0);
        assert_eq!(assemble("", 0x40).unwrap().entry_point, 0x40);
        assert_eq!(
            assemble(".address 5\n.data hex 1", 0).unwrap().entry_point,
            5
        );
        assert_eq!(assemble(".data hex 1\nHALT", 0).unwrap().entry_point, 1);
        assert_eq!(assemble("HALT", 0x30).unwrap().segments[0].address, 0x30);

        let final_byte =
            assemble(&format!(".address {}\n.data hex 7", MEMORY_SIZE - 1), 0).unwrap();
        assert_eq!(final_byte.segments[0].address, MEMORY_SIZE - 1);
        assert!(assemble("HALT", MEMORY_SIZE).is_err());
        assert!(assemble(&format!(".address {}\nHALT", MEMORY_SIZE - 1), 0).is_err());
        assert!(assemble(&format!(".address {}\n.data hex 1 2", MEMORY_SIZE - 1), 0).is_err());
        assert!(assemble(&format!(".address {}\nJMPX 0", MEMORY_SIZE - 2), 0).is_err());
        assert!(assemble(".address 0\nHALT\n.address 1\nHALT", 0).is_err());
    }

    #[test]
    fn encodes_signed_stack_offsets_and_branch_address_boundaries() {
        for offset in -128i16..=127 {
            let image = assemble(&format!("ADJSP {offset}"), 0).unwrap();
            let instruction =
                (lookup(BASE_OPCODES, "ADJSP").unwrap() << 11) | (offset as u16 & 0xff);
            assert_eq!(
                image.segments[0].data,
                instruction.to_be_bytes(),
                "ADJSP {offset}"
            );
        }

        for mnemonic in ["JMP", "CALL", "BZ", "BC"] {
            let opcode = lookup(BASE_OPCODES, mnemonic).unwrap();
            for target in [0, 1, 0xff, 0x100, 0x400, 0x7fe, 0x7ff] {
                let image = assemble(&format!("{mnemonic} {target}"), 0).unwrap();
                assert_eq!(
                    image.segments[0].data,
                    ((opcode << 11) | target).to_be_bytes(),
                    "{mnemonic} {target:#x}"
                );
            }
        }

        for (mnemonic, operation) in [("JMPX", 0), ("CALLX", 1), ("BZX", 2), ("BCX", 3)] {
            for target in [0, 1, 0xffff, 0x10000, 0x12345, MEMORY_SIZE - 1] {
                let image = assemble(&format!("{mnemonic} {target}"), 0).unwrap();
                assert_eq!(image.segments[0].data.len(), 6);
                assert_eq!(
                    u16::from_be_bytes([image.segments[0].data[0], image.segments[0].data[1]]),
                    FAR_CONTROL_PREFIX | (operation << 9)
                );
                assert_eq!(
                    u16::from_be_bytes([image.segments[0].data[2], image.segments[0].data[3]]),
                    (target >> 16) as u16
                );
                assert_eq!(
                    u16::from_be_bytes([image.segments[0].data[4], image.segments[0].data[5]]),
                    target as u16
                );
            }
        }
    }

    #[test]
    fn rejects_register_immediate_and_branch_boundary_violations() {
        for source in [
            "ADJSP 128",
            "ADJSP -129",
            "LDI R0, -1",
            "LDI R0, 65536",
            "LDI_H R0, 256",
            "LDWS R0, 256",
            "STWS R0, -1",
            "JMP 0x800",
            "CALL -1",
            "BZ 0x800",
            "BC -1",
            "JZ R0, 256",
            "JZ R8, 0",
            "JMPX 0x40000000",
            "CALLX -1",
            "BZX 0x40000000",
            "F32ADD R0, R1",
            "F64ADD R0, R2",
        ] {
            assert!(
                assemble(source, 0).is_err(),
                "expected invalid assembly to fail: {source}"
            );
        }
    }

    #[test]
    fn rejects_malformed_data_labels_operands_and_addresses() {
        for source in [
            ".data",
            ".data hex",
            ".data xyz 1",
            ".data hex 100",
            ".data hex zz",
            ".data oct 8",
            ".data bin 2",
            ".data string",
            ".data string abc",
            ".data string \"abc",
            ".data string \"a\"\"b\"",
            ".data string \"\\x\"",
            ".data string ,",
            ".data hex ,",
            ".address",
            ".address x",
            ".address 0x40000000",
            ".address -1",
            ".addressx 1",
            ".address 1 2",
            "a:\na:",
            "1a: HALT",
            "a b: HALT",
            "HALT: x",
            "LDI R0",
            "LDI R0, 1, 2",
            "JMPX",
            "JMPX 1, 2",
            "HALT 1",
            "RET 1",
            "JMP nowhere",
            "JZ R0, nowhere",
            "LDA R0, nowhere",
            "LDI R0, -1",
            "LDI R0, 0x10000",
            "LDA R0, 0x40000000",
        ] {
            assert!(
                assemble(source, 0).is_err(),
                "expected invalid assembly to fail: {source}"
            );
        }
    }

    #[test]
    fn assembles_comments_whitespace_and_common_numeric_forms() {
        let image = assemble(
            "  ; full line comment\n# hash comment\nLDI R0, 0x10 ; inline\nLDI R1, 0b11\nLDI R2, 0o7\nLDI R3, 12\n\tHALT # trailing\n",
            0,
        )
        .unwrap();
        assert_eq!(
            image.segments[0].data,
            b"\x08\x10\x09\x03\x0a\x07\x0b\x0c\x00\x00"
        );
    }

    #[test]
    fn rejects_invalid_inputs_and_output_overlaps() {
        assert!(
            assemble("INC R8", 0)
                .unwrap_err()
                .to_string()
                .contains("invalid register")
        );
        assert!(
            assemble("NOPE", 0)
                .unwrap_err()
                .to_string()
                .contains("unknown instruction")
        );
        assert!(
            assemble("HALT\n.address 0\n.data hex 01", 0)
                .unwrap_err()
                .to_string()
                .contains("overlaps")
        );
        assert!(assemble("LDI R0, missing", 0).is_err());
        assert!(assemble("NOP R0", 0).is_err());
        assert!(assemble(".data string \"\\x\"", 0).is_err());
        assert!(assemble(".data string \"a\"\"b\"", 0).is_err());
        assert!(assemble(".data string \"a\"b", 0).is_err());
        assert!(assemble("HALT", MEMORY_SIZE).is_err());
    }

    #[test]
    fn assembles_the_full_command_tour_with_extended_instruction_sizes() {
        let image = assemble(include_str!("../../examples/commands.asm"), 0).unwrap();
        assert_eq!(image.entry_point, 0x80);
        assert_eq!(
            image
                .segments
                .iter()
                .map(|segment| segment.data.len())
                .sum::<usize>(),
            632
        );
    }

    #[test]
    fn assembles_the_directive_example_into_five_sparse_segments() {
        let image = assemble(include_str!("../../examples/directives.asm"), 0).unwrap();
        assert_eq!(image.entry_point, 0x100);
        assert_eq!(
            image
                .segments
                .iter()
                .map(|segment| (segment.address, segment.data.len()))
                .collect::<Vec<_>>(),
            [(0x100, 8), (0x200, 3), (0x210, 2), (0x220, 2), (0x230, 13)]
        );
        assert_eq!(
            image
                .segments
                .iter()
                .map(|segment| (segment.address, segment.data.as_slice()))
                .collect::<Vec<_>>(),
            [
                (0x100, &b"\x08\x20\x59\x06\x00\x00\x00\x00"[..]),
                (0x200, &b"\x53\x43\xff"[..]),
                (0x210, &b"\x53\xff"[..]),
                (0x220, &b"\x53\x43"[..]),
                (0x230, &b"SC-16 ready!\n"[..]),
            ]
        );
    }

    #[test]
    fn matches_python_reference_examples_and_bitmap_font() {
        let example = assemble(include_str!("../../examples/example.asm"), 0).unwrap();
        assert_eq!(example.entry_point, 0);
        assert_eq!(example.segments.len(), 1);
        assert_eq!(example.segments[0].data.len(), 94);
        assert_eq!(&example.segments[0].data[..2], b"\x08\x0a");
        assert_eq!(&example.segments[0].data[92..], b"\x00\x00");

        let font = assemble(include_str!("../../fonts/font5x7.asm"), 0).unwrap();
        let font_segment = font
            .segments
            .iter()
            .find(|segment| segment.address == 0x4400)
            .unwrap();
        assert_eq!(font_segment.data.len(), 101 * 8);
        assert_eq!(&font_segment.data[..8], &[0; 8]);
        assert_eq!(
            &font_segment.data[8..16],
            &[0x20, 0x20, 0x20, 0x20, 0x20, 0, 0x20, 0]
        );
    }

    #[test]
    fn expands_nested_includes_and_reports_cycles() {
        let directory = std::env::temp_dir().join(format!(
            "sc16-assembler-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(directory.join("parts")).unwrap();
        fs::write(directory.join("parts").join("leaf.asm"), "INC R1\n").unwrap();
        fs::write(
            directory.join("parts").join("middle.asm"),
            ".include \"leaf.asm\"\nINC R2\n",
        )
        .unwrap();
        fs::write(
            directory.join("main.asm"),
            ".include \"parts/middle.asm\"\nHALT\n",
        )
        .unwrap();
        let image = assemble_file(&directory.join("main.asm"), 0).unwrap();
        assert_eq!(image.segments[0].data, b"\x69\x00\x6a\x00\x00\x00");
        fs::write(
            directory.join("parts").join("leaf.asm"),
            ".include \"../main.asm\"\n",
        )
        .unwrap();
        assert!(
            assemble_file(&directory.join("main.asm"), 0)
                .unwrap_err()
                .to_string()
                .contains("circular .include")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn expands_utf8_bom_includes_and_preserves_file_error_context() {
        let directory = std::env::temp_dir().join(format!(
            "sc16-assembler-bom-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("fragment.asm"), "\u{feff}INC R1\n").unwrap();
        fs::write(
            directory.join("main.asm"),
            "\u{feff}.include \"fragment.asm\"\nHALT\n",
        )
        .unwrap();
        let image = assemble_file(&directory.join("main.asm"), 0).unwrap();
        assert_eq!(image.segments[0].data, b"\x69\x00\x00\x00");
        assert!(
            assemble_with_base_directory(".include \"missing.asm\"", 0, &directory,)
                .unwrap_err()
                .to_string()
                .contains("cannot include")
        );
        assert!(
            assemble_file(&directory.join("missing.asm"), 0)
                .unwrap_err()
                .to_string()
                .contains("missing.asm")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn expands_in_memory_includes_from_the_supplied_directory() {
        let directory = std::env::temp_dir().join(format!(
            "sc16-assembler-memory-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("fragment.asm"), "HALT\n").unwrap();
        let image =
            assemble_with_base_directory(".include \"fragment.asm\"", 0x10, &directory).unwrap();
        assert_eq!(image.entry_point, 0x10);
        assert_eq!(image.segments[0].data, b"\x00\x00");
        fs::remove_dir_all(directory).unwrap();
    }
}
