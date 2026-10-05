import ast
import re
from dataclasses import dataclass
from pathlib import Path

from src.isa import (
    MULDIV_IMMEDIATE_FLAG,
    MULDIV_OPERATIONS,
    MULDIV_PREFIX,
    OPCODES,
    OP_LDI_H,
)

MEMORY_SIZE = 1 << 19


class AssemblyError(ValueError):
    """Raised when assembly source cannot be encoded for the SC-16."""


@dataclass(frozen=True)
class MemorySegment:
    address: int
    data: bytes


@dataclass(frozen=True)
class AssemblyImage:
    segments: tuple[MemorySegment, ...]
    entry_point: int


def _error(line_number: int, message: str) -> AssemblyError:
    return AssemblyError(f"line {line_number}: {message}")


def _strip_comment(line: str) -> str:
    quote = None
    escaped = False
    for index, char in enumerate(line):
        if escaped:
            escaped = False
        elif char == "\\" and quote is not None:
            escaped = True
        elif quote is not None and char == quote:
            quote = None
        elif quote is None and char in ("'", '"'):
            quote = char
        elif quote is None and char in (";", "#"):
            return line[:index]
    return line


def _source_statements(source: str) -> list[tuple[int, str]]:
    statements = []
    for line_number, source_line in enumerate(source.splitlines(), 1):
        line = _strip_comment(source_line).strip()
        if not line:
            continue
        label_match = re.match(r"^([A-Za-z_][A-Za-z0-9_]*):", line)
        if label_match:
            statements.append((line_number, f"{label_match.group(1)}:"))
            line = line[label_match.end():].strip()
        elif ":" in line and not line.startswith("."):
            raise _error(line_number, "invalid label")
        if line:
            statements.append((line_number, line))
    return statements


def _expand_includes(
    source: str,
    base_directory: Path,
    include_stack: tuple[Path, ...] = (),
) -> str:
    expanded_lines: list[str] = []
    for line_number, line in enumerate(source.splitlines(), 1):
        statement = _strip_comment(line).strip()
        if not statement.lower().startswith(".include"):
            expanded_lines.append(line)
            continue
        match = re.fullmatch(r"\.include\s+(.+)", statement, re.IGNORECASE)
        if match is None:
            raise _error(line_number, '.include expects one quoted file path')
        try:
            include_name = ast.literal_eval(match.group(1))
        except (SyntaxError, ValueError) as exc:
            raise _error(line_number, '.include expects one quoted file path') from exc
        if not isinstance(include_name, str) or not include_name:
            raise _error(line_number, '.include expects one non-empty quoted file path')

        include_path = (base_directory / include_name).resolve()
        if not include_path.is_file():
            # Try searching in the 'lib' and 'fonts' directories relative to the root of the project
            # We assume the root is where the execution starts or a known directory.
            # A simpler way is to check a few levels up for these folders.
            found = False
            current = base_directory
            for _ in range(5): # search up to 5 levels
                for folder in ("lib", "fonts"):
                    lib_path = current / folder / Path(include_name).name
                    if lib_path.is_file():
                        include_path = lib_path.resolve()
                        found = True
                        break
                if found: break
                current = current.parent
                if current == current.parent: break
        
        if include_path in include_stack:
            cycle = " -> ".join(str(path) for path in (*include_stack, include_path))
            raise _error(line_number, f"circular .include detected: {cycle}")
        try:
            included_source = include_path.read_text(encoding="utf-8-sig")
        except (OSError, UnicodeError) as exc:
            raise _error(line_number, f"cannot include {include_path}: {exc}") from exc
        expanded_lines.append(
            _expand_includes(
                included_source,
                include_path.parent,
                (*include_stack, include_path),
            )
        )
    return "\n".join(expanded_lines)


def _parse_number(value: str, line_number: int) -> int:
    try:
        return int(value, 0)
    except ValueError as exc:
        raise _error(line_number, f"expected a numeric value, got {value!r}") from exc


