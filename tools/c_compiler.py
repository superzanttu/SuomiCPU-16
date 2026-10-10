"""A small C90-oriented compiler for the SC-16."""

import argparse
import ast
import decimal
import datetime
import os
import re
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Sequence

from tools.assembler import AssemblyError, AssemblyImage, assemble

WORD_MASK = 0xFFFF
CODE_BASE = 0x4800
GLOBAL_BASE = 0x2000
STACK_OFFSET_LIMIT = 0xFF
FRAME_SIZE_LIMIT = 126
SCREEN_LIBRARY_FUNCTIONS = {
    "SCREEN_CLEAR": ("void", ()),
    "SCREEN_SET_CURSOR": ("void", ("unsigned char", "unsigned char")),
    "SCREEN_GET_CURSOR": ("int", ()),
    "SCREEN_PUTCHAR": ("void", ("char", "unsigned char")),
    "SCREEN_PRINT": ("void", ("char*", "unsigned char")),
    "SCREEN_INPUT": ("unsigned int", ("char*", "unsigned int", "unsigned char")),
}
GRAPHICS_LIBRARY_FUNCTIONS = {
    "gfx_clear": ("void", ("unsigned char",)),
    "gfx_pixel": ("void", ("int", "int", "unsigned char")),
    "gfx_rect": ("void", ("int", "int", "int", "int", "unsigned char")),
    "gfx_line": ("void", ("int", "int", "int", "int", "unsigned char")),
    "gfx_sprite": ("void", ("int", "int", "int", "int", "unsigned char*")),
    "gfx_bitmap": ("void", ("int", "int", "int", "int", "unsigned char*", "unsigned char")),
    "gfx_poly": ("void", ("int", "int", "int", "char*", "unsigned char")),
    "gfx_text": ("void", ("int", "int", "char*", "unsigned char")),
    "gfx_present": ("void", ()),
    "gfx_keys": ("unsigned int", ()),
    "gfx_random": ("unsigned int", ()),
    "gfx_ticks": ("unsigned int", ()),
    "gfx_rtc": ("unsigned int", ("unsigned char",)),
    "gfx_speed": ("void", ("int",)),
    "gfx_irq_init": ("void", ()),
    "gfx_irq_ticks": ("unsigned int", ()),
    "gfx_irq_keys": ("unsigned int", ()),
    "gfx_keys_ext": ("unsigned int", ()),
    "gfx_mouse_x": ("int", ()),
    "gfx_mouse_y": ("int", ()),
    "gfx_mouse_buttons": ("unsigned int", ()),
    "gfx_mouse_events": ("unsigned int", ()),
    "gfx_tilemap": ("void", ("unsigned char*", "int", "int", "int", "int")),
    "gfx_getchar": ("int", ()),
    "net_open": ("int", ("char*",)),
    "net_close": ("void", ()),
    "net_send": ("int", ("unsigned char*", "int")),
    "net_recv": ("int", ("unsigned char*",)),
    "net_players": ("int", ()),
    "net_slot": ("int", ()),
    "net_active": ("int", ("int",)),
    "net_full": ("int", ()),
    "net_sender": ("int", ()),
    "net_ready": ("int", ()),
    "net_is_open": ("int", ()),
    "gfx_save": ("void", ()),
    "gfx_restore": ("void", ()),
    "gfx_sound": ("void", ("int", "int", "int", "int", "int")),
}
LIBRARY_FUNCTIONS = {**SCREEN_LIBRARY_FUNCTIONS, **GRAPHICS_LIBRARY_FUNCTIONS}
LIBRARY_FUNCTIONS["abort"] = ("void", ())
LIBRARY_FUNCTIONS["exit"] = ("void", ("int",))
LIBRARY_FILES = {
    **{name: "lib_text.asm" for name in SCREEN_LIBRARY_FUNCTIONS},
    **{name: "lib_cgfx.asm" for name in GRAPHICS_LIBRARY_FUNCTIONS},
}
STANDARD_C_SOURCE = {
    **{
        name: "string.c"
        for name in (
            "memchr", "memcmp", "memcpy", "memmove", "memset", "strcat",
            "strchr", "strcmp", "strcoll", "strcpy", "strcspn", "strerror",
            "strlen", "strncat", "strncmp", "strncpy", "strpbrk", "strrchr",
            "strspn", "strstr", "strtok", "strxfrm",
        )
    },
    **{
        name: "ctype.c"
        for name in (
            "isalnum", "isalpha", "iscntrl", "isdigit", "isgraph", "islower",
            "isprint", "ispunct", "isspace", "isupper", "isxdigit",
            "tolower", "toupper",
        )
    },
    **{
        name: "stdlib.c"
        for name in (
            "abs", "labs", "atoi", "atol", "strtol", "strtoul", "rand", "srand",
            "getenv", "system",
        )
    },
    **{name: "locale.c" for name in ("setlocale", "localeconv")},
    **{name: "time.c" for name in ("clock", "time")},
}


ALU_NAMES = {"+": "ADD", "-": "SUB", "&": "AND", "|": "OR", "^": "XOR"}
SIGNED_BRANCHES = {"<": "BLTX", "<=": "BLEX", ">": "BGTX", ">=": "BGEX"}
UNSIGNED_BRANCHES = {"<": "BCX", "<=": "BLSX", ">": "BHIX", ">=": "BNCX"}
# Inverted conditions, used to branch when a comparison is false
SIGNED_INVERSE = {"<": "BGEX", "<=": "BGTX", ">": "BLEX", ">=": "BLTX"}
UNSIGNED_INVERSE = {"<": "BNCX", "<=": "BHIX", ">": "BLSX", ">=": "BCX"}

class CCompilerError(ValueError):
    """Raised for unsupported or invalid input in the supported C subset."""


@dataclass(frozen=True)
class Token:
    value: str
    line: int
    filename: str | None = None


@dataclass(frozen=True)
class _Macro:
    parameters: tuple[str, ...] | None
    replacement: str


@dataclass
class Expr:
    kind: str
    value: Any = None
    children: tuple["Expr", ...] = ()
    line: int = 0


@dataclass
class Local:
    name: str
    c_type: str
    initializer: Expr | None
    slot: int
    line: int
    array_size: int = 0


@dataclass
class Stmt:
    kind: str
    value: Any = None
    children: Sequence[Any] = field(default_factory=list)
    line: int = 0


@dataclass
class Function:
    name: str
    return_type: str
    parameters: list[tuple[str, str | None]]
    body: Stmt
    local_count: int
    line: int


@dataclass
class Global:
    name: str
    c_type: str
    initializer: Expr | None
    line: int
    array_size: int = 0


@dataclass(frozen=True)
class _Field:
    name: str
    c_type: str
    offset: int
    array_size: int = 0


@dataclass
class _Record:
    tag: str
    is_union: bool
    fields: dict[str, _Field] = field(default_factory=dict)
    size: int = 0
    complete: bool = False


_MULTI_OPERATORS = (
    "<<=", ">>=", "++", "--", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=",
    "/=", "%=", "&=", "|=", "^=", "<<", ">>", "##", "->",
)
_SINGLE_OPERATORS = set("{}();,:.[]=+-*/%<>!~&|^#?")
_STORAGE_WORDS = {"static", "extern", "register", "auto", "const", "volatile"}
_TYPE_WORDS = {"int", "void", "char", "short", "long", "float", "double", "signed", "unsigned"}
_PRECEDENCE = {
    ",": 0,
    "||": 1,
    "&&": 2,
    "|": 3,
    "^": 4,
    "&": 5,
    "==": 6,
    "!=": 6,
    "<": 7,
    "<=": 7,
    ">": 7,
    ">=": 7,
    "<<": 8,
    ">>": 8,
    "+": 9,
    "-": 9,
    "*": 10,
    "/": 10,
    "%": 10,
}
_ASSIGNMENTS = {
    "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=",
}


def _type_size(c_type: str, records: dict[str, _Record] | None = None) -> int:
    if c_type.endswith("*"):
        return 3
    if records is not None and c_type in records:
        return records[c_type].size
    return 1 if c_type in ("char", "unsigned char") else 2


def _object_size(c_type: str, records: dict[str, _Record] | None = None) -> int:
    if c_type.endswith("*"):
        return 3
    if records is not None and c_type in records:
        return records[c_type].size
    return 2


def _lex_raw(source: str, filename: str) -> list[Token]:
    tokens: list[Token] = []
    position = 0
    line = 1
    token_filename = filename
    while position < len(source):
        char = source[position]
        if char.isspace():
            if char == "\n":
                line += 1
            position += 1
            continue
        if source.startswith("\0SC16_LINE:", position):
            end = source.find("\n", position)
            marker = source[position + len("\0SC16_LINE:"):end]
            fields = marker.split(":", 1)
            if not fields[0].isdigit() or int(fields[0]) < 1:
                raise CCompilerError(f"{filename}:{line}: invalid internal line marker")
            line = int(fields[0]) - 1
            if len(fields) == 2 and fields[1]:
                try:
                    token_filename = bytes.fromhex(fields[1]).decode("utf-8")
                except (ValueError, UnicodeError) as exc:
                    raise CCompilerError(
                        f"{filename}:{line}: invalid internal filename marker"
                    ) from exc
            position = end if end >= 0 else len(source)
            continue
        if source.startswith("//", position):
            end = source.find("\n", position)
            position = len(source) if end < 0 else end
            continue
        if source.startswith("/*", position):
            end = source.find("*/", position + 2)
            if end < 0:
                raise CCompilerError(f"{token_filename}:{line}: unterminated comment")
            line += source.count("\n", position, end + 2)
            position = end + 2
            continue
        float_match = re.match(
            r"(?:[0-9]+\.[0-9]*|\.[0-9]+)(?:[eE][+-]?[0-9]+)?[fFlL]?"
            r"|[0-9]+[eE][+-]?[0-9]+[fFlL]?",
            source[position:],
        )
        if float_match is not None:
            literal = float_match.group(0)
            tokens.append(Token(literal, line, token_filename))
            position += len(literal)
            continue
        if char == '"':
            quote_start = position
            quote_line = line
            position += 1
            escaped = False
            while position < len(source):
                current = source[position]
                if current == "\n" and not escaped:
                    raise CCompilerError(
                        f"{token_filename}:{quote_line}: newline in string literal"
                    )
                if escaped:
                    escaped = False
                elif current == "\\":
                    escaped = True
                elif current == '"':
                    position += 1
                    break
                position += 1
            else:
                raise CCompilerError(
                    f"{token_filename}:{quote_line}: unterminated string literal"
                )
            try:
                value = ast.literal_eval(source[quote_start:position])
            except (SyntaxError, ValueError) as exc:
                raise CCompilerError(
                    f"{token_filename}:{quote_line}: invalid string literal"
                ) from exc
            if not isinstance(value, str):
                raise CCompilerError(
                    f"{token_filename}:{quote_line}: invalid string literal"
                )
            tokens.append(Token(
                f"<string:{value.encode('utf-8').hex()}>", quote_line, token_filename
            ))
            continue
        if char == "'":
            quote = char
            start = position
            start_line = line
            position += 1
            escaped = False
            while position < len(source):
                current = source[position]
                if current == "\n" and not escaped:
                    raise CCompilerError(
                        f"{token_filename}:{start_line}: newline in character/string literal"
                    )
                if escaped:
                    escaped = False
                elif current == "\\":
                    escaped = True
                elif current == quote:
                    position += 1
                    break
                if current == "\n":
                    line += 1
                position += 1
            else:
                raise CCompilerError(
                    f"{token_filename}:{start_line}: unterminated character/string literal"
                )
            literal = source[start:position]
            try:
                value = ast.literal_eval(literal)
            except (SyntaxError, ValueError) as exc:
                raise CCompilerError(
                    f"{token_filename}:{start_line}: invalid character constant"
                ) from exc
            if not isinstance(value, str) or len(value) != 1:
                raise CCompilerError(
                    f"{token_filename}:{start_line}: character constants must contain one character"
                )
            tokens.append(Token(str(ord(value)), start_line, token_filename))
            continue
        if char.isalpha() or char == "_":
            start = position
            position += 1
            while position < len(source) and (
                source[position].isalnum() or source[position] == "_"
            ):
                position += 1
            tokens.append(Token(source[start:position], line, token_filename))
            continue
        if char.isdigit():
            start = position
            position += 1
            while position < len(source) and (
                source[position].isalnum() or source[position] == "_"
            ):
                position += 1
            tokens.append(Token(source[start:position], line, token_filename))
            continue
        operator = next(
            (candidate for candidate in _MULTI_OPERATORS
             if source.startswith(candidate, position)),
            None,
        )
        if operator is not None:
            tokens.append(Token(operator, line, token_filename))
            position += len(operator)
        elif char in _SINGLE_OPERATORS:
            tokens.append(Token(char, line, token_filename))
            position += 1
        else:
            raise CCompilerError(
                f"{token_filename}:{line}: unsupported character {char!r}"
            )
    tokens.append(Token("<eof>", line, token_filename))
    return tokens


