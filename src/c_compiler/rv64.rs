use std::collections::{BTreeMap, BTreeSet};

use super::{
    CCompilerError, Expression, ExpressionKind, ExternalDeclaration, FunctionDeclaration,
    Statement, StatementKind, TranslationUnitParser, error, lex_unexpanded, parse_integer_constant,
};
use crate::kiuas16_abi::STACK_ALIGNMENT;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Type {
    Int,
    Long,
    Byte,
    BytePointer,
    IntPointer,
    LongPointer,
    Void,
}

impl Type {
    fn parse(text: &str) -> Option<Self> {
        Some(match text.split_whitespace().collect::<String>().as_str() {
            "int" | "signed" | "signedint" => Self::Int,
            "long" | "longint" | "signedlong" | "signedlongint" => Self::Long,
            "unsignedchar" => Self::Byte,
            "volatileunsignedchar*" | "unsignedvolatilechar*" | "unsignedcharvolatile*" => {
                Self::BytePointer
            }
            "volatileint*" | "intvolatile*" => Self::IntPointer,
            "volatilelong*" | "longvolatile*" | "volatilelongint*" | "longintvolatile*" => {
                Self::LongPointer
            }
            "void" => Self::Void,
            _ => return None,
        })
    }

    fn integer(self) -> bool {
        matches!(self, Self::Int | Self::Long | Self::Byte)
    }

    fn compatible(self, other: Self) -> bool {
        self == other || self.integer() && other.integer()
    }