def _parse_register(value: str, line_number: int) -> int:
    match = re.fullmatch(r"R([0-7])", value.strip(), re.IGNORECASE)
    if match is None:
        raise _error(line_number, f"invalid register {value!r}; expected R0-R7")
    return int(match.group(1))


def _parse_operands(text: str, line_number: int) -> tuple[str, list[str]]:
    parts = text.split(None, 1)
    mnemonic = parts[0].upper()
    if mnemonic not in OPCODES:
        raise _error(line_number, f"unknown instruction {parts[0]!r}")
    if len(parts) == 1 or not parts[1].strip():
        return mnemonic, []
    return mnemonic, [operand.strip() for operand in parts[1].split(",")]


def _parse_string_data(text: str, line_number: int) -> bytes:
    values: list[int] = []
    found_string = False
    position = 0
    while position < len(text):
        while position < len(text) and (text[position].isspace() or text[position] == ","):
            position += 1
        if position == len(text):
            break
        if text[position] not in ("'", '"'):
            raise _error(line_number, ".data string expects quoted string values")
        quote = text[position]
        start = position
        position += 1
        escaped = False
        while position < len(text):
            char = text[position]
            position += 1
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                break
        else:
            raise _error(line_number, "unterminated string in .data")
        try:
            value = ast.literal_eval(text[start:position])
        except (SyntaxError, ValueError) as exc:
            raise _error(line_number, "invalid string in .data") from exc
        found_string = True
        values.extend(value.encode("utf-8"))
        if position < len(text) and not (
            text[position].isspace() or text[position] == ","
        ):
            raise _error(line_number, "expected a separator between strings")
    if not found_string:
        raise _error(line_number, ".data string requires at least one string")
    return bytes(values)


def _parse_data(text: str, line_number: int) -> bytes:
    parts = text.split(None, 1)
    if len(parts) != 2:
        raise _error(line_number, ".data expects a format and values")
    data_format, values_text = parts[0].lower(), parts[1].strip()
    if data_format == "string":
        return _parse_string_data(values_text, line_number)
    bases = {"hex": (16, r"(?:0[xX])?"), "oct": (8, r"(?:0[oO])?"), "bin": (2, r"(?:0[bB])?")}
    if data_format not in bases:
        raise _error(line_number, f"unsupported .data format {parts[0]!r}")
    base, prefix = bases[data_format]
    tokens = [token for token in re.split(r"[\s,]+", values_text) if token]
    if not tokens:
        raise _error(line_number, f".data {data_format} requires at least one value")
    result = bytearray()
    digit_pattern = {16: r"[0-9a-fA-F]+", 8: r"[0-7]+", 2: r"[01]+"}[base]
    for token in tokens:
        digits = re.sub(rf"^{prefix}", "", token)
        if re.fullmatch(digit_pattern, digits) is None:
            raise _error(line_number, f"invalid {data_format} data value {token!r}")
        number = int(digits, base)
        if number > 0xFF:
            raise _error(line_number, ".data values must fit in one byte")
        result.append(number)
    return bytes(result)


def _parse_instruction(text: str, line_number: int) -> tuple[str, list[str]]:
    return _parse_operands(text, line_number)


def _instruction_size(text: str, line_number: int) -> int:
    mnemonic, operands = _parse_instruction(text, line_number)
    if mnemonic in ("JMPX", "CALLX", "BZX", "BCX"):
        if len(operands) != 1:
            raise _error(line_number, f"{mnemonic} expects one address or label")
        return 6
    if mnemonic in MULDIV_OPERATIONS:
        if len(operands) != 2:
            raise _error(line_number, f"{mnemonic} expects a register and a register or immediate")
        return 6 if not re.fullmatch(r"R[0-7]", operands[1].strip(), re.IGNORECASE) else 4
    if mnemonic == "LDA":
        if len(operands) != 2:
            raise _error(line_number, "LDA expects a register and an address or label")
        return 4
    if mnemonic == "LDI":
        if len(operands) != 2:
            raise _error(line_number, "LDI expects a register and an immediate")
        value = _parse_number(operands[1], line_number)
        if not 0 <= value <= 0xFFFF:
            raise _error(line_number, "LDI immediate must be between 0 and 65535")
        return 4 if value > 0xFF else 2
    return 2