class _Parser:
    def __init__(self, source: str, filename: str):
        self.filename = filename
        self.tokens = _lex(source, filename)
        self.position = 0
        self.globals: list[Global] = []
        self.functions: list[Function] = []
        self.prototypes: dict[
            str, tuple[str, int | None, tuple[str, ...] | None]
        ] = {}
        self._local_count = 0
        self.typedef_scopes: list[dict[str, str]] = [{}]
        self.enum_constants: dict[str, int] = {}
        self.enum_tags: set[str] = set()
        self.records: dict[str, _Record] = {}
        self.global_declarations: dict[str, Global] = {}
        self._anonymous_record_index = 0

    @property
    def token(self) -> Token:
        token = self.tokens[self.position]
        if token.filename is not None:
            self.filename = token.filename
        return token

    def peek(self, distance: int = 1) -> str:
        index = min(self.position + distance, len(self.tokens) - 1)
        return self.tokens[index].value

    def fail(self, message: str) -> CCompilerError:
        return CCompilerError(f"{self.filename}:{self.token.line}: {message}")

    def take(self) -> Token:
        token = self.token
        self.position += 1
        if token.filename is not None:
            self.filename = token.filename
        return token

    def accept(self, value: str) -> bool:
        if self.token.value == value:
            self.take()
            return True
        return False

    def expect(self, value: str) -> Token:
        if self.token.value != value:
            raise self.fail(f"expected {value!r}, got {self.token.value!r}")
        return self.take()

    def parse_type(self) -> str:
        if self.token.value == "enum":
            self.take()
            tag = None
            if re.fullmatch(r"[A-Za-z_]\w*", self.token.value):
                tag = self.take().value
            if self.accept("{"):
                if tag is not None:
                    self.enum_tags.add(tag)
                value = -1
                while self.token.value != "}":
                    name = self.expect_identifier("expected an enumerator name")
                    value += 1
                    if self.accept("="):
                        value = self._enum_constant_value(self.parse_expression(1))
                    if not -0x8000 <= value <= 0x7FFF:
                        raise CCompilerError(
                            f"{self.filename}:{name.line}: enumerator value is not "
                            "representable as int"
                        )
                    if name.value in self.enum_constants:
                        raise CCompilerError(
                            f"{self.filename}:{name.line}: duplicate enumerator {name.value!r}"
                        )
                    self.enum_constants[name.value] = value
                    if not self.accept(","):
                        break
                    if self.token.value == "}":
                        break
                self.expect("}")
            elif tag is None or tag not in self.enum_tags:
                raise self.fail("enum reference requires a previously declared tag")
            return "int"
        if self.token.value in ("struct", "union"):
            return self._parse_record_specifier()
        for scope in reversed(self.typedef_scopes):
            if self.token.value in scope:
                return scope[self.take().value]
        unsigned = False
        signed = False
        short = False
        long = False
        base = None
        while self.token.value in _STORAGE_WORDS | _TYPE_WORDS:
            if self.token.value == "unsigned":
                if unsigned:
                    raise self.fail("duplicate type specifier 'unsigned'")
                unsigned = True
            elif self.token.value == "signed":
                if signed:
                    raise self.fail("duplicate type specifier 'signed'")
                signed = True
            elif self.token.value in _TYPE_WORDS - {"signed", "unsigned"}:
                current = self.take().value
                if current == "short":
                    if short:
                        raise self.fail("duplicate type specifier 'short'")
                    short = True
                elif current == "long":
                    if long:
                        raise self.fail("duplicate type specifier 'long'")
                    long = True
                elif base is not None:
                    raise self.fail("invalid combination of type specifiers")
                else:
                    base = current
                continue
            self.take()
        if short and long:
            raise self.fail("short and long cannot be combined")
        if base is None and (signed or unsigned or short or long):
            base = "int"
        if base is None:
            raise self.fail("expected int or void type")
        if signed and unsigned:
            raise self.fail("both signed and unsigned were specified")
        if base == "void":
            if unsigned or signed or short or long:
                raise self.fail("void cannot be combined with other type specifiers")
            return "void"
        if base in ("float", "double"):
            if unsigned or signed or short or (long and base != "double"):
                raise self.fail("invalid combination of floating type specifiers")
            return "float"
        if base == "char" and (short or long):
            raise self.fail("char cannot be combined with short or long")
        if base == "char":
            return "unsigned char" if unsigned else "char"
        return "unsigned int" if unsigned else "int"

    def _parse_record_specifier(self) -> str:
        keyword = self.take().value
        is_union = keyword == "union"
        tag = None
        if re.fullmatch(r"[A-Za-z_]\w*", self.token.value):
            tag = self.take().value
        if tag is None:
            self._anonymous_record_index += 1
            tag = f"__anonymous_{self._anonymous_record_index}"
        record_type = f"{keyword} {tag}"
        record = self.records.setdefault(record_type, _Record(record_type, is_union))
        if self.accept("{"):
            if record.complete:
                raise self.fail(f"redefinition of {record_type}")
            fields: dict[str, _Field] = {}
            offset = 0
            maximum_size = 0
            while self.token.value != "}":
                if self.token.value == "<eof>":
                    raise self.fail(f"unterminated {record_type} definition")
                field_type = self.parse_type()
                while True:
                    declarator_type = self._pointer_type(field_type)
                    field_name = self.expect_identifier("expected a field name")
                    declarator_type, array_size = self._declarator_suffix(declarator_type)
                    if array_size == -1:
                        raise CCompilerError(
                            f"{self.filename}:{field_name.line}: record array fields require a bound"
                        )
                    if declarator_type == "void":
                        raise CCompilerError(
                            f"{self.filename}:{field_name.line}: fields cannot have void type"
                        )
                    if declarator_type in self.records and not self.records[declarator_type].complete:
                        raise CCompilerError(
                            f"{self.filename}:{field_name.line}: field has incomplete type"
                        )
                    field_width = (
                        array_size * _type_size(declarator_type, self.records)
                        if array_size else _type_size(declarator_type, self.records)
                    )
                    if not is_union and _type_size(declarator_type, self.records) > 1:
                        offset = (offset + 1) & ~1
                    field_offset = 0 if is_union else offset
                    if field_name.value in fields:
                        raise CCompilerError(
                            f"{self.filename}:{field_name.line}: duplicate field "
                            f"{field_name.value!r}"
                        )
                    fields[field_name.value] = _Field(
                        field_name.value, declarator_type, field_offset, array_size
                    )
                    if is_union:
                        maximum_size = max(maximum_size, field_width)
                    else:
                        offset += field_width
                    if not self.accept(","):
                        break
                self.expect(";")
            self.expect("}")
            size = maximum_size if is_union else offset
            record.fields = fields
            record.size = (size + 1) & ~1
            record.complete = True
        elif not record.complete and tag.startswith("__anonymous_"):
            raise self.fail("anonymous record references are invalid")
        return record_type

    def expect_identifier(self, message: str) -> Token:
        if not re.fullmatch(r"[A-Za-z_]\w*", self.token.value):
            raise self.fail(message)
        return self.take()

    def _enum_constant_value(self, expression: Expr) -> int:
        if expression.kind == "constant":
            return int(expression.value)
        if expression.kind == "name" and expression.value in self.enum_constants:
            return self.enum_constants[str(expression.value)]
        if expression.kind == "sizeof_type":
            return _type_size(str(expression.value), self.records)
        if expression.kind == "cast" and expression.value in (
            "char", "unsigned char", "int", "unsigned int"
        ):
            return self._enum_constant_value(expression.children[0])
        if expression.kind == "unary":
            value = self._enum_constant_value(expression.children[0])
            operator = str(expression.value)
            if operator == "+":
                return value
            if operator == "-":
                return -value
            if operator == "~":
                return ~value
            if operator == "!":
                return int(not value)
        if expression.kind == "conditional":
            condition = self._enum_constant_value(expression.children[0])
            selected = expression.children[1] if condition else expression.children[2]
            return self._enum_constant_value(selected)
        if expression.kind == "binary":
            left = self._enum_constant_value(expression.children[0])
            if expression.value == "&&" and not left:
                return 0
            if expression.value == "||" and left:
                return 1
            right = self._enum_constant_value(expression.children[1])
            operations = {
                "+": lambda: left + right, "-": lambda: left - right,
                "*": lambda: left * right,
                "/": lambda: abs(left) // abs(right) * (-1 if (left < 0) != (right < 0) else 1),
                "%": lambda: left - (
                    abs(left) // abs(right) * (-1 if (left < 0) != (right < 0) else 1)
                ) * right,
                "<<": lambda: left << right,
                ">>": lambda: left >> right, "&": lambda: left & right,
                "|": lambda: left | right, "^": lambda: left ^ right,
                "==": lambda: int(left == right), "!=": lambda: int(left != right),
                "<": lambda: int(left < right), "<=": lambda: int(left <= right),
                ">": lambda: int(left > right), ">=": lambda: int(left >= right),
                "&&": lambda: int(bool(left) and bool(right)),
                "||": lambda: int(bool(left) or bool(right)),
            }
            if expression.value in operations:
                try:
                    return operations[str(expression.value)]()
                except (ZeroDivisionError, ValueError, OverflowError) as exc:
                    raise self.fail("invalid enum constant expression") from exc
        raise self.fail("expression is not an integer constant expression")

    def _pointer_type(self, c_type: str) -> str:
        pointer_count = 0
        while self.accept("*"):
            pointer_count += 1
        return c_type + ("*" * pointer_count)

    def _declarator_suffix(self, c_type: str) -> tuple[str, int]:
        if not self.accept("["):
            return c_type, 0
        if self.accept("]"):
            return c_type, -1
        expression = self.parse_expression()
        self.expect("]")
        try:
            size = self._enum_constant_value(expression)
        except CCompilerError as exc:
            raise CCompilerError(
                f"{self.filename}:{expression.line}: array size must be a positive "
                "integer constant expression"
            ) from exc
        if size <= 0 or size > 0xFFFF:
            raise CCompilerError(
                f"{self.filename}:{expression.line}: array size is out of range"
            )
        return c_type, size

    def _infer_array_size(
        self,
        array_size: int,
        c_type: str,
        initializer: Expr | None,
        line: int,
        *,
        file_scope: bool = False,
    ) -> int:
        if array_size != -1:
            return array_size
        if initializer is None:
            if file_scope:
                return 1
            raise CCompilerError(
                f"{self.filename}:{line}: an array with omitted size requires an initializer"
            )
        if initializer.kind == "string":
            if c_type not in ("char", "unsigned char"):
                raise CCompilerError(
                    f"{self.filename}:{line}: string initializer requires a character array"
                )
            return len(initializer.value)
        if initializer.kind == "init_list":
            if not initializer.children:
                raise CCompilerError(
                    f"{self.filename}:{line}: an array initializer cannot be empty"
                )
            return len(initializer.children)
        raise CCompilerError(
            f"{self.filename}:{line}: an array with omitted size requires an initializer"
        )

    def _initializer(self) -> Expr:
        if self.accept("{"):
            values = []
            while self.token.value != "}":
                values.append(self._initializer())
                if not self.accept(","):
                    break
            self.expect("}")
            return Expr("init_list", children=tuple(values), line=self.token.line)
        return self.parse_expression(1)

    def parse(self) -> tuple[list[Global], list[Function]]:
        while self.token.value != "<eof>":
            self.parse_external_declaration()
        if not any(function.name == "main" for function in self.functions):
            raise CCompilerError(f"{self.filename}: no definition for main()")
        return self.globals, self.functions

    def parse_external_declaration(self) -> None:
        is_typedef = self.accept("typedef")
        implicit_int_function = (
            not is_typedef
            and re.fullmatch(r"[A-Za-z_]\w*", self.token.value) is not None
            and self.peek() == "("
            and not any(self.token.value in scope for scope in self.typedef_scopes)
        )
        c_type = "int" if implicit_int_function else self.parse_type()
        if is_typedef:
            self.parse_typedef_declarators(c_type)
            return
        if self.accept(";"):
            return
        declarator_type = self._pointer_type(c_type)
        name_token = self.take()
        if not re.fullmatch(r"[A-Za-z_]\w*", name_token.value):
            raise self.fail("expected an external identifier")
        name = name_token.value
        if self.accept("("):
            empty_parameter_list = self.token.value == ")"
            old_style_names = None
            if (
                re.fullmatch(r"[A-Za-z_]\w*", self.token.value)
                and self.token.value not in _STORAGE_WORDS | _TYPE_WORDS
                and self.token.value not in ("struct", "union", "enum")
                and not any(self.token.value in scope for scope in self.typedef_scopes)
            ):
                old_style_names = [self.take().value]
                while self.accept(","):
                    old_style_names.append(
                        self.expect_identifier("expected an old-style parameter name").value
                    )
                if len(set(old_style_names)) != len(old_style_names):
                    raise self.fail("duplicate old-style parameter name")
                self.expect(")")
                parameters = [("int", parameter_name) for parameter_name in old_style_names]
            else:
                parameters = self.parse_parameters()
            if self.accept(";"):
                if implicit_int_function:
                    raise CCompilerError(
                        f"{self.filename}:{name_token.line}: implicit int is only "
                        "supported for function definitions"
                    )
                if old_style_names is not None:
                    raise CCompilerError(
                        f"{self.filename}:{name_token.line}: identifier-list function "
                        "declarators are only valid in definitions"
                    )
                signature = (
                    declarator_type,
                    None if empty_parameter_list else len(parameters),
                    None if empty_parameter_list else tuple(
                        parameter_type for parameter_type, _ in parameters
                    ),
                )
                self._merge_function_declaration(name, signature, name_token.line)
                return
            if self.token.value != "{" and old_style_names is None:
                raise self.fail("expected ';' or function body")
            if any(function.name == name for function in self.functions):
                raise CCompilerError(
                    f"{self.filename}:{name_token.line}: duplicate definition of {name}"
                )
            if old_style_names is not None:
                parameters = self.parse_old_style_parameter_declarations(old_style_names)
            signature = (
                declarator_type,
                len(parameters),
                tuple(parameter_type for parameter_type, _ in parameters),
            )
            self._merge_function_declaration(
                name,
                signature,
                name_token.line,
                old_style=old_style_names is not None,
            )
            self._local_count = 0
            body = self.parse_compound()
            if any(param_name is None for _, param_name in parameters):
                raise CCompilerError(
                    f"{self.filename}:{name_token.line}: function definitions require parameter names"
                )
            self.functions.append(
                Function(name, declarator_type, parameters, body, self._local_count, name_token.line)
            )
            return
        self.parse_global_declarators(c_type, name_token, declarator_type)

    def _merge_function_declaration(
        self,
        name: str,
        signature: tuple[str, int | None, tuple[str, ...] | None],
        line: int,
        *,
        old_style: bool = False,
    ) -> None:
        previous = self.prototypes.get(name)
        if previous is not None and (
            previous[0] != signature[0]
            or (
                previous[1] is not None
                and signature[1] is not None
                and previous[1] != signature[1]
            )
            or (
                previous[2] is not None
                and signature[2] is not None
                and previous[2] != (
                    tuple(
                        self._default_argument_promotion(parameter_type)
                        for parameter_type in signature[2]
                    )
                    if old_style else signature[2]
                )
            )
        ):
            raise CCompilerError(
                f"{self.filename}:{line}: conflicting declaration of {name}"
            )
        if previous is None or signature[1] is not None:
            self.prototypes[name] = signature

    @staticmethod
    def _default_argument_promotion(c_type: str) -> str:
        if c_type in ("char", "unsigned char"):
            return "int"
        return c_type

    def parse_old_style_parameter_declarations(
        self, names: list[str]
    ) -> list[tuple[str, str]]:
        declared = {name: "int" for name in names}
        explicitly_declared: set[str] = set()
        while self.token.value != "{":
            c_type = self.parse_type()
            while True:
                parameter_type = self._pointer_type(c_type)
                token = self.expect_identifier("expected an old-style parameter name")
                parameter_type, array_size = self._declarator_suffix(parameter_type)
                if array_size:
                    parameter_type += "*"
                if token.value not in declared:
                    raise CCompilerError(
                        f"{self.filename}:{token.line}: declaration for parameter "
                        f"{token.value!r} is not in the identifier list"
                    )
                if token.value in explicitly_declared:
                    raise CCompilerError(
                        f"{self.filename}:{token.line}: duplicate declaration for "
                        f"parameter {token.value!r}"
                    )
                if parameter_type == "void":
                    raise CCompilerError(
                        f"{self.filename}:{token.line}: parameter cannot have void type"
                    )
                declared[token.value] = parameter_type
                explicitly_declared.add(token.value)
                if not self.accept(","):
                    break
            self.expect(";")
            if self.token.value == "<eof>":
                raise self.fail("expected function body after old-style parameter declarations")
        return [(declared[name], name) for name in names]

    def parse_typedef_declarators(self, c_type: str) -> None:
        while True:
            alias_type = self._pointer_type(c_type)
            token = self.expect_identifier("expected a typedef name")
            alias_type, array_size = self._declarator_suffix(alias_type)
            if array_size:
                raise CCompilerError(
                    f"{self.filename}:{token.line}: array typedefs are not supported"
                )
            self.typedef_scopes[-1][token.value] = alias_type
            if not self.accept(","):
                break
        self.expect(";")

    def parse_parameters(self) -> list[tuple[str, str | None]]:
        if self.accept(")"):
            return []
        if self.token.value == "void" and self.peek() == ")":
            self.take()
            self.expect(")")
            return []
        parameters = []
        while True:
            c_type = self.parse_type()
            c_type = self._pointer_type(c_type)
            if c_type == "void":
                raise self.fail("void is only valid as an empty parameter list")
            name = None
            if re.fullmatch(r"[A-Za-z_]\w*", self.token.value):
                name = self.take().value
            _, array_size = self._declarator_suffix(c_type)
            if array_size != 0:
                c_type += "*"
            parameters.append((c_type, name))
            if not self.accept(","):
                break
        self.expect(")")
        return parameters

    def parse_global_declarators(
        self, c_type: str, first_name: Token, first_type: str
    ) -> None:
        name_token = first_name
        current_type = first_type
        while True:
            name = name_token.value
            current_type, array_size = self._declarator_suffix(current_type)
            if current_type == "void":
                raise CCompilerError(
                    f"{self.filename}:{name_token.line}: objects cannot have void type"
                )
            if current_type in self.records and not self.records[current_type].complete:
                raise CCompilerError(
                    f"{self.filename}:{name_token.line}: object has incomplete record type"
                )
            initializer = self._initializer() if self.accept("=") else None
            array_size = self._infer_array_size(
                array_size, current_type, initializer, name_token.line, file_scope=True
            )
            declaration = self.global_declarations.get(name)
            if declaration is None:
                declaration = Global(
                    name, current_type, initializer, name_token.line, array_size
                )
                self.global_declarations[name] = declaration
                self.globals.append(declaration)
            elif (
                declaration.c_type != current_type
                or declaration.array_size != array_size
            ):
                raise CCompilerError(
                    f"{self.filename}:{name_token.line}: conflicting declaration "
                    f"of global variable {name!r}"
                )
            elif initializer is not None:
                if declaration.initializer is not None:
                    raise CCompilerError(
                        f"{self.filename}:{name_token.line}: multiple definitions "
                        f"of global variable {name!r}"
                    )
                declaration.initializer = initializer
            if not self.accept(","):
                break
            current_type = self._pointer_type(c_type)
            name_token = self.take()
            if not re.fullmatch(r"[A-Za-z_]\w*", name_token.value):
                raise self.fail("expected a global variable name")
        self.expect(";")

    def _is_declaration(self) -> bool:
        return (
            self.token.value in _STORAGE_WORDS | _TYPE_WORDS | {
                "enum", "struct", "union", "typedef",
            }
            or any(self.token.value in scope for scope in self.typedef_scopes)
        )

    def parse_compound(self) -> Stmt:
        brace = self.expect("{")
        statements: list[Stmt] = []
        declarations_allowed = True
        self.typedef_scopes.append({})
        while self.token.value != "}":
            if self.token.value == "<eof>":
                raise self.fail("unterminated compound statement")
            if self._is_declaration():
                if not declarations_allowed:
                    raise self.fail("C90 declarations must precede statements in a block")
                statements.extend(self.parse_local_declarations())
            else:
                declarations_allowed = False
                statements.append(self.parse_statement())
        self.take()
        self.typedef_scopes.pop()
        return Stmt("block", children=statements, line=brace.line)

    def parse_local_declarations(self) -> list[Stmt]:
        is_typedef = self.accept("typedef")
        c_type = self.parse_type()
        if is_typedef:
            self.parse_typedef_declarators(c_type)
            return []
        if self.accept(";"):
            return []
        declarations = []
        while True:
            current_type = self._pointer_type(c_type)
            token = self.take()
            if not re.fullmatch(r"[A-Za-z_]\w*", token.value):
                raise self.fail("expected a local variable name")
            current_type, array_size = self._declarator_suffix(current_type)
            if current_type == "void":
                raise self.fail("local objects cannot have void type")
            if current_type in self.records and not self.records[current_type].complete:
                raise CCompilerError(
                    f"{self.filename}:{token.line}: object has incomplete record type"
                )
            initializer = self._initializer() if self.accept("=") else None
            array_size = self._infer_array_size(
                array_size, current_type, initializer, token.line
            )
            local = Local(
                token.value, current_type, initializer, self._local_count, token.line, array_size
            )
            self._local_count += (
                _object_size(current_type, self.records)
                if array_size == 0
                else array_size * _type_size(current_type, self.records)
            )
            declarations.append(Stmt("decl", value=local, line=token.line))
            if not self.accept(","):
                break
        self.expect(";")
        return declarations

    def parse_statement(self) -> Stmt:
        token = self.token
        if token.value == "{":
            return self.parse_compound()
        if self.accept(";"):
            return Stmt("empty", line=token.line)
        if self.accept("if"):
            self.expect("(")
            condition = self.parse_expression()
            self.expect(")")
            then_branch = self.parse_statement()
            else_branch = self.parse_statement() if self.accept("else") else None
            return Stmt("if", children=[condition, then_branch, else_branch], line=token.line)
        if self.accept("while"):
            self.expect("(")
            condition = self.parse_expression()
            self.expect(")")
            return Stmt("while", children=[condition, self.parse_statement()], line=token.line)
        if self.accept("do"):
            do_body = self.parse_statement()
            self.expect("while")
            self.expect("(")
            condition = self.parse_expression()
            self.expect(")")
            self.expect(";")
            return Stmt("do", children=[do_body, condition], line=token.line)
        if self.accept("for"):
            self.expect("(")
            initializer = None if self.token.value == ";" else self.parse_expression()
            self.expect(";")
            condition = None if self.token.value == ";" else self.parse_expression()
            self.expect(";")
            increment = None if self.token.value == ")" else self.parse_expression()
            self.expect(")")
            return Stmt(
                "for",
                children=[initializer, condition, increment, self.parse_statement()],
                line=token.line,
            )
        if self.accept("switch"):
            self.expect("(")
            condition = self.parse_expression()
            self.expect(")")
            self.expect("{")
            cases: list[tuple[Expr | None, list[Stmt], int]] = []
            seen_default = False
            while self.token.value != "}":
                case_token = self.token
                if self.accept("case"):
                    case_value = self.parse_expression(1)
                    self.expect(":")
                    try:
                        constant_value = self._enum_constant_value(case_value)
                    except CCompilerError as exc:
                        raise self.fail("case labels must be constant integer expressions")
                    case_value = Expr(
                        "constant", constant_value & WORD_MASK, line=case_value.line
                    )
                    body: list[Stmt] = []
                    while self.token.value not in ("case", "default", "}"):
                        body.append(self.parse_statement())
                    cases.append((case_value, body, case_token.line))
                elif self.accept("default"):
                    if seen_default:
                        raise CCompilerError(
                            f"{self.filename}:{case_token.line}: duplicate default label"
                        )
                    seen_default = True
                    self.expect(":")
                    body = []
                    while self.token.value not in ("case", "default", "}"):
                        body.append(self.parse_statement())
                    cases.append((None, body, case_token.line))
                else:
                    raise self.fail("expected case or default label in switch")
            self.expect("}")
            return Stmt("switch", value=cases, children=[condition], line=token.line)
        if self.accept("return"):
            result = None if self.token.value == ";" else self.parse_expression()
            self.expect(";")
            return Stmt("return", children=[result], line=token.line)
        if self.accept("break"):
            self.expect(";")
            return Stmt("break", line=token.line)
        if self.accept("continue"):
            self.expect(";")
            return Stmt("continue", line=token.line)
        if self.accept("goto"):
            label = self.expect_identifier("expected a label name after goto")
            self.expect(";")
            return Stmt("goto", value=label.value, line=token.line)
        if (
            re.fullmatch(r"[A-Za-z_]\w*", self.token.value)
            and self.peek() == ":"
        ):
            label = self.take()
            self.take()
            return Stmt("label", value=label.value, children=[self.parse_statement()], line=label.line)
        expression = self.parse_expression()
        self.expect(";")
        return Stmt("expression", children=[expression], line=token.line)

    def parse_expression(self, minimum: int = 0) -> Expr:
        left = self.parse_unary()
        while True:
            operator = self.token.value
            if operator in _ASSIGNMENTS and minimum <= 1:
                token = self.take()
                if left.kind != "name" and not (
                    left.kind in ("index", "member")
                    or (left.kind == "unary" and left.value == "*")
                ):
                    raise CCompilerError(
                        f"{self.filename}:{token.line}: assignment requires a scalar variable"
                    )
                right = self.parse_expression(1)
                left = Expr("assign", (operator, left), (right,), token.line)
                continue
            if operator == "?" and minimum <= 1:
                token = self.take()
                when_true = self.parse_expression()
                self.expect(":")
                when_false = self.parse_expression(1)
                left = Expr(
                    "conditional",
                    None,
                    (left, when_true, when_false),
                    token.line,
                )
                continue
            if operator not in _PRECEDENCE or _PRECEDENCE[operator] < minimum:
                break
            token = self.take()
            right = self.parse_expression(_PRECEDENCE[operator] + 1)
            left = Expr("binary", operator, (left, right), token.line)
        return left

    def parse_unary(self) -> Expr:
        token = self.token
        if self.accept("sizeof"):
            if self.token.value == "(":
                saved_position = self.position
                self.take()
                try:
                    c_type = self.parse_type()
                    c_type = self._pointer_type(c_type)
                    self.expect(")")
                except CCompilerError:
                    self.position = saved_position
                else:
                    return Expr("sizeof_type", c_type, line=token.line)
            return Expr("sizeof_expr", children=(self.parse_unary(),), line=token.line)
        if self.token.value == "(":
            saved_position = self.position
            self.take()
            try:
                c_type = self.parse_type()
                c_type = self._pointer_type(c_type)
                self.expect(")")
            except CCompilerError:
                self.position = saved_position
            else:
                child = self.parse_unary()
                return Expr("cast", c_type, (child,), token.line)
        if token.value in ("+", "-", "!", "~", "&", "*", "++", "--"):
            operator = self.take().value
            child = self.parse_unary()
            if operator in ("++", "--") and child.kind not in (
                "name", "index", "member",
            ) and not (child.kind == "unary" and child.value == "*"):
                raise CCompilerError(
                    f"{self.filename}:{token.line}: increment requires a scalar variable"
                )
            return Expr("unary", operator, (child,), token.line)
        expression = self.parse_primary()
        while True:
            if self.accept("["):
                index = self.parse_expression()
                self.expect("]")
                expression = Expr("index", None, (expression, index), expression.line)
            elif self.accept("("):
                if expression.kind not in ("name", "index", "member") and not (
                    expression.kind == "unary" and expression.value == "*"
                ):
                    raise CCompilerError(
                        f"{self.filename}:{expression.line}: only direct function calls are supported"
                    )
                arguments = []
                if self.token.value != ")":
                    while True:
                        arguments.append(self.parse_expression(1))
                        if not self.accept(","):
                            break
                self.expect(")")
                expression = Expr("call", expression.value, tuple(arguments), expression.line)
            elif self.token.value in ("++", "--"):
                operator = self.take()
                if expression.kind != "name":
                    raise CCompilerError(
                        f"{self.filename}:{operator.line}: increment requires a scalar variable"
                    )
                expression = Expr("postfix", operator.value, (expression,), operator.line)
            elif self.token.value in (".", "->"):
                operator = self.take()
                field_name = self.expect_identifier("expected a member name")
                expression = Expr(
                    "member", (operator.value, field_name.value), (expression,), operator.line
                )
            else:
                break
        return expression

    def parse_primary(self) -> Expr:
        token = self.take()
        if token.value == "(":
            expression = self.parse_expression()
            self.expect(")")
            return expression
        if re.fullmatch(r"[A-Za-z_]\w*", token.value):
            if token.value in {"sizeof", "struct", "union", "enum", "typedef"}:
                raise CCompilerError(
                    f"{self.filename}:{token.line}: {token.value} is not supported"
                )
            return Expr("name", token.value, line=token.line)
        if token.value[0].isdigit():
            float_match = re.fullmatch(
                r"(?:(?:[0-9]+\.[0-9]*|\.[0-9]+)(?:[eE][+-]?[0-9]+)?|[0-9]+[eE][+-]?[0-9]+)[fFlL]?",
                token.value,
            )
            if float_match is not None:
                numeric = token.value.rstrip("fFlL")
                try:
                    value = decimal.Decimal(numeric)
                    bits = struct.pack(">e", float(value))
                except (decimal.InvalidOperation, OverflowError) as exc:
                    raise CCompilerError(
                        f"{self.filename}:{token.line}: floating constant is out of range"
                    ) from exc
                return Expr("float_constant", int.from_bytes(bits, "big"), line=token.line)
            match = re.fullmatch(
                r"(0[xX][0-9a-fA-F]+|[0-9]+)(?:[uU]?[lL]?|[lL][uU]?)",
                token.value,
            )
            if match is None:
                raise CCompilerError(
                    f"{self.filename}:{token.line}: invalid integer constant {token.value!r}"
                )
            digits = match.group(1)
            base = 16 if digits.lower().startswith("0x") else (
                8 if len(digits) > 1 and digits.startswith("0") else 10
            )
            try:
                number = int(digits, base)
            except ValueError as exc:
                raise CCompilerError(
                    f"{self.filename}:{token.line}: invalid integer constant {token.value!r}"
                ) from exc
            if number > WORD_MASK:
                if digits.lower().startswith("0x") and number < (1 << 19):
                    return Expr("address_constant", number, line=token.line)
                raise CCompilerError(
                    f"{self.filename}:{token.line}: integer constant exceeds 16 bits"
                )
            return Expr("constant", number & WORD_MASK, line=token.line)
        if token.value.startswith("<string:") and token.value.endswith(">"):
            value = bytes.fromhex(token.value[8:-1]) + b"\0"
            return Expr("string", value, line=token.line)
        raise CCompilerError(
            f"{self.filename}:{token.line}: expected an expression, got {token.value!r}"
        )


