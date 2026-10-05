"""A small, self-contained C89 subset compiler for the SC-8."""

import argparse
import ast
import decimal
import os
import re
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Sequence

from assembler import AssemblyError, AssemblyImage, assemble

WORD_MASK = 0xFFFF
CODE_BASE = 0x4800
GLOBAL_BASE = 0xE800
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
    "gfx_irq_init": ("void", ()),
    "gfx_irq_ticks": ("unsigned int", ()),
    "gfx_irq_keys": ("unsigned int", ()),
}
LIBRARY_FUNCTIONS = {**SCREEN_LIBRARY_FUNCTIONS, **GRAPHICS_LIBRARY_FUNCTIONS}
LIBRARY_FILES = {
    **{name: "lib_text.asm" for name in SCREEN_LIBRARY_FUNCTIONS},
    **{name: "lib_cgfx.asm" for name in GRAPHICS_LIBRARY_FUNCTIONS},
}


class CCompilerError(ValueError):
    """Raised for unsupported or invalid input in the supported C subset."""


@dataclass(frozen=True)
class Token:
    value: str
    line: int


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


_MULTI_OPERATORS = (
    "++", "--", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=",
    "/=", "%=", "&=", "|=", "^=", "<<", ">>",
)
_SINGLE_OPERATORS = set("{}();,:[]=+-*/%<>!~&|^")
_STORAGE_WORDS = {"static", "extern", "register", "auto", "const", "volatile"}
_TYPE_WORDS = {"int", "void", "char", "short", "long", "float", "double", "signed", "unsigned"}
_PRECEDENCE = {
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
    "+": 8,
    "-": 8,
    "<<": 8,
    ">>": 8,
    "*": 9,
    "/": 9,
    "%": 9,
}
_ASSIGNMENTS = {"=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^="}


def _type_size(c_type: str) -> int:
    if c_type.endswith("*"):
        return 3
    return 1 if c_type in ("char", "unsigned char") else 2


def _object_size(c_type: str) -> int:
    return 3 if c_type.endswith("*") else 2


def _lex_raw(source: str, filename: str) -> list[Token]:
    tokens: list[Token] = []
    position = 0
    line = 1
    while position < len(source):
        char = source[position]
        if char.isspace():
            if char == "\n":
                line += 1
            position += 1
            continue
        if source.startswith("//", position):
            end = source.find("\n", position)
            position = len(source) if end < 0 else end
            continue
        if source.startswith("/*", position):
            end = source.find("*/", position + 2)
            if end < 0:
                raise CCompilerError(f"{filename}:{line}: unterminated comment")
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
            tokens.append(Token(literal, line))
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
                    raise CCompilerError(f"{filename}:{quote_line}: newline in string literal")
                if escaped:
                    escaped = False
                elif current == "\\":
                    escaped = True
                elif current == '"':
                    position += 1
                    break
                position += 1
            else:
                raise CCompilerError(f"{filename}:{quote_line}: unterminated string literal")
            try:
                value = ast.literal_eval(source[quote_start:position])
            except (SyntaxError, ValueError) as exc:
                raise CCompilerError(f"{filename}:{quote_line}: invalid string literal") from exc
            if not isinstance(value, str):
                raise CCompilerError(f"{filename}:{quote_line}: invalid string literal")
            tokens.append(Token(f"<string:{value.encode('utf-8').hex()}>", quote_line))
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
                        f"{filename}:{start_line}: newline in character/string literal"
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
                    f"{filename}:{start_line}: unterminated character/string literal"
                )
            literal = source[start:position]
            try:
                value = ast.literal_eval(literal)
            except (SyntaxError, ValueError) as exc:
                raise CCompilerError(
                    f"{filename}:{start_line}: invalid character constant"
                ) from exc
            if not isinstance(value, str) or len(value) != 1:
                raise CCompilerError(
                    f"{filename}:{start_line}: character constants must contain one character"
                )
            tokens.append(Token(str(ord(value)), start_line))
            continue
        if char.isalpha() or char == "_":
            start = position
            position += 1
            while position < len(source) and (
                source[position].isalnum() or source[position] == "_"
            ):
                position += 1
            tokens.append(Token(source[start:position], line))
            continue
        if char.isdigit():
            start = position
            position += 1
            while position < len(source) and (
                source[position].isalnum() or source[position] == "_"
            ):
                position += 1
            tokens.append(Token(source[start:position], line))
            continue
        operator = next(
            (candidate for candidate in _MULTI_OPERATORS
             if source.startswith(candidate, position)),
            None,
        )
        if operator is not None:
            tokens.append(Token(operator, line))
            position += len(operator)
        elif char in _SINGLE_OPERATORS:
            tokens.append(Token(char, line))
            position += 1
        else:
            raise CCompilerError(
                f"{filename}:{line}: unsupported character {char!r}"
            )
    tokens.append(Token("<eof>", line))
    return tokens