def _assemble_instruction(
    text: str, line_number: int, labels: dict[str, int]
) -> list[int]:
    mnemonic, operands = _parse_instruction(text, line_number)
    opcode = OPCODES[mnemonic]

    def require_count(count: int) -> None:
        if len(operands) != count:
            raise _error(line_number, f"{mnemonic} expects {count} operand(s)")

    def immediate(value: str, maximum: int) -> int:
        number = _parse_number(value, line_number)
        if not 0 <= number <= maximum:
            raise _error(
                line_number,
                f"{mnemonic} operand must be between 0 and {maximum:#x}",
            )
        return number

    def target_address(value: str) -> int:
        if value in labels:
            return labels[value]
        return _parse_number(value, line_number)

    far_control = {"JMPX": 0xF81D, "CALLX": 0xF91D, "BZX": 0xFA1D, "BCX": 0xFB1D}
    if mnemonic in far_control:
        require_count(1)
        target = target_address(operands[0])
        if not 0 <= target < MEMORY_SIZE:
            raise _error(line_number, f"{mnemonic} target is outside available memory")
        return [far_control[mnemonic], target & 0xFFFF, target >> 16]

    if mnemonic in MULDIV_OPERATIONS:
        require_count(2)
        destination = _parse_register(operands[0], line_number)
        first = (opcode << 11) | (destination << 8) | MULDIV_PREFIX
        control = MULDIV_OPERATIONS[mnemonic] << 12
        if re.fullmatch(r"R[0-7]", operands[1].strip(), re.IGNORECASE):
            return [first, control | _parse_register(operands[1], line_number)]
        value = _parse_number(operands[1], line_number)
        if not -0x8000 <= value <= 0xFFFF:
            raise _error(line_number, f"{mnemonic} immediate must fit in 16 bits")
        return [first, control | MULDIV_IMMEDIATE_FLAG, value & 0xFFFF]

    extended = {
        "MOVSP": 0,
        "FADD": 1,
        "FSUB": 2,
        "FMUL": 3,
        "FDIV": 4,
        "FCMP": 5,
        "ITOF": 6,
        "FTOI": 7,
    }
    if mnemonic in extended:
        if mnemonic == "MOVSP":
            require_count(1)
            register = _parse_register(operands[0], line_number)
            return [(opcode << 11) | (register << 8) | (7 << 2)]
        if mnemonic in ("ITOF", "FTOI"):
            require_count(1)
            register = _parse_register(operands[0], line_number)
            return [(opcode << 11) | (extended[mnemonic] << 5) | (register << 8)]
        require_count(2)
        destination = _parse_register(operands[0], line_number)
        source_register = _parse_register(operands[1], line_number)
        return [
            (opcode << 11)
            | (destination << 8)
            | (extended[mnemonic] << 5)
            | (source_register << 2)
        ]

    if mnemonic == "LDI":
        require_count(2)
        destination = _parse_register(operands[0], line_number)
        value = immediate(operands[1], 0xFFFF)
        words = [(opcode << 11) | (destination << 8) | (value & 0xFF)]
        if value > 0xFF:
            words.append((OP_LDI_H << 11) | (destination << 8) | (value >> 8))
        return words
    if mnemonic == "LDA":
        require_count(2)
        destination = _parse_register(operands[0], line_number)
        value = target_address(operands[1])
        if not 0 <= value < MEMORY_SIZE:
            raise _error(line_number, "LDA address is outside available memory")
        return [
            (OPCODES["LDI"] << 11) | (destination << 8) | (value & 0xFF),
            (OP_LDI_H << 11) | (destination << 8) | (value >> 8),
        ]
    if mnemonic == "LDI_H":
        require_count(2)
        destination = _parse_register(operands[0], line_number)
        value = immediate(operands[1], 0xFF)
        return [(opcode << 11) | (destination << 8) | value]
    if mnemonic in ("HALT", "EI", "DI", "RTI", "RET"):
        require_count(0)
        return [opcode << 11]
    if mnemonic == "LD":
        require_count(2)
        destination = _parse_register(operands[0], line_number)
        address_register = _parse_register(operands[1], line_number)
        return [(opcode << 11) | (destination << 8) | (address_register << 5)]
    if mnemonic == "LDW":
        require_count(2)
        destination = _parse_register(operands[0], line_number)
        address_register = _parse_register(operands[1], line_number)
        return [(opcode << 11) | (destination << 8) | (address_register << 5)]
    if mnemonic == "ST":
        require_count(2)
        address_register = _parse_register(operands[0], line_number)
        value_register = _parse_register(operands[1], line_number)
        return [(opcode << 11) | (value_register << 8) | (address_register << 5)]
    if mnemonic == "STW":
        require_count(2)
        address_register = _parse_register(operands[0], line_number)
        value_register = _parse_register(operands[1], line_number)
        return [(opcode << 11) | (value_register << 8) | (address_register << 5)]
    if mnemonic in ("LDWS", "STWS"):
        require_count(2)
        register = _parse_register(operands[0], line_number)
        offset = immediate(operands[1], 0xFF)
        return [(opcode << 11) | (register << 8) | offset]
    if mnemonic == "ADJSP":
        require_count(1)
        try:
            offset = int(operands[0], 0)
        except ValueError as exc:
            raise _error(line_number, "ADJSP expects a signed byte offset") from exc
        if not -128 <= offset <= 127:
            raise _error(line_number, "ADJSP offset must be between -128 and 127")
        return [(opcode << 11) | (offset & 0xFF)]
    if mnemonic == "MOV":
        require_count(2)
        destination = _parse_register(operands[0], line_number)
        source_register = _parse_register(operands[1], line_number)
        return [(opcode << 11) | (destination << 8) | (source_register << 5)]
    if mnemonic in ("ADD", "SUB", "AND", "OR", "XOR", "CMP"):
        require_count(2)
        destination = _parse_register(operands[0], line_number)
        source_register = _parse_register(operands[1], line_number)
        return [
            (opcode << 11)
            | (destination << 8)
            | (destination << 5)
            | (source_register << 2)
        ]
    if mnemonic == "NOT":
        if len(operands) not in (1, 2):
            raise _error(line_number, "NOT expects a destination and optional source")
        destination = _parse_register(operands[0], line_number)
        source_register = (
            _parse_register(operands[1], line_number)
            if len(operands) == 2
            else destination
        )
        return [(opcode << 11) | (destination << 8) | (source_register << 5)]
    if mnemonic in ("INC", "DEC", "PUSH", "POP", "SHR"):
        require_count(1)
        register = _parse_register(operands[0], line_number)
        return [(opcode << 11) | (register << 8)]
    if mnemonic in ("JMP", "CALL", "BZ", "BC"):
        require_count(1)
        target = target_address(operands[0])
        if not 0 <= target <= 0x7FF:
            raise _error(
                line_number,
                f"{mnemonic} target must be between 0 and 0x7ff",
            )
        return [(opcode << 11) | target]
    if mnemonic == "JZ":
        require_count(2)
        register = _parse_register(operands[0], line_number)
        target = target_address(operands[1])
        if not 0 <= target <= 0xFF:
            raise _error(line_number, "JZ target must be between 0 and 0xff")
        return [(opcode << 11) | (register << 8) | target]
    raise _error(line_number, f"unsupported instruction {mnemonic!r}")  # pragma: no cover