@dataclass(frozen=True)
class _Variable:
    local_offset: int | None = None
    parameter_offset: int | None = None
    global_address: int | None = None
    c_type: str = "int"
    array_size: int = 0
    constant_value: int | None = None


class _CodeGenerator:
    def __init__(
        self,
        globals_: list[Global],
        functions: list[Function],
        filename: str,
        records: dict[str, _Record] | None = None,
    ):
        self.globals = globals_
        self.functions = functions
        self.filename = filename
        self.records = records or {}
        self.lines: list[str] = []
        self.label_index = 0
        self.stack_depth = 0
        self.frame_size = 0
        self.scopes: list[dict[str, _Variable]] = []
        self.break_labels: list[str] = []
        self._jump_false: str | None = None
        self._discard: Expr | None = None
        self._nomask: Expr | None = None
        self.continue_labels: list[str] = []
        self.used_helpers: set[str] = set()
        self.function_names = {function.name for function in functions}
        self.functions_by_name = {function.name: function for function in functions}
        self.global_addresses: dict[str, int] = {}
        self.global_variables: dict[str, Global] = {}
        self.string_data: list[tuple[str, bytes]] = []
        self.current_function: Function | None = None
        self.prototypes: dict[
            str, tuple[str, int | None, tuple[str, ...] | None]
        ] = {}
        self.used_libraries: set[str] = set()
        self.enum_constants: dict[str, int] = {}
        self.label_names: set[str] = set()

    def emit(self, line: str) -> None:
        self.lines.append(line)

    def label(self, prefix: str = "L") -> str:
        self.label_index += 1
        return f"__{prefix}_{self.label_index}"

    def error(self, line: int, message: str) -> CCompilerError:
        return CCompilerError(f"{self.filename}:{line}: {message}")

    def generate(self) -> str:
        for function in self.functions:
            if function.name in LIBRARY_FUNCTIONS:
                raise self.error(
                    function.line,
                    f"{function.name} is reserved by the standard library",
                )
        self._allocate_globals()
        self._collect_string_literals()
        self.emit(".address 0")
        self.emit("__c_entry:")
        self.emit("    JMP __c_boot")
        self.emit(".address 6")
        self.emit("__c_boot:")
        self.emit("    CALLX __c_function_main")
        self.emit("    HALT")
        self.emit(f".address 0x{CODE_BASE:04X}")
        for function in self.functions:
            self._generate_function(function)
        for helper in sorted(self.used_helpers):
            self._generate_helper(helper)
        self.emit(f".address 0x{GLOBAL_BASE:04X}")
        for global_ in self.globals:
            address = self.global_addresses[global_.name]
            if address == GLOBAL_BASE:
                self.emit(f"__c_global_{global_.name}:")
            self._emit_global(global_)
        if self.string_data:
            cursor = (GLOBAL_BASE + sum(self._global_size(item) for item in self.globals) + 1) & ~1
            self.emit(f".address 0x{cursor:04X}")
            for label, data in self.string_data:
                self.emit(f"{label}:")
                self._emit_bytes(data)
                cursor += len(data)
        source_directory = Path(self.filename).resolve().parent
        libraries = sorted(self.used_libraries)
        if "lib_cgfx.asm" in libraries and "lib_text.asm" not in libraries:
            libraries.append("fonts/font5x7.asm")
        for library in libraries:
            if "font5x7.asm" in library:
                library_path = Path(__file__).resolve().parent.parent / "fonts" / Path(library).name
            else:
                library_path = Path(__file__).resolve().parent.parent / "lib" / Path(library).name
            include_path = os.path.relpath(library_path, source_directory).replace("\\", "/")
            self.emit(f'.include "{include_path}"')
        return "\n".join(self.lines) + "\n"

    def _global_size(self, global_: Global) -> int:
        if global_.array_size:
            return global_.array_size * _type_size(global_.c_type, self.records)
        return _type_size(global_.c_type, self.records)

    def _allocate_globals(self) -> None:
        address = GLOBAL_BASE
        for global_ in self.globals:
            if global_.name in self.global_addresses:
                raise self.error(global_.line, f"duplicate global variable {global_.name!r}")
            if global_.name in self.function_names:
                raise self.error(global_.line, f"{global_.name!r} names both a function and object")
            self.global_addresses[global_.name] = address
            self.global_variables[global_.name] = global_
            address += self._global_size(global_)
        if address > 0x10000:
            raise CCompilerError(f"{self.filename}: global variables exceed 16-bit addressable data")

    def _collect_string_literals(self) -> None:
        def visit(expression: Expr | None) -> None:
            if expression is None:
                return
            if expression.kind == "string":
                data = expression.value
                label = f"__c_string_{len(self.string_data)}"
                self.string_data.append((label, data))
                expression.value = label
            for child in expression.children:
                visit(child)

        def visit_statement(statement: Stmt) -> None:
            if statement.kind == "decl":
                local = statement.value
                if not (
                    local.array_size
                    and local.initializer is not None
                    and local.initializer.kind == "string"
                ):
                    visit(local.initializer)
            for child in statement.children:
                if isinstance(child, Stmt):
                    visit_statement(child)
                elif isinstance(child, Expr):
                    visit(child)
            if statement.kind == "switch":
                for _, body, _ in statement.value:
                    for child in body:
                        visit_statement(child)

        for function in self.functions:
            visit_statement(function.body)

    def _emit_bytes(self, data: bytes) -> None:
        for start in range(0, len(data), 16):
            values = ", ".join(f"{byte:02X}" for byte in data[start:start + 16])
            self.emit(f".data hex {values}")

    def _emit_global(self, global_: Global) -> None:
        data = self._serialize_initializer(
            global_.c_type, global_.array_size, global_.initializer, global_.line
        )
        self._emit_bytes(data)

    def _serialize_initializer(
        self,
        c_type: str,
        array_size: int,
        initializer: Expr | None,
        line: int,
    ) -> bytes:
        if array_size:
            size = array_size * _type_size(c_type, self.records)
            if initializer is None:
                return bytes(size)
            if initializer.kind == "string":
                if c_type not in ("char", "unsigned char"):
                    raise self.error(line, "string initializer requires a char array")
                data = bytes(initializer.value)
                if len(data) > size:
                    raise self.error(line, "string initializer is too long for array")
                return data + bytes(size - len(data))
            if initializer.kind != "init_list":
                raise self.error(line, "array initializer must be a brace list or string")
            if len(initializer.children) > array_size:
                raise self.error(line, "too many elements in array initializer")
            values = [
                self._serialize_initializer(c_type, 0, child, line)
                for child in initializer.children
            ]
            return b"".join(values) + bytes(size - sum(map(len, values)))
        if c_type in self.records:
            record = self.records[c_type]
            if not record.complete:
                raise self.error(line, "initializer has incomplete record type")
            data = bytearray(record.size)
            if initializer is None:
                return bytes(data)
            if initializer.kind != "init_list":
                raise self.error(line, "record initializer must be a brace list")
            fields = list(record.fields.values())
            if len(initializer.children) > len(fields) or (
                record.is_union and len(initializer.children) > 1
            ):
                raise self.error(line, "too many values in record initializer")
            for field_, value in zip(fields, initializer.children):
                field_data = self._serialize_initializer(
                    field_.c_type, field_.array_size, value, line
                )
                data[field_.offset:field_.offset + len(field_data)] = field_data
            return bytes(data)
        if initializer is None:
            return bytes(_type_size(c_type, self.records))
        if initializer.kind == "string":
            raise self.error(line, "a string literal cannot initialize a scalar")
        if c_type.endswith("*"):
            is_address_constant = initializer.kind == "address_constant"
            is_null_pointer_constant = (
                self._is_integer_constant_expression(initializer)
                and self._constant(initializer) == 0
            )
            if not is_address_constant and not is_null_pointer_constant:
                raise self.error(
                    line,
                    "pointer initializer must be an address constant or null pointer constant",
                )
        value = self._constant(initializer)
        size = _type_size(c_type, self.records)
        if c_type.endswith("*") and not 0 <= value < (1 << 19):
            raise self.error(line, "pointer initializer is outside the 19-bit address range")
        if c_type.endswith("*"):
            return (value & WORD_MASK).to_bytes(2, "big") + bytes((value >> 16,))
        return (value & ((1 << (size * 8)) - 1)).to_bytes(size, "big")

    def _constant(self, expression: Expr | None) -> int:
        if expression is None:
            return 0
        if expression.kind == "constant":
            return int(expression.value) & WORD_MASK
        if expression.kind == "name" and str(expression.value) in self.enum_constants:
            return self.enum_constants[str(expression.value)] & WORD_MASK
        if expression.kind == "sizeof_type":
            return _type_size(str(expression.value), self.records)
        if expression.kind == "sizeof_expr":
            return self._sizeof_expression(expression.children[0])
        if expression.kind == "conditional":
            condition = self._constant(expression.children[0])
            return self._constant(expression.children[1] if condition else expression.children[2])
        if (
            expression.kind == "cast"
            and expression.children[0].kind == "unary"
            and expression.children[0].value == "&"
        ):
            return self._constant(expression.children[0])
        if expression.kind == "address_constant":
            return int(expression.value)
        if expression.kind == "float_constant":
            return int(expression.value) & WORD_MASK
        if expression.kind == "unary":
            if expression.value == "&" and expression.children[0].kind == "member":
                return self._constant_member_offset(
                    expression.children[0], expression.line
                )
            value = self._constant(expression.children[0])
            operator = str(expression.value)
            if operator == "+":
                return value
            if operator == "-":
                return -value & WORD_MASK
            if operator == "~":
                return ~value & WORD_MASK
            if operator == "!":
                return int(value == 0)
        if expression.kind == "binary":
            left = self._constant(expression.children[0])
            right = self._constant(expression.children[1])
            operator = str(expression.value)
            if operator == ",":
                return right
            if operator == "+":
                return (left + right) & WORD_MASK
            if operator == "-":
                return (left - right) & WORD_MASK
            if operator == "*":
                return (left * right) & WORD_MASK
            if operator == "/" and right:
                dividend, divisor = self._signed(left), self._signed(right)
                quotient = abs(dividend) // abs(divisor)
                if (dividend < 0) != (divisor < 0):
                    quotient = -quotient
                return quotient & WORD_MASK
            if operator == "%" and right:
                dividend, divisor = self._signed(left), self._signed(right)
                quotient = abs(dividend) // abs(divisor)
                if (dividend < 0) != (divisor < 0):
                    quotient = -quotient
                return (dividend - quotient * divisor) & WORD_MASK
            if operator == "&":
                return left & right
            if operator == "|":
                return left | right
            if operator == "^":
                return left ^ right
            if operator == "==":
                return int(left == right)
            if operator == "!=":
                return int(left != right)
            signed_left, signed_right = self._signed(left), self._signed(right)
            if operator == "<":
                return int(signed_left < signed_right)
            if operator == "<=":
                return int(signed_left <= signed_right)
            if operator == ">":
                return int(signed_left > signed_right)
            if operator == ">=":
                return int(signed_left >= signed_right)
            if operator == "&&":
                return int(left != 0 and right != 0)
            if operator == "||":
                return int(left != 0 or right != 0)
        raise self.error(expression.line, "global initializers must be integer constant expressions")

    def _is_integer_constant_expression(self, expression: Expr) -> bool:
        if expression.kind in {"constant", "sizeof_type", "sizeof_expr"}:
            return True
        if expression.kind == "name":
            return str(expression.value) in self.enum_constants
        if expression.kind == "cast":
            return (
                expression.value
                in {"char", "unsigned char", "int", "unsigned int"}
                and self._is_integer_constant_expression(expression.children[0])
            )
        if expression.kind == "unary":
            return (
                expression.value in {"+", "-", "~", "!"}
                and self._is_integer_constant_expression(expression.children[0])
            )
        if expression.kind == "binary":
            return (
                expression.value
                in {
                    "+", "-", "*", "/", "%", "<<", ">>", "<", "<=", ">", ">=",
                    "==", "!=", "&", "^", "|", "&&", "||",
                }
                and all(
                    self._is_integer_constant_expression(child)
                    for child in expression.children
                )
            )
        if expression.kind == "conditional":
            return all(
                self._is_integer_constant_expression(child)
                for child in expression.children
            )
        return False

    def _constant_member_offset(self, expression: Expr, line: int) -> int:
        if expression.kind != "member":
            raise self.error(line, "member designator is not an integer constant expression")
        operator, field_name = expression.value
        base = expression.children[0]
        if operator == "->":
            if (
                base.kind != "cast"
                or not base.children
                or base.children[0].kind != "constant"
                or int(base.children[0].value) != 0
            ):
                raise self.error(line, "member address is not a constant offset")
            record_type = str(base.value)
            if not record_type.endswith("*"):
                raise self.error(line, "member designator does not point to a record")
            record_type = record_type[:-1]
            base_offset = 0
        else:
            record_type = self._expression_type(base)
            base_offset = (
                self._constant_member_offset(base, line)
                if base.kind == "member"
                else 0
            )
        field_ = self._record_field(record_type, str(field_name), line)
        return base_offset + field_.offset

    @staticmethod
    def _signed(value: int) -> int:
        return value - 0x10000 if value & 0x8000 else value

    def _generate_function(self, function: Function) -> None:
        self.current_function = function
        self.label_names = set()
        goto_targets: set[str] = set()

        def collect_labels(statement: Stmt) -> None:
            if statement.kind == "label":
                label_name = str(statement.value)
                if label_name in self.label_names:
                    raise self.error(statement.line, f"duplicate label {label_name!r}")
                self.label_names.add(label_name)
            elif statement.kind == "goto":
                goto_targets.add(str(statement.value))
            for child in statement.children:
                if isinstance(child, Stmt):
                    collect_labels(child)
            if statement.kind == "switch":
                for _, body, _ in statement.value:
                    for child in body:
                        collect_labels(child)

        collect_labels(function.body)
        undefined_labels = goto_targets - self.label_names
        if undefined_labels:
            name = sorted(undefined_labels)[0]
            raise self.error(function.line, f"goto references undefined label {name!r}")
        declaration = self.prototypes.get(function.name)
        function_signature = (
            function.return_type,
            len(function.parameters),
            tuple(parameter_type for parameter_type, _ in function.parameters),
        )
        if declaration is not None and declaration[1] is not None:
            compatible_parameters = declaration[2]
            if compatible_parameters is not None and any(
                declaration_type != definition_type
                for declaration_type, definition_type in zip(
                    compatible_parameters, function_signature[2]
                )
            ):
                raise self.error(
                    function.line,
                    f"definition of {function.name!r} conflicts with declaration",
                )
            if declaration[0] != function_signature[0] or declaration[1] != function_signature[1]:
                raise self.error(
                    function.line,
                    f"definition of {function.name!r} conflicts with declaration",
                )
        if function.name == "main" and function.parameters:
            raise self.error(function.line, "main() must not have parameters")
        if function.local_count > FRAME_SIZE_LIMIT:
            raise self.error(
                function.line,
                f"function frame exceeds {FRAME_SIZE_LIMIT} bytes",
            )
        self.frame_size = function.local_count
        parameter_bytes = sum(
            _object_size(c_type, self.records) for c_type, _ in function.parameters
        )
        if self.frame_size + 2 + parameter_bytes > STACK_OFFSET_LIMIT:
            raise self.error(function.line, "parameters exceed stack-relative address range")
        self.stack_depth = 0
        self.scopes = [{}]
        parameter_offset = self.frame_size + 2
        for c_type, name in function.parameters:
            if name is None:
                parameter_offset += _object_size(c_type, self.records)
                continue
            if name in self.scopes[0]:
                raise self.error(function.line, f"duplicate parameter name {name!r}")
            self.scopes[0][name] = _Variable(
                parameter_offset=parameter_offset,
                c_type=c_type,
            )
            parameter_offset += _object_size(c_type, self.records)
        self.emit(f"__c_function_{function.name}:")
        if self.frame_size:
            self.emit(f"    ADJSP -{self.frame_size}")
        for statement in function.body.children:
            self._statement(statement)
        self.emit("    LDI R0, 0")
        self._emit_epilogue()

    def _emit_epilogue(self) -> None:
        if self.frame_size:
            self.emit(f"    ADJSP {self.frame_size}")
        self.emit("    RET")

    def _lookup(self, name: str, line: int) -> _Variable:
        for scope in reversed(self.scopes):
            if name in scope:
                return scope[name]
        if name in self.global_addresses:
            global_ = self.global_variables[name]
            return _Variable(
                global_address=self.global_addresses[name],
                c_type=global_.c_type,
                array_size=global_.array_size,
            )
        if name in self.enum_constants:
            return _Variable(c_type="int", constant_value=self.enum_constants[name])
        raise self.error(line, f"undeclared identifier {name!r}")

    def _stack_offset(self, offset: int, line: int) -> int:
        adjusted = offset + self.stack_depth
        if not 0 <= adjusted <= STACK_OFFSET_LIMIT:
            raise self.error(line, "temporary stack use exceeds stack-relative address range")
        return adjusted

    def _emit_load_pointer(self, register: str) -> None:
        repeat = self.label("pointer_load")
        done = self.label("pointer_load_done")
        self.emit(f"    LDW {register}, R7")
        self.emit("    INC R7")
        self.emit("    INC R7")
        self.emit("    LD R5, R7")
        self.emit("    LDI R4, 0")
        self.emit("    LDI R6, 0xFFFF")
        self.emit("    INC R6")
        self.emit("    LDI R3, 0")
        self.emit(f"{repeat}:")
        self.emit("    CMP R5, R3")
        self.emit(f"    BZX {done}")
        self.emit("    ADD R4, R6")
        self.emit("    DEC R5")
        self.emit("    CMP R5, R3")
        self.emit(f"    BZX {done}")
        self.emit(f"    JMPX {repeat}")
        self.emit(f"{done}:")
        self.emit(f"    ADD {register}, R4")

    def _emit_store_pointer(self, register: str) -> None:
        repeat = self.label("pointer_store")
        done = self.label("pointer_store_done")
        self.emit(f"    STW R7, {register}")
        self.emit("    INC R7")
        self.emit("    INC R7")
        self.emit(f"    MOV R5, {register}")
        self.emit("    LDI R4, 0xFFFF")
        self.emit("    INC R4")
        self.emit("    LDI R6, 0")
        self.emit("    LDI R3, 0")
        self.emit(f"{repeat}:")
        self.emit("    CMP R5, R4")
        self.emit(f"    BCX {done}")
        self.emit("    SUB R5, R4")
        self.emit("    INC R6")
        self.emit(f"    JMPX {repeat}")
        self.emit(f"{done}:")
        self.emit("    ST R7, R6")

    def _load_variable(self, variable: _Variable, register: str, line: int) -> None:
        if variable.constant_value is not None:
            self.emit(f"    LDI {register}, {variable.constant_value & WORD_MASK}")
            return
        if variable.array_size:
            self._array_address(variable, register, line)
            return
        if variable.c_type.endswith("*"):
            if variable.global_address is not None:
                self.emit(f"    LDI R7, {variable.global_address}")
            else:
                offset = variable.local_offset
                if offset is None:
                    offset = variable.parameter_offset
                assert offset is not None
                self.emit("    MOVSP R7")
                adjusted = self._stack_offset(offset, line)
                if adjusted:
                    self.emit(f"    LDI R6, {adjusted}")
                    self.emit("    ADD R7, R6")
            self._emit_load_pointer(register)
            return
        if variable.global_address is not None:
            self.emit(f"    LDI R7, {variable.global_address}")
            self.emit(
                f"    {'LD' if _type_size(variable.c_type, self.records) == 1 else 'LDW'} "
                f"{register}, R7"
            )
            self._convert_loaded(register, variable.c_type)
            return
        offset = variable.local_offset
        if offset is None:
            offset = variable.parameter_offset
        assert offset is not None
        self.emit(f"    LDWS {register}, {self._stack_offset(offset, line)}")
        self._convert_loaded(register, variable.c_type)

    def _store_variable(
        self, variable: _Variable, register: str, line: int, source_type: str = "int"
    ) -> None:
        if variable.constant_value is not None:
            raise self.error(line, "cannot assign to an enumerator")
        self._convert_to_type(register, variable.c_type, source_type)
        if variable.c_type.endswith("*"):
            if variable.global_address is not None:
                self.emit(f"    LDI R7, {variable.global_address}")
            else:
                offset = variable.local_offset
                if offset is None:
                    offset = variable.parameter_offset
                assert offset is not None
                self.emit("    MOVSP R7")
                adjusted = self._stack_offset(offset, line)
                if adjusted:
                    self.emit(f"    LDI R6, {adjusted}")
                    self.emit("    ADD R7, R6")
            self._emit_store_pointer(register)
            return
        if variable.global_address is not None:
            self.emit(f"    LDI R7, {variable.global_address}")
            self.emit(
                f"    {'ST' if _type_size(variable.c_type, self.records) == 1 else 'STW'} "
                f"R7, {register}"
            )
            return
        offset = variable.local_offset
        if offset is None:
            offset = variable.parameter_offset
        assert offset is not None
        self.emit(f"    STWS {register}, {self._stack_offset(offset, line)}")

    def _array_address(self, variable: _Variable, register: str, line: int) -> None:
        if variable.global_address is not None:
            self.emit(f"    LDI {register}, {variable.global_address}")
            return
        offset = variable.local_offset
        if offset is None:
            offset = variable.parameter_offset
        if offset is None:
            raise self.error(line, "array has no storage address")
        offset += self.stack_depth
        self.emit(f"    MOVSP {register}")
        if offset:
            self.emit(f"    LDI R7, {offset}")
            self.emit(f"    ADD {register}, R7")

    def _convert_loaded(self, register: str, c_type: str, byte_loaded: bool = False) -> None:
        if c_type in ("char", "unsigned char") and not byte_loaded:
            self.emit(f"    LDI R7, 0x00FF")
            self.emit(f"    AND {register}, R7")
        if c_type == "char":
            self.emit("    LDI R7, 0x80")
            self.emit(f"    XOR {register}, R7")
            self.emit(f"    SUB {register}, R7")

    def _convert_to_type(self, register: str, c_type: str, source_type: str) -> None:
        if c_type == "float" and source_type != "float":
            self.emit(f"    ITOF {register}")
        elif c_type != "float" and source_type == "float":
            self.emit(f"    FTOI {register}")
        if c_type in ("char", "unsigned char"):
            self.emit("    LDI R7, 0x00FF")
            self.emit(f"    AND {register}, R7")

    def _statement(self, statement: Stmt) -> None:
        kind = statement.kind
        if kind == "label":
            self.emit(f"__c_label_{self.current_function.name}_{statement.value}:")
            self._statement(statement.children[0])
            return
        if kind == "goto":
            self.emit(
                f"    JMPX __c_label_{self.current_function.name}_{statement.value}"
            )
            return
        if kind == "empty":
            return
        if kind == "block":
            self.scopes.append({})
            for child in statement.children:
                self._statement(child)
            self.scopes.pop()
            return
        if kind == "decl":
            local: Local = statement.value
            current = self.scopes[-1]
            if local.name in current:
                raise self.error(local.line, f"duplicate local variable {local.name!r}")
            variable = _Variable(
                local_offset=local.slot,
                c_type=local.c_type,
                array_size=local.array_size,
            )
            current[local.name] = variable
            if local.array_size:
                self._initialize_local_array(variable, local, statement.line)
            elif local.c_type in self.records and local.initializer is not None:
                data = self._serialize_initializer(
                    local.c_type, 0, local.initializer, local.line
                )
                self._initialize_local_bytes(variable, data, local.line)
            elif local.initializer is not None:
                self._expression(local.initializer)
                self._store_variable(
                    variable,
                    "R0",
                    local.line,
                    self._expression_type(local.initializer),
                )
            return
        if kind == "expression":
            self._discarded_expression(statement.children[0])
            return
        if kind == "return":
            expression = statement.children[0]
            return_type = (
                self.current_function.return_type
                if self.current_function is not None else "void"
            )
            if self.current_function is not None:
                if return_type == "void" and expression is not None:
                    raise self.error(statement.line, "void functions cannot return a value")
                if return_type != "void" and expression is None:
                    raise self.error(statement.line, "non-void functions must return a value")
            if expression is None:
                self.emit("    LDI R0, 0")
            else:
                source_type = self._expression_type(expression)
                if self.current_function is not None:
                    integer_types = {
                        "char", "unsigned char", "int", "unsigned int"
                    }
                    numeric_types = integer_types | {"float"}
                    if return_type in numeric_types:
                        if source_type not in numeric_types:
                            raise self.error(
                                statement.line,
                                "return expression is incompatible with function type",
                            )
                    elif return_type.endswith("*"):
                        compatible_pointer = (
                            source_type.endswith("*")
                            and (
                                source_type == return_type
                                or source_type == "void*"
                                or return_type == "void*"
                            )
                        )
                        null_pointer_constant = (
                            source_type in integer_types
                            and self._is_integer_constant_expression(expression)
                            and self._constant(expression) == 0
                        )
                        if not compatible_pointer and not null_pointer_constant:
                            raise self.error(
                                statement.line,
                                "return expression is incompatible with function type",
                            )
                    else:
                        raise self.error(
                            statement.line,
                            "returning this type is not supported",
                        )
                self._expression(expression)
                if self.current_function is not None:
                    self._convert_to_type("R0", return_type, source_type)
            self._emit_epilogue()
            return
        if kind == "if":
            condition, then_branch, else_branch = statement.children
            else_label = self.label("else")
            end_label = self.label("endif")
            self._branch_if_false(condition, else_label)
            self._statement(then_branch)
            if else_branch is not None:
                self.emit(f"    JMPX {end_label}")
                self.emit(f"{else_label}:")
                self._statement(else_branch)
                self.emit(f"{end_label}:")
            else:
                self.emit(f"{else_label}:")
            return
        if kind in ("while", "do", "for"):
            self._loop_statement(statement)
            return
        if kind == "switch":
            self._switch_statement(statement)
            return
        if kind in ("break", "continue"):
            labels = self.break_labels if kind == "break" else self.continue_labels
            if not labels:
                raise self.error(statement.line, f"{kind} outside a loop")
            self.emit(f"    JMPX {labels[-1]}")
            return
        raise self.error(statement.line, f"unsupported statement {kind}")

    def _discarded_expression(self, expression: Expr) -> None:
        """Generate an expression whose value is unused, so the store can skip 16-bit masking."""
        if expression.kind == "postfix":
            expression = Expr("unary", expression.value, expression.children, expression.line)
        if expression.kind in ("assign", "unary"):
            self._discard = expression
        self._expression(expression)
        self._discard = None

    def _branch_if_false(self, condition: Expr, target: str) -> None:
        """Jump to target when the condition is zero; comparisons and && / || branch directly."""
        if condition.kind == "binary":
            operator = str(condition.value)
            left, right = condition.children
            if operator in ("&&", "||"):
                self._expression_type(condition)
                if operator == "&&":
                    self._branch_if_false(left, target)
                    self._branch_if_false(right, target)
                else:
                    second = self.label("or_second")
                    done = self.label("or_done")
                    self._branch_if_false(left, second)
                    self.emit(f"    JMPX {done}")
                    self.emit(f"{second}:")
                    self._branch_if_false(right, target)
                    self.emit(f"{done}:")
                return
            if (operator in ("==", "!=", "<", "<=", ">", ">=")
                    and "float" not in (self._expression_type(left), self._expression_type(right))):
                self._jump_false = target
                self._expression(condition)
                return
        self._expression(condition)
        if self._expression_type(condition) == "float":
            self.emit("    LDI R1, 0")
            self.emit("    FCMP R0, R1")
        else:
            self.emit("    CMP R0, 0")
        self.emit(f"    BZX {target}")

    def _loop_statement(self, statement: Stmt) -> None:
        kind = statement.kind
        start = self.label("loop")
        continue_label = self.label("continue")
        end = self.label("break")
        self.break_labels.append(end)
        self.continue_labels.append(continue_label)
        if kind == "while":
            condition, body = statement.children
            self.emit(f"{start}:")
            self._branch_if_false(condition, end)
            self._statement(body)
            self.emit(f"{continue_label}:")
            self.emit(f"    JMPX {start}")
        elif kind == "do":
            body, condition = statement.children
            self.emit(f"{start}:")
            self._statement(body)
            self.emit(f"{continue_label}:")
            self._branch_if_false(condition, end)
            self.emit(f"    JMPX {start}")
        else:
            initializer, condition, increment, body = statement.children
            if initializer is not None:
                self._discarded_expression(initializer)
            self.emit(f"{start}:")
            if condition is not None:
                self._branch_if_false(condition, end)
            self._statement(body)
            self.emit(f"{continue_label}:")
            if increment is not None:
                self._discarded_expression(increment)
            self.emit(f"    JMPX {start}")
        self.emit(f"{end}:")
        self.break_labels.pop()
        self.continue_labels.pop()

    def _switch_statement(self, statement: Stmt) -> None:
        expression = statement.children[0]
        value_type = self._expression_type(expression)
        if value_type not in {"char", "unsigned char", "int", "unsigned int"}:
            raise self.error(
                expression.line,
                "switch expression must have integer type",
            )
        cases = statement.value
        default_label = None
        for value, _, _ in cases:
            if value is None:
                default_label = self.label("switch_default")
                break
        end_label = self.label("switch_end")
        case_labels = [
            self.label("switch_case") for value, _, _ in cases if value is not None
        ]
        self._expression(expression)
        self.emit("    PUSH R0")
        self.stack_depth += 2
        case_index = 0
        seen_values: set[int] = set()
        for value, _, line in cases:
            if value is None:
                continue
            constant = self._constant(value)
            if constant in seen_values:
                raise self.error(line, "duplicate case value")
            seen_values.add(constant)
            self.emit("    LDWS R0, 0")
            self.emit(f"    LDI R1, {constant}")
            self.emit("    CMP R0, R1")
            self.emit(f"    BZX {case_labels[case_index]}")
            case_index += 1
        self.emit(f"    JMPX {default_label or end_label}")
        self.break_labels.append(end_label)
        case_index = 0
        for value, body, _ in cases:
            if value is None:
                label = default_label
            else:
                label = case_labels[case_index]
                case_index += 1
            self.emit(f"{label}:")
            for child in body:
                self._statement(child)
        self.break_labels.pop()
        self.emit(f"{end_label}:")
        self.emit("    ADJSP 2")
        self.stack_depth -= 2

    def _initialize_local_array(self, variable: _Variable, local: Local, line: int) -> None:
        data = self._serialize_initializer(
            local.c_type, local.array_size, local.initializer, line
        )
        self._initialize_local_bytes(variable, data, line)

    def _initialize_local_bytes(
        self, variable: _Variable, data: bytes, line: int
    ) -> None:
        for index, value in enumerate(data):
            self.emit("    MOVSP R7")
            offset = int(variable.local_offset or 0) + self.stack_depth + index
            if offset:
                self.emit(f"    LDI R6, {offset}")
                self.emit("    ADD R7, R6")
            self.emit(f"    LDI R0, {value}")
            self.emit("    ST R7, R0")

    def _expression_type(self, expression: Expr) -> str:
        if expression.kind == "constant":
            return "int"
        if expression.kind == "float_constant":
            return "float"
        if expression.kind == "string":
            return "char*"
        if expression.kind in ("sizeof_type", "sizeof_expr"):
            return "unsigned int"
        if expression.kind == "name":
            variable = self._lookup(str(expression.value), expression.line)
            if variable.array_size:
                return variable.c_type + "*"
            return variable.c_type
        if expression.kind == "call":
            function = self.functions_by_name.get(str(expression.value))
            if function is None:
                builtin = LIBRARY_FUNCTIONS.get(str(expression.value))
                if builtin is None:
                    raise self.error(expression.line, f"undefined function {expression.value!r}")
                return builtin[0]
            return function.return_type
        if expression.kind == "index":
            base_type = self._expression_type(expression.children[0])
            if base_type.endswith("*"):
                return base_type[:-1]
            raise self.error(expression.line, "subscripted expression is not a pointer or array")
        if expression.kind == "member":
            operator, field_name = expression.value
            base_type = self._expression_type(expression.children[0])
            if operator == "->":
                if not base_type.endswith("*"):
                    raise self.error(expression.line, "-> requires a pointer to a record")
                base_type = base_type[:-1]
            field_ = self._record_field(base_type, field_name, expression.line)
            return field_.c_type + "*" if field_.array_size else field_.c_type
        if expression.kind == "assign":
            target = expression.value[1]
            return self._expression_type(target)
        if expression.kind == "cast":
            return str(expression.value)
        if expression.kind == "conditional":
            condition_type = self._expression_type(expression.children[0])
            true_type = self._expression_type(expression.children[1])
            false_type = self._expression_type(expression.children[2])
            arithmetic_types = {
                "char", "unsigned char", "int", "unsigned int", "float"
            }
            if condition_type not in arithmetic_types and not condition_type.endswith("*"):
                raise self.error(
                    expression.line,
                    "conditional operator requires a scalar condition",
                )
            if true_type in arithmetic_types and false_type in arithmetic_types:
                return self._usual_arithmetic_type(true_type, false_type)
            if true_type == false_type and true_type == "void":
                return "void"
            if true_type.endswith("*") and false_type.endswith("*"):
                if true_type == false_type:
                    return true_type
                if true_type == "void*":
                    return true_type
                if false_type == "void*":
                    return false_type
                raise self.error(
                    expression.line,
                    "conditional pointer operands have incompatible types",
                )
            if true_type.endswith("*") and false_type in {
                "char", "unsigned char", "int", "unsigned int"
            }:
                if (
                    self._is_integer_constant_expression(expression.children[2])
                    and self._constant(expression.children[2]) == 0
                ):
                    return true_type
            if false_type.endswith("*") and true_type in {
                "char", "unsigned char", "int", "unsigned int"
            }:
                if (
                    self._is_integer_constant_expression(expression.children[1])
                    and self._constant(expression.children[1]) == 0
                ):
                    return false_type
            raise self.error(
                expression.line,
                "conditional operands have incompatible types",
            )
        if expression.kind == "unary":
            operator = str(expression.value)
            child_type = self._expression_type(expression.children[0])
            if operator == "&":
                return child_type + "*"
            if operator == "*":
                if not child_type.endswith("*"):
                    raise self.error(expression.line, "cannot dereference a non-pointer")
                return child_type[:-1]
            if operator in ("+", "-", "~") and child_type in (
                "char", "unsigned char"
            ):
                return "int"
            return "int" if operator == "!" else child_type
        if expression.kind == "postfix":
            return self._expression_type(expression.children[0])
        if expression.kind == "binary":
            operator = str(expression.value)
            if operator == ",":
                return self._expression_type(expression.children[1])
            left_type = self._expression_type(expression.children[0])
            right_type = self._expression_type(expression.children[1])
            integer_types = {"char", "unsigned char", "int", "unsigned int"}
            arithmetic_types = integer_types | {"float"}
            left_integer = left_type in integer_types
            right_integer = right_type in integer_types
            left_arithmetic = left_type in arithmetic_types
            right_arithmetic = right_type in arithmetic_types
            left_pointer = left_type.endswith("*")
            right_pointer = right_type.endswith("*")
            if operator in ("&&", "||"):
                if not (
                    (left_arithmetic or left_pointer)
                    and (right_arithmetic or right_pointer)
                ):
                    raise self.error(
                        expression.line,
                        f"operator {operator} requires scalar operands",
                    )
                return "int"
            if operator in ("==", "!=", "<", "<=", ">", ">="):
                if left_arithmetic and right_arithmetic:
                    return "int"
                compatible_pointers = (
                    left_pointer
                    and right_pointer
                    and (
                        left_type == right_type
                        or (
                            operator in ("==", "!=")
                            and (left_type == "void*" or right_type == "void*")
                        )
                    )
                )
                if compatible_pointers and operator not in ("==", "!="):
                    self._require_pointer_arithmetic_type(left_type, expression.line)
                null_pointer_comparison = (
                    operator in ("==", "!=")
                    and (
                        (
                            left_pointer
                            and right_integer
                            and self._is_integer_constant_expression(
                                expression.children[1]
                            )
                            and self._constant(expression.children[1]) == 0
                        )
                        or (
                            right_pointer
                            and left_integer
                            and self._is_integer_constant_expression(
                                expression.children[0]
                            )
                            and self._constant(expression.children[0]) == 0
                        )
                    )
                )
                if compatible_pointers or null_pointer_comparison:
                    return "int"
                raise self.error(
                    expression.line,
                    f"operator {operator} requires arithmetic operands or "
                    "compatible pointers",
                )
            if operator in ("<<", ">>", "%", "&", "|", "^"):
                if not left_integer or not right_integer:
                    raise self.error(
                        expression.line,
                        f"operator {operator} requires integer operands",
                    )
                return self._usual_arithmetic_type(left_type, right_type)
            if operator in ("*", "/"):
                if not left_arithmetic or not right_arithmetic:
                    raise self.error(
                        expression.line,
                        f"operator {operator} requires arithmetic operands",
                    )
                if left_type == "float" or right_type == "float":
                    return "float"
                return self._usual_arithmetic_type(left_type, right_type)
            if operator == "+":
                if left_arithmetic and right_arithmetic:
                    return (
                        "float"
                        if left_type == "float" or right_type == "float"
                        else self._usual_arithmetic_type(left_type, right_type)
                    )
                if left_pointer and right_integer:
                    self._require_pointer_arithmetic_type(left_type, expression.line)
                    return left_type
                if right_pointer and left_integer:
                    self._require_pointer_arithmetic_type(right_type, expression.line)
                    return right_type
                raise self.error(
                    expression.line,
                    "operator + requires arithmetic operands or a pointer and integer",
                )
            if operator == "-":
                if left_arithmetic and right_arithmetic:
                    return (
                        "float"
                        if left_type == "float" or right_type == "float"
                        else self._usual_arithmetic_type(left_type, right_type)
                    )
                if left_pointer and right_integer:
                    self._require_pointer_arithmetic_type(left_type, expression.line)
                    return left_type
                if (
                    left_pointer
                    and right_pointer
                    and left_type == right_type
                ):
                    self._require_pointer_arithmetic_type(left_type, expression.line)
                    return "int"
                raise self.error(
                    expression.line,
                    "operator - requires arithmetic operands, a pointer and integer, "
                    "or compatible pointers",
                )
            if left_type == "float" or right_type == "float":
                return "float"
            raise self.error(expression.line, f"unsupported binary operator {operator}")
        raise self.error(expression.line, "could not determine expression type")

    def _require_pointer_arithmetic_type(self, pointer_type: str, line: int) -> None:
        pointed_type = pointer_type[:-1]
        record = self.records.get(pointed_type)
        if pointed_type == "void" or (record is not None and not record.complete):
            raise self.error(
                line,
                "pointer arithmetic requires a pointer to a complete object type",
            )

    @staticmethod
    def _integer_promoted_type(c_type: str) -> str:
        if c_type in ("char", "unsigned char"):
            return "int"
        return c_type

    @staticmethod
    def _usual_arithmetic_type(left_type: str, right_type: str) -> str:
        if left_type == "float" or right_type == "float":
            return "float"
        if left_type.endswith("*") and right_type.endswith("*"):
            return left_type if left_type == right_type else "int"
        if left_type.endswith("*"):
            return left_type
        if right_type.endswith("*"):
            return right_type
        left_type = _CodeGenerator._integer_promoted_type(left_type)
        right_type = _CodeGenerator._integer_promoted_type(right_type)
        if left_type.startswith("unsigned") or right_type.startswith("unsigned"):
            return "unsigned int"
        return "int"

    def _sizeof_expression(self, expression: Expr) -> int:
        if expression.kind == "name":
            variable = self._lookup(str(expression.value), expression.line)
            if variable.array_size:
                return variable.array_size * _type_size(variable.c_type, self.records)
        return _type_size(self._expression_type(expression), self.records)

    def _record_field(self, c_type: str, name: str, line: int) -> _Field:
        record = self.records.get(c_type)
        if record is None or not record.complete:
            raise self.error(line, "member access requires a complete struct or union")
        field_ = record.fields.get(name)
        if field_ is None:
            raise self.error(line, f"record has no member {name!r}")
        return field_

    def _scale_index(self, register: str, stride: int) -> None:
        if stride == 1:
            return
        if stride == 2:
            self.emit(f"    ADD {register}, {register}")
            return
        self.emit(f"    LDI R7, {stride}")
        self.emit(f"    MUL {register}, R7")

    def _lvalue_address(self, expression: Expr, line: int) -> str:
        if expression.kind == "name":
            variable = self._lookup(str(expression.value), line)
            if variable.constant_value is not None:
                raise self.error(line, "enumerators do not have an address")
            if variable.array_size:
                self._array_address(variable, "R0", line)
            elif variable.global_address is not None:
                self.emit(f"    LDI R0, {variable.global_address}")
            else:
                offset = variable.local_offset
                if offset is None:
                    offset = variable.parameter_offset
                if offset is None:
                    raise self.error(line, "variable has no address")
                self.emit("    MOVSP R0")
                offset += self.stack_depth
                if offset:
                    self.emit(f"    LDI R7, {offset}")
                    self.emit("    ADD R0, R7")
            return variable.c_type
        if expression.kind == "index":
            base, index = expression.children
            element_type = self._expression_type(expression)
            element_size = _type_size(element_type, self.records)
            if base.kind == "name":
                array = self._lookup(str(base.value), line)
                if array.array_size:
                    if index.kind == "constant" and array.global_address is not None:
                        element = array.global_address + int(index.value) * element_size
                        self.emit(f"    LDI R0, {element & WORD_MASK}")
                        return element_type
                    self._expression(index)
                    self._scale_index("R0", element_size)
                    self._array_address(array, "R1", line)
                    self.emit("    ADD R0, R1")
                    return element_type
            self._expression(base)
            self.emit("    ADJSP -3")
            self.emit("    MOVSP R7")
            self._emit_store_pointer("R0")
            self.stack_depth += 3
            self._expression(index)
            self._scale_index("R0", element_size)
            self.emit("    MOV R2, R0")
            self.emit("    MOVSP R7")
            self._emit_load_pointer("R1")
            self.emit("    ADD R1, R2")
            self.emit("    MOV R0, R1")
            self.emit("    ADJSP 3")
            self.stack_depth -= 3
            return element_type
        if expression.kind == "member":
            operator, field_name = expression.value
            base = expression.children[0]
            base_type = self._expression_type(base)
            if operator == "->":
                if not base_type.endswith("*"):
                    raise self.error(line, "-> requires a pointer to a record")
                self._expression(base)
                base_type = base_type[:-1]
            else:
                base_type = self._lvalue_address(base, line)
            field_ = self._record_field(base_type, field_name, line)
            if field_.offset:
                self.emit(f"    LDI R7, {field_.offset}")
                self.emit("    ADD R0, R7")
            return field_.c_type
        if expression.kind == "unary" and expression.value == "*":
            self._expression(expression.children[0])
            return self._expression_type(expression)
        raise self.error(line, "expression is not assignable")

    def _load_lvalue(self, expression: Expr, line: int) -> None:
        if expression.kind == "name":
            variable = self._lookup(str(expression.value), line)
            if variable.array_size:
                raise self.error(line, "array expression requires an index")
            if variable.c_type in self.records:
                raise self.error(line, "record value operations are not supported")
            self._load_variable(variable, "R0", line)
            return
        c_type = self._lvalue_address(expression, line)
        if c_type in self.records:
            raise self.error(line, "record value operations are not supported")
        if c_type.endswith("*"):
            self.emit("    MOV R7, R0")
            self._emit_load_pointer("R0")
        else:
            self.emit(
                f"    {'LD' if _type_size(c_type, self.records) == 1 else 'LDW'} R0, R0"
            )
        self._convert_loaded(
            "R0",
            c_type,
            _type_size(c_type, self.records) == 1 and not c_type.endswith("*"),
        )

    def _store_lvalue(
        self,
        expression: Expr,
        register: str,
        source_type: str,
        line: int,
        source_expression: Expr | None = None,
    ) -> None:
        c_type = self._expression_type(expression)
        if c_type in self.records:
            raise self.error(line, "record assignment is not supported")
        integer_types = {"char", "unsigned char", "int", "unsigned int"}
        numeric_types = integer_types | {"float"}
        if c_type in numeric_types:
            if source_type not in numeric_types:
                raise self.error(line, "assignment expression is incompatible with target type")
        elif c_type.endswith("*"):
            compatible_pointer = (
                source_type.endswith("*")
                and (
                    source_type == c_type
                    or source_type == "void*"
                    or c_type == "void*"
                )
            )
            null_pointer_constant = (
                source_type in integer_types
                and source_expression is not None
                and self._is_integer_constant_expression(source_expression)
                and self._constant(source_expression) == 0
            )
            if not compatible_pointer and not null_pointer_constant:
                raise self.error(line, "assignment expression is incompatible with target type")
        else:
            raise self.error(line, "assignment to this type is not supported")
        self._convert_to_type(register, c_type, source_type)
        if expression.kind == "name":
            variable = self._lookup(str(expression.value), line)
            if variable.array_size:
                raise self.error(line, "cannot assign to an array")
            self._store_variable(variable, register, line, c_type)
            return
        if c_type.endswith("*"):
            self.emit("    ADJSP -3")
            self.emit("    MOVSP R7")
            self._emit_store_pointer(register)
            self.stack_depth += 3
            self._lvalue_address(expression, line)
            self.emit("    ADJSP -3")
            self.emit("    MOVSP R7")
            self._emit_store_pointer("R0")
            self.stack_depth += 3
            self.emit("    MOVSP R7")
            self._emit_load_pointer("R1")
            self.emit("    MOVSP R7")
            self.emit("    LDI R6, 3")
            self.emit("    ADD R7, R6")
            self._emit_load_pointer("R0")
            self.emit("    MOV R7, R1")
            self._emit_store_pointer("R0")
            self.emit("    ADJSP 6")
            self.stack_depth -= 6
            return
        self.emit(f"    PUSH {register}")
        self.stack_depth += 2
        self._lvalue_address(expression, line)
        self.emit("    LDWS R1, 0")
        self.emit(
            f"    {'ST' if _type_size(c_type, self.records) == 1 else 'STW'} R0, R1"
        )
        self.emit("    ADJSP 2")
        self.stack_depth -= 2

    def _expression(self, expression: Expr) -> None:
        kind = expression.kind
        if kind == "constant":
            self.emit(f"    LDI R0, {int(expression.value) & WORD_MASK}")
            return
        if kind == "sizeof_type":
            self.emit(f"    LDI R0, {_type_size(str(expression.value), self.records)}")
            return
        if kind == "sizeof_expr":
            self.emit(f"    LDI R0, {self._sizeof_expression(expression.children[0])}")
            return
        if kind == "address_constant":
            self.emit(f"    LDA R0, 0x{int(expression.value):05X}")
            return
        if kind == "float_constant":
            self.emit(f"    LDI R0, 0x{int(expression.value) & WORD_MASK:04X}")
            return
        if kind == "string":
            self.emit(f"    LDA R0, {expression.value}")
            return
        if kind == "member":
            operator, field_name = expression.value
            base_type = self._expression_type(expression.children[0])
            if operator == "->":
                if not base_type.endswith("*"):
                    raise self.error(expression.line, "-> requires a pointer to a record")
                base_type = base_type[:-1]
            field_ = self._record_field(base_type, field_name, expression.line)
            if field_.array_size:
                self._lvalue_address(expression, expression.line)
            else:
                self._load_lvalue(expression, expression.line)
            return
        if kind == "cast":
            child = expression.children[0]
            self._expression(child)
            self._convert_to_type(
                "R0", str(expression.value), self._expression_type(child)
            )
            return
        if kind == "conditional":
            false_label = self.label("conditional_false")
            done = self.label("conditional_end")
            condition, when_true, when_false = expression.children
            result_type = self._expression_type(expression)
            self._branch_if_false(condition, false_label)
            self._expression(when_true)
            self._convert_to_type(
                "R0", result_type, self._expression_type(when_true)
            )
            self.emit(f"    JMPX {done}")
            self.emit(f"{false_label}:")
            self._expression(when_false)
            self._convert_to_type(
                "R0", result_type, self._expression_type(when_false)
            )
            self.emit(f"{done}:")
            return
        if kind == "name":
            variable = self._lookup(str(expression.value), expression.line)
            if variable.constant_value is not None:
                self.emit(f"    LDI R0, {variable.constant_value & WORD_MASK}")
                return
            if variable.c_type in self.records and not variable.array_size:
                raise self.error(expression.line, "record values are only supported as lvalues")
            if variable.array_size:
                self._array_address(variable, "R0", expression.line)
            else:
                self._load_variable(variable, "R0", expression.line)
            return
        if kind == "call":
            name = str(expression.value)
            if name not in self.function_names and name not in LIBRARY_FUNCTIONS:
                raise self.error(expression.line, f"undefined function {name!r}")
            arguments = expression.children
            if name in self.functions_by_name:
                parameter_types = [
                    c_type for c_type, _ in self.functions_by_name[name].parameters
                ]
            else:
                parameter_types = list(LIBRARY_FUNCTIONS[name][1])
                declared = self.prototypes.get(name)
                if (
                    declared is not None
                    and declared[1] is not None
                    and declared[1] != len(parameter_types)
                ):
                    raise self.error(expression.line, f"invalid declaration for {name}")
                if name in LIBRARY_FILES:
                    self.used_libraries.add(LIBRARY_FILES[name])
            expected = len(parameter_types)
            if len(arguments) != expected:
                raise self.error(expression.line, f"{name}() expects {expected} argument(s)")
            if len(arguments) > 63:
                raise self.error(expression.line, "too many function arguments")
            argument_bytes = 0
            for index in range(len(arguments) - 1, -1, -1):
                argument = arguments[index]
                parameter_type = parameter_types[index]
                source_type = self._expression_type(argument)
                integer_types = {"char", "unsigned char", "int", "unsigned int"}
                numeric_types = integer_types | {"float"}
                if parameter_type in numeric_types:
                    if source_type not in numeric_types:
                        raise self.error(
                            argument.line,
                            f"argument {index + 1} to {name}() is incompatible "
                            "with its parameter type",
                        )
                elif parameter_type.endswith("*"):
                    compatible_pointer = (
                        source_type.endswith("*")
                        and (
                            source_type == parameter_type
                            or source_type == "void*"
                            or parameter_type == "void*"
                        )
                    )
                    null_pointer_constant = (
                        source_type in integer_types
                        and self._is_integer_constant_expression(argument)
                        and self._constant(argument) == 0
                    )
                    if not compatible_pointer and not null_pointer_constant:
                        raise self.error(
                            argument.line,
                            f"argument {index + 1} to {name}() is incompatible "
                            "with its parameter type",
                        )
                else:
                    raise self.error(
                        argument.line,
                        f"passing this type to {name}() is not supported",
                    )
                self._expression(argument)
                self._convert_to_type("R0", parameter_type, source_type)
                width = _object_size(parameter_type, self.records)
                if width == 3:
                    self.emit("    ADJSP -3")
                    self.emit("    MOVSP R7")
                    self._emit_store_pointer("R0")
                else:
                    self.emit("    PUSH R0")
                self.stack_depth += width
                argument_bytes += width
            if name in ("abort", "exit"):
                self.emit("    HALT")
            elif name in LIBRARY_FUNCTIONS and name not in self.function_names:
                self.emit(f"    CALLX {name}")
            else:
                self.emit(f"    CALLX __c_function_{name}")
            if argument_bytes:
                self.emit(f"    ADJSP {argument_bytes}")
                self.stack_depth -= argument_bytes
            return
        if kind == "assign":
            operator, target = expression.value
            target_type = self._expression_type(target)
            discarded = self._discard is expression
            self._discard = None
            if operator == "=":
                if discarded and expression.children[0].kind == "binary":
                    self._nomask = expression.children[0]
                self._expression(expression.children[0])
            else:
                self._load_lvalue(target, expression.line)
                self.emit("    PUSH R0")
                self.stack_depth += 2
                self._expression(expression.children[0])
                self.emit("    MOV R1, R0")
                self.emit("    LDWS R0, 0")
                binary_operator = str(operator[:-1])
                if binary_operator in ("<<", ">>"):
                    helper = (
                        "__runtime_shl"
                        if binary_operator == "<<"
                        else (
                            "__runtime_shr"
                            if target_type.startswith("unsigned")
                            or target_type in ("char", "unsigned char")
                            else "__runtime_sar"
                        )
                    )
                    self.used_helpers.add(helper)
                    self.emit(f"    CALLX {helper}")
                else:
                    self._binary_operation(
                        binary_operator, expression.line, target_type, not discarded
                    )
                self.emit("    ADJSP 2")
                self.stack_depth -= 2
            source_type = (
                target_type
                if operator != "="
                else self._expression_type(expression.children[0])
            )
            self._store_lvalue(
                target, "R0", source_type, expression.line, expression.children[0]
            )
            return
        if kind == "unary":
            operator = str(expression.value)
            child = expression.children[0]
            child_type = self._expression_type(child)
            integer_types = {"char", "unsigned char", "int", "unsigned int"}
            arithmetic_types = integer_types | {"float"}
            if operator in ("+", "-") and child_type not in arithmetic_types:
                raise self.error(
                    expression.line,
                    f"unary {operator} requires an arithmetic operand",
                )
            if operator == "~" and child_type not in integer_types:
                raise self.error(
                    expression.line,
                    "bitwise complement requires an integer operand",
                )
            if (
                operator == "!"
                and child_type not in arithmetic_types
                and not child_type.endswith("*")
            ):
                raise self.error(
                    expression.line,
                    "logical negation requires a scalar operand",
                )
            if operator == "&":
                self._lvalue_address(child, expression.line)
                return
            if operator == "*":
                self._load_lvalue(expression, expression.line)
                return
            if operator in ("++", "--"):
                c_type = self._expression_type(child)
                discarded = self._discard is expression
                self._discard = None
                self._load_lvalue(child, expression.line)
                if c_type.endswith("*"):
                    self.emit(f"    LDI R7, {_type_size(c_type[:-1], self.records)}")
                    self.emit(f"    {'ADD' if operator == '++' else 'SUB'} R0, R7")
                else:
                    self.emit(f"    {'INC' if operator == '++' else 'DEC'} R0")
                    if not discarded:
                        self._mask_result()
                self._store_lvalue(child, "R0", c_type, expression.line)
                return
            self._expression(child)
            if operator == "-":
                if child_type == "float":
                    self.emit("    LDI R7, 0x8000")
                    self.emit("    XOR R0, R7")
                else:
                    self.emit("    NEG R0")
            elif operator == "!":
                self._boolean_not(child_type)
            elif operator == "~":
                self.emit("    LDI R7, 0xFFFF")
                self.emit("    XOR R0, R7")
            return
        if kind == "postfix":
            child = expression.children[0]
            c_type = self._expression_type(child)
            self._load_lvalue(child, expression.line)
            if c_type.endswith("*"):
                self.emit("    ADJSP -3")
                self.emit("    MOVSP R7")
                self._emit_store_pointer("R0")
                self.stack_depth += 3
                self.emit(f"    LDI R7, {_type_size(c_type[:-1], self.records)}")
                if expression.value == "++":
                    self.emit("    ADD R0, R7")
                else:
                    self.emit("    SUB R0, R7")
                saved_size = 3
            else:
                self.emit("    PUSH R0")
                self.stack_depth += 2
                self.emit(f"    {'INC' if expression.value == '++' else 'DEC'} R0")
                self.emit("    LDI R7, 0xFFFF")
                self.emit("    AND R0, R7")
                saved_size = 2
            self._store_lvalue(child, "R0", c_type, expression.line)
            if saved_size == 3:
                self.emit("    MOVSP R7")
                self._emit_load_pointer("R0")
            else:
                self.emit("    LDWS R0, 0")
            self.emit(f"    ADJSP {saved_size}")
            self.stack_depth -= saved_size
            return
        if kind == "index":
            self._load_lvalue(expression, expression.line)
            return
        if kind == "binary":
            self._binary_expression(expression)
            return
        raise self.error(expression.line, f"unsupported expression {kind}")

    def _binary_expression(self, expression: Expr) -> None:
        operator = str(expression.value)
        left, right = expression.children
        if operator == ",":
            self._discarded_expression(left)
            self._expression(right)
            return
        jump_false, self._jump_false = self._jump_false, None
        self._expression_type(expression)
        if operator in ("&&", "||"):
            true_label = self.label("logic_true")
            false_label = self.label("logic_false")
            end_label = self.label("logic_end")
            if operator == "&&":
                self._branch_if_false(left, false_label)
            else:
                second_label = self.label("logic_second")
                self._branch_if_false(left, second_label)
                self.emit(f"    JMPX {true_label}")
                self.emit(f"{second_label}:")
            self._branch_if_false(right, false_label)
            self.emit(f"{true_label}:")
            self.emit("    LDI R0, 1")
            self.emit(f"    JMPX {end_label}")
            self.emit(f"{false_label}:")
            self.emit("    LDI R0, 0")
            self.emit(f"{end_label}:")
            return
        left_type = self._expression_type(left)
        right_type = self._expression_type(right)
        promoted_left_type = self._integer_promoted_type(left_type)
        promoted_right_type = self._integer_promoted_type(right_type)
        if right.kind == "constant" and self._constant_binary(expression, operator, left, left_type, jump_false):
            return
        self._expression(left)
        left_size = _object_size(left_type, self.records)
        if left_type.endswith("*"):
            self.emit("    ADJSP -3")
            self.emit("    MOVSP R7")
            self._emit_store_pointer("R0")
        else:
            self.emit("    PUSH R0")
        self.stack_depth += left_size
        self._expression(right)
        pointer_type = left_type if left_type.endswith("*") else (
            right_type if right_type.endswith("*") and operator == "+" else None
        )
        if pointer_type is not None and left_type.endswith("*") and not right_type.endswith("*"):
            stride = _type_size(pointer_type[:-1], self.records)
            self._scale_index("R0", stride)
        if pointer_type is not None and left_type.endswith("*"):
            self.emit("    MOV R1, R0")
            self.emit("    MOVSP R7")
            self._emit_load_pointer("R0")
        else:
            self.emit("    MOV R1, R0")
            self.emit("    LDWS R0, 0")
            if pointer_type is not None:
                stride = _type_size(pointer_type[:-1], self.records)
                self._scale_index("R0", stride)
        self.emit(f"    ADJSP {left_size}")
        self.stack_depth -= left_size
        result_type = self._expression_type(expression)
        if operator not in ("==", "!=", "<", "<=", ">", ">=") and result_type == "float":
            if left_type != "float":
                self.emit("    ITOF R0")
            if right_type != "float":
                self.emit("    ITOF R1")
        if operator in ("<<", ">>"):
            if left_type == "float" or right_type == "float" or left_type.endswith("*"):
                raise self.error(expression.line, f"operator {operator} requires integers")
            if operator == "<<":
                helper = "__runtime_shl"
            else:
                helper = (
                    "__runtime_shr"
                    if promoted_left_type.startswith("unsigned")
                    else "__runtime_sar"
                )
            self.used_helpers.add(helper)
            self.emit(f"    CALLX {helper}")
        elif operator in ("+", "-", "&", "|", "^", "*", "/", "%"):
            self._binary_operation(operator, expression.line, result_type, self._nomask is not expression)
        else:
            compare_type = "float" if left_type == "float" or right_type == "float" else (
                "unsigned int" if promoted_left_type.startswith("unsigned")
                or promoted_right_type.startswith("unsigned")
                or left_type.endswith("*") or right_type.endswith("*")
                else "int"
            )
            if compare_type == "float":
                if left_type != "float":
                    self.emit("    ITOF R0")
                if right_type != "float":
                    self.emit("    ITOF R1")
            self._comparison(operator, expression.line, compare_type, jump_false=jump_false)

    def _constant_binary(self, expression: Expr, operator: str, left: Expr, left_type: str,
                         jump_false: str | None = None) -> bool:
        """Emit `left <op> constant` without the operand stack; False if not applicable."""
        value = int(expression.children[1].value) & WORD_MASK
        if left_type == "float" or left_type.endswith("*"):
            return False
        shifts = ("<<", ">>")
        compares = ("==", "!=", "<", "<=", ">", ">=")
        if operator in shifts:
            if value > 15:
                return False
        elif operator not in ("+", "-", "&", "|", "^", "*", "/", "%") + compares:
            return False
        unsigned_left = self._integer_promoted_type(left_type).startswith("unsigned")
        self._expression(left)
        if operator in shifts:
            if value:
                mnemonic = "SHL" if operator == "<<" else ("LSR" if unsigned_left else "ASR")
                self.emit(f"    {mnemonic} R0, {value}")
            return True
        if operator in compares:
            if value < 256:
                self.emit(f"    LDI R1, {value}")
                self.emit("    CMP R0, R1")
            else:
                self.emit(f"    CMP R0, {value}")
            compare_type = "unsigned int" if unsigned_left else "int"
            self._comparison(operator, expression.line, compare_type, compared=True,
                             jump_false=jump_false)
            return True
        result_type = self._expression_type(expression)
        if operator in ("*", "/", "%"):
            unsigned = result_type.startswith("unsigned")
            mnemonic = {"*": "MUL", "/": "DIV" if unsigned else "DIVS",
                        "%": "MOD" if unsigned else "MODS"}[operator]
            self.emit(f"    {mnemonic} R0, {value}")
        elif operator in ("&", "|", "^") and value < 256:
            self.emit(f"    LDI R1, {value}")
            self.emit(f"    {ALU_NAMES[operator]} R0, R1")
        else:
            self.emit(f"    {ALU_NAMES[operator]} R0, {value}")
        return True

    def _binary_operation(
        self, operator: str, line: int, result_type: str = "int", mask: bool = True
    ) -> None:
        if result_type == "float":
            if operator not in ("+", "-", "*", "/"):
                raise self.error(line, f"operator {operator} is invalid for float")
            mnemonic = {"+": "FADD", "-": "FSUB", "*": "FMUL", "/": "FDIV"}[operator]
            self.emit(f"    {mnemonic} R0, R1")
            return
        if operator == "+":
            self.emit("    ADD R0, R1")
            if mask and not result_type.endswith("*"):
                self._mask_result()
        elif operator == "-":
            self.emit("    SUB R0, R1")
            if mask and not result_type.endswith("*"):
                self._mask_result()
        elif operator in ("&", "|", "^"):
            mnemonic = {"&": "AND", "|": "OR", "^": "XOR"}[operator]
            self.emit(f"    {mnemonic} R0, R1")
        elif operator in ("*", "/", "%"):
            unsigned = result_type.startswith("unsigned")
            mnemonic = {
                "*": "MUL",
                "/": "DIV" if unsigned else "DIVS",
                "%": "MOD" if unsigned else "MODS",
            }[operator]
            self.emit(f"    {mnemonic} R0, R1")
        else:
            raise self.error(line, f"unsupported binary operator {operator!r}")

    def _mask_result(self) -> None:
        self.emit("    LDI R7, 0xFFFF")
        self.emit("    AND R0, R7")

    def _comparison(self, operator: str, line: int, compare_type: str = "int",
                    compared: bool = False, jump_false: str | None = None) -> None:
        if jump_false is not None and compare_type != "float":
            if not compared:
                self.emit("    CMP R0, R1")
            if operator == "==":
                branch = "BNZX"
            elif operator == "!=":
                branch = "BZX"
            else:
                branch = (SIGNED_INVERSE if compare_type == "int" else UNSIGNED_INVERSE)[operator]
            self.emit(f"    {branch} {jump_false}")
            return
        true_label = self.label("cmp_true")
        false_label = self.label("cmp_false")
        end_label = self.label("cmp_end")
        if compare_type != "float" and operator in ("<", "<=", ">", ">="):
            if not compared:
                self.emit("    CMP R0, R1")
            table = SIGNED_BRANCHES if compare_type == "int" else UNSIGNED_BRANCHES
            self.emit(f"    {table[operator]} {true_label}")
        elif operator in ("<", "<=", ">", ">="):
            if operator in (">", ">="):
                self.emit("    MOV R2, R0")
                self.emit("    MOV R0, R1")
                self.emit("    MOV R1, R2")
            if compare_type == "float":
                self.emit("    FCMP R0, R1")
            else:
                if compare_type == "int":
                    self.emit("    LDI R2, 0x8000")
                    self.emit("    XOR R0, R2")
                    self.emit("    XOR R1, R2")
                self.emit("    CMP R0, R1")
            if operator in ("<", ">"):
                self.emit(f"    BCX {true_label}")
            else:
                self.emit(f"    BCX {true_label}")
                self.emit(f"    BZX {true_label}")
        else:
            if not compared:
                self.emit("    FCMP R0, R1" if compare_type == "float" else "    CMP R0, R1")
            if operator == "==":
                self.emit(f"    BZX {true_label}")
            elif operator == "!=":
                self.emit(f"    BZX {false_label}")
                self.emit(f"    JMPX {true_label}")
            else:
                raise self.error(line, f"unsupported comparison {operator!r}")
        self.emit(f"    JMPX {false_label}")
        self.emit(f"{true_label}:")
        self.emit("    LDI R0, 1")
        self.emit(f"    JMPX {end_label}")
        self.emit(f"{false_label}:")
        self.emit("    LDI R0, 0")
        self.emit(f"{end_label}:")

    def _boolean_not(self, c_type: str) -> None:
        false_label = self.label("not_false")
        end_label = self.label("not_end")
        if c_type == "float":
            self.emit("    LDI R1, 0")
            self.emit("    FCMP R0, R1")
        else:
            self.emit("    CMP R0, 0")
        self.emit(f"    BZX {end_label}")
        self.emit(f"{false_label}:")
        self.emit("    LDI R0, 0")
        self.emit(f"    JMPX {end_label}_done")
        self.emit(f"{end_label}:")
        self.emit("    LDI R0, 1")
        self.emit(f"{end_label}_done:")

    def _generate_shift(self, helper: str) -> None:
        loop, done = f"{helper}_loop", f"{helper}_done"
        self.emit(f"{helper}:")
        self.emit("    LDI R2, 0")
        self.emit(f"{loop}:")
        self.emit("    CMP R1, R2")
        self.emit(f"    BZX {done}")
        if helper == "__runtime_shl":
            self.emit("    ADD R0, R0")
            self.emit("    LDI R7, 0xFFFF")
            self.emit("    AND R0, R7")
        elif helper == "__runtime_shr":
            self.emit("    SHR R0")
        else:
            self.emit("    MOV R6, R0")
            self.emit("    LDI R7, 0x8000")
            self.emit("    AND R6, R7")
            self.emit("    SHR R0")
            self.emit("    OR R0, R6")
        self.emit("    DEC R1")
        self.emit(f"    JMPX {loop}")
        self.emit(f"{done}:")
        self.emit("    RET")

    def _generate_helper(self, helper: str) -> None:
        if helper in ("__runtime_shl", "__runtime_shr", "__runtime_sar"):
            self._generate_shift(helper)