    fn pointee(self) -> Option<Self> {
        match self {
            Self::BytePointer => Some(Self::Byte),
            Self::IntPointer => Some(Self::Int),
            Self::LongPointer => Some(Self::Long),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Signature {
    result: Type,
    parameters: Vec<Type>,
}

struct Value {
    ty: Type,
    kind: ValueKind,
}

enum ValueKind {
    Constant(i64),
    Local(usize),
    Convert(Box<Value>),
    Unary(String, Box<Value>),
    Binary(String, Box<Value>, Box<Value>),
    Call(String, Vec<Value>),
    Load(Box<Value>),
    Assign(Box<Value>, Box<Value>),
}

enum Action {
    Block(Vec<Action>),
    Evaluate(Value),
    Return(Option<Value>),
    If(Value, Box<Action>, Option<Box<Action>>),
    While(Value, Box<Action>),
}

struct Lower<'a> {
    filename: &'a str,
    signatures: &'a BTreeMap<String, Signature>,
    scopes: Vec<BTreeMap<String, (usize, Type)>>,
    slots: usize,
    result: Type,
}

impl Lower<'_> {
    fn fail(&self, line: usize, message: &str) -> CCompilerError {
        error(self.filename, line, message)
    }

    fn local(&mut self, name: &str, ty: Type, line: usize) -> Result<usize, CCompilerError> {
        let scope = self.scopes.last_mut().expect("function scope");
        if scope.contains_key(name) {
            return Err(error(self.filename, line, "duplicate local declaration"));
        }
        if self.slots >= 120 {
            return Err(self.fail(
                line,
                "RV64 subset supports at most 120 local/parameter slots",
            ));
        }
        let slot = self.slots;
        scope.insert(name.to_owned(), (slot, ty));
        self.slots += 1;
        Ok(slot)
    }

    fn convert(&self, value: Value, ty: Type, line: usize) -> Result<Value, CCompilerError> {
        if value.ty == Type::Void || ty == Type::Void || !ty.compatible(value.ty) {
            return Err(self.fail(line, "incompatible RV64 subset types"));
        }
        Ok(if value.ty == ty {
            value
        } else {
            Value {
                ty,
                kind: ValueKind::Convert(Box::new(value)),
            }
        })
    }

    fn value(&mut self, expression: &Expression) -> Result<Value, CCompilerError> {
        let line = expression.line;
        let value = match &expression.kind {
            ExpressionKind::Atom(token) => {
                if let Some((slot, ty)) = self.scopes.iter().rev().find_map(|s| s.get(token)) {
                    Value {
                        ty: *ty,
                        kind: ValueKind::Local(*slot),
                    }
                } else if let Some(number) = parse_integer_constant(token) {
                    if token.contains(['u', 'U']) || i32::try_from(number).is_err() {
                        return Err(self.fail(
                            line,
                            "integer literal requires signed 32-bit value in initial RV64 subset",
                        ));
                    }
                    Value {
                        ty: if token.contains(['l', 'L']) {
                            Type::Long
                        } else {
                            Type::Int
                        },
                        kind: ValueKind::Constant(number),
                    }
                } else {
                    return Err(
                        self.fail(line, &format!("undefined or unsupported value {token:?}"))
                    );
                }
            }
            ExpressionKind::Cast { c_type, expression } => {
                let ty = Type::parse(c_type)
                    .ok_or_else(|| self.fail(line, "unsupported RV64 cast type"))?;
                let value = self.value(expression)?;
                if ty.pointee().is_some() && value.ty.integer() {
                    Value {
                        ty,
                        kind: ValueKind::Convert(Box::new(value)),
                    }
                } else {
                    self.convert(value, ty, line)?
                }
            }
            ExpressionKind::Unary { operator, operand } => {
                let value = self.value(operand)?;
                if operator == "*" && value.ty.pointee().is_some() {
                    Value {
                        ty: value.ty.pointee().expect("checked pointer"),
                        kind: ValueKind::Load(Box::new(value)),
                    }
                } else if matches!(operator.as_str(), "+" | "-" | "~" | "!") && value.ty.integer() {
                    let ty = if value.ty == Type::Long {
                        Type::Long
                    } else {
                        Type::Int
                    };
                    let value = self.convert(value, ty, line)?;
                    Value {
                        ty: if operator == "!" { Type::Int } else { ty },
                        kind: ValueKind::Unary(operator.clone(), Box::new(value)),
                    }
                } else {
                    return Err(self.fail(line, "unsupported RV64 unary operation"));
                }
            }
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => {
                if !matches!(
                    operator.as_str(),
                    "+" | "-" | "*" | "&" | "|" | "^" | "==" | "!=" | "<" | "<=" | ">" | ">="
                ) {
                    return Err(self.fail(line, "unsupported RV64 binary operation"));
                }
                let left = self.value(left)?;
                let right = self.value(right)?;
                if !left.ty.integer() || !right.ty.integer() {
                    return Err(self.fail(line, "RV64 arithmetic requires integer operands"));
                }
                let ty = if left.ty == Type::Long || right.ty == Type::Long {
                    Type::Long
                } else {
                    Type::Int
                };
                let left = self.convert(left, ty, line)?;
                let right = self.convert(right, ty, line)?;
                Value {
                    ty: if matches!(operator.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=") {
                        Type::Int
                    } else {
                        ty
                    },
                    kind: ValueKind::Binary(operator.clone(), Box::new(left), Box::new(right)),
                }
            }
            ExpressionKind::Call {
                function,
                arguments,
            } => {
                let ExpressionKind::Atom(name) = &function.kind else {
                    return Err(self.fail(
                        line,
                        "indirect calls are not supported by initial RV64 subset",
                    ));
                };
                if self
                    .scopes
                    .iter()
                    .rev()
                    .any(|scope| scope.contains_key(name))
                {
                    return Err(self.fail(line, "called name is a local object"));
                }
                let signature = self
                    .signatures
                    .get(name)
                    .ok_or_else(|| self.fail(line, "call requires a defined prototyped function"))?
                    .clone();
                if signature.parameters.len() != arguments.len() {
                    return Err(self.fail(line, "function argument count mismatch"));
                }
                let mut values = Vec::new();
                for (argument, ty) in arguments.iter().zip(&signature.parameters) {
                    let value = self.value(argument)?;
                    values.push(self.convert(value, *ty, line)?);
                }
                Value {
                    ty: signature.result,
                    kind: ValueKind::Call(name.clone(), values),
                }
            }
            ExpressionKind::Assignment {
                operator,
                target,
                value,
            } if operator == "=" => {
                let target = self.value(target)?;
                if !matches!(target.kind, ValueKind::Local(_) | ValueKind::Load(_)) {
                    return Err(self.fail(line, "assignment requires a modifiable object"));
                }
                let value = self.value(value)?;
                let value = self.convert(value, target.ty, line)?;
                Value {
                    ty: target.ty,
                    kind: ValueKind::Assign(Box::new(target), Box::new(value)),
                }
            }
            _ => return Err(self.fail(line, "unsupported expression in initial RV64 subset")),
        };
        Ok(value)
    }

    fn condition(&mut self, expression: &Expression) -> Result<Value, CCompilerError> {
        let value = self.value(expression)?;
        if !value.ty.integer() {
            return Err(self.fail(
                expression.line,
                "condition requires an integer in initial RV64 subset",
            ));
        }
        Ok(value)
    }

    fn action(&mut self, statement: &Statement) -> Result<Action, CCompilerError> {
        let line = statement.line;
        Ok(match &statement.kind {
            StatementKind::Compound(statements) => {
                self.scopes.push(BTreeMap::new());
                let actions = statements
                    .iter()
                    .map(|s| self.action(s))
                    .collect::<Result<_, _>>()?;
                self.scopes.pop();
                Action::Block(actions)
            }
            StatementKind::Declaration(declarations) => {
                let mut actions = Vec::new();
                for declaration in declarations {
                    if declaration.array_length.is_some()
                        || !declaration.array_dimensions.is_empty()
                    {
                        return Err(
                            self.fail(line, "arrays are not supported by initial RV64 subset")
                        );
                    }
                    let ty = Type::parse(&declaration.c_type)
                        .filter(|ty| *ty != Type::Void)
                        .ok_or_else(|| self.fail(line, "unsupported RV64 local type"))?;
                    let slot = self.local(&declaration.name, ty, line)?;
                    if let Some(initializer) = &declaration.initializer {
                        let value = self.value(initializer)?;
                        let value = self.convert(value, ty, line)?;
                        actions.push(Action::Evaluate(Value {
                            ty,
                            kind: ValueKind::Assign(
                                Box::new(Value {
                                    ty,
                                    kind: ValueKind::Local(slot),
                                }),
                                Box::new(value),
                            ),
                        }));
                    }
                }
                Action::Block(actions)
            }
            StatementKind::Expression(expression) => {
                if let Some(expression) = expression {
                    Action::Evaluate(self.value(expression)?)
                } else {
                    Action::Block(Vec::new())
                }
            }
            StatementKind::Return(expression) => Action::Return(match expression {
                Some(expression) => {
                    let value = self.value(expression)?;
                    Some(self.convert(value, self.result, line)?)
                }
                None if self.result == Type::Void => None,
                None => return Err(self.fail(line, "non-void return requires a value")),
            }),
            StatementKind::If {
                condition,
                then_branch,
                else_branch,
            } => Action::If(
                self.condition(condition)?,
                Box::new(self.action(then_branch)?),
                else_branch
                    .as_ref()
                    .map(|s| self.action(s).map(Box::new))
                    .transpose()?,
            ),
            StatementKind::While { condition, body } => {
                Action::While(self.condition(condition)?, Box::new(self.action(body)?))
            }
            _ => return Err(self.fail(line, "unsupported statement in initial RV64 subset")),
        })
    }
}

struct Emit {
    assembly: String,
    next_label: usize,
    return_label: String,
}

impl Emit {
    fn line(&mut self, text: impl AsRef<str>) {
        self.assembly.push_str(text.as_ref());
        self.assembly.push('\n');
    }