def assemble(
    source: str,
    origin: int = 0,
    base_directory: str | Path | None = None,
) -> AssemblyImage:
    """Assemble source into segments; .include paths are relative to base_directory."""
    if not 0 <= origin < MEMORY_SIZE:
        raise AssemblyError(f"invalid assembly origin {origin:#x}")

    include_directory = Path(base_directory) if base_directory is not None else Path.cwd()
    source = _expand_includes(source, include_directory.resolve())
    statements = _source_statements(source)
    labels: dict[str, int] = {}
    located_statements: list[tuple[int, str, int, bytes | None]] = []
    address = origin
    first_instruction: int | None = None
    first_emitted_address: int | None = None

    for line_number, text in statements:
        if text.endswith(":"):
            label = text[:-1]
            if label in labels:
                raise _error(line_number, f"duplicate label {label!r}")
            labels[label] = address
            continue
        if text.lower().startswith(".address"):
            parts = text.split(None, 1)
            if len(parts) != 2 or parts[0].lower() != ".address":
                raise _error(line_number, ".address expects one numeric address")
            address = _parse_number(parts[1], line_number)
            if not 0 <= address < MEMORY_SIZE:
                raise _error(line_number, ".address is outside available memory")
            located_statements.append((line_number, text, address, None))
            continue
        if text.lower().startswith(".data"):
            parts = text.split(None, 1)
            if len(parts) != 2 or parts[0].lower() != ".data":
                raise _error(line_number, ".data expects a format and values")
            data = _parse_data(parts[1], line_number)
            located_statements.append((line_number, text, address, data))
            if data and first_emitted_address is None:
                first_emitted_address = address
            address += len(data)
        else:
            instruction_size = _instruction_size(text, line_number)
            located_statements.append((line_number, text, address, None))
            if first_instruction is None:
                first_instruction = address
            if first_emitted_address is None:
                first_emitted_address = address
            address += instruction_size
        if address > MEMORY_SIZE:
            raise _error(line_number, "assembled program exceeds available memory")

    memory_bytes: dict[int, int] = {}
    for line_number, text, start, data in located_statements:
        if text.lower().startswith(".address"):
            continue
        if data is not None:
            emitted = data
        else:
            words = _assemble_instruction(text, line_number, labels)
            emitted = b"".join(word.to_bytes(2, "big") for word in words)
        for offset, value in enumerate(emitted):
            location = start + offset
            if location in memory_bytes:
                raise _error(line_number, f"output overlaps previously emitted data at {location:#x}")
            memory_bytes[location] = value

    segments: list[MemorySegment] = []
    current_start: int | None = None
    current_data = bytearray()
    for location in sorted(memory_bytes):
        if current_start is None or current_start + len(current_data) != location:
            if current_start is not None:
                segments.append(MemorySegment(current_start, bytes(current_data)))
            current_start = location
            current_data = bytearray()
        current_data.append(memory_bytes[location])
    if current_start is not None:
        segments.append(MemorySegment(current_start, bytes(current_data)))
    if first_instruction is not None:
        entry_point = first_instruction
    elif first_emitted_address is not None:
        entry_point = first_emitted_address
    else:
        entry_point = origin
    return AssemblyImage(tuple(segments), entry_point)


def assemble_file(path: str | Path, origin: int = 0) -> AssemblyImage:
    """Read and assemble an UTF-8 SC-16 assembly file."""
    source_path = Path(path).resolve()
    try:
        source = source_path.read_text(encoding="utf-8-sig")
    except (OSError, UnicodeError) as exc:
        raise AssemblyError(f"cannot read {source_path}: {exc}") from exc
    try:
        return assemble(source, origin, source_path.parent)
    except AssemblyError as exc:
        raise AssemblyError(f"{source_path}: {exc}") from exc