def _compile(source: str, filename: str) -> tuple[str, AssemblyImage]:
    parser = _Parser(source, filename)
    globals_, functions = parser.parse()
    standard_calls = _called_functions(parser) - {
        function.name for function in functions
    }
    runtime_sources = sorted(
        {STANDARD_C_SOURCE[name] for name in standard_calls if name in STANDARD_C_SOURCE}
    )
    if runtime_sources:
        library_root = Path(__file__).resolve().parent.parent / "lib"
        library_sources = []
        for runtime_source in runtime_sources:
            library_path = library_root / runtime_source
            try:
                library_sources.append(library_path.read_text(encoding="utf-8"))
            except (OSError, UnicodeError) as exc:
                raise CCompilerError(
                    f"{filename}: cannot read standard C library source {library_path}: {exc}"
                ) from exc
        parser = _Parser(source + "\n" + "\n".join(library_sources), filename)
        globals_, functions = parser.parse()
    generator = _CodeGenerator(globals_, functions, filename, parser.records)
    generator.prototypes = parser.prototypes
    generator.enum_constants = parser.enum_constants
    assembly = generator.generate()
    try:
        source_directory = Path(filename).resolve().parent
        image = assemble(assembly, base_directory=source_directory)
    except AssemblyError as exc:
        raise CCompilerError(f"{filename}: generated SC-16 assembly is invalid: {exc}") from exc
    return assembly, image