    fn label(&mut self) -> String {
        let label = format!(".Lrv64_{}", self.next_label);
        self.next_label += 1;
        label
    }

    fn push(&mut self) {
        self.line("addi sp, sp, -16\nsd a0, 0(sp)");
    }

    fn pop(&mut self, register: &str) {
        self.line(format!("ld {register}, 0(sp)\naddi sp, sp, 16"));
    }

    fn normalize(&mut self, ty: Type) {
        match ty {
            Type::Int => self.line("addiw a0, a0, 0"),
            Type::Byte => self.line("andi a0, a0, 255"),
            _ => {}
        }
    }

    fn value(&mut self, value: &Value) {
        match &value.kind {
            ValueKind::Constant(number) => self.line(format!("li a0, {number}")),
            ValueKind::Local(slot) => self.line(format!("ld a0, -{}(s0)", 24 + slot * 8)),
            ValueKind::Convert(inner) => self.value(inner),
            ValueKind::Unary(operator, inner) => {
                self.value(inner);
                match operator.as_str() {
                    "-" => self.line("sub a0, zero, a0"),
                    "~" => self.line("xori a0, a0, -1"),
                    "!" => self.line("sltiu a0, a0, 1"),
                    _ => {}
                }
            }
            ValueKind::Binary(operator, left, right) => {
                self.value(left);
                self.push();
                self.value(right);
                self.pop("t0");
                self.line(match operator.as_str() {
                    "+" => "add a0, t0, a0",
                    "-" => "sub a0, t0, a0",
                    "*" => "mul a0, t0, a0",
                    "&" => "and a0, t0, a0",
                    "|" => "or a0, t0, a0",
                    "^" => "xor a0, t0, a0",
                    "<" => "slt a0, t0, a0",
                    ">" => "slt a0, a0, t0",
                    "<=" => "slt a0, a0, t0\nxori a0, a0, 1",
                    ">=" => "slt a0, t0, a0\nxori a0, a0, 1",
                    "==" => "xor a0, t0, a0\nsltiu a0, a0, 1",
                    "!=" => "xor a0, t0, a0\nsltu a0, zero, a0",
                    _ => unreachable!("checked operator"),
                });
            }
            ValueKind::Call(name, arguments) => {
                for argument in arguments {
                    self.value(argument);
                    self.push();
                }
                for index in (0..arguments.len()).rev() {
                    self.pop(&format!("a{index}"));
                }
                self.line(format!("call {name}"));
            }
            ValueKind::Load(pointer) => {
                self.value(pointer);
                self.line(match value.ty {
                    Type::Byte => "lbu a0, 0(a0)",
                    Type::Int => "lw a0, 0(a0)",
                    Type::Long => "ld a0, 0(a0)",
                    _ => unreachable!("checked pointee"),
                });
            }
            ValueKind::Assign(target, value) => match &target.kind {
                ValueKind::Local(slot) => {
                    self.value(value);
                    self.line(format!("sd a0, -{}(s0)", 24 + slot * 8));
                }
                ValueKind::Load(pointer) => {
                    self.value(pointer);
                    self.push();
                    self.value(value);
                    self.pop("t0");
                    self.line(match target.ty {
                        Type::Byte => "sb a0, 0(t0)",
                        Type::Int => "sw a0, 0(t0)",
                        Type::Long => "sd a0, 0(t0)",
                        _ => unreachable!("checked pointee"),
                    });
                }
                _ => unreachable!("checked assignment"),
            },
        }
        self.normalize(value.ty);
    }

