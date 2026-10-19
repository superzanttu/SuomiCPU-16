#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub value: String,
    pub line: usize,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CCompilerError(pub String);

impl std::fmt::Display for CCompilerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CCompilerError {}

const MULTI_OPERATORS: [&str; 22] = [
    "<<=", ">>=", "++", "--", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "%=",
    "&=", "|=", "^=", "<<", ">>", "##", "->",
];

fn error(filename: &str, line: usize, message: &str) -> CCompilerError {
    CCompilerError(format!("{filename}:{line}: {message}"))
}

fn float_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut position = start;
    let mut digits_before = 0;
    while bytes.get(position).is_some_and(u8::is_ascii_digit) {
        position += 1;
        digits_before += 1;
    }

    let mut is_float = false;
    if bytes.get(position) == Some(&b'.') {
        is_float = true;
        position += 1;
        while bytes.get(position).is_some_and(u8::is_ascii_digit) {
            position += 1;
        }
    } else if digits_before == 0 {
        return None;
    }

    if bytes
        .get(position)
        .is_some_and(|byte| matches!(byte, b'e' | b'E'))
    {
        let mut exponent_end = position + 1;
        if bytes
            .get(exponent_end)
            .is_some_and(|byte| matches!(byte, b'+' | b'-'))
        {
            exponent_end += 1;
        }
        let digits_start = exponent_end;
        while bytes.get(exponent_end).is_some_and(u8::is_ascii_digit) {
            exponent_end += 1;
        }
        if exponent_end > digits_start {
            is_float = true;
            position = exponent_end;
        }
    }

    if !is_float {
        return None;
    }
    if bytes
        .get(position)
        .is_some_and(|byte| matches!(byte, b'f' | b'F' | b'l' | b'L'))
    {
        position += 1;
    }
    Some(position)
}

fn decode_quoted(value: &str) -> Result<String, &'static str> {
    let characters: Vec<char> = value.chars().collect();
    let mut decoded = String::with_capacity(value.len());
    let mut position = 0;
    while position < characters.len() {
        if characters[position] != '\\' {
            decoded.push(characters[position]);
            position += 1;
            continue;
        }
        position += 1;
        let Some(&escape) = characters.get(position) else {
            return Err("invalid string literal");
        };
        position += 1;
        let simple = match escape {
            '\\' => Some('\\'),
            '\'' => Some('\''),
            '"' => Some('"'),
            'a' => Some('\u{7}'),
            'b' => Some('\u{8}'),
            'f' => Some('\u{c}'),
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            'v' => Some('\u{b}'),
            '\n' => None,
            _ => None,
        };
        if matches!(escape, '\n') {
            continue;
        }
        if let Some(character) = simple {
            decoded.push(character);
            continue;
        }

        let radix = match escape {
            'x' => Some((16, 2)),
            'u' => Some((16, 4)),
            'U' => Some((16, 8)),
            _ => None,
        };
        if let Some((radix, width)) = radix {
            let end = position + width;
            let Some(digits) = characters.get(position..end) else {
                return Err("invalid string literal");
            };
            let digits: String = digits.iter().collect();
            let value =
                u32::from_str_radix(&digits, radix).map_err(|_| "invalid string literal")?;
            decoded.push(char::from_u32(value).ok_or("invalid string literal")?);
            position = end;
            continue;
        }

        if ('0'..='7').contains(&escape) {
            let mut value = escape.to_digit(8).expect("octal digit");
            for _ in 0..2 {
                if let Some(digit) = characters
                    .get(position)
                    .and_then(|character| character.to_digit(8))
                {
                    value = value * 8 + digit;
                    position += 1;
                } else {
                    break;
                }
            }
            decoded.push(char::from_u32(value).ok_or("invalid string literal")?);
            continue;
        }

        decoded.push('\\');
        decoded.push(escape);
    }
    Ok(decoded)
}