def _called_functions(parser: _Parser) -> set[str]:
    names: set[str] = set()

    def visit(value: Any) -> None:
        if isinstance(value, Expr):
            if value.kind == "call":
                names.add(str(value.value))
            for child in value.children:
                visit(child)
        elif isinstance(value, Stmt):
            visit(value.value)
            visit(value.children)
        elif isinstance(value, Local):
            visit(value.initializer)
        elif isinstance(value, Global):
            visit(value.initializer)
        elif isinstance(value, (list, tuple)):
            for child in value:
                visit(child)

    for global_ in parser.globals:
        visit(global_)
    for function in parser.functions:
        visit(function.body)
    return names


def compile_to_assembly(source: str, filename: str = "<input>") -> str:
    """Compile the supported C90 subset to SC-16 assembly source."""
    assembly, _ = _compile(source, filename)
    return assembly


def compile_source(source: str, filename: str = "<input>") -> AssemblyImage:
    """Compile C source to an assembled SC-16 program image."""
    return _compile(source, filename)[1]


def compile_file(path: str | Path) -> AssemblyImage:
    source_path = Path(path).resolve()
    try:
        source = source_path.read_text(encoding="utf-8-sig")
    except (OSError, UnicodeError) as exc:
        raise CCompilerError(f"cannot read {source_path}: {exc}") from exc
    return compile_source(source, str(source_path))


