use crate::MemorySegment;
use crate::image::ProgramImage;
use std::collections::BTreeMap;

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
    let (base, digits) = if let Some(rest) = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))
    {
        (16, rest)
    } else if let Some(rest) = unsigned
        .strip_prefix("0o")
        .or_else(|| unsigned.strip_prefix("0O"))
    {
        (8, rest)
    } else if let Some(rest) = unsigned
        .strip_prefix("0b")
        .or_else(|| unsigned.strip_prefix("0B"))
    {
        (2, rest)
    } else {
        (10, unsigned)
    };
    let parsed = i64::from_str_radix(digits, base)
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
                let escaped = text
                    .chars()
                    .next()
                    .ok_or_else(|| error(line, "unterminated string in .data"))?;
                text = &text[escaped.len_utf8()..];
                value.push(match escaped {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '\\' => '\\',
                    '\'' => '\'',
                    '"' => '"',
                    other => {
                        return Err(error(line, format!("unsupported string escape \\{other}")));
                    }
                });
            } else {
                value.push(character);
            }
        }
        if !closed {
            return Err(error(line, "unterminated string in .data"));
        }
        found = true;
        result.extend_from_slice(value.as_bytes());
        let rest = text.trim_start();
        if !rest.is_empty() && !rest.starts_with(',') && !rest.starts_with(['\'', '"']) {
            return Err(error(line, "expected a separator between strings"));
        }
    }
    if found {
        Ok(result)
    } else {
        Err(error(line, ".data string requires at least one string"))
    }
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
    let opcode = match mnemonic.as_str() {
        "HALT" => 0x00,
        "LDI" => 0x01,
        "LD" => 0x02,
        "ST" => 0x03,
        "MOV" => 0x04,
        "ADD" => 0x05,
        "SUB" => 0x06,
        "AND" => 0x07,
        "OR" => 0x08,
        "XOR" => 0x09,
        "NOT" => 0x0a,
        "JMP" => 0x0b,
        "JZ" => 0x0c,
        "INC" => 0x0d,
        "DEC" => 0x0e,
        "CMP" => 0x0f,
        "LDI_H" => 0x10,
        "EI" => 0x11,
        "DI" => 0x12,
        "RTI" => 0x13,
        "PUSH" => 0x14,
        "POP" => 0x15,
        "CALL" => 0x16,
        "RET" => 0x17,
        "BZ" => 0x18,
        "LDW" => 0x19,
        "STW" => 0x1a,
        "LDWS" => 0x1b,
        "STWS" => 0x1c,
        "ADJSP" => 0x1d,
        "BC" => 0x1e,
        "SHR" => 0x1f,
        "NOP" => 0x04,
        _ => return Err(error(line, format!("unknown instruction {mnemonic:?}"))),
    };
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
                words.push((0x10 << 11) | (rd << 8) | (value >> 8));
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

pub fn assemble(source: &str, origin: u32) -> Result<ProgramImage, AssemblyError> {
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
            let size = if text.to_ascii_uppercase().starts_with("LDI ") {
                let (_, args) = text.split_once(char::is_whitespace).unwrap();
                let operands = split_operands(args);
                if operands.len() != 2 {
                    return Err(error(line, "LDI expects a register and an immediate"));
                }
                let value = parse_number(operands[1], line)?;
                if !(0..=0xffff).contains(&value) {
                    return Err(error(line, "LDI immediate must be between 0 and 65535"));
                }
                if value > 0xff { 4 } else { 2 }
            } else {
                2
            };
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
            assemble(".data string \"key:value\"; ignored", 0)
                .unwrap()
                .segments[0]
                .data,
            b"key:value"
        );
        assert_eq!(
            assemble(".data hex 0b1", 0).unwrap().segments[0].data,
            &[0x0b]
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
        assert!(assemble("HALT", MEMORY_SIZE).is_err());
    }
}