    fn action(&mut self, action: &Action) {
        match action {
            Action::Block(actions) => {
                for action in actions {
                    self.action(action);
                }
            }
            Action::Evaluate(value) => self.value(value),
            Action::Return(value) => {
                if let Some(value) = value {
                    self.value(value);
                }
                self.line(format!("j {}", self.return_label));
            }
            Action::If(condition, then_branch, else_branch) => {
                let otherwise = self.label();
                let done = self.label();
                self.value(condition);
                self.line(format!("beq a0, zero, {otherwise}"));
                self.action(then_branch);
                self.line(format!("j {done}\n{otherwise}:"));
                if let Some(branch) = else_branch {
                    self.action(branch);
                }
                self.line(format!("{done}:"));
            }
            Action::While(condition, body) => {
                let start = self.label();
                let done = self.label();
                self.line(format!("{start}:"));
                self.value(condition);
                self.line(format!("beq a0, zero, {done}"));
                self.action(body);
                self.line(format!("j {start}\n{done}:"));
            }
        }
    }
}

fn signature(declaration: &FunctionDeclaration) -> Option<Signature> {
    if declaration.variadic || !declaration.has_prototype || declaration.parameters.len() > 8 {
        return None;
    }
    Some(Signature {
        result: Type::parse(&declaration.return_type)?,
        parameters: declaration
            .parameters
            .iter()
            .map(|p| Type::parse(&p.c_type).filter(|ty| *ty != Type::Void))
            .collect::<Option<_>>()?,
    })
}

pub fn compile_to_assembly(source: &str, filename: &str) -> Result<String, CCompilerError> {
    let tokens = lex_unexpanded(source, filename)?;
    for token in &tokens {
        if matches!(
            token.value.as_str(),
            "static"
                | "extern"
                | "typedef"
                | "const"
                | "enum"
                | "struct"
                | "union"
                | "register"
                | "auto"
                | "restrict"
                | "#"
        ) {
            return Err(error(
                filename,
                token.line,
                "preprocessing, storage classes, typedefs, records, enums and const are not supported by initial RV64 subset",
            ));
        }
    }
    let unit = TranslationUnitParser::new(tokens).parse()?;
    let mut signatures = BTreeMap::new();
    let mut definitions = BTreeSet::new();
    for declaration in &unit.declarations {
        let (function, line, definition) = match declaration {
            ExternalDeclaration::FunctionDefinition(f) => (&f.declaration, f.body.line, true),
            ExternalDeclaration::FunctionPrototype(f) => (f, 1, false),
            _ => {
                return Err(error(
                    filename,
                    1,
                    "globals are not supported by initial RV64 subset",
                ));
            }
        };
        if function.name.starts_with(".Lrv64_") || function.name == "_start" {
            return Err(error(filename, line, "reserved RV64 startup symbol"));
        }
        let signature = signature(function)
            .ok_or_else(|| error(filename, line, "unsupported RV64 function signature; use prototypes and at most eight scalar parameters"))?;
        if let Some(previous) = signatures.insert(function.name.clone(), signature.clone()) {
            if previous != signature {
                return Err(error(filename, line, "conflicting function declarations"));
            }
        }
        if definition && !definitions.insert(function.name.clone()) {
            return Err(error(filename, line, "duplicate function definition"));
        }
    }
    if signatures.keys().any(|name| !definitions.contains(name)) {
        return Err(error(
            filename,
            1,
            "every declared function must be defined in the source unit",
        ));
    }
    if signatures.get("main")
        != Some(&Signature {
            result: Type::Int,
            parameters: Vec::new(),
        })
    {
        return Err(error(filename, 1, "RV64 ROM requires int main(void)"));
    }
    let mut emit = Emit {
        assembly: ".text\n.globl _start\n_start:\nli sp, 1\nslli sp, sp, 31\nli t0, 65536\nadd sp, sp, t0\ncall main\n.Lrv64_halt:\nwfi\nj .Lrv64_halt\n".to_owned(),
        next_label: 0,
        return_label: String::new(),
    };
    let mut visible = BTreeMap::new();
    for declaration in &unit.declarations {
        let function = match declaration {
            ExternalDeclaration::FunctionDefinition(function) => function,
            ExternalDeclaration::FunctionPrototype(function) => {
                visible.insert(function.name.clone(), signatures[&function.name].clone());
                continue;
            }
            ExternalDeclaration::Global(_) => unreachable!("checked declarations"),
        };
        let sig = &signatures[&function.declaration.name];
        visible.insert(function.declaration.name.clone(), sig.clone());
        let mut lower = Lower {
            filename,
            signatures: &visible,
            scopes: vec![BTreeMap::new()],
            slots: 0,
            result: sig.result,
        };
        for (parameter, ty) in function.declaration.parameters.iter().zip(&sig.parameters) {
            let name = parameter.name.as_ref().ok_or_else(|| {
                error(
                    filename,
                    function.body.line,
                    "function definition requires named parameters",
                )
            })?;
            lower.local(name, *ty, function.body.line)?;
        }
        let StatementKind::Compound(statements) = &function.body.kind else {
            unreachable!("parser function body")
        };
        let body = Action::Block(
            statements
                .iter()
                .map(|s| lower.action(s))
                .collect::<Result<_, _>>()?,
        );
        let frame = (16 + lower.slots * 8).div_ceil(STACK_ALIGNMENT) * STACK_ALIGNMENT;
        emit.return_label = emit.label();
        emit.line(format!(
            "{}:\naddi sp, sp, -{frame}\nsd ra, {}(sp)\nsd s0, {}(sp)\naddi s0, sp, {frame}",
            function.declaration.name,
            frame - 8,
            frame - 16,
        ));
        for (index, ty) in sig.parameters.iter().enumerate() {
            emit.line(format!("mv a0, a{index}"));
            emit.normalize(*ty);
            emit.line(format!("sd a0, -{}(s0)", 24 + index * 8));
        }
        emit.action(&body);
        if function.declaration.name == "main" {
            emit.line("li a0, 0");
        } else if sig.result != Type::Void {
            emit.line("ebreak");
        }
        emit.line(format!(
            "{}:\nld ra, -8(s0)\nld t0, -16(s0)\nmv sp, s0\nmv s0, t0\nret",
            emit.return_label
        ));
    }
    Ok(emit.assembly)
}