def _flat_binary(image: AssemblyImage) -> bytes:
    end = max(
        (segment.address + len(segment.data) for segment in image.segments),
        default=0,
    )
    output = bytearray(end)
    for segment in image.segments:
        output[segment.address:segment.address + len(segment.data)] = segment.data
    return bytes(output)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Compile the supported SC-16 C90 subset.")
    parser.add_argument("source", help="input C source file")
    parser.add_argument("-o", "--output", help="output path; defaults to input .bin")
    parser.add_argument(
        "-S",
        "--assembly",
        action="store_true",
        help="emit assembly source instead of a flat binary image",
    )
    args = parser.parse_args(argv)
    source_path = Path(args.source)
    output_path = Path(args.output) if args.output else source_path.with_suffix(
        ".asm" if args.assembly else ".bin"
    )
    try:
        source = source_path.read_text(encoding="utf-8-sig")
        assembly, image = _compile(source, str(source_path))
        if args.assembly:
            output_directory = output_path.resolve().parent
            for library in ("lib_text.asm", "lib_cgfx.asm", "fonts/font5x7.asm"):
                library_path = Path(__file__).resolve().parent / library
                include_path = os.path.relpath(library_path, output_directory).replace("\\", "/")
                assembly = re.sub(
                    rf'(?m)^\.include "[^"]*{re.escape(Path(library).name)}"$',
                    lambda _match, path=include_path: f'.include "{path}"',
                    assembly,
                )
            output_path.write_text(assembly, encoding="utf-8")
        else:
            output_path.write_bytes(_flat_binary(image))
    except (CCompilerError, OSError) as exc:
        print(exc, file=sys.stderr)
        return 1
    print(f"wrote {output_path}")
    return 0