fn parse_line_marker(
    marker: &str,
    filename: &str,
    line: usize,
) -> Result<(usize, Option<String>), CCompilerError> {
    let mut fields = marker.splitn(2, ':');
    let line_value = fields.next().unwrap_or_default();
    let parsed_line = line_value
        .parse::<usize>()
        .ok()
        .filter(|line| *line > 0)
        .ok_or_else(|| error(filename, line, "invalid internal line marker"))?;
    let marked_filename = match fields.next() {
        Some(hex) if !hex.is_empty() => {
            if hex.len() % 2 != 0 {
                return Err(error(filename, line, "invalid internal filename marker"));
            }
            let mut bytes = Vec::with_capacity(hex.len() / 2);
            for pair in hex.as_bytes().chunks(2) {
                let pair = std::str::from_utf8(pair)
                    .map_err(|_| error(filename, line, "invalid internal filename marker"))?;
                bytes.push(
                    u8::from_str_radix(pair, 16)
                        .map_err(|_| error(filename, line, "invalid internal filename marker"))?,
                );
            }
            Some(
                String::from_utf8(bytes)
                    .map_err(|_| error(filename, line, "invalid internal filename marker"))?,
            )
        }
        _ => None,
    };
    Ok((parsed_line, marked_filename))
}

pub fn lex_raw(source: &str, filename: &str) -> Result<Vec<Token>, CCompilerError> {
    let mut tokens = Vec::new();
    let mut position = 0;
    let mut line = 1;
    let mut token_filename = filename.to_owned();

    while position < source.len() {
        let remaining = &source[position..];
        let character = remaining
            .chars()
            .next()
            .expect("position is a char boundary");
        if character.is_whitespace() {
            if character == '\n' {
                line += 1;
            }
            position += character.len_utf8();
            continue;
        }

        if remaining.starts_with("\0SC16_LINE:") {
            let marker_start = position + "\0SC16_LINE:".len();
            let marker_end = source[marker_start..]
                .find('\n')
                .map(|offset| marker_start + offset)
                .unwrap_or(source.len());
            let marker = &source[marker_start..marker_end];
            let (marked_line, marked_filename) = parse_line_marker(marker, &token_filename, line)?;
            line = marked_line - 1;
            if let Some(marked_filename) = marked_filename {
                token_filename = marked_filename;
            }
            position = marker_end;
            continue;
        }

        if remaining.starts_with("//") {
            position += remaining.find('\n').unwrap_or(remaining.len());
            continue;
        }
        if let Some(comment_body) = remaining.strip_prefix("/*") {
            let Some(end) = comment_body.find("*/").map(|offset| position + 2 + offset) else {
                return Err(error(&token_filename, line, "unterminated comment"));
            };
            line += source[position..end + 2].matches('\n').count();
            position = end + 2;
            continue;
        }

        if let Some(end) = float_end(source, position) {
            tokens.push(Token {
                value: source[position..end].to_owned(),
                line,
                filename: Some(token_filename.clone()),
            });
            position = end;
            continue;
        }

        if character == '"' || character == '\'' {
            let quote = character;
            let start_line = line;
            position += quote.len_utf8();
            let content_start = position;
            let mut escaped = false;
            let mut closed = false;
            while position < source.len() {
                let current = source[position..]
                    .chars()
                    .next()
                    .expect("valid char boundary");
                if current == '\n' && !escaped {
                    let message = if quote == '"' {
                        "newline in string literal"
                    } else {
                        "newline in character/string literal"
                    };
                    return Err(error(&token_filename, start_line, message));
                }
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == quote {
                    let content_end = position;
                    position += current.len_utf8();
                    let decoded = decode_quoted(&source[content_start..content_end])
                        .map_err(|message| error(&token_filename, start_line, message))?;
                    let value = if quote == '"' {
                        format!(
                            "<string:{}>",
                            decoded
                                .as_bytes()
                                .iter()
                                .map(|byte| format!("{byte:02x}"))
                                .collect::<String>()
                        )
                    } else if decoded.chars().count() == 1 {
                        (decoded.chars().next().expect("one character") as u32).to_string()
                    } else {
                        return Err(error(
                            &token_filename,
                            start_line,
                            "character constants must contain one character",
                        ));
                    };
                    tokens.push(Token {
                        value,
                        line: start_line,
                        filename: Some(token_filename.clone()),
                    });
                    closed = true;
                    break;
                }
                if quote == '\'' && current == '\n' {
                    line += 1;
                }
                position += current.len_utf8();
            }
            if !closed {
                let message = if quote == '"' {
                    "unterminated string literal"
                } else {
                    "unterminated character/string literal"
                };
                return Err(error(&token_filename, start_line, message));
            }
            continue;
        }

        if character.is_alphabetic() || character == '_' {
            let start = position;
            position += character.len_utf8();
            while position < source.len() {
                let next = source[position..]
                    .chars()
                    .next()
                    .expect("valid char boundary");
                if !next.is_alphanumeric() && next != '_' {
                    break;
                }
                position += next.len_utf8();
            }
            tokens.push(Token {
                value: source[start..position].to_owned(),
                line,
                filename: Some(token_filename.clone()),
            });
            continue;
        }

        if character.is_ascii_digit() {
            let start = position;
            position += character.len_utf8();
            while position < source.len() {
                let next = source[position..]
                    .chars()
                    .next()
                    .expect("valid char boundary");
                if !next.is_alphanumeric() && next != '_' {
                    break;
                }
                position += next.len_utf8();
            }
            tokens.push(Token {
                value: source[start..position].to_owned(),
                line,
                filename: Some(token_filename.clone()),
            });
            continue;
        }

        if let Some(operator) = MULTI_OPERATORS
            .iter()
            .find(|operator| remaining.starts_with(**operator))
        {
            tokens.push(Token {
                value: (*operator).to_owned(),
                line,
                filename: Some(token_filename.clone()),
            });
            position += operator.len();
            continue;
        }
        if "{}();,:.[]=+-*/%<>!~&|^#?".contains(character) {
            tokens.push(Token {
                value: character.to_string(),
                line,
                filename: Some(token_filename.clone()),
            });
            position += character.len_utf8();
            continue;
        }
        return Err(error(
            &token_filename,
            line,
            &format!("unsupported character {character:?}"),
        ));
    }

    tokens.push(Token {
        value: "<eof>".to_owned(),
        line,
        filename: Some(token_filename),
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(source: &str) -> Vec<String> {
        lex_raw(source, "fixture.c")
            .unwrap()
            .into_iter()
            .map(|token| token.value)
            .collect()
    }

    #[test]
    fn tokenizes_c90_operators_in_longest_first_order() {
        assert_eq!(
            values("a <<= 2; b -> c; ++a; x && y; x ## y;"),
            [
                "a", "<<=", "2", ";", "b", "->", "c", ";", "++", "a", ";", "x", "&&", "y", ";",
                "x", "##", "y", ";", "<eof>"
            ]
        );
    }

    #[test]
    fn encodes_string_and_character_literals_as_compiler_tokens() {
        assert_eq!(
            values(r#""A\n\u03bb" '\x41' '\101'"#),
            ["<string:410acebb>", "65", "65", "<eof>"]
        );
    }

    #[test]
    fn recognizes_decimal_floating_constant_forms() {
        assert_eq!(
            values(".5 1. 1e-2 2.5F 0x1e"),
            [".5", "1.", "1e-2", "2.5F", "0x1e", "<eof>"]
        );
    }

    #[test]
    fn tracks_comment_lines_and_internal_file_markers() {
        let tokens = lex_raw(
            "/* first\nsecond */\n\0SC16_LINE:27:696e636c7564652e68\nname",
            "root.c",
        )
        .unwrap();
        assert_eq!(tokens[0].value, "name");
        assert_eq!(tokens[0].line, 27);
        assert_eq!(tokens[0].filename.as_deref(), Some("include.h"));
        assert_eq!(tokens[1].value, "<eof>");
        assert_eq!(tokens[1].line, 27);
    }

    #[test]
    fn reports_unterminated_comments_and_invalid_character_constants() {
        assert_eq!(
            lex_raw("/* open", "fixture.c").unwrap_err().to_string(),
            "fixture.c:1: unterminated comment"
        );
        assert_eq!(
            lex_raw("'ab'", "fixture.c").unwrap_err().to_string(),
            "fixture.c:1: character constants must contain one character"
        );
    }

    #[test]
    fn reports_unknown_source_characters_with_filename_and_line() {
        assert_eq!(
            lex_raw("int x;\n@", "sample.c").unwrap_err().to_string(),
            "sample.c:2: unsupported character '@'"
        );
    }
}