class _Parser:
    def __init__(self, source: str, filename: str):
        self.filename = filename
        self.tokens = _lex(source, filename)
        self.position = 0
        self.globals: list[Global] = []
        self.functions: list[Function] = []
        self.prototypes: dict[str, tuple[str, int]] = {}
        self._local_count = 0

    @property
    def token(self) -> Token:
        return self.tokens[self.position]

    def peek(self, distance: int = 1) -> str:
        index = min(self.position + distance, len(self.tokens) - 1)
        return self.tokens[index].value

    def fail(self, message: str) -> CCompilerError:
        return CCompilerError(f"{self.filename}:{self.token.line}: {message}")

    def take(self) -> Token:
        token = self.token
        self.position += 1
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
        unsigned = False
        signed = False
        base = None
        while self.token.value in _STORAGE_WORDS | _TYPE_WORDS:
            if self.token.value == "unsigned":
                unsigned = True
            elif self.token.value == "signed":
                signed = True
            elif self.token.value in _TYPE_WORDS - {"signed", "unsigned"}:
                current = self.take().value
                if current not in ("short", "long"):
                    if base is not None and base != current:
                        raise self.fail("invalid combination of type specifiers")
                    base = current
                continue
            self.take()
        if base is None:
            raise self.fail("expected int or void type")
        if signed and unsigned:
            raise self.fail("both signed and unsigned were specified")
        if base == "void":
            if unsigned or signed:
                raise self.fail("void cannot be signed or unsigned")
            return "void"
        if base in ("float", "double"):
            if unsigned or signed:
                raise self.fail("floating types cannot be signed or unsigned")
            return "float"
        if base == "char":
            return "unsigned char" if unsigned else "char"
        return "unsigned int" if unsigned else "int"

    def _pointer_type(self, c_type: str) -> str:
        pointer_count = 0
        while self.accept("*"):
            pointer_count += 1
        return c_type + ("*" * pointer_count)

    def _declarator_suffix(self, c_type: str) -> tuple[str, int]:
        if not self.accept("["):
            return c_type, 0
        size_token = self.take()
        if not size_token.value.isdigit():
            raise CCompilerError(
                f"{self.filename}:{size_token.line}: array size must be a positive integer constant"
            )
        size = int(size_token.value, 10)
        if size <= 0 or size > 0xFFFF:
            raise CCompilerError(f"{self.filename}:{size_token.line}: array size is out of range")
        self.expect("]")
        if c_type.endswith("*"):
            raise self.fail("arrays of pointers are not supported")
        return c_type, size

    def _initializer(self) -> Expr:
        if self.accept("{"):
            values = []
            while self.token.value != "}":
                values.append(self._initializer())
                if not self.accept(","):
                    break
            self.expect("}")
            return Expr("init_list", children=tuple(values), line=self.token.line)
        return self.parse_expression()

    def parse(self) -> tuple[list[Global], list[Function]]:
        while self.token.value != "<eof>":
            self.parse_external_declaration()
        if not any(function.name == "main" for function in self.functions):
            raise CCompilerError(f"{self.filename}: no definition for main()")
        return self.globals, self.functions

    def parse_external_declaration(self) -> None:
        c_type = self.parse_type()
        declarator_type = self._pointer_type(c_type)
        name_token = self.take()
        if not re.fullmatch(r"[A-Za-z_]\w*", name_token.value):
            raise self.fail("expected an external identifier")
        name = name_token.value
        if self.accept("("):
            parameters = self.parse_parameters()
            if self.accept(";"):
                previous = self.prototypes.get(name)
                signature = (declarator_type, len(parameters))
                if previous is not None and previous != signature:
                    raise CCompilerError(
                        f"{self.filename}:{name_token.line}: conflicting declaration of {name}"
                    )
                self.prototypes[name] = signature
                return
            if self.token.value != "{":
                raise self.fail("expected ';' or function body")
            if any(function.name == name for function in self.functions):
                raise CCompilerError(
                    f"{self.filename}:{name_token.line}: duplicate definition of {name}"
                )
            previous = self.prototypes.get(name)
            signature = (declarator_type, len(parameters))
            if previous is not None and previous != signature:
                raise CCompilerError(
                    f"{self.filename}:{name_token.line}: definition of {name} conflicts with declaration"
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
            self.prototypes[name] = (declarator_type, len(parameters))
            return
        self.parse_global_declarators(c_type, name_token, declarator_type)

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
            if c_type == "void":
                raise self.fail("void is only valid as an empty parameter list")
            c_type = self._pointer_type(c_type)
            name = None
            if re.fullmatch(r"[A-Za-z_]\w*", self.token.value):
                name = self.take().value
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
            initializer = self._initializer() if self.accept("=") else None
            self.globals.append(
                Global(name, current_type, initializer, name_token.line, array_size)
            )
            if not self.accept(","):
                break
            current_type = self._pointer_type(c_type)
            name_token = self.take()
            if not re.fullmatch(r"[A-Za-z_]\w*", name_token.value):
                raise self.fail("expected a global variable name")
        self.expect(";")

    def _is_declaration(self) -> bool:
        return self.token.value in _STORAGE_WORDS | _TYPE_WORDS

    def parse_compound(self) -> Stmt:
        brace = self.expect("{")
        statements: list[Stmt] = []
        declarations_allowed = True
        while self.token.value != "}":
            if self.token.value == "<eof>":
                raise self.fail("unterminated compound statement")
            if self._is_declaration():
                if not declarations_allowed:
                    raise self.fail("C89 declarations must precede statements in a block")
                statements.extend(self.parse_local_declarations())
            else:
                declarations_allowed = False
                statements.append(self.parse_statement())
        self.take()
        return Stmt("block", children=statements, line=brace.line)

    def parse_local_declarations(self) -> list[Stmt]:
        c_type = self.parse_type()
        declarations = []
        while True:
            current_type = self._pointer_type(c_type)
            token = self.take()
            if not re.fullmatch(r"[A-Za-z_]\w*", token.value):
                raise self.fail("expected a local variable name")
            current_type, array_size = self._declarator_suffix(current_type)
            if current_type == "void":
                raise self.fail("local objects cannot have void type")
            initializer = self._initializer() if self.accept("=") else None
            local = Local(
                token.value, current_type, initializer, self._local_count, token.line, array_size
            )
            self._local_count += (
                _object_size(current_type)
                if array_size == 0
                else array_size * _type_size(current_type)
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
                    case_value = self.parse_expression()
                    self.expect(":")
                    if case_value.kind not in ("constant", "unary", "binary"):
                        raise self.fail("case labels must be constant integer expressions")
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
        expression = self.parse_expression()
        self.expect(";")
        return Stmt("expression", children=[expression], line=token.line)

    def parse_expression(self, minimum: int = 1) -> Expr:
        left = self.parse_unary()
        while True:
            operator = self.token.value
            if operator in _ASSIGNMENTS and minimum <= 1:
                token = self.take()
                if left.kind != "name" and not (
                    left.kind == "index" or (left.kind == "unary" and left.value == "*")
                ):
                    raise CCompilerError(
                        f"{self.filename}:{token.line}: assignment requires a scalar variable"
                    )
                right = self.parse_expression(1)
                left = Expr("assign", (operator, left), (right,), token.line)
                continue
            precedence = _PRECEDENCE.get(operator, 0)
            if precedence < minimum:
                break
            token = self.take()
            right = self.parse_expression(precedence + 1)
            left = Expr("binary", operator, (left, right), token.line)
        return left

    def parse_unary(self) -> Expr:
        token = self.token
        if token.value in ("+", "-", "!", "~", "&", "*", "++", "--"):
            operator = self.take().value
            child = self.parse_unary()
            if operator in ("++", "--") and child.kind != "name":
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
                if expression.kind != "name":
                    raise CCompilerError(
                        f"{self.filename}:{expression.line}: only direct function calls are supported"
                    )
                arguments = []
                if self.token.value != ")":
                    while True:
                        arguments.append(self.parse_expression())
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
            match = re.fullmatch(r"(0[xX][0-9a-fA-F]+|[0-9]+)", token.value)
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


class _CodeGenerator:
    def __init__(
        self,
        globals_: list[Global],
        functions: list[Function],
        filename: str,
    ):
        self.globals = globals_
        self.functions = functions
        self.filename = filename
        self.lines: list[str] = []
        self.label_index = 0
        self.stack_depth = 0
        self.frame_size = 0
        self.scopes: list[dict[str, _Variable]] = []
        self.break_labels: list[str] = []
        self.continue_labels: list[str] = []
        self.used_helpers: set[str] = set()
        self.function_names = {function.name for function in functions}
        self.functions_by_name = {function.name: function for function in functions}
        self.global_addresses: dict[str, int] = {}
        self.global_variables: dict[str, Global] = {}
        self.string_data: list[tuple[str, bytes]] = []
        self.current_function: Function | None = None
        self.prototypes: dict[str, tuple[str, int]] = {}
        self.used_libraries: set[str] = set()

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
            library_path = Path(__file__).resolve().parent / library
            include_path = os.path.relpath(library_path, source_directory).replace("\\", "/")
            self.emit(f'.include "{include_path}"')
        return "\n".join(self.lines) + "\n"

    def _global_size(self, global_: Global) -> int:
        if global_.array_size:
            return global_.array_size * _type_size(global_.c_type)
        return _object_size(global_.c_type)

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
        size = self._global_size(global_)
        initializer = global_.initializer
        if global_.array_size:
            if initializer is None:
                self._emit_bytes(bytes(size))
                return
            if initializer.kind == "string":
                if global_.c_type not in ("char", "unsigned char"):
                    raise self.error(global_.line, "string initializer requires a char array")
                data = initializer.value
                if len(data) > size:
                    raise self.error(global_.line, "string initializer is too long for array")
                self._emit_bytes(data + bytes(size - len(data)))
                return
            if initializer.kind != "init_list":
                raise self.error(global_.line, "array initializer must be a brace list or string")
            if len(initializer.children) > global_.array_size:
                raise self.error(global_.line, "too many elements in array initializer")
            values = [self._constant(item) for item in initializer.children]
            values.extend([0] * (global_.array_size - len(values)))
            if _type_size(global_.c_type) == 1:
                self._emit_bytes(bytes(value & 0xFF for value in values))
            else:
                self._emit_bytes(b"".join(value.to_bytes(2, "big") for value in values))
            return
        if initializer is not None and initializer.kind == "string":
            raise self.error(global_.line, "a string literal cannot initialize a scalar")
        value = self._constant(initializer) if initializer else 0
        if global_.c_type in ("char", "unsigned char"):
            value &= 0xFF
        if global_.c_type.endswith("*") and not 0 <= value < (1 << 19):
            raise self.error(global_.line, "pointer initializer is outside the 19-bit address range")
        self._emit_bytes(value.to_bytes(_object_size(global_.c_type), "big"))

    def _constant(self, expression: Expr | None) -> int:
        if expression is None:
            return 0
        if expression.kind == "constant":
            return int(expression.value) & WORD_MASK
        if expression.kind == "float_constant":
            return int(expression.value) & WORD_MASK
        if expression.kind == "unary":
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

    @staticmethod
    def _signed(value: int) -> int:
        return value - 0x10000 if value & 0x8000 else value

    def _generate_function(self, function: Function) -> None:
        self.current_function = function
        declaration = self.prototypes.get(function.name)
        if declaration is not None and declaration != (
            function.return_type,
            len(function.parameters),
        ):
            raise self.error(function.line, f"definition of {function.name!r} conflicts with declaration")
        if function.name == "main" and function.parameters:
            raise self.error(function.line, "main() must not have parameters")
        if function.local_count > FRAME_SIZE_LIMIT:
            raise self.error(
                function.line,
                f"function frame exceeds {FRAME_SIZE_LIMIT} bytes",
            )
        self.frame_size = function.local_count
        parameter_bytes = sum(_object_size(c_type) for c_type, _ in function.parameters)
        if self.frame_size + 2 + parameter_bytes > STACK_OFFSET_LIMIT:
            raise self.error(function.line, "parameters exceed stack-relative address range")
        self.stack_depth = 0
        self.scopes = [{}]
        parameter_offset = self.frame_size + 2
        for c_type, name in function.parameters:
            if name is None:
                parameter_offset += _object_size(c_type)
                continue
            if name in self.scopes[0]:
                raise self.error(function.line, f"duplicate parameter name {name!r}")
            self.scopes[0][name] = _Variable(
                parameter_offset=parameter_offset,
                c_type=c_type,
            )
            parameter_offset += _object_size(c_type)
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
            self.emit(f"    LDW {register}, R7")
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
            self.emit(f"    STW R7, {register}")
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

    def _convert_loaded(self, register: str, c_type: str) -> None:
        if c_type in ("char", "unsigned char"):
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
            self._expression(statement.children[0])
            return
        if kind == "return":
            expression = statement.children[0]
            if self.current_function is not None:
                if self.current_function.return_type == "void" and expression is not None:
                    raise self.error(statement.line, "void functions cannot return a value")
                if self.current_function.return_type != "void" and expression is None:
                    raise self.error(statement.line, "int functions must return a value")
            if expression is None:
                self.emit("    LDI R0, 0")
            else:
                self._expression(expression)
            self._emit_epilogue()
            return
        if kind == "if":
            condition, then_branch, else_branch = statement.children
            else_label = self.label("else")
            end_label = self.label("endif")
            self._expression(condition)
            if self._expression_type(condition) == "float":
                self.emit("    LDI R1, 0")
                self.emit("    FCMP R0, R1")
            else:
                self.emit("    LDI R1, 0")
                self.emit("    CMP R0, R1")
            self.emit(f"    BZX {else_label}")
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
            self._expression(condition)
            self.emit("    LDI R1, 0")
            self.emit("    CMP R0, R1")
            self.emit(f"    BZX {end}")
            self._statement(body)
            self.emit(f"{continue_label}:")
            self.emit(f"    JMPX {start}")
        elif kind == "do":
            body, condition = statement.children
            self.emit(f"{start}:")
            self._statement(body)
            self.emit(f"{continue_label}:")
            self._expression(condition)
            self.emit("    LDI R1, 0")
            self.emit("    CMP R0, R1")
            self.emit(f"    BZX {end}")
            self.emit(f"    JMPX {start}")
        else:
            initializer, condition, increment, body = statement.children
            if initializer is not None:
                self._expression(initializer)
            self.emit(f"{start}:")
            if condition is not None:
                self._expression(condition)
                self.emit("    LDI R1, 0")
                self.emit("    CMP R0, R1")
                self.emit(f"    BZX {end}")
            self._statement(body)
            self.emit(f"{continue_label}:")
            if increment is not None:
                self._expression(increment)
            self.emit(f"    JMPX {start}")
        self.emit(f"{end}:")
        self.break_labels.pop()
        self.continue_labels.pop()

    def _switch_statement(self, statement: Stmt) -> None:
        expression = statement.children[0]
        value_type = self._expression_type(expression)
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
            self.emit("    FCMP R0, R1" if value_type == "float" else "    CMP R0, R1")
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
        initializer = local.initializer
        if initializer is None:
            return
        if initializer.kind == "string":
            if local.c_type not in ("char", "unsigned char"):
                raise self.error(line, "string initializer requires a char array")
            string_data = initializer.value
            if isinstance(string_data, str):
                values = list(string_data.encode("utf-8")) + [0]
            else:
                values = list(string_data)
        elif initializer.kind == "init_list":
            values = [self._constant(value) for value in initializer.children]
        else:
            raise self.error(line, "array initializer must be a brace list or string")
        if len(values) > local.array_size:
            raise self.error(line, "too many values in array initializer")
        width = _type_size(local.c_type)
        for index, value in enumerate(values):
            self.emit("    MOVSP R7")
            offset = int(variable.local_offset or 0) + self.stack_depth + index * width
            if offset:
                self.emit(f"    LDI R6, {offset}")
                self.emit("    ADD R7, R6")
            self.emit(f"    LDI R0, {value & (0xFF if width == 1 else WORD_MASK)}")
            self.emit(f"    {'ST' if width == 1 else 'STW'} R7, R0")

    def _expression_type(self, expression: Expr) -> str:
        if expression.kind == "constant":
            return "int"
        if expression.kind == "float_constant":
            return "float"
        if expression.kind == "string":
            return "char*"
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
        if expression.kind == "assign":
            target = expression.value[1]
            return self._expression_type(target)
        if expression.kind == "unary":
            operator = str(expression.value)
            child_type = self._expression_type(expression.children[0])
            if operator == "&":
                return child_type + "*"
            if operator == "*":
                if not child_type.endswith("*"):
                    raise self.error(expression.line, "cannot dereference a non-pointer")
                return child_type[:-1]
            return "int" if operator == "!" else child_type
        if expression.kind == "postfix":
            return self._expression_type(expression.children[0])
        if expression.kind == "binary":
            operator = str(expression.value)
            left_type = self._expression_type(expression.children[0])
            right_type = self._expression_type(expression.children[1])
            if operator in ("==", "!=", "<", "<=", ">", ">=", "&&", "||"):
                return "int"
            if left_type == "float" or right_type == "float":
                return "float"
            if left_type.endswith("*"):
                if operator == "-" and right_type.endswith("*"):
                    return "int"
                return left_type
            if right_type.endswith("*") and operator == "+":
                return right_type
            if left_type.startswith("unsigned") or right_type.startswith("unsigned"):
                return "unsigned int"
            return "int"
        raise self.error(expression.line, "could not determine expression type")

    def _lvalue_address(self, expression: Expr, line: int) -> str:
        if expression.kind == "name":
            variable = self._lookup(str(expression.value), line)
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
            self._expression(base)
            self.emit("    ADJSP -3")
            self.emit("    MOVSP R7")
            self._emit_store_pointer("R0")
            self.stack_depth += 3
            self._expression(index)
            if _type_size(element_type) == 2:
                self.emit("    ADD R0, R0")
            self.emit("    MOV R2, R0")
            self.emit("    MOVSP R7")
            self._emit_load_pointer("R1")
            self.emit("    ADD R1, R2")
            self.emit("    MOV R0, R1")
            self.emit("    ADJSP 3")
            self.stack_depth -= 3
            return element_type
        if expression.kind == "unary" and expression.value == "*":
            self._expression(expression.children[0])
            return self._expression_type(expression)
        raise self.error(line, "expression is not assignable")

    def _load_lvalue(self, expression: Expr, line: int) -> None:
        if expression.kind == "name":
            variable = self._lookup(str(expression.value), line)
            if variable.array_size:
                raise self.error(line, "array expression requires an index")
            self._load_variable(variable, "R0", line)
            return
        c_type = self._lvalue_address(expression, line)
        self.emit("    MOV R7, R0")
        if c_type.endswith("*"):
            self._emit_load_pointer("R0")
        else:
            self.emit(f"    {'LD' if _type_size(c_type) == 1 else 'LDW'} R0, R7")
        self._convert_loaded("R0", c_type)

    def _store_lvalue(
        self, expression: Expr, register: str, source_type: str, line: int
    ) -> None:
        c_type = self._expression_type(expression)
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
            self.emit("    LDI R6, 3")
            self.emit("    ADD R7, R6")
            self._emit_load_pointer("R1")
            self.emit("    MOVSP R7")
            self._emit_load_pointer("R0")
            self.emit("    MOV R7, R0")
            self._emit_store_pointer("R1")
            self.emit("    ADJSP 6")
            self.stack_depth -= 6
            return
        self.emit(f"    PUSH {register}")
        self.stack_depth += 2
        self._lvalue_address(expression, line)
        self.emit("    MOV R7, R0")
        self.emit("    LDWS R1, 0")
        self.emit(f"    {'ST' if _type_size(c_type) == 1 else 'STW'} R7, R1")
        self.emit("    ADJSP 2")
        self.stack_depth -= 2

    def _expression(self, expression: Expr) -> None:
        kind = expression.kind
        if kind == "constant":
            self.emit(f"    LDI R0, {int(expression.value) & WORD_MASK}")
            return
        if kind == "float_constant":
            self.emit(f"    LDI R0, 0x{int(expression.value) & WORD_MASK:04X}")
            return
        if kind == "string":
            self.emit(f"    LDA R0, {expression.value}")
            return
        if kind == "name":
            variable = self._lookup(str(expression.value), expression.line)
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
                if declared is not None and declared[1] != len(parameter_types):
                    raise self.error(expression.line, f"invalid declaration for {name}")
                self.used_libraries.add(LIBRARY_FILES[name])
            expected = len(parameter_types)
            if len(arguments) != expected:
                raise self.error(expression.line, f"{name}() expects {expected} argument(s)")
            if len(arguments) > 63:
                raise self.error(expression.line, "too many function arguments")
            argument_bytes = 0
            for index in range(len(arguments) - 1, -1, -1):
                argument = arguments[index]
                self._expression(argument)
                width = _object_size(parameter_types[index])
                if width == 3:
                    self.emit("    ADJSP -3")
                    self.emit("    MOVSP R7")
                    self._emit_store_pointer("R0")
                else:
                    self.emit("    PUSH R0")
                self.stack_depth += width
                argument_bytes += width
            if name in LIBRARY_FUNCTIONS and name not in self.function_names:
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
            if operator == "=":
                self._expression(expression.children[0])
            else:
                self._load_lvalue(target, expression.line)
                self.emit("    PUSH R0")
                self.stack_depth += 2
                self._expression(expression.children[0])
                self.emit("    MOV R1, R0")
                self.emit("    LDWS R0, 0")
                self._binary_operation(str(operator[:-1]), expression.line, target_type)
                self.emit("    ADJSP 2")
                self.stack_depth -= 2
            source_type = (
                target_type
                if operator != "="
                else self._expression_type(expression.children[0])
            )
            self._store_lvalue(target, "R0", source_type, expression.line)
            return
        if kind == "unary":
            operator = str(expression.value)
            child = expression.children[0]
            if operator == "&":
                self._lvalue_address(child, expression.line)
                return
            if operator == "*":
                self._load_lvalue(expression, expression.line)
                return
            if operator in ("++", "--"):
                c_type = self._expression_type(child)
                self._load_lvalue(child, expression.line)
                if c_type.endswith("*"):
                    self.emit(f"    LDI R7, {_type_size(c_type[:-1])}")
                    self.emit(f"    {'ADD' if operator == '++' else 'SUB'} R0, R7")
                else:
                    self.emit(f"    {'INC' if operator == '++' else 'DEC'} R0")
                    self.emit("    LDI R7, 0xFFFF")
                    self.emit("    AND R0, R7")
                self._store_lvalue(child, "R0", c_type, expression.line)
                return
            self._expression(child)
            if operator == "-":
                if self._expression_type(child) == "float":
                    self.emit("    LDI R7, 0x8000")
                    self.emit("    XOR R0, R7")
                else:
                    self.emit("    LDI R1, 0")
                    self.emit("    SUB R1, R0")
                    self.emit("    MOV R0, R1")
                    self._mask_result()
            elif operator == "!":
                self._boolean_not(self._expression_type(child))
            elif operator == "~":
                if self._expression_type(child) == "float":
                    raise self.error(expression.line, "bitwise complement is invalid for float")
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
                self.emit(f"    LDI R7, {_type_size(c_type[:-1])}")
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
        if operator in ("&&", "||"):
            second_label = self.label("logic_second")
            true_label = self.label("logic_true")
            false_label = self.label("logic_false")
            end_label = self.label("logic_end")
            self._expression(left)
            self.emit("    LDI R1, 0")
            self.emit("    CMP R0, R1")
            if operator == "&&":
                self.emit(f"    BZX {false_label}")
            else:
                self.emit(f"    BZX {second_label}")
                self.emit(f"    JMPX {true_label}")
                self.emit(f"{second_label}:")
            self._expression(right)
            self.emit("    LDI R1, 0")
            self.emit("    CMP R0, R1")
            self.emit(f"    BZX {false_label}")
            self.emit(f"    JMPX {true_label}")
            self.emit(f"{true_label}:")
            self.emit("    LDI R0, 1")
            self.emit(f"    JMPX {end_label}")
            self.emit(f"{false_label}:")
            self.emit("    LDI R0, 0")
            self.emit(f"{end_label}:")
            return
        left_type = self._expression_type(left)
        right_type = self._expression_type(right)
        self._expression(left)
        left_size = _object_size(left_type)
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
            stride = _type_size(pointer_type[:-1])
            if stride == 2:
                self.emit("    ADD R0, R0")
            elif stride == 3:
                self.emit("    MOV R7, R0")
                self.emit("    ADD R0, R7")
                self.emit("    ADD R0, R7")
        if pointer_type is not None and left_type.endswith("*"):
            self.emit("    MOV R1, R0")
            self.emit("    MOVSP R7")
            self._emit_load_pointer("R0")
        else:
            self.emit("    MOV R1, R0")
            self.emit("    LDWS R0, 0")
            if pointer_type is not None:
                stride = _type_size(pointer_type[:-1])
                if stride == 2:
                    self.emit("    ADD R0, R0")
                elif stride == 3:
                    self.emit("    MOV R7, R0")
                    self.emit("    ADD R0, R7")
                    self.emit("    ADD R0, R7")
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
                helper = "__runtime_shr" if left_type.startswith("unsigned") or left_type in ("char", "unsigned char") else "__runtime_sar"
            self.used_helpers.add(helper)
            self.emit(f"    CALLX {helper}")
        elif operator in ("+", "-", "&", "|", "^", "*", "/", "%"):
            self._binary_operation(operator, expression.line, result_type)
        else:
            compare_type = "float" if left_type == "float" or right_type == "float" else (
                "unsigned int" if left_type.startswith("unsigned") or right_type.startswith("unsigned")
                or left_type in ("char", "unsigned char") or right_type in ("char", "unsigned char")
                or left_type.endswith("*") or right_type.endswith("*")
                else "int"
            )
            if compare_type == "float":
                if left_type != "float":
                    self.emit("    ITOF R0")
                if right_type != "float":
                    self.emit("    ITOF R1")
            self._comparison(operator, expression.line, compare_type)

    def _binary_operation(self, operator: str, line: int, result_type: str = "int") -> None:
        if result_type == "float":
            if operator not in ("+", "-", "*", "/"):
                raise self.error(line, f"operator {operator} is invalid for float")
            mnemonic = {"+": "FADD", "-": "FSUB", "*": "FMUL", "/": "FDIV"}[operator]
            self.emit(f"    {mnemonic} R0, R1")
            return
        if operator == "+":
            self.emit("    ADD R0, R1")
            if not result_type.endswith("*"):
                self._mask_result()
        elif operator == "-":
            self.emit("    SUB R0, R1")
            if not result_type.endswith("*"):
                self._mask_result()
        elif operator in ("&", "|", "^"):
            mnemonic = {"&": "AND", "|": "OR", "^": "XOR"}[operator]
            self.emit(f"    {mnemonic} R0, R1")
        elif operator in ("*", "/", "%"):
            unsigned = result_type.startswith("unsigned")
            helper = {
                "*": "__runtime_mul",
                "/": "__runtime_udiv" if unsigned else "__runtime_div",
                "%": "__runtime_umod" if unsigned else "__runtime_mod",
            }[operator]
            self.used_helpers.add(helper)
            self.emit(f"    CALLX {helper}")
        else:
            raise self.error(line, f"unsupported binary operator {operator!r}")

    def _mask_result(self) -> None:
        self.emit("    LDI R7, 0xFFFF")
        self.emit("    AND R0, R7")

    def _comparison(self, operator: str, line: int, compare_type: str = "int") -> None:
        true_label = self.label("cmp_true")
        false_label = self.label("cmp_false")
        end_label = self.label("cmp_end")
        if operator in ("<", "<=", ">", ">="):
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
            self.emit("    LDI R1, 0")
            self.emit("    CMP R0, R1")
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
        elif helper == "__runtime_mul":
            self._generate_multiply()
        elif helper in ("__runtime_div", "__runtime_mod", "__runtime_udiv", "__runtime_umod"):
            self._generate_divide(
                modulo=helper.endswith("mod"),
                signed=not helper.startswith("__runtime_u"),
            )

    def _generate_multiply(self) -> None:
        self.emit("__runtime_mul:")
        self.emit("    MOV R2, R0")
        self.emit("    LDI R4, 0")
        self.emit("    LDI R7, 0x8000")
        self.emit("    AND R2, R7")
        self.emit("    LDI R6, 0")
        self.emit("    CMP R2, R6")
        self.emit("    BZX __mul_left_positive")
        self.emit("    LDI R5, 0")
        self.emit("    SUB R5, R0")
        self.emit("    LDI R7, 0xFFFF")
        self.emit("    AND R5, R7")
        self.emit("    MOV R0, R5")
        self.emit("    LDI R5, 1")
        self.emit("    XOR R4, R5")
        self.emit("__mul_left_positive:")
        self.emit("    MOV R2, R1")
        self.emit("    LDI R7, 0x8000")
        self.emit("    AND R2, R7")
        self.emit("    LDI R6, 0")
        self.emit("    CMP R2, R6")
        self.emit("    BZX __mul_right_positive")
        self.emit("    LDI R5, 0")
        self.emit("    SUB R5, R1")
        self.emit("    LDI R7, 0xFFFF")
        self.emit("    AND R5, R7")
        self.emit("    MOV R1, R5")
        self.emit("    LDI R5, 1")
        self.emit("    XOR R4, R5")
        self.emit("__mul_right_positive:")
        self.emit("    LDI R2, 0")
        self.emit("    LDI R7, 0xFFFF")
        self.emit("    LDI R6, 0")
        self.emit("__mul_loop:")
        self.emit("    MOV R3, R1")
        self.emit("    LDI R5, 1")
        self.emit("    AND R3, R5")
        self.emit("    CMP R3, R6")
        self.emit("    BZX __mul_skip_add")
        self.emit("    ADD R2, R0")
        self.emit("    AND R2, R7")
        self.emit("__mul_skip_add:")
        self.emit("    ADD R0, R0")
        self.emit("    AND R0, R7")
        self.emit("    SHR R1")
        self.emit("    CMP R1, R6")
        self.emit("    BZX __mul_done")
        self.emit("    JMPX __mul_loop")
        self.emit("__mul_done:")
        self.emit("    LDI R5, 1")
        self.emit("    AND R4, R5")
        self.emit("    CMP R4, R6")
        self.emit("    BZX __mul_positive")
        self.emit("    LDI R5, 0")
        self.emit("    SUB R5, R2")
        self.emit("    AND R5, R7")
        self.emit("    MOV R2, R5")
        self.emit("__mul_positive:")
        self.emit("    MOV R0, R2")
        self.emit("    RET")

    def _generate_divide(self, modulo: bool, signed: bool = True) -> None:
        name = (
            "__runtime_mod" if modulo else "__runtime_div"
        ) if signed else (
            "__runtime_umod" if modulo else "__runtime_udiv"
        )
        prefix = name
        self.emit(f"{name}:")
        self.emit("    MOV R3, R0")
        self.emit("    MOV R4, R1")
        self.emit("    LDI R5, 0")  # dividend sign for modulo, quotient sign for division
        self.emit("    LDI R6, 0")  # divisor sign
        if signed:
            self.emit("    LDI R7, 0x8000")
            self.emit("    MOV R2, R3")
            self.emit("    AND R2, R7")
            self.emit("    LDI R1, 0")
            self.emit("    CMP R2, R1")
            self.emit(f"    BZX {prefix}_dividend_positive")
            self.emit("    LDI R1, 0")
            self.emit("    SUB R1, R3")
            self.emit("    LDI R7, 0xFFFF")
            self.emit("    AND R1, R7")
            self.emit("    MOV R3, R1")
            self.emit("    LDI R5, 1")
            self.emit(f"{prefix}_dividend_positive:")
            self.emit("    LDI R7, 0x8000")
            self.emit("    MOV R2, R4")
            self.emit("    AND R2, R7")
            self.emit("    LDI R1, 0")
            self.emit("    CMP R2, R1")
            self.emit(f"    BZX {prefix}_divisor_positive")
            self.emit("    LDI R1, 0")
            self.emit("    SUB R1, R4")
            self.emit("    LDI R7, 0xFFFF")
            self.emit("    AND R1, R7")
            self.emit("    MOV R4, R1")
            self.emit("    LDI R6, 1")
            self.emit(f"{prefix}_divisor_positive:")
            self.emit("    PUSH R5")
            self.emit("    PUSH R6")
        self.emit("    LDI R2, 0")
        self.emit("    LDI R7, 0")
        self.emit("    LDI R1, 16")
        self.emit("    LDI R5, 0")
        self.emit("    CMP R4, R5")
        self.emit(f"    BZX {prefix}_divide_by_zero")
        self.emit(f"{prefix}_loop:")
        self.emit("    MOV R0, R3")
        self.emit("    LDI R6, 0")
        self.emit("    LDI_H R6, 0x80")
        self.emit("    AND R0, R6")
        self.emit("    CMP R0, R5")
        self.emit(f"    BZX {prefix}_zero_bit")
        self.emit("    ADD R3, R3")
        self.emit("    LDI R6, 0xFF")
        self.emit("    LDI_H R6, 0xFF")
        self.emit("    AND R3, R6")
        self.emit("    ADD R7, R7")
        self.emit("    INC R7")
        self.emit(f"    JMPX {prefix}_compare_remainder")
        self.emit(f"{prefix}_zero_bit:")
        self.emit("    ADD R3, R3")
        self.emit("    LDI R6, 0xFF")
        self.emit("    LDI_H R6, 0xFF")
        self.emit("    AND R3, R6")
        self.emit("    ADD R7, R7")
        self.emit(f"{prefix}_compare_remainder:")
        self.emit("    ADD R2, R2")
        self.emit("    CMP R7, R4")
        self.emit(f"    BCX {prefix}_next_bit")
        self.emit("    SUB R7, R4")
        self.emit("    INC R2")
        self.emit(f"{prefix}_next_bit:")
        self.emit("    DEC R1")
        self.emit("    CMP R1, R5")
        self.emit(f"    BZX {prefix}_finish")
        self.emit(f"    JMPX {prefix}_loop")
        self.emit(f"{prefix}_divide_by_zero:")
        self.emit("    MOV R7, R3")
        self.emit(f"    JMPX {prefix}_finish")
        self.emit(f"{prefix}_finish:")
        if modulo:
            self.emit("    MOV R0, R7")
            if signed:
                self.emit("    POP R6")
                self.emit("    POP R5")
                self.emit("    LDI R1, 0")
                self.emit("    CMP R5, R1")
                self.emit(f"    BZX {prefix}_return")
                self.emit("    LDI R1, 0")
                self.emit("    SUB R1, R0")
                self.emit("    MOV R0, R1")
        else:
            self.emit("    MOV R0, R2")
            if signed:
                self.emit("    POP R6")
                self.emit("    POP R5")
                self.emit("    LDI R1, 0")
                self.emit("    CMP R5, R6")
                self.emit(f"    BZX {prefix}_return")
                self.emit("    LDI R1, 0")
                self.emit("    SUB R1, R0")
                self.emit("    MOV R0, R1")
        self.emit(f"{prefix}_return:")
        self.emit("    RET")


def _compile(source: str, filename: str) -> tuple[str, AssemblyImage]:
    parser = _Parser(source, filename)
    globals_, functions = parser.parse()
    generator = _CodeGenerator(globals_, functions, filename)
    generator.prototypes = parser.prototypes
    assembly = generator.generate()
    try:
        source_directory = Path(filename).resolve().parent
        image = assemble(assembly, base_directory=source_directory)
    except AssemblyError as exc:
        raise CCompilerError(f"{filename}: generated SC-8 assembly is invalid: {exc}") from exc
    return assembly, image


def compile_to_assembly(source: str, filename: str = "<input>") -> str:
    """Compile the supported C89 subset to SC-8 assembly source."""
    assembly, _ = _compile(source, filename)
    return assembly


def compile_source(source: str, filename: str = "<input>") -> AssemblyImage:
    """Compile C source to an assembled SC-8 program image."""
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
    parser = argparse.ArgumentParser(description="Compile the SC-8 C89 subset.")
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


def _preprocess(source: str, filename: str, defines: dict[str, str], depth: int = 0) -> str:
    """Handle #define NAME value and #include lines; other text is passed through."""
    if depth > 8:
        raise CCompilerError(f"{filename}: #include nested too deeply")
    output: list[str] = []
    for number, text in enumerate(source.split("\n"), 1):
        match = _DIRECTIVE.match(text)
        if match is None:
            output.append(text)
            continue
        name, rest = match.groups()
        where = f"{filename}:{number}"
        if name == "define":
            parts = re.match(r"([A-Za-z_]\w*)(\(?)\s*(.*)$", rest)
            if parts is None:
                raise CCompilerError(f"{where}: invalid #define")
            if parts.group(2):
                raise CCompilerError(f"{where}: function-like macros are not supported")
            defines[parts.group(1)] = re.sub(r"//.*$", "", parts.group(3)).strip()
            output.append("")
        elif name == "include":
            quoted = re.fullmatch(r'"([^"]+)"|<([^>]+)>', rest)
            if quoted is None:
                raise CCompilerError(f"{where}: invalid #include")
            target = quoted.group(1) or quoted.group(2)
            candidates = [Path(filename).resolve().parent / target,
                          Path(__file__).resolve().parent / target]
            found = next((path for path in candidates if path.is_file()), None)
            if found is None:
                raise CCompilerError(f"{where}: cannot find include file {target!r}")
            included = found.read_text(encoding="utf-8")
            output.append(_preprocess(included, str(found), defines, depth + 1).replace("\n", " "))
        else:
            raise CCompilerError(f"{where}: unsupported directive #{name}")
    return "\n".join(output)


def _expand_macros(tokens: list[Token], defines: dict[str, str], filename: str) -> list[Token]:
    cache: dict[str, list[str]] = {}

    def body(name: str, active: tuple[str, ...]) -> list[str]:
        if name not in cache:
            values: list[str] = []
            for token in _lex_raw(defines[name], filename)[:-1]:
                if token.value in defines and token.value not in active + (name,):
                    values.extend(body(token.value, active + (name,)))
                else:
                    values.append(token.value)
            cache[name] = values
        return cache[name]

    result: list[Token] = []
    for token in tokens:
        if token.value in defines:
            result.extend(Token(value, token.line) for value in body(token.value, ()))
        else:
            result.append(token)
    return result


def _lex(source: str, filename: str) -> list[Token]:
    defines: dict[str, str] = {}
    source = _preprocess(source, filename, defines)
    tokens = _lex_raw(source, filename)
    return _expand_macros(tokens, defines, filename) if defines else tokens


if __name__ == "__main__":
    raise SystemExit(main())