_DIRECTIVE = re.compile(r"^[ \t]*#[ \t]*(\w+)[ \t]*(.*?)[ \t]*$")
_TRIGRAPHS = str.maketrans({
    "=": "#", "/": "\\", "'": "^", "(": "[", ")": "]", "!": "|",
    "<": "{", ">": "}", "-": "~",
})
_TRIGRAPH_PATTERN = re.compile(r"\?\?[=/\'()!<>-]")


def _translation_lines(source: str) -> list[str]:
    source = _TRIGRAPH_PATTERN.sub(
        lambda match: match.group(0)[2].translate(_TRIGRAPHS), source
    )
    source = source.replace("\r\n", "\n").replace("\r", "\n")
    source = re.sub(r"\\\n", "", source)
    return source.split("\n")


def _token_spelling(token: Token) -> str:
    value = token.value
    if value.startswith("<string:") and value.endswith(">"):
        contents = bytes.fromhex(value[8:-1]).decode("utf-8", errors="replace")
        return '"' + contents.replace("\\", "\\\\").replace('"', '\\"') + '"'
    return value


def _macro_tokens(
    macro: _Macro,
    defines: dict[str, _Macro],
    filename: str,
    line: int,
    disabled: frozenset[str],
    depth: int,
) -> list[Token]:
    return _expand_macros(
        [
            Token(token.value, line, filename)
            for token in _lex_raw(macro.replacement, filename)[:-1]
        ],
        defines,
        filename,
        disabled,
        depth + 1,
    )


def _expand_macros(
    tokens: list[Token],
    defines: dict[str, _Macro],
    filename: str,
    disabled: frozenset[str] = frozenset(),
    depth: int = 0,
) -> list[Token]:
    if depth > 100:
        raise CCompilerError(f"{filename}: macro expansion nested too deeply")
    result: list[Token] = []
    index = 0
    while index < len(tokens):
        token = tokens[index]
        token_filename = token.filename or filename
        if token.value == "__FILE__" and token.value not in disabled:
            encoded = token_filename.encode("utf-8").hex()
            result.append(Token(f"<string:{encoded}>", token.line, token_filename))
            index += 1
            continue
        if token.value == "__LINE__" and token.value not in disabled:
            result.append(Token(str(token.line), token.line, token_filename))
            index += 1
            continue
        macro = defines.get(token.value)
        if macro is None or token.value in disabled:
            result.append(token)
            index += 1
            continue
        if macro.parameters is None:
            replacement = _macro_tokens(
                macro, defines, token_filename, token.line, disabled | {token.value}, depth
            )
            result.extend(replacement)
            index += 1
            continue
        if index + 1 >= len(tokens) or tokens[index + 1].value != "(":
            result.append(token)
            index += 1
            continue
        arguments: list[list[Token]] = [[]]
        nesting = 0
        cursor = index + 2
        while cursor < len(tokens):
            value = tokens[cursor].value
            if value == "(":
                nesting += 1
            elif value == ")":
                if nesting == 0:
                    break
                nesting -= 1
            elif value == "," and nesting == 0:
                arguments.append([])
                cursor += 1
                continue
            arguments[-1].append(tokens[cursor])
            cursor += 1
        if cursor == len(tokens):
            raise CCompilerError(
                f"{token_filename}:{token.line}: unterminated macro invocation"
            )
        if not macro.parameters and len(arguments) == 1 and not arguments[0]:
            arguments.clear()
        if len(arguments) != len(macro.parameters):
            raise CCompilerError(
                f"{token_filename}:{token.line}: macro {token.value} expects "
                f"{len(macro.parameters)} argument(s), got {len(arguments)}"
            )
        raw_arguments = dict(zip(macro.parameters, arguments))
        expanded_arguments = {
            name: _expand_macros(value, defines, token_filename, disabled, depth + 1)
            for name, value in raw_arguments.items()
        }
        replacement_tokens = [
            Token(item.value, token.line, token_filename)
            for item in _lex_raw(macro.replacement, token_filename)[:-1]
        ]
        substituted: list[Token] = []
        position = 0
        while position < len(replacement_tokens):
            current = replacement_tokens[position]
            if (
                current.value == "#"
                and position + 1 < len(replacement_tokens)
                and replacement_tokens[position + 1].value in raw_arguments
            ):
                parameter = replacement_tokens[position + 1].value
                text = " ".join(_token_spelling(item) for item in raw_arguments[parameter])
                literal = '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'
                substituted.extend(_lex_raw(literal, token_filename)[:-1])
                position += 2
                continue
            if current.value in raw_arguments:
                use_raw = (
                    (position > 0 and replacement_tokens[position - 1].value == "##")
                    or (
                        position + 1 < len(replacement_tokens)
                        and replacement_tokens[position + 1].value == "##"
                    )
                )
                argument = raw_arguments[current.value] if use_raw else expanded_arguments[current.value]
                substituted.extend(argument)
            else:
                substituted.append(current)
            position += 1
        pasted: list[Token] = []
        position = 0
        while position < len(substituted):
            if substituted[position].value != "##":
                pasted.append(substituted[position])
                position += 1
                continue
            if not pasted or position + 1 >= len(substituted):
                raise CCompilerError(
                    f"{token_filename}:{token.line}: invalid ## in macro replacement"
                )
            left = pasted.pop()
            right = substituted[position + 1]
            spelling = _token_spelling(left) + _token_spelling(right)
            joined = _lex_raw(spelling, token_filename)[:-1]
            if len(joined) != 1:
                raise CCompilerError(
                    f"{token_filename}:{token.line}: ## does not form one token"
                )
            pasted.append(Token(joined[0].value, token.line, token_filename))
            position += 2
        rescanned = _expand_macros(
            pasted,
            defines,
            token_filename,
            disabled | {token.value},
            depth + 1,
        )
        result.extend(rescanned)
        index = cursor + 1
    return result


def _pp_integer(token: Token, filename: str) -> int:
    match = re.fullmatch(
        r"(0[xX][0-9a-fA-F]+|0[0-7]*|[1-9][0-9]*)(?:[uU][lL]?|[lL][uU]?)?",
        token.value,
    )
    if match is None:
        raise CCompilerError(f"{filename}:{token.line}: invalid integer in #if: {token.value!r}")
    value = re.match(r"(0[xX][0-9a-fA-F]+|[0-9]+)", token.value)
    assert value is not None
    digits = value.group(1)
    base = 16 if digits.lower().startswith("0x") else (
        8 if len(digits) > 1 and digits.startswith("0") else 10
    )
    try:
        return int(digits, base)
    except ValueError as exc:
        raise CCompilerError(
            f"{filename}:{token.line}: invalid integer in #if: {token.value!r}"
        ) from exc


def _evaluate_pp_expression(
    expression: str, defines: dict[str, _Macro], filename: str, line: int
) -> int:
    tokens = [Token(token.value, line) for token in _lex_raw(expression, filename)[:-1]]
    defined_tokens: list[Token] = []
    index = 0
    while index < len(tokens):
        if tokens[index].value != "defined":
            defined_tokens.append(tokens[index])
            index += 1
            continue
        index += 1
        parenthesized = index < len(tokens) and tokens[index].value == "("
        if parenthesized:
            index += 1
        if index >= len(tokens) or not re.fullmatch(r"[A-Za-z_]\w*", tokens[index].value):
            raise CCompilerError(f"{filename}:{line}: expected identifier after defined")
        name = tokens[index].value
        index += 1
        if parenthesized:
            if index >= len(tokens) or tokens[index].value != ")":
                raise CCompilerError(f"{filename}:{line}: expected ')' after defined")
            index += 1
        defined_tokens.append(Token("1" if name in defines or name == "__LINE__" else "0", line))
    expanded = _expand_macros(defined_tokens, defines, filename)
    normalized: list[Token] = []
    for token in expanded:
        if re.fullmatch(r"[A-Za-z_]\w*", token.value):
            normalized.append(Token("0", token.line))
        elif token.value.startswith("<string:"):
            raise CCompilerError(f"{filename}:{line}: strings are invalid in #if expressions")
        else:
            normalized.append(token)

    precedence = {
        "||": 1, "&&": 2, "|": 3, "^": 4, "&": 5,
        "==": 6, "!=": 6, "<": 7, "<=": 7, ">": 7, ">=": 7,
        "<<": 8, ">>": 8, "+": 9, "-": 9, "*": 10, "/": 10, "%": 10,
    }
    position = 0

    def primary() -> Any:
        nonlocal position
        if position >= len(normalized):
            raise CCompilerError(f"{filename}:{line}: incomplete #if expression")
        current = normalized[position]
        position += 1
        if current.value in ("+", "-", "!", "~"):
            return ("unary", current.value, primary())
        if current.value == "(":
            value = parse_expression(1)
            if position >= len(normalized) or normalized[position].value != ")":
                raise CCompilerError(f"{filename}:{line}: expected ')' in #if expression")
            position += 1
            return value
        if re.match(r"[0-9]", current.value):
            return ("value", _pp_integer(current, filename))
        raise CCompilerError(
            f"{filename}:{line}: invalid token in #if expression: {current.value!r}"
        )

    def parse_expression(minimum: int) -> Any:
        nonlocal position
        left = primary()
        while position < len(normalized):
            operator = normalized[position].value
            level = precedence.get(operator, 0)
            if level < minimum:
                if operator == "?" and minimum <= 1:
                    position += 1
                    when_true = parse_expression(1)
                    if position >= len(normalized) or normalized[position].value != ":":
                        raise CCompilerError(
                            f"{filename}:{line}: expected ':' in #if conditional expression"
                        )
                    position += 1
                    when_false = parse_expression(1)
                    left = ("conditional", left, when_true, when_false)
                    continue
                break
            position += 1
            right = parse_expression(level + 1)
            left = ("binary", operator, left, right)
        return left

    tree = parse_expression(1)
    if position != len(normalized):
        raise CCompilerError(
            f"{filename}:{line}: unexpected token in #if expression: "
            f"{normalized[position].value!r}"
        )

    def evaluate(node: Any) -> int:
        if node[0] == "value":
            return int(node[1])
        if node[0] == "unary":
            operator, value = node[1], evaluate(node[2])
            return {"+": lambda: value, "-": lambda: -value,
                    "!": lambda: int(not value), "~": lambda: ~value}[operator]()
        if node[0] == "conditional":
            return evaluate(node[2] if evaluate(node[1]) else node[3])
        operator = node[1]
        left = evaluate(node[2])
        if operator == "&&" and not left:
            return 0
        if operator == "||" and left:
            return 1
        right = evaluate(node[3])
        operations = {
            "||": lambda: int(bool(left) or bool(right)),
            "&&": lambda: int(bool(left) and bool(right)),
            "|": lambda: left | right,
            "^": lambda: left ^ right,
            "&": lambda: left & right,
            "==": lambda: int(left == right),
            "!=": lambda: int(left != right),
            "<": lambda: int(left < right),
            "<=": lambda: int(left <= right),
            ">": lambda: int(left > right),
            ">=": lambda: int(left >= right),
            "<<": lambda: left << right,
            ">>": lambda: left >> right,
            "+": lambda: left + right,
            "-": lambda: left - right,
            "*": lambda: left * right,
            "/": lambda: (
                abs(left) // abs(right) * (-1 if (left < 0) != (right < 0) else 1)
            ),
            "%": lambda: left - (
                abs(left) // abs(right) * (-1 if (left < 0) != (right < 0) else 1)
            ) * right,
        }
        try:
            return operations[operator]()
        except (KeyError, ZeroDivisionError, ValueError, OverflowError) as exc:
            raise CCompilerError(f"{filename}:{line}: invalid operation in #if expression") from exc

    return evaluate(tree)


def _preprocess(
    source: str,
    filename: str,
    defines: dict[str, _Macro],
    depth: int = 0,
) -> str:
    if depth > 8:
        raise CCompilerError(f"{filename}: #include nested too deeply")
    lines = _translation_lines(source)
    output: list[str] = []
    conditionals: list[dict[str, bool]] = []
    active = True
    number = 0
    line_offset = 0
    diagnostic_filename = filename
    while number < len(lines):
        text = lines[number]
        number += 1
        logical_line = number + line_offset
        directive = _DIRECTIVE.match(text)
        if directive is None:
            output.append(text if active else "")
            continue
        name, rest = directive.groups()
        rest = rest.strip()
        where = f"{diagnostic_filename}:{logical_line}"
        if name in ("if", "ifdef", "ifndef"):
            parent_active = active
            if name == "if":
                selected = (
                    bool(_evaluate_pp_expression(rest, defines, filename, logical_line))
                    if active else False
                )
            else:
                identifier = re.fullmatch(r"[A-Za-z_]\w*", rest)
                if identifier is None:
                    raise CCompilerError(f"{where}: expected one identifier after #{name}")
                selected = identifier.group(0) in defines
                if name == "ifndef":
                    selected = not selected
                selected = selected and active
            conditionals.append({
                "parent": parent_active,
                "taken": selected,
                "else": False,
            })
            active = selected
            output.append("")
            continue
        if name == "elif":
            if not conditionals or conditionals[-1]["else"]:
                raise CCompilerError(f"{where}: #elif without matching #if")
            frame = conditionals[-1]
            selected = (
                bool(_evaluate_pp_expression(rest, defines, filename, logical_line))
                if frame["parent"] and not frame["taken"] else False
            )
            frame["taken"] = frame["taken"] or selected
            active = frame["parent"] and selected
            output.append("")
            continue
        if name in ("else", "endif"):
            if not conditionals:
                raise CCompilerError(f"{where}: #{name} without matching #if")
            if name == "else":
                frame = conditionals[-1]
                if frame["else"]:
                    raise CCompilerError(f"{where}: duplicate #else")
                frame["else"] = True
                active = frame["parent"] and not frame["taken"]
                frame["taken"] = True
            else:
                frame = conditionals.pop()
                active = frame["parent"]
            output.append("")
            continue
        if not active:
            output.append("")
            continue
        if name == "define":
            definition = re.match(r"([A-Za-z_]\w*)(.*)$", rest)
            if definition is None:
                raise CCompilerError(f"{where}: invalid #define")
            macro_name, tail = definition.groups()
            parameters = None
            replacement = tail.lstrip()
            if tail.startswith("("):
                signature = re.match(r"\(([^)]*)\)(.*)$", tail)
                if signature is None:
                    raise CCompilerError(f"{where}: invalid function-like macro definition")
                parameter_text, replacement = signature.groups()
                parameters = tuple(item.strip() for item in parameter_text.split(",") if item.strip())
                if (
                    (parameter_text.strip() and len(parameters) != len(parameter_text.split(",")))
                    or any(not re.fullmatch(r"[A-Za-z_]\w*", item) for item in parameters)
                    or len(set(parameters)) != len(parameters)
                ):
                    raise CCompilerError(f"{where}: invalid macro parameter list")
                replacement = replacement.lstrip()
            replacement = re.sub(r"/\*.*?\*/", " ", replacement)
            macro = _Macro(parameters, replacement)
            previous = defines.get(macro_name)
            if previous is not None and previous != macro:
                raise CCompilerError(f"{where}: incompatible redefinition of {macro_name}")
            defines[macro_name] = macro
        elif name == "undef":
            identifier = re.fullmatch(r"[A-Za-z_]\w*", rest)
            if identifier is None:
                raise CCompilerError(f"{where}: invalid #undef")
            defines.pop(identifier.group(0), None)
        elif name == "include":
            expanded = _expand_macros(_lex_raw(rest, filename)[:-1], defines, filename)
            if len(expanded) == 1 and expanded[0].value.startswith("<string:"):
                target = bytes.fromhex(expanded[0].value[8:-1]).decode("utf-8")
                quoted = True
            else:
                spelling = "".join(_token_spelling(item) for item in expanded)
                match = re.fullmatch(r'"([^"]+)"|<([^>]+)>', spelling)
                if match is None:
                    raise CCompilerError(f"{where}: invalid #include")
                target = match.group(1) or match.group(2)
                quoted = match.group(1) is not None
            candidates = []
            if quoted:
                candidates.append(Path(filename).resolve().parent / target)
            candidates.append(Path(__file__).resolve().parent.parent / "lib" / target)
            found = next((path for path in candidates if path.is_file()), None)
            if found is None:
                raise CCompilerError(f"{where}: cannot find include file {target!r}")
            try:
                included = found.read_text(encoding="utf-8")
            except (OSError, UnicodeError) as exc:
                raise CCompilerError(f"{where}: cannot read include file {target!r}: {exc}") from exc
            output.append(
                f"\0SC16_LINE:1:{str(found).encode('utf-8').hex()}"
            )
            output.append(_preprocess(included, str(found), defines, depth + 1))
            output.append(
                f"\0SC16_LINE:{logical_line}:"
                f"{diagnostic_filename.encode('utf-8').hex()}"
            )
        elif name == "error":
            raise CCompilerError(f"{where}: #error {rest}".rstrip())
        elif name == "pragma":
            pass
        elif name == "line":
            expanded = _expand_macros(_lex_raw(rest, filename)[:-1], defines, filename)
            rest = " ".join(_token_spelling(token) for token in expanded)
            line_directive = re.fullmatch(r'([0-9]+)(?:\s+("(?:\\.|[^"\\])*"))?', rest)
            if line_directive is None or int(line_directive.group(1)) < 1:
                raise CCompilerError(f"{where}: invalid #line")
            line_number = int(line_directive.group(1))
            if line_directive.group(2) is not None:
                try:
                    mapped_filename = ast.literal_eval(line_directive.group(2))
                except (SyntaxError, ValueError) as exc:
                    raise CCompilerError(f"{where}: invalid #line filename") from exc
                if not isinstance(mapped_filename, str):
                    raise CCompilerError(f"{where}: invalid #line filename")
                diagnostic_filename = mapped_filename
            output.append(
                f"\0SC16_LINE:{line_number}:"
                f"{diagnostic_filename.encode('utf-8').hex()}"
            )
            line_offset = line_number - (number + 1)
            continue
        else:
            raise CCompilerError(f"{where}: unsupported directive #{name}")
        output.append("")
    if conditionals:
        raise CCompilerError(f"{filename}: unterminated conditional directive")
    return "\n".join(output)


def _lex(source: str, filename: str) -> list[Token]:
    timestamp = datetime.datetime.now()
    defines: dict[str, _Macro] = {
        "__STDC__": _Macro(None, "1"),
        "__FILE__": _Macro(None, '"' + filename.replace("\\", "\\\\").replace('"', '\\"') + '"'),
        "__DATE__": _Macro(None, '"' + timestamp.strftime("%b %d %Y") + '"'),
        "__TIME__": _Macro(None, '"' + timestamp.strftime("%H:%M:%S") + '"'),
    }
    source = _preprocess(source, filename, defines)
    tokens = _lex_raw(source, filename)
    if defines:
        tokens = _expand_macros(tokens, defines, filename)
    concatenated: list[Token] = []
    for token in tokens:
        if (
            concatenated
            and token.value.startswith("<string:")
            and token.value.endswith(">")
            and concatenated[-1].value.startswith("<string:")
            and concatenated[-1].value.endswith(">")
        ):
            previous = concatenated[-1]
            left = bytes.fromhex(previous.value[8:-1])
            right = bytes.fromhex(token.value[8:-1])
            concatenated[-1] = Token(
                f"<string:{(left + right).hex()}>", previous.line
            )
        else:
            concatenated.append(token)
    return concatenated


if __name__ == "__main__":
    raise SystemExit(main())
