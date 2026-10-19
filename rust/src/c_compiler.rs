use crate::image::ProgramImage;
use std::collections::{HashMap, HashSet};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub line: usize,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpressionKind {
    Atom(String),
    Unary {
        operator: String,
        operand: Box<Expression>,
    },
    Binary {
        operator: String,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Assignment {
        operator: String,
        target: Box<Expression>,
        value: Box<Expression>,
    },
    Conditional {
        condition: Box<Expression>,
        when_true: Box<Expression>,
        when_false: Box<Expression>,
    },
    Call {
        function: Box<Expression>,
        arguments: Vec<Expression>,
    },
    Index {
        expression: Box<Expression>,
        index: Box<Expression>,
    },
    Member {
        expression: Box<Expression>,
        operator: String,
        name: String,
    },
    Postfix {
        operator: String,
        expression: Box<Expression>,
    },
    Cast {
        c_type: String,
        expression: Box<Expression>,
    },
    SizeofType(String),
    SizeofExpression(Box<Expression>),
    InitializerList(Vec<Expression>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub c_type: String,
    pub name: String,
    pub array_length: Option<usize>,
    pub initializer: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    pub kind: StatementKind,
    pub line: usize,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementKind {
    Compound(Vec<Statement>),
    Declaration(Vec<Declaration>),
    Expression(Option<Expression>),
    Return(Option<Expression>),
    If {
        condition: Expression,
        then_branch: Box<Statement>,
        else_branch: Option<Box<Statement>>,
    },
    While {
        condition: Expression,
        body: Box<Statement>,
    },
    DoWhile {
        body: Box<Statement>,
        condition: Expression,
    },
    For {
        initializer: Option<Box<Statement>>,
        condition: Option<Expression>,
        update: Option<Expression>,
        body: Box<Statement>,
    },
    Switch {
        expression: Expression,
        cases: Vec<(Option<Expression>, Vec<Statement>)>,
    },
    Break,
    Continue,
    Goto(String),
    Label {
        name: String,
        statement: Box<Statement>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParameter {
    pub c_type: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDeclaration {
    pub return_type: String,
    pub name: String,
    pub parameters: Vec<FunctionParameter>,
    pub variadic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDefinition {
    pub declaration: FunctionDeclaration,
    pub body: Statement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalDeclaration {
    Global(Declaration),
    FunctionPrototype(FunctionDeclaration),
    FunctionDefinition(FunctionDefinition),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationUnit {
    pub declarations: Vec<ExternalDeclaration>,
}

const NATIVE_GLOBAL_SCALAR: u8 = u8::MAX;
const NATIVE_GLOBAL_ARRAY: u8 = u8::MAX - 1;

struct NativeGlobal {
    name: String,
    data: Vec<u8>,
    is_array: bool,
}

pub struct ExpressionParser {
    tokens: Vec<Token>,
    position: usize,
}

fn evaluate_integer_constant(expression: &Expression) -> Result<i64, CCompilerError> {
    let fail = || {
        error(
            expression.filename.as_deref().unwrap_or("<source>"),
            expression.line,
            "expected an integer constant expression",
        )
    };
    match &expression.kind {
        ExpressionKind::Atom(value) => parse_integer_constant(value).ok_or_else(fail),
        ExpressionKind::Unary { operator, operand } => {
            let value = evaluate_integer_constant(operand)?;
            match operator.as_str() {
                "+" => Ok(value),
                "-" => value.checked_neg().ok_or_else(fail),
                "!" => Ok(i64::from(value == 0)),
                "~" => Ok((!(value as u32)) as i32 as i64),
                _ => Err(fail()),
            }
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            let left = evaluate_integer_constant(left)?;
            if operator == "&&" && left == 0 {
                return Ok(0);
            }
            if operator == "||" && left != 0 {
                return Ok(1);
            }
            let right = evaluate_integer_constant(right)?;
            let result = match operator.as_str() {
                "||" => i64::from(right != 0),
                "&&" => i64::from(right != 0),
                "|" => (left as u32 | right as u32) as i32 as i64,
                "^" => (left as u32 ^ right as u32) as i32 as i64,
                "&" => (left as u32 & right as u32) as i32 as i64,
                "==" => i64::from(left == right),
                "!=" => i64::from(left != right),
                "<" => i64::from(left < right),
                "<=" => i64::from(left <= right),
                ">" => i64::from(left > right),
                ">=" => i64::from(left >= right),
                "<<" if (0..32).contains(&right) => {
                    ((left as u32).wrapping_shl(right as u32)) as i32 as i64
                }
                ">>" if (0..32).contains(&right) => ((left as i32) >> (right as u32)) as i64,
                "+" => left.checked_add(right).ok_or_else(fail)?,
                "-" => left.checked_sub(right).ok_or_else(fail)?,
                "*" => left.checked_mul(right).ok_or_else(fail)?,
                "/" if right != 0 => left.checked_div(right).ok_or_else(fail)?,
                "%" if right != 0 => left.checked_rem(right).ok_or_else(fail)?,
                _ => return Err(fail()),
            };
            Ok(result)
        }
        ExpressionKind::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            if evaluate_integer_constant(condition)? != 0 {
                evaluate_integer_constant(when_true)
            } else {
                evaluate_integer_constant(when_false)
            }
        }
        ExpressionKind::InitializerList(_) => Err(fail()),
        _ => Err(fail()),
    }
}

fn parse_integer_constant(token: &str) -> Option<i64> {
    let suffix_start = token.find(['u', 'U', 'l', 'L']).unwrap_or(token.len());
    let digits = &token[..suffix_start];
    let suffix = &token[suffix_start..];
    if !suffix
        .chars()
        .all(|character| matches!(character, 'u' | 'U' | 'l' | 'L'))
        || suffix.len() > 2
        || suffix
            .chars()
            .filter(|character| matches!(character, 'u' | 'U'))
            .count()
            > 1
        || suffix
            .chars()
            .filter(|character| matches!(character, 'l' | 'L'))
            .count()
            > 1
    {
        return None;
    }
    let (base, number) = if let Some(number) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (16, number)
    } else if digits.len() > 1 && digits.starts_with('0') {
        (8, &digits[1..])
    } else {
        (10, digits)
    };
    let value = if number.is_empty() && base == 8 {
        0
    } else {
        i64::from_str_radix(number, base).ok()?
    };
    (value <= i64::from(u32::MAX)).then_some(value)
}

pub fn compile_minimal_program(
    source: &str,
    filename: &str,
) -> Result<Option<ProgramImage>, CCompilerError> {
    let mut parser = match TranslationUnitParser::from_source(source, filename) {
        Ok(parser) => parser,
        Err(_) => return Ok(None),
    };
    let unit = match parser.parse() {
        Ok(unit) => unit,
        Err(_) => return Ok(None),
    };
    let mut functions = Vec::new();
    let mut globals: Vec<NativeGlobal> = Vec::new();
    let mut names = HashSet::new();
    let mut global_names = HashSet::new();
    let mut prototypes = HashMap::new();
    for declaration in &unit.declarations {
        match declaration {
            ExternalDeclaration::FunctionDefinition(function) => {
                if function.declaration.return_type.trim() != "int"
                    || function.declaration.variadic
                    || function.declaration.parameters.len() > 3
                    || global_names.contains(function.declaration.name.as_str())
                    || function
                        .declaration
                        .parameters
                        .iter()
                        .any(|parameter| parameter.c_type.trim() != "int")
                    || !names.insert(function.declaration.name.as_str())
                {
                    return Ok(None);
                }
                functions.push(function);
            }
            ExternalDeclaration::FunctionPrototype(function) => {
                if function.return_type.trim() != "int"
                    || function.variadic
                    || global_names.contains(function.name.as_str())
                    || function.parameters.len() > 3
                    || function
                        .parameters
                        .iter()
                        .any(|parameter| parameter.c_type.trim() != "int")
                {
                    return Ok(None);
                }
                if prototypes
                    .insert(function.name.as_str(), function.parameters.len())
                    .is_some_and(|previous| previous != function.parameters.len())
                {
                    return Ok(None);
                }
            }
            ExternalDeclaration::Global(global) => {
                if global.c_type.trim() != "int"
                    || prototypes.contains_key(global.name.as_str())
                    || !global_names.insert(global.name.as_str())
                    || !names.insert(global.name.as_str())
                {
                    return Ok(None);
                }
                let is_array = global.array_length.is_some();
                let element_count = global.array_length.unwrap_or(1);
                let Some(byte_count) = element_count.checked_mul(2) else {
                    return Ok(None);
                };
                if byte_count > 16 * 1024 * 1024 || is_array && global.initializer.is_some() {
                    return Ok(None);
                }
                let mut data = vec![0; byte_count];
                if !is_array && let Some(initializer) = &global.initializer {
                    let Ok(value) = evaluate_integer_constant(initializer) else {
                        return Ok(None);
                    };
                    data[0] = (value as u16 >> 8) as u8;
                    data[1] = value as u8;
                }
                globals.push(NativeGlobal {
                    name: global.name.clone(),
                    data,
                    is_array,
                });
            }
        }
    }
    let Some(main) = functions
        .iter()
        .find(|function| function.declaration.name == "main")
    else {
        return Ok(None);
    };
    if !main.declaration.parameters.is_empty() {
        return Ok(None);
    }
    if functions.iter().any(|function| {
        prototypes
            .get(function.declaration.name.as_str())
            .is_some_and(|arity| *arity != function.declaration.parameters.len())
    }) {
        return Ok(None);
    }
    let defined_functions: HashMap<String, usize> = functions
        .iter()
        .map(|function| {
            (
                function.declaration.name.clone(),
                function.declaration.parameters.len(),
            )
        })
        .collect();
    if functions
        .iter()
        .any(|function| !statement_calls_are_defined(&function.body, &defined_functions))
    {
        return Ok(None);
    }
    let mut assembly = String::from(
        ".address 0\n\
         __c_entry:\n\
         \x20\x20\x20\x20JMP __c_boot\n\
         .address 10\n\
         __c_boot:\n\
         \x20\x20\x20\x20CALLX __c_function_main\n\
         \x20\x20\x20\x20HALT\n\
         .address 0x4800\n",
    );
    let mut label_counter = 0usize;
    for function in functions {
        let StatementKind::Compound(statements) = &function.body.kind else {
            return Ok(None);
        };
        let mut locals = HashMap::new();
        for (index, parameter) in function.declaration.parameters.iter().enumerate() {
            let Some(name) = &parameter.name else {
                return Ok(None);
            };
            if locals.insert(name.clone(), index as u8 + 1).is_some() {
                return Ok(None);
            }
        }
        let mut next_local_register = function.declaration.parameters.len() as u8 + 1;
        if !collect_native_locals(statements, &mut locals, &mut next_local_register) {
            return Ok(None);
        }
        for global in &globals {
            locals
                .entry(global.name.clone())
                .or_insert(if global.is_array {
                    NATIVE_GLOBAL_ARRAY
                } else {
                    NATIVE_GLOBAL_SCALAR
                });
        }
        assembly.push_str(&format!("__c_function_{}:\n", function.declaration.name));
        for (index, _) in function.declaration.parameters.iter().enumerate() {
            assembly.push_str(&format!("    LDWS R{}, {}\n", index + 1, 4 + index * 2));
        }
        let mut loop_labels = Vec::new();
        let mut returned = false;
        for statement in statements {
            if !returned {
                let Some(statement_returns) = emit_native_statement(
                    statement,
                    &locals,
                    &mut label_counter,
                    &mut loop_labels,
                    &mut assembly,
                ) else {
                    return Ok(None);
                };
                returned = statement_returns;
            }
        }
        if !returned {
            return Ok(None);
        }
    }
    assembly.push_str(".address 0x80000\n");
    for global in &globals {
        assembly.push_str(&format!("__c_global_{}:\n", global.name));
        for chunk in global.data.chunks(16) {
            assembly.push_str(".data hex ");
            for (index, byte) in chunk.iter().enumerate() {
                if index > 0 {
                    assembly.push(' ');
                }
                assembly.push_str(&format!("0x{byte:02X}"));
            }
            assembly.push('\n');
        }
    }
    crate::assembler::assemble(&assembly, 0)
        .map(Some)
        .map_err(|error| CCompilerError(format!("{filename}: {error}")))
}

fn expression_calls_are_defined(
    expression: &Expression,
    defined_functions: &HashMap<String, usize>,
) -> bool {
    match &expression.kind {
        ExpressionKind::Call {
            function,
            arguments,
        } => {
            matches!(&function.kind, ExpressionKind::Atom(name) if defined_functions.get(name) == Some(&arguments.len()))
                && expression_calls_are_defined(function, defined_functions)
                && arguments
                    .iter()
                    .all(|argument| expression_calls_are_defined(argument, defined_functions))
        }
        ExpressionKind::Unary { operand, .. }
        | ExpressionKind::Cast {
            expression: operand,
            ..
        }
        | ExpressionKind::Postfix {
            expression: operand,
            ..
        }
        | ExpressionKind::SizeofExpression(operand) => {
            expression_calls_are_defined(operand, defined_functions)
        }
        ExpressionKind::Binary { left, right, .. }
        | ExpressionKind::Assignment {
            target: left,
            value: right,
            ..
        }
        | ExpressionKind::Index {
            expression: left,
            index: right,
        } => {
            expression_calls_are_defined(left, defined_functions)
                && expression_calls_are_defined(right, defined_functions)
        }
        ExpressionKind::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            expression_calls_are_defined(condition, defined_functions)
                && expression_calls_are_defined(when_true, defined_functions)
                && expression_calls_are_defined(when_false, defined_functions)
        }
        ExpressionKind::Member { expression, .. } => {
            expression_calls_are_defined(expression, defined_functions)
        }
        ExpressionKind::InitializerList(values) => values
            .iter()
            .all(|value| expression_calls_are_defined(value, defined_functions)),
        ExpressionKind::Atom(_) | ExpressionKind::SizeofType(_) => true,
    }
}

fn statement_calls_are_defined(
    statement: &Statement,
    defined_functions: &HashMap<String, usize>,
) -> bool {
    let expression_is_defined =
        |expression: &Expression| expression_calls_are_defined(expression, defined_functions);
    match &statement.kind {
        StatementKind::Compound(statements) => statements
            .iter()
            .all(|statement| statement_calls_are_defined(statement, defined_functions)),
        StatementKind::Declaration(declarations) => declarations
            .iter()
            .filter_map(|declaration| declaration.initializer.as_ref())
            .all(expression_is_defined),
        StatementKind::Expression(expression) | StatementKind::Return(expression) => {
            expression.as_ref().is_none_or(expression_is_defined)
        }
        StatementKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expression_is_defined(condition)
                && statement_calls_are_defined(then_branch, defined_functions)
                && else_branch
                    .as_ref()
                    .is_none_or(|branch| statement_calls_are_defined(branch, defined_functions))
        }
        StatementKind::While { condition, body } | StatementKind::DoWhile { condition, body } => {
            expression_is_defined(condition) && statement_calls_are_defined(body, defined_functions)
        }
        StatementKind::For {
            initializer,
            condition,
            update,
            body,
        } => {
            initializer
                .as_ref()
                .is_none_or(|statement| statement_calls_are_defined(statement, defined_functions))
                && condition.as_ref().is_none_or(expression_is_defined)
                && update.as_ref().is_none_or(expression_is_defined)
                && statement_calls_are_defined(body, defined_functions)
        }
        StatementKind::Switch { expression, cases } => {
            expression_is_defined(expression)
                && cases.iter().all(|(value, statements)| {
                    value.as_ref().is_none_or(expression_is_defined)
                        && statements.iter().all(|statement| {
                            statement_calls_are_defined(statement, defined_functions)
                        })
                })
        }
        StatementKind::Break
        | StatementKind::Continue
        | StatementKind::Goto(_)
        | StatementKind::Label { .. } => true,
    }
}

fn collect_native_locals(
    statements: &[Statement],
    locals: &mut HashMap<String, u8>,
    next_register: &mut u8,
) -> bool {
    for statement in statements {
        match &statement.kind {
            StatementKind::Declaration(declarations) => {
                for declaration in declarations {
                    if declaration.c_type.trim() != "int"
                        || declaration.array_length.is_some()
                        || locals.contains_key(&declaration.name)
                        || *next_register > 3
                    {
                        return false;
                    }
                    locals.insert(declaration.name.clone(), *next_register);
                    *next_register += 1;
                }
            }
            StatementKind::Compound(statements) => {
                if !collect_native_locals(statements, locals, next_register) {
                    return false;
                }
            }
            StatementKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                if !collect_native_locals(std::slice::from_ref(then_branch), locals, next_register)
                    || else_branch.as_ref().is_some_and(|branch| {
                        !collect_native_locals(std::slice::from_ref(branch), locals, next_register)
                    })
                {
                    return false;
                }
            }
            StatementKind::While { body, .. }
            | StatementKind::DoWhile { body, .. }
            | StatementKind::For { body, .. } => {
                if !collect_native_locals(std::slice::from_ref(body), locals, next_register) {
                    return false;
                }
                if let StatementKind::For {
                    initializer: Some(initializer),
                    ..
                } = &statement.kind
                    && !collect_native_locals(
                        std::slice::from_ref(initializer),
                        locals,
                        next_register,
                    )
                {
                    return false;
                }
            }
            StatementKind::Switch { cases, .. } => {
                for (_, statements) in cases {
                    if !collect_native_locals(statements, locals, next_register) {
                        return false;
                    }
                }
            }
            _ => {}
        }
    }
    true
}

fn emit_native_expression(
    expression: &Expression,
    destination: u8,
    temporary_registers: &[u8],
    locals: &HashMap<String, u8>,
    label_counter: &mut usize,
    assembly: &mut String,
) -> bool {
    match &expression.kind {
        ExpressionKind::Atom(value) => {
            if let Some(value) = parse_integer_constant(value) {
                if !(0..=0xffff).contains(&value) {
                    return false;
                }
                assembly.push_str(&format!("    LDI R{destination}, 0x{value:04X}\n"));
            } else if let Some(source) = locals.get(value) {
                if *source == NATIVE_GLOBAL_SCALAR {
                    assembly.push_str(&format!(
                        "    LDI30 R7, __c_global_{value}\n    LDW R{destination}, R7\n"
                    ));
                } else if *source == NATIVE_GLOBAL_ARRAY {
                    assembly.push_str(&format!("    LDI30 R{destination}, __c_global_{value}\n"));
                } else if *source != destination {
                    assembly.push_str(&format!("    MOV R{destination}, R{source}\n"));
                }
            } else {
                return false;
            }
            true
        }
        ExpressionKind::Unary { operator, operand } => {
            if !matches!(operator.as_str(), "+" | "-" | "~" | "!")
                || !emit_native_expression(
                    operand,
                    destination,
                    temporary_registers,
                    locals,
                    label_counter,
                    assembly,
                )
            {
                return false;
            }
            match operator.as_str() {
                "+" => {}
                "-" => {
                    assembly.push_str(&format!("    NOT R{destination}\n    INC R{destination}\n"))
                }
                "~" => assembly.push_str(&format!("    NOT R{destination}\n")),
                "!" => {
                    let label = *label_counter;
                    *label_counter += 1;
                    assembly.push_str(&format!(
                        "    LDI R7, 0\n    CMP R{destination}, R7\n\
                         BZX __c_not_{label}_true\n\
                         LDI R{destination}, 0\n\
                         JMPX __c_not_{label}_end\n\
                         __c_not_{label}_true:\n\
                         LDI R{destination}, 1\n\
                         __c_not_{label}_end:\n"
                    ));
                }
                _ => unreachable!(),
            }
            true
        }
        ExpressionKind::Cast { c_type, expression }
            if matches!(c_type.trim(), "int" | "signed" | "unsigned") =>
        {
            emit_native_expression(
                expression,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            )
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } if matches!(
            operator.as_str(),
            "+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "<<" | ">>"
        ) =>
        {
            let Some((&right_register, remaining_registers)) = temporary_registers.split_first()
            else {
                return false;
            };
            if !emit_native_expression(
                left,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            ) {
                return false;
            }
            assembly.push_str(&format!("    PUSH R{destination}\n"));
            if !emit_native_expression(
                right,
                right_register,
                remaining_registers,
                locals,
                label_counter,
                assembly,
            ) {
                return false;
            }
            assembly.push_str(&format!("    POP R{destination}\n"));
            let instruction = match operator.as_str() {
                "+" => "ADD",
                "-" => "SUB",
                "*" => "MUL",
                "/" => "DIVS",
                "%" => "MODS",
                "&" => "AND",
                "|" => "OR",
                "^" => "XOR",
                "<<" => "SHL",
                ">>" => "ASR",
                _ => unreachable!(),
            };
            assembly.push_str(&format!(
                "    {instruction} R{destination}, R{right_register}\n"
            ));
            true
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } if matches!(operator.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=") => {
            let Some((&right_register, remaining_registers)) = temporary_registers.split_first()
            else {
                return false;
            };
            if !emit_native_expression(
                left,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            ) {
                return false;
            }
            assembly.push_str(&format!("    PUSH R{destination}\n"));
            if !emit_native_expression(
                right,
                right_register,
                remaining_registers,
                locals,
                label_counter,
                assembly,
            ) {
                return false;
            }
            assembly.push_str(&format!("    POP R{destination}\n"));
            let label = *label_counter;
            *label_counter += 1;
            let (branch, truth_label, end_label) = match operator.as_str() {
                "==" => ("BZX", "true", "end"),
                "!=" => ("BNZX", "true", "end"),
                "<" => ("BLTX", "true", "end"),
                "<=" => ("BLEX", "true", "end"),
                ">" => ("BGTX", "true", "end"),
                ">=" => ("BGEX", "true", "end"),
                _ => unreachable!(),
            };
            assembly.push_str(&format!(
                "    CMP R{destination}, R{right_register}\n\
                 {branch} __c_cmp_{label}_{truth_label}\n\
                 LDI R{destination}, 0\n\
                 JMPX __c_cmp_{label}_{end_label}\n\
                 __c_cmp_{label}_{truth_label}:\n\
                 LDI R{destination}, 1\n\
                 __c_cmp_{label}_{end_label}:\n"
            ));
            true
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } if matches!(operator.as_str(), "&&" | "||") => {
            let label = *label_counter;
            *label_counter += 1;
            if !emit_native_expression(
                left,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            ) {
                return false;
            }
            let (short_branch, truth_branch) = if operator == "&&" {
                ("BZX", "BNZX")
            } else {
                ("BNZX", "BNZX")
            };
            assembly.push_str(&format!(
                "    LDI R7, 0\n    CMP R{destination}, R7\n\
                 {short_branch} __c_logic_{label}_short\n"
            ));
            if !emit_native_expression(
                right,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            ) {
                return false;
            }
            assembly.push_str(&format!(
                "    LDI R7, 0\n    CMP R{destination}, R7\n\
                 {truth_branch} __c_logic_{label}_true\n\
                 __c_logic_{label}_short:\n\
                 LDI R{destination}, {}\n\
                 JMPX __c_logic_{label}_end\n\
                 __c_logic_{label}_true:\n\
                 LDI R{destination}, {}\n\
                 __c_logic_{label}_end:\n",
                if operator == "&&" { 0 } else { 1 },
                if operator == "&&" { 1 } else { 0 }
            ));
            true
        }
        ExpressionKind::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            let label = *label_counter;
            *label_counter += 1;
            if !emit_native_expression(
                condition,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            ) {
                return false;
            }
            assembly.push_str(&format!(
                "    LDI R7, 0\n    CMP R{destination}, R7\n\
                 BZX __c_cond_{label}_false\n"
            ));
            if !emit_native_expression(
                when_true,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            ) {
                return false;
            }
            assembly.push_str(&format!(
                "    JMPX __c_cond_{label}_end\n__c_cond_{label}_false:\n"
            ));
            if !emit_native_expression(
                when_false,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            ) {
                return false;
            }
            assembly.push_str(&format!("__c_cond_{label}_end:\n"));
            true
        }
        ExpressionKind::Call {
            function,
            arguments,
        } => {
            let ExpressionKind::Atom(name) = &function.kind else {
                return false;
            };
            if arguments.len() > 3 {
                return false;
            }
            for register in 1..=6 {
                assembly.push_str(&format!("    PUSH R{register}\n"));
            }
            for (index, argument) in arguments.iter().enumerate() {
                if !emit_native_expression(
                    argument,
                    4 + index as u8,
                    &temporary_registers[index.min(temporary_registers.len())..],
                    locals,
                    label_counter,
                    assembly,
                ) {
                    return false;
                }
            }
            for index in (0..arguments.len()).rev() {
                assembly.push_str(&format!("    PUSH R{}\n", 4 + index));
            }
            assembly.push_str(&format!("    CALLX __c_function_{name}\n"));
            if !arguments.is_empty() {
                assembly.push_str(&format!("    ADJSP {}\n", arguments.len() * 2));
            }
            for register in (1..=6).rev() {
                assembly.push_str(&format!("    POP R{register}\n"));
            }
            if destination != 0 {
                assembly.push_str(&format!("    MOV R{destination}, R0\n"));
            }
            true
        }
        ExpressionKind::Index { expression, index } => {
            let ExpressionKind::Atom(name) = &expression.kind else {
                return false;
            };
            if destination == 7
                || locals.get(name) != Some(&NATIVE_GLOBAL_ARRAY)
                || !emit_native_expression(
                    index,
                    destination,
                    temporary_registers,
                    locals,
                    label_counter,
                    assembly,
                )
            {
                return false;
            }
            assembly.push_str(&format!(
                "    SHL R{destination}, 1\n\
                 LDI30 R7, __c_global_{name}\n\
                 ADD R7, R{destination}\n\
                 LDW R{destination}, R7\n"
            ));
            true
        }
        _ => false,
    }
}

fn emit_native_assignment(
    expression: &Expression,
    temporary_registers: &[u8],
    locals: &HashMap<String, u8>,
    label_counter: &mut usize,
    assembly: &mut String,
) -> bool {
    let ExpressionKind::Assignment {
        operator,
        target,
        value,
    } = &expression.kind
    else {
        return false;
    };
    if let ExpressionKind::Index {
        expression: base,
        index,
    } = &target.kind
    {
        let ExpressionKind::Atom(name) = &base.kind else {
            return false;
        };
        if locals.get(name) != Some(&NATIVE_GLOBAL_ARRAY) {
            return false;
        }
        let Some((&value_register, remaining_registers)) = temporary_registers.split_first() else {
            return false;
        };
        if !emit_native_expression(
            value,
            value_register,
            remaining_registers,
            locals,
            label_counter,
            assembly,
        ) {
            return false;
        }
        if !emit_native_expression(index, 0, &[5, 6, 7], locals, label_counter, assembly) {
            return false;
        }
        assembly.push_str(&format!(
            "    SHL R0, 1\n\
             LDI30 R7, __c_global_{name}\n\
             ADD R7, R0\n"
        ));
        if operator != "=" {
            let binary_operator = match operator.as_str() {
                "+=" => "ADD",
                "-=" => "SUB",
                "*=" => "MUL",
                "/=" => "DIVS",
                "%=" => "MODS",
                "<<=" => "SHL",
                ">>=" => "ASR",
                "&=" => "AND",
                "|=" => "OR",
                "^=" => "XOR",
                _ => return false,
            };
            assembly.push_str(&format!(
                "    LDW R0, R7\n    {binary_operator} R0, R{value_register}\n"
            ));
            assembly.push_str("    STW R7, R0\n");
        } else {
            assembly.push_str(&format!("    STW R7, R{value_register}\n"));
        }
        return true;
    }
    let ExpressionKind::Atom(name) = &target.kind else {
        return false;
    };
    let Some(&destination) = locals.get(name) else {
        return false;
    };
    let is_global = matches!(destination, NATIVE_GLOBAL_SCALAR | NATIVE_GLOBAL_ARRAY);
    if destination == NATIVE_GLOBAL_ARRAY {
        return false;
    }
    let binary_operator = match operator.as_str() {
        "=" => None,
        "+=" => Some("ADD"),
        "-=" => Some("SUB"),
        "*=" => Some("MUL"),
        "/=" => Some("DIVS"),
        "%=" => Some("MODS"),
        "<<=" => Some("SHL"),
        ">>=" => Some("ASR"),
        "&=" => Some("AND"),
        "|=" => Some("OR"),
        "^=" => Some("XOR"),
        _ => return false,
    };
    let Some((&value_register, remaining_registers)) = temporary_registers.split_first() else {
        return false;
    };
    if !emit_native_expression(
        value,
        value_register,
        remaining_registers,
        locals,
        label_counter,
        assembly,
    ) {
        return false;
    }
    if is_global {
        if let Some(binary_operator) = binary_operator {
            assembly.push_str(&format!(
                "    LDI30 R7, __c_global_{name}\n    LDW R0, R7\n\
                 {binary_operator} R0, R{value_register}\n\
                 LDI30 R7, __c_global_{name}\n    STW R7, R0\n"
            ));
        } else {
            assembly.push_str(&format!(
                "    LDI30 R7, __c_global_{name}\n    STW R7, R{value_register}\n"
            ));
        }
        return true;
    }
    match binary_operator {
        Some(binary_operator) => assembly.push_str(&format!(
            "    {binary_operator} R{destination}, R{value_register}\n"
        )),
        None if destination != value_register => {
            assembly.push_str(&format!("    MOV R{destination}, R{value_register}\n"))
        }
        None => {}
    }
    true
}

fn emit_native_statement(
    statement: &Statement,
    locals: &HashMap<String, u8>,
    label_counter: &mut usize,
    loop_labels: &mut Vec<(String, String)>,
    assembly: &mut String,
) -> Option<bool> {
    match &statement.kind {
        StatementKind::Declaration(declarations) => {
            for declaration in declarations {
                if let Some(initializer) = &declaration.initializer {
                    let destination = *locals.get(&declaration.name)?;
                    if !emit_native_expression(
                        initializer,
                        destination,
                        &[4, 5, 6, 7],
                        locals,
                        label_counter,
                        assembly,
                    ) {
                        return None;
                    }
                }
            }
            Some(false)
        }
        StatementKind::Expression(None) => Some(false),
        StatementKind::Expression(Some(expression)) => {
            if emit_native_assignment(expression, &[4, 5, 6, 7], locals, label_counter, assembly)
                || emit_native_update(expression, locals, label_counter, assembly)
            {
                Some(false)
            } else {
                None
            }
        }
        StatementKind::Return(Some(expression)) => {
            if !emit_native_expression(
                expression,
                0,
                &[4, 5, 6, 7],
                locals,
                label_counter,
                assembly,
            ) {
                return None;
            }
            assembly.push_str("    RET\n");
            Some(true)
        }
        StatementKind::Break => {
            let (break_label, _) = loop_labels.last()?;
            assembly.push_str(&format!("    JMPX {break_label}\n"));
            Some(false)
        }
        StatementKind::Continue => {
            let (_, continue_label) = loop_labels.last()?;
            if continue_label.is_empty() {
                return None;
            }
            assembly.push_str(&format!("    JMPX {continue_label}\n"));
            Some(false)
        }
        StatementKind::Compound(statements) => {
            let mut branch_returns = false;
            for statement in statements {
                if branch_returns {
                    break;
                }
                branch_returns =
                    emit_native_statement(statement, locals, label_counter, loop_labels, assembly)?;
            }
            Some(branch_returns)
        }
        StatementKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let label = *label_counter;
            *label_counter += 1;
            if !emit_native_expression(condition, 0, &[4, 5, 6, 7], locals, label_counter, assembly)
            {
                return None;
            }
            assembly.push_str("    LDI R7, 0\n    CMP R0, R7\n");
            let else_returns = if let Some(else_branch) = else_branch {
                assembly.push_str(&format!("    BNZX __c_if_{label}_then\n"));
                let returns = emit_native_statement(
                    else_branch,
                    locals,
                    label_counter,
                    loop_labels,
                    assembly,
                )?;
                assembly.push_str(&format!("    JMPX __c_if_{label}_end\n"));
                returns
            } else {
                assembly.push_str(&format!("    BZX __c_if_{label}_end\n"));
                false
            };
            assembly.push_str(&format!("__c_if_{label}_then:\n"));
            let then_returns =
                emit_native_statement(then_branch, locals, label_counter, loop_labels, assembly)?;
            assembly.push_str(&format!("__c_if_{label}_end:\n"));
            Some(else_branch.is_some() && else_returns && then_returns)
        }
        StatementKind::While { condition, body } => {
            let label = *label_counter;
            *label_counter += 1;
            assembly.push_str(&format!("__c_while_{label}_begin:\n"));
            if !emit_native_expression(condition, 0, &[4, 5, 6, 7], locals, label_counter, assembly)
            {
                return None;
            }
            assembly.push_str(&format!(
                "    LDI R7, 0\n    CMP R0, R7\n    BZX __c_while_{label}_end\n"
            ));
            loop_labels.push((
                format!("__c_while_{label}_end"),
                format!("__c_while_{label}_begin"),
            ));
            emit_native_statement(body, locals, label_counter, loop_labels, assembly)?;
            loop_labels.pop();
            assembly.push_str(&format!(
                "    JMPX __c_while_{label}_begin\n__c_while_{label}_end:\n"
            ));
            Some(false)
        }
        StatementKind::DoWhile { body, condition } => {
            let label = *label_counter;
            *label_counter += 1;
            assembly.push_str(&format!("__c_do_{label}_begin:\n"));
            loop_labels.push((
                format!("__c_do_{label}_end"),
                format!("__c_do_{label}_condition"),
            ));
            emit_native_statement(body, locals, label_counter, loop_labels, assembly)?;
            loop_labels.pop();
            assembly.push_str(&format!("__c_do_{label}_condition:\n"));
            if !emit_native_expression(condition, 0, &[4, 5, 6, 7], locals, label_counter, assembly)
            {
                return None;
            }
            assembly.push_str(&format!(
                "    LDI R7, 0\n    CMP R0, R7\n    BNZX __c_do_{label}_begin\n"
            ));
            assembly.push_str(&format!("__c_do_{label}_end:\n"));
            Some(false)
        }
        StatementKind::For {
            initializer,
            condition,
            update,
            body,
        } => {
            if let Some(initializer) = initializer {
                emit_native_statement(initializer, locals, label_counter, loop_labels, assembly)?;
            }
            let label = *label_counter;
            *label_counter += 1;
            assembly.push_str(&format!("__c_for_{label}_begin:\n"));
            if let Some(condition) = condition {
                if !emit_native_expression(
                    condition,
                    0,
                    &[4, 5, 6, 7],
                    locals,
                    label_counter,
                    assembly,
                ) {
                    return None;
                }
                assembly.push_str(&format!(
                    "    LDI R7, 0\n    CMP R0, R7\n    BZX __c_for_{label}_end\n"
                ));
            }
            loop_labels.push((
                format!("__c_for_{label}_end"),
                format!("__c_for_{label}_update"),
            ));
            emit_native_statement(body, locals, label_counter, loop_labels, assembly)?;
            loop_labels.pop();
            assembly.push_str(&format!("__c_for_{label}_update:\n"));
            if let Some(update) = update
                && !emit_native_assignment(update, &[4, 5, 6, 7], locals, label_counter, assembly)
                && !emit_native_update(update, locals, label_counter, assembly)
            {
                return None;
            }
            assembly.push_str(&format!(
                "    JMPX __c_for_{label}_begin\n__c_for_{label}_end:\n"
            ));
            Some(false)
        }
        StatementKind::Switch { expression, cases } => {
            if !emit_native_expression(
                expression,
                0,
                &[4, 5, 6, 7],
                locals,
                label_counter,
                assembly,
            ) {
                return None;
            }
            let mut case_labels = Vec::with_capacity(cases.len());
            let mut default_label = None;
            let mut case_values = HashSet::new();
            for (value, _) in cases {
                let dispatch_label = format!("__c_switch_{}", *label_counter);
                *label_counter += 1;
                let body_label = format!("__c_switch_body_{}", *label_counter);
                *label_counter += 1;
                if let Some(value) = value {
                    let Ok(value) = evaluate_integer_constant(value) else {
                        return None;
                    };
                    let value = value as u16;
                    if !case_values.insert(value) {
                        return None;
                    }
                    case_labels.push((Some(value), dispatch_label, body_label));
                } else {
                    if default_label.replace(dispatch_label.clone()).is_some() {
                        return None;
                    }
                    case_labels.push((None, dispatch_label, body_label));
                }
            }
            let end_label = format!("__c_switch_{}_end", *label_counter);
            *label_counter += 1;
            assembly.push_str("    PUSH R0\n");
            for (value, dispatch_label, _) in &case_labels {
                if let Some(value) = value {
                    assembly.push_str(&format!(
                        "    LDWS R6, 0\n    LDI R7, 0x{value:04X}\n\
                         CMP R6, R7\n    BZX {dispatch_label}\n"
                    ));
                }
            }
            let fallback = default_label.as_deref().unwrap_or(&end_label);
            if default_label.is_none() {
                assembly.push_str("    POP R0\n");
            }
            assembly.push_str(&format!("    JMPX {fallback}\n"));
            for (_, dispatch_label, body_label) in &case_labels {
                assembly.push_str(&format!(
                    "{dispatch_label}:\n    POP R0\n    JMPX {body_label}\n"
                ));
            }
            let continue_label = loop_labels
                .iter()
                .rev()
                .find(|(_, label)| !label.is_empty())
                .map_or("", |(_, label)| label.as_str())
                .to_owned();
            loop_labels.push((end_label.clone(), continue_label));
            for ((_, statements), (_, _, body_label)) in cases.iter().zip(&case_labels) {
                assembly.push_str(&format!("{body_label}:\n"));
                for statement in statements {
                    emit_native_statement(statement, locals, label_counter, loop_labels, assembly)?;
                }
            }
            loop_labels.pop();
            assembly.push_str(&format!("{end_label}:\n"));
            Some(false)
        }
        _ => None,
    }
}

fn emit_native_update(
    expression: &Expression,
    locals: &HashMap<String, u8>,
    label_counter: &mut usize,
    assembly: &mut String,
) -> bool {
    let (operator, expression) = match &expression.kind {
        ExpressionKind::Postfix {
            operator,
            expression,
        }
        | ExpressionKind::Unary {
            operator,
            operand: expression,
        } if matches!(operator.as_str(), "++" | "--") => (operator, expression),
        _ => return false,
    };
    let instruction = match operator.as_str() {
        "++" => "INC",
        "--" => "DEC",
        _ => return false,
    };
    if let ExpressionKind::Index {
        expression: base,
        index,
    } = &expression.kind
    {
        let ExpressionKind::Atom(name) = &base.kind else {
            return false;
        };
        if locals.get(name) != Some(&NATIVE_GLOBAL_ARRAY)
            || !emit_native_expression(index, 0, &[5, 6, 7], locals, label_counter, assembly)
        {
            return false;
        }
        assembly.push_str(&format!(
            "    SHL R0, 1\n\
             LDI30 R7, __c_global_{name}\n\
             ADD R7, R0\n\
             LDW R0, R7\n\
             {instruction} R0\n\
             STW R7, R0\n"
        ));
        return true;
    }
    let ExpressionKind::Atom(name) = &expression.kind else {
        return false;
    };
    let Some(register) = locals.get(name) else {
        return false;
    };
    if matches!(*register, NATIVE_GLOBAL_SCALAR | NATIVE_GLOBAL_ARRAY) {
        if *register == NATIVE_GLOBAL_ARRAY {
            return false;
        }
        assembly.push_str(&format!(
            "    LDI30 R7, __c_global_{name}\n    LDW R0, R7\n\
             {instruction} R0\n    LDI30 R7, __c_global_{name}\n    STW R7, R0\n"
        ));
    } else {
        assembly.push_str(&format!("    {instruction} R{register}\n"));
    }
    true
}

pub struct StatementParser {
    expressions: ExpressionParser,
}

pub struct TranslationUnitParser {
    statements: StatementParser,
}

const ASSIGNMENTS: [&str; 11] = [
    "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=",
];
const TYPE_WORDS: [&str; 12] = [
    "void", "char", "short", "int", "long", "float", "double", "signed", "unsigned", "const",
    "volatile", "_Bool",
];
const STORAGE_WORDS: [&str; 6] = [
    "auto", "extern", "register", "static", "typedef", "restrict",
];

fn precedence(operator: &str) -> Option<u8> {
    Some(match operator {
        "," => 0,
        "||" => 1,
        "&&" => 2,
        "|" => 3,
        "^" => 4,
        "&" => 5,
        "==" | "!=" => 6,
        "<" | "<=" | ">" | ">=" => 7,
        "<<" | ">>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "%" => 10,
        _ => return None,
    })
}

impl ExpressionParser {
    pub fn new(tokens: Vec<Token>) -> Self {
        let tokens = if tokens.is_empty() {
            vec![Token {
                value: "<eof>".to_owned(),
                line: 1,
                filename: None,
            }]
        } else {
            tokens
        };
        Self {
            tokens,
            position: 0,
        }
    }

    pub fn from_source(source: &str, filename: &str) -> Result<Self, CCompilerError> {
        Ok(Self::new(lex_raw(source, filename)?))
    }

    pub fn parse(&mut self) -> Result<Expression, CCompilerError> {
        self.parse_expression(0)
    }

    pub fn is_at_end(&self) -> bool {
        self.token().value == "<eof>"
    }

    fn token(&self) -> &Token {
        &self.tokens[self.position.min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> Token {
        let token = self.token().clone();
        if token.value != "<eof>" {
            self.position += 1;
        }
        token
    }

    fn accept(&mut self, value: &str) -> Option<Token> {
        if self.token().value == value {
            Some(self.advance())
        } else {
            None
        }
    }

    fn expect(&mut self, value: &str) -> Result<Token, CCompilerError> {
        self.accept(value)
            .ok_or_else(|| self.fail(&format!("expected {value:?}, got {:?}", self.token().value)))
    }

    fn fail(&self, message: &str) -> CCompilerError {
        let token = self.token();
        error(
            token.filename.as_deref().unwrap_or("<source>"),
            token.line,
            message,
        )
    }

    fn make(&self, token: &Token, kind: ExpressionKind) -> Expression {
        Expression {
            kind,
            line: token.line,
            filename: token.filename.clone(),
        }
    }

    fn parse_expression(&mut self, minimum: u8) -> Result<Expression, CCompilerError> {
        let mut left = self.parse_unary()?;
        loop {
            let operator = self.token().value.as_str();
            if ASSIGNMENTS.contains(&operator) && minimum <= 1 {
                let token = self.advance();
                if !Self::is_lvalue(&left) {
                    return Err(error(
                        token.filename.as_deref().unwrap_or("<source>"),
                        token.line,
                        "assignment requires a scalar variable",
                    ));
                }
                let value = self.parse_expression(1)?;
                left = self.make(
                    &token,
                    ExpressionKind::Assignment {
                        operator: token.value.clone(),
                        target: Box::new(left),
                        value: Box::new(value),
                    },
                );
                continue;
            }
            if operator == "?" && minimum <= 1 {
                let token = self.advance();
                let when_true = self.parse_expression(0)?;
                self.expect(":")?;
                let when_false = self.parse_expression(1)?;
                left = self.make(
                    &token,
                    ExpressionKind::Conditional {
                        condition: Box::new(left),
                        when_true: Box::new(when_true),
                        when_false: Box::new(when_false),
                    },
                );
                continue;
            }
            let Some(operator_precedence) = precedence(operator) else {
                break;
            };
            if operator_precedence < minimum {
                break;
            }
            let token = self.advance();
            let right = self.parse_expression(operator_precedence + 1)?;
            left = self.make(
                &token,
                ExpressionKind::Binary {
                    operator: token.value.clone(),
                    left: Box::new(left),
                    right: Box::new(right),
                },
            );
        }
        Ok(left)
    }

    fn is_lvalue(expression: &Expression) -> bool {
        match expression.kind {
            ExpressionKind::Atom(ref name) => {
                !name.starts_with("<string:")
                    && name != "<eof>"
                    && name != "sizeof"
                    && name
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_alphabetic() || character == '_')
            }
            ExpressionKind::Index { .. } | ExpressionKind::Member { .. } => true,
            ExpressionKind::Unary { ref operator, .. } => operator == "*",
            _ => false,
        }
    }

    fn parse_type_name(&mut self) -> Result<String, CCompilerError> {
        if matches!(self.token().value.as_str(), "struct" | "union" | "enum") {
            return Err(self.fail("record and enum types are not supported here yet"));
        }
        let mut words = Vec::new();
        while TYPE_WORDS.contains(&self.token().value.as_str()) {
            words.push(self.advance().value);
        }
        if !words.iter().any(|word| {
            matches!(
                word.as_str(),
                "void" | "char" | "short" | "int" | "long" | "float" | "double" | "_Bool"
            )
        }) {
            return Err(self.fail("expected a type name"));
        }
        let mut c_type = words.join(" ");
        let mut pointer_count = 0;
        while self.accept("*").is_some() {
            pointer_count += 1;
            while matches!(
                self.token().value.as_str(),
                "const" | "volatile" | "restrict"
            ) {
                self.advance();
            }
        }
        for _ in 0..pointer_count {
            c_type.push('*');
        }
        Ok(c_type)
    }

    fn parse_unary(&mut self) -> Result<Expression, CCompilerError> {
        let token = self.token().clone();
        if self.accept("sizeof").is_some() {
            if self.token().value == "(" && self.starts_type_name_within_parens() {
                self.advance();
                let c_type = self.parse_type_name()?;
                self.expect(")")?;
                return Ok(self.make(&token, ExpressionKind::SizeofType(c_type)));
            }
            let expression = self.parse_unary()?;
            return Ok(self.make(
                &token,
                ExpressionKind::SizeofExpression(Box::new(expression)),
            ));
        }
        if self.token().value == "(" && self.starts_type_name_within_parens() {
            self.advance();
            let c_type = self.parse_type_name()?;
            self.expect(")")?;
            let expression = self.parse_unary()?;
            return Ok(self.make(
                &token,
                ExpressionKind::Cast {
                    c_type,
                    expression: Box::new(expression),
                },
            ));
        }
        if matches!(
            token.value.as_str(),
            "+" | "-" | "!" | "~" | "&" | "*" | "++" | "--"
        ) {
            self.advance();
            let operand = self.parse_unary()?;
            if matches!(token.value.as_str(), "++" | "--") && !Self::is_lvalue(&operand) {
                return Err(error(
                    token.filename.as_deref().unwrap_or("<source>"),
                    token.line,
                    "increment requires a modifiable lvalue",
                ));
            }
            return Ok(self.make(
                &token,
                ExpressionKind::Unary {
                    operator: token.value.clone(),
                    operand: Box::new(operand),
                },
            ));
        }

        let mut expression = self.parse_primary()?;
        loop {
            if let Some(open) = self.accept("[") {
                let index = self.parse_expression(0)?;
                self.expect("]")?;
                expression = self.make(
                    &open,
                    ExpressionKind::Index {
                        expression: Box::new(expression),
                        index: Box::new(index),
                    },
                );
            } else if let Some(open) = self.accept("(") {
                if !Self::is_callable(&expression)
                    && !matches!(
                        expression.kind,
                        ExpressionKind::Unary { ref operator, .. } if operator == "*"
                    )
                {
                    return Err(error(
                        open.filename.as_deref().unwrap_or("<source>"),
                        open.line,
                        "only direct function calls are supported",
                    ));
                }
                let mut arguments = Vec::new();
                if self.token().value != ")" {
                    loop {
                        arguments.push(self.parse_expression(1)?);
                        if self.accept(",").is_none() {
                            break;
                        }
                    }
                }
                self.expect(")")?;
                expression = self.make(
                    &open,
                    ExpressionKind::Call {
                        function: Box::new(expression),
                        arguments,
                    },
                );
            } else if matches!(self.token().value.as_str(), "++" | "--") {
                let operator = self.advance();
                if !Self::is_lvalue(&expression) {
                    return Err(error(
                        operator.filename.as_deref().unwrap_or("<source>"),
                        operator.line,
                        "increment requires a modifiable lvalue",
                    ));
                }
                expression = self.make(
                    &operator,
                    ExpressionKind::Postfix {
                        operator: operator.value.clone(),
                        expression: Box::new(expression),
                    },
                );
            } else if matches!(self.token().value.as_str(), "." | "->") {
                let operator = self.advance();
                let name = self.advance();
                if !name
                    .value
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_alphabetic() || character == '_')
                {
                    return Err(error(
                        name.filename.as_deref().unwrap_or("<source>"),
                        name.line,
                        "expected a member name",
                    ));
                }
                expression = self.make(
                    &operator,
                    ExpressionKind::Member {
                        expression: Box::new(expression),
                        operator: operator.value.clone(),
                        name: name.value,
                    },
                );
            } else {
                break;
            }
        }
        Ok(expression)
    }

    fn starts_type_name_within_parens(&self) -> bool {
        self.token().value == "("
            && TYPE_WORDS.contains(
                &self
                    .tokens
                    .get(self.position + 1)
                    .map_or("", |token| token.value.as_str()),
            )
    }

    fn is_callable(expression: &Expression) -> bool {
        match &expression.kind {
            ExpressionKind::Atom(name) => name
                .chars()
                .next()
                .is_some_and(|character| character.is_alphabetic() || character == '_'),
            ExpressionKind::Index { .. } | ExpressionKind::Member { .. } => true,
            _ => false,
        }
    }

    fn parse_primary(&mut self) -> Result<Expression, CCompilerError> {
        let token = self.advance();
        if token.value == "(" {
            let expression = self.parse_expression(0)?;
            self.expect(")")?;
            return Ok(expression);
        }
        let first = token.value.chars().next();
        let is_identifier =
            first.is_some_and(|character| character.is_alphabetic() || character == '_');
        if is_identifier
            && (TYPE_WORDS.contains(&token.value.as_str())
                || STORAGE_WORDS.contains(&token.value.as_str()))
        {
            return Err(error(
                token.filename.as_deref().unwrap_or("<source>"),
                token.line,
                &format!("{} is not supported", token.value),
            ));
        }
        if token.value == "<eof>" || matches!(token.value.as_str(), ")" | "]" | "}" | ";" | ",") {
            return Err(error(
                token.filename.as_deref().unwrap_or("<source>"),
                token.line,
                "expected an expression",
            ));
        }
        Ok(self.make(&token, ExpressionKind::Atom(token.value.clone())))
    }
}

impl StatementParser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            expressions: ExpressionParser::new(tokens),
        }
    }

    pub fn from_source(source: &str, filename: &str) -> Result<Self, CCompilerError> {
        Ok(Self::new(lex_raw(source, filename)?))
    }

    pub fn parse(&mut self) -> Result<Statement, CCompilerError> {
        let statement = self.parse_statement()?;
        if !self.expressions.is_at_end() {
            return Err(self.expressions.fail("unexpected token after statement"));
        }
        Ok(statement)
    }

    fn at(&self, value: &str) -> bool {
        self.expressions.token().value == value
    }

    fn expect(&mut self, value: &str) -> Result<Token, CCompilerError> {
        self.expressions.expect(value)
    }

    fn accept(&mut self, value: &str) -> Option<Token> {
        self.expressions.accept(value)
    }

    fn statement(&self, token: &Token, kind: StatementKind) -> Statement {
        Statement {
            kind,
            line: token.line,
            filename: token.filename.clone(),
        }
    }

    fn parse_statement(&mut self) -> Result<Statement, CCompilerError> {
        let start = self.expressions.token().clone();
        if let Some(open) = self.accept("{") {
            let mut statements = Vec::new();
            while !self.at("}") && !self.expressions.is_at_end() {
                statements.push(self.parse_statement()?);
            }
            if self.expressions.is_at_end() {
                return Err(error(
                    open.filename.as_deref().unwrap_or("<source>"),
                    open.line,
                    "unterminated compound statement",
                ));
            }
            self.expect("}")?;
            return Ok(self.statement(&open, StatementKind::Compound(statements)));
        }
        if self.starts_declaration() {
            return self.parse_declaration();
        }
        if self.accept(";").is_some() {
            return Ok(self.statement(&start, StatementKind::Expression(None)));
        }
        if self.accept("return").is_some() {
            let value = if self.at(";") {
                None
            } else {
                Some(self.expressions.parse_expression(0)?)
            };
            self.expect(";")?;
            return Ok(self.statement(&start, StatementKind::Return(value)));
        }
        if self.accept("if").is_some() {
            self.expect("(")?;
            let condition = self.expressions.parse_expression(0)?;
            self.expect(")")?;
            let then_branch = Box::new(self.parse_statement()?);
            let else_branch = if self.accept("else").is_some() {
                Some(Box::new(self.parse_statement()?))
            } else {
                None
            };
            return Ok(self.statement(
                &start,
                StatementKind::If {
                    condition,
                    then_branch,
                    else_branch,
                },
            ));
        }
        if self.accept("while").is_some() {
            self.expect("(")?;
            let condition = self.expressions.parse_expression(0)?;
            self.expect(")")?;
            let body = Box::new(self.parse_statement()?);
            return Ok(self.statement(&start, StatementKind::While { condition, body }));
        }
        if self.accept("do").is_some() {
            let body = Box::new(self.parse_statement()?);
            self.expect("while")?;
            self.expect("(")?;
            let condition = self.expressions.parse_expression(0)?;
            self.expect(")")?;
            self.expect(";")?;
            return Ok(self.statement(&start, StatementKind::DoWhile { body, condition }));
        }
        if self.accept("for").is_some() {
            self.expect("(")?;
            let initializer = if self.at(";") {
                self.expect(";")?;
                None
            } else if self.starts_declaration() {
                Some(Box::new(self.parse_declaration()?))
            } else {
                let expression = self.expressions.parse_expression(0)?;
                self.expect(";")?;
                Some(Box::new(self.statement(
                    &start,
                    StatementKind::Expression(Some(expression)),
                )))
            };
            let condition = if self.at(";") {
                None
            } else {
                Some(self.expressions.parse_expression(0)?)
            };
            self.expect(";")?;
            let update = if self.at(")") {
                None
            } else {
                Some(self.expressions.parse_expression(0)?)
            };
            self.expect(")")?;
            let body = Box::new(self.parse_statement()?);
            return Ok(self.statement(
                &start,
                StatementKind::For {
                    initializer,
                    condition,
                    update,
                    body,
                },
            ));
        }
        if self.accept("break").is_some() {
            self.expect(";")?;
            return Ok(self.statement(&start, StatementKind::Break));
        }
        if self.accept("continue").is_some() {
            self.expect(";")?;
            return Ok(self.statement(&start, StatementKind::Continue));
        }
        if self.accept("goto").is_some() {
            let name = self.expect_identifier("expected a label name")?;
            self.expect(";")?;
            return Ok(self.statement(&start, StatementKind::Goto(name.value)));
        }
        if self.is_identifier() && self.peek_value(1) == ":" {
            let name = self.expressions.advance();
            self.expressions.advance();
            let statement = Box::new(self.parse_statement()?);
            return Ok(self.statement(
                &name,
                StatementKind::Label {
                    name: name.value.clone(),
                    statement,
                },
            ));
        }
        if self.accept("switch").is_some() {
            self.expect("(")?;
            let expression = self.expressions.parse_expression(0)?;
            self.expect(")")?;
            self.expect("{")?;
            let mut cases = Vec::new();
            let mut has_default = false;
            while !self.at("}") && !self.expressions.is_at_end() {
                let value = if self.accept("case").is_some() {
                    let value = self.expressions.parse_expression(1)?;
                    self.expect(":")?;
                    Some(value)
                } else if self.accept("default").is_some() {
                    if has_default {
                        return Err(self.expressions.fail("duplicate default label"));
                    }
                    has_default = true;
                    self.expect(":")?;
                    None
                } else {
                    return Err(self
                        .expressions
                        .fail("expected case or default label in switch"));
                };
                let mut statements = Vec::new();
                while !matches!(
                    self.expressions.token().value.as_str(),
                    "case" | "default" | "}"
                ) && !self.expressions.is_at_end()
                {
                    statements.push(self.parse_statement()?);
                }
                cases.push((value, statements));
            }
            self.expect("}")?;
            return Ok(self.statement(&start, StatementKind::Switch { expression, cases }));
        }
        if matches!(self.expressions.token().value.as_str(), "case" | "default") {
            return Err(self
                .expressions
                .fail("case and default labels must be inside a switch"));
        }
        let expression = self.expressions.parse_expression(0)?;
        self.expect(";")?;
        Ok(self.statement(&start, StatementKind::Expression(Some(expression))))
    }

    fn starts_declaration(&self) -> bool {
        TYPE_WORDS.contains(&self.expressions.token().value.as_str())
            || STORAGE_WORDS.contains(&self.expressions.token().value.as_str())
    }

    fn is_identifier(&self) -> bool {
        self.expressions
            .token()
            .value
            .chars()
            .next()
            .is_some_and(|character| character.is_alphabetic() || character == '_')
            && !TYPE_WORDS.contains(&self.expressions.token().value.as_str())
            && !STORAGE_WORDS.contains(&self.expressions.token().value.as_str())
    }

    fn peek_value(&self, distance: usize) -> &str {
        self.expressions
            .tokens
            .get(self.expressions.position + distance)
            .map_or("<eof>", |token| token.value.as_str())
    }

    fn expect_identifier(&mut self, message: &str) -> Result<Token, CCompilerError> {
        if self.is_identifier() {
            Ok(self.expressions.advance())
        } else {
            Err(self.expressions.fail(message))
        }
    }

    fn parse_initializer(&mut self) -> Result<Expression, CCompilerError> {
        if let Some(open) = self.accept("{") {
            let mut values = Vec::new();
            if !self.at("}") {
                loop {
                    values.push(self.parse_initializer()?);
                    if self.accept(",").is_none() || self.at("}") {
                        break;
                    }
                }
            }
            self.expect("}")?;
            return Ok(self.expressions.make(
                &open,
                ExpressionKind::InitializerList(values),
            ));
        }
        self.expressions.parse_expression(1)
    }

    fn parse_declaration(&mut self) -> Result<Statement, CCompilerError> {
        let start = self.expressions.token().clone();
        while matches!(
            self.expressions.token().value.as_str(),
            "static" | "extern" | "register" | "auto" | "const" | "volatile" | "restrict"
        ) {
            self.expressions.advance();
        }
        let mut type_words = Vec::new();
        while TYPE_WORDS.contains(&self.expressions.token().value.as_str()) {
            type_words.push(self.expressions.advance().value);
        }
        if !type_words.iter().any(|word| {
            matches!(
                word.as_str(),
                "void" | "char" | "short" | "int" | "long" | "float" | "double" | "_Bool"
            )
        }) {
            return Err(self.expressions.fail("expected a declaration type"));
        }
        let base_type = type_words.join(" ");
        let mut declarations = Vec::new();
        loop {
            let mut c_type = base_type.clone();
            while self.accept("*").is_some() {
                c_type.push('*');
                while matches!(
                    self.expressions.token().value.as_str(),
                    "const" | "volatile" | "restrict"
                ) {
                    self.expressions.advance();
                }
            }
            let name = self.expect_identifier("expected a variable name")?;
            let array_length = self.parse_array_suffix()?;
            let initializer = if self.accept("=").is_some() {
                Some(self.parse_initializer()?)
            } else {
                None
            };
            declarations.push(Declaration {
                c_type,
                name: name.value,
                array_length,
                initializer,
            });
            if self.accept(",").is_none() {
                break;
            }
        }
        self.expect(";")?;
        Ok(self.statement(&start, StatementKind::Declaration(declarations)))
    }

    fn parse_array_suffix(&mut self) -> Result<Option<usize>, CCompilerError> {
        if self.accept("[").is_none() {
            return Ok(None);
        }
        let expression = self.expressions.parse_expression(0)?;
        self.expect("]")?;
        let value = evaluate_integer_constant(&expression)?;
        usize::try_from(value).map(Some).map_err(|_| {
            error(
                expression.filename.as_deref().unwrap_or("<source>"),
                expression.line,
                "array length must be a non-negative integer constant",
            )
        })
    }
}

impl TranslationUnitParser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            statements: StatementParser::new(tokens),
        }
    }

    pub fn from_source(source: &str, filename: &str) -> Result<Self, CCompilerError> {
        Ok(Self::new(lex_raw(source, filename)?))
    }

    pub fn parse(&mut self) -> Result<TranslationUnit, CCompilerError> {
        let mut declarations = Vec::new();
        while !self.statements.expressions.is_at_end() {
            declarations.extend(self.parse_external_declaration()?);
        }
        Ok(TranslationUnit { declarations })
    }

    fn parse_external_declaration(&mut self) -> Result<Vec<ExternalDeclaration>, CCompilerError> {
        let token = self.statements.expressions.token().clone();
        let (base_type, storage_class) = self.parse_base_type()?;
        if self.statements.accept(";").is_some() {
            return Ok(Vec::new());
        }
        let (declarator_type, name) = self.parse_declarator(&base_type)?;
        if self.statements.accept("(").is_some() {
            let (parameters, variadic) = self.parse_parameters()?;
            let declaration = FunctionDeclaration {
                return_type: declarator_type,
                name: name.value,
                parameters,
                variadic,
            };
            if self.statements.accept(";").is_some() {
                return Ok(vec![ExternalDeclaration::FunctionPrototype(declaration)]);
            }
            if storage_class.as_deref() == Some("typedef") {
                return Err(self
                    .statements
                    .expressions
                    .fail("a function definition cannot use typedef storage class"));
            }
            let body = self.statements.parse_statement()?;
            if !matches!(body.kind, StatementKind::Compound(_)) {
                return Err(error(
                    token.filename.as_deref().unwrap_or("<source>"),
                    token.line,
                    "expected a function body",
                ));
            }
            return Ok(vec![ExternalDeclaration::FunctionDefinition(
                FunctionDefinition { declaration, body },
            )]);
        }

        let mut globals = Vec::new();
        let mut next = Some((declarator_type, name));
        while let Some((declarator_type, name)) = next.take() {
            let array_length = self.parse_array_suffix()?;
            let initializer = if self.statements.accept("=").is_some() {
                Some(self.statements.parse_initializer()?)
            } else {
                None
            };
            globals.push(ExternalDeclaration::Global(Declaration {
                c_type: declarator_type.clone(),
                name: name.value.clone(),
                array_length,
                initializer,
            }));
            next = if self.statements.accept(",").is_some() {
                Some(self.parse_declarator(&base_type)?)
            } else {
                None
            };
        }
        self.statements.expect(";")?;
        Ok(globals)
    }

    fn parse_base_type(&mut self) -> Result<(String, Option<String>), CCompilerError> {
        let mut storage_class = None;
        let mut qualifiers = Vec::new();
        while matches!(
            self.statements.expressions.token().value.as_str(),
            "static" | "extern" | "register" | "auto" | "typedef" | "const" | "volatile"
        ) {
            let value = self.statements.expressions.advance().value;
            if matches!(value.as_str(), "const" | "volatile") {
                qualifiers.push(value);
            } else if storage_class.replace(value).is_some() {
                return Err(self
                    .statements
                    .expressions
                    .fail("multiple storage-class specifiers in declaration"));
            }
        }
        let mut words = Vec::new();
        while TYPE_WORDS.contains(&self.statements.expressions.token().value.as_str()) {
            words.push(self.statements.expressions.advance().value);
        }
        if !words.iter().any(|word| {
            matches!(
                word.as_str(),
                "void" | "char" | "short" | "int" | "long" | "float" | "double" | "_Bool"
            )
        }) {
            return Err(self
                .statements
                .expressions
                .fail("expected a supported declaration type"));
        }
        let mut c_type = words.join(" ");
        if !qualifiers.is_empty() {
            c_type = format!("{} {c_type}", qualifiers.join(" "));
        }
        Ok((c_type, storage_class))
    }

    fn parse_declarator(&mut self, base_type: &str) -> Result<(String, Token), CCompilerError> {
        let mut c_type = base_type.to_owned();
        while self.statements.accept("*").is_some() {
            c_type.push('*');
            while matches!(
                self.statements.expressions.token().value.as_str(),
                "const" | "volatile"
            ) {
                self.statements.expressions.advance();
            }
        }
        let name = self
            .statements
            .expect_identifier("expected an external identifier")?;
        Ok((c_type, name))
    }

    fn parse_array_suffix(&mut self) -> Result<Option<usize>, CCompilerError> {
        if self.statements.accept("[").is_none() {
            return Ok(None);
        }
        let expression = self.statements.expressions.parse_expression(0)?;
        self.statements.expect("]")?;
        let length = evaluate_integer_constant(&expression)?;
        usize::try_from(length).map(Some).map_err(|_| {
            error(
                expression.filename.as_deref().unwrap_or("<source>"),
                expression.line,
                "array length must be a non-negative integer constant",
            )
        })
    }

    fn parse_parameters(&mut self) -> Result<(Vec<FunctionParameter>, bool), CCompilerError> {
        let mut parameters = Vec::new();
        let mut variadic = false;
        if self.statements.accept(")").is_some() {
            return Ok((parameters, variadic));
        }
        if self.statements.at("void") && self.statements.peek_value(1) == ")" {
            self.statements.expressions.advance();
            self.statements.expect(")")?;
            return Ok((parameters, variadic));
        }
        loop {
            if self.statements.at(".") && self.statements.peek_value(1) == "." {
                self.statements.expressions.advance();
                self.statements.expressions.advance();
                self.statements.expect(".")?;
                variadic = true;
                break;
            }
            let (base_type, storage) = self.parse_base_type()?;
            if storage.is_some() {
                return Err(self
                    .statements
                    .expressions
                    .fail("storage class is not valid for a function parameter"));
            }
            let (c_type, name) = if self.statements.at(",") || self.statements.at(")") {
                (base_type, None)
            } else {
                let (c_type, name) = self.parse_declarator(&base_type)?;
                (c_type, Some(name.value))
            };
            parameters.push(FunctionParameter { c_type, name });
            if self.statements.accept(",").is_none() {
                break;
            }
        }
        self.statements.expect(")")?;
        Ok((parameters, variadic))
    }
}

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

    fn expression(source: &str) -> Expression {
        let mut parser = ExpressionParser::from_source(source, "expression.c").unwrap();
        let expression = parser.parse().unwrap();
        assert!(parser.is_at_end(), "unparsed token: {:?}", parser.token());
        expression
    }

    fn atom(name: &str, line: usize) -> Expression {
        located_atom(name, line, "expression.c")
    }

    fn located_atom(name: &str, line: usize, filename: &str) -> Expression {
        Expression {
            kind: ExpressionKind::Atom(name.to_owned()),
            line,
            filename: Some(filename.to_owned()),
        }
    }

    #[test]
    fn parses_binary_precedence_and_left_associativity() {
        assert_eq!(
            expression("a + b * c - d"),
            Expression {
                kind: ExpressionKind::Binary {
                    operator: "-".to_owned(),
                    left: Box::new(Expression {
                        kind: ExpressionKind::Binary {
                            operator: "+".to_owned(),
                            left: Box::new(atom("a", 1)),
                            right: Box::new(Expression {
                                kind: ExpressionKind::Binary {
                                    operator: "*".to_owned(),
                                    left: Box::new(atom("b", 1)),
                                    right: Box::new(atom("c", 1)),
                                },
                                line: 1,
                                filename: Some("expression.c".to_owned()),
                            }),
                        },
                        line: 1,
                        filename: Some("expression.c".to_owned()),
                    }),
                    right: Box::new(atom("d", 1)),
                },
                line: 1,
                filename: Some("expression.c".to_owned()),
            }
        );
    }

    #[test]
    fn parses_right_associative_assignment_and_conditional_expressions() {
        let assignment = expression("a = b += c");
        assert!(matches!(
            assignment.kind,
            ExpressionKind::Assignment {
                operator,
                value,
                ..
            } if operator == "=" && matches!(
                value.kind,
                ExpressionKind::Assignment { ref operator, .. } if operator == "+="
            )
        ));
        let conditional = expression("a ? b : c ? d : e");
        assert!(matches!(
            conditional.kind,
            ExpressionKind::Conditional { when_false, .. }
                if matches!(when_false.kind, ExpressionKind::Conditional { .. })
        ));
    }

    #[test]
    fn parses_calls_indexes_members_and_postfix_operators() {
        let parsed = expression("object->items[index + 1]++");
        assert!(matches!(
            parsed.kind,
            ExpressionKind::Postfix {
                operator,
                expression,
            } if operator == "++" && matches!(
                expression.kind,
                ExpressionKind::Index { ref expression, .. }
                    if matches!(expression.kind, ExpressionKind::Member { ref operator, .. } if operator == "->")
            )
        ));
        let call = expression("f(a, g(b + 1))");
        assert!(matches!(
            call.kind,
            ExpressionKind::Call { arguments, .. } if arguments.len() == 2
        ));
    }

    #[test]
    fn parses_simple_casts_and_sizeof_type_names() {
        assert!(matches!(
            expression("(unsigned long *)pointer").kind,
            ExpressionKind::Cast { c_type, .. } if c_type == "unsigned long*"
        ));
        assert!(matches!(
            expression("sizeof(unsigned char*)").kind,
            ExpressionKind::SizeofType(c_type) if c_type == "unsigned char*"
        ));
    }

    #[test]
    fn rejects_invalid_assignment_targets_and_indirect_calls() {
        assert_eq!(
            ExpressionParser::from_source("1 = value", "bad.c")
                .unwrap()
                .parse()
                .unwrap_err()
                .to_string(),
            "bad.c:1: assignment requires a scalar variable"
        );
        assert_eq!(
            ExpressionParser::from_source("(a + b)(c)", "bad.c")
                .unwrap()
                .parse()
                .unwrap_err()
                .to_string(),
            "bad.c:1: only direct function calls are supported"
        );
    }

    fn statement(source: &str) -> Statement {
        StatementParser::from_source(source, "statement.c")
            .unwrap()
            .parse()
            .unwrap()
    }

    #[test]
    fn parses_compound_block_with_local_declarations_and_returns() {
        assert_eq!(
            statement("{ unsigned int count = 3, *pointer; return count; }"),
            Statement {
                kind: StatementKind::Compound(vec![
                    Statement {
                        kind: StatementKind::Declaration(vec![
                            Declaration {
                                c_type: "unsigned int".to_owned(),
                                name: "count".to_owned(),
                                array_length: None,
                                initializer: Some(located_atom("3", 1, "statement.c")),
                            },
                            Declaration {
                                c_type: "unsigned int*".to_owned(),
                                name: "pointer".to_owned(),
                                array_length: None,
                                initializer: None,
                            },
                        ]),
                        line: 1,
                        filename: Some("statement.c".to_owned()),
                    },
                    Statement {
                        kind: StatementKind::Return(Some(located_atom("count", 1, "statement.c",))),
                        line: 1,
                        filename: Some("statement.c".to_owned()),
                    },
                ]),
                line: 1,
                filename: Some("statement.c".to_owned()),
            }
        );
    }

    #[test]
    fn parses_if_else_and_nested_loop_control() {
        let parsed = statement("if (ready) { while (active) { break; } } else { continue; }");
        assert!(matches!(
            parsed.kind,
            StatementKind::If {
                then_branch,
                else_branch: Some(_),
                ..
            } if matches!(
                then_branch.kind,
                StatementKind::Compound(ref statements)
                    if matches!(
                        statements[0].kind,
                        StatementKind::While { ref body, .. }
                            if matches!(
                                body.kind,
                                StatementKind::Compound(ref nested)
                                    if matches!(nested[0].kind, StatementKind::Break)
                            )
                    )
            )
        ));
    }

    #[test]
    fn parses_switch_cases_and_reports_duplicate_default() {
        let parsed = statement("switch (value) { case 1: value++; default: break; }");
        assert!(matches!(parsed.kind, StatementKind::Switch { .. }));
        let error =
            StatementParser::from_source("switch (value) { default: ; default: ; }", "switch.c")
                .unwrap()
                .parse()
                .unwrap_err();
        assert_eq!(error.to_string(), "switch.c:1: duplicate default label");
    }

    #[test]
    fn parses_for_declaration_do_while_and_labels() {
        let parsed = statement("for (int i = 0; i < 4; i++) { work(i); }");
        assert!(matches!(
            parsed.kind,
            StatementKind::For {
                initializer: Some(initializer),
                condition: Some(_),
                update: Some(_),
                body,
            } if matches!(initializer.kind, StatementKind::Declaration(_))
                && matches!(body.kind, StatementKind::Compound(_))
        ));

        assert!(matches!(
            statement("again: do { tick(); } while (pending);").kind,
            StatementKind::Label { name, statement }
                if name == "again" && matches!(statement.kind, StatementKind::DoWhile { .. })
        ));
    }

    #[test]
    fn reports_malformed_control_statements_and_unterminated_blocks() {
        assert_eq!(
            StatementParser::from_source("if ready return 1;", "bad.c")
                .unwrap()
                .parse()
                .unwrap_err()
                .to_string(),
            "bad.c:1: expected \"(\", got \"ready\""
        );
        assert_eq!(
            StatementParser::from_source("{ return 0;", "bad.c")
                .unwrap()
                .parse()
                .unwrap_err()
                .to_string(),
            "bad.c:1: unterminated compound statement"
        );
    }

    fn translation_unit(source: &str) -> TranslationUnit {
        TranslationUnitParser::from_source(source, "unit.c")
            .unwrap()
            .parse()
            .unwrap()
    }

    #[test]
    fn parses_global_declarations_and_function_prototypes() {
        assert_eq!(
            translation_unit("unsigned int counter = 3; char *message; int sum(int, int);"),
            TranslationUnit {
                declarations: vec![
                    ExternalDeclaration::Global(Declaration {
                        c_type: "unsigned int".to_owned(),
                        name: "counter".to_owned(),
                        array_length: None,
                        initializer: Some(located_atom("3", 1, "unit.c")),
                    }),
                    ExternalDeclaration::Global(Declaration {
                        c_type: "char*".to_owned(),
                        name: "message".to_owned(),
                        array_length: None,
                        initializer: None,
                    }),
                    ExternalDeclaration::FunctionPrototype(FunctionDeclaration {
                        return_type: "int".to_owned(),
                        name: "sum".to_owned(),
                        parameters: vec![
                            FunctionParameter {
                                c_type: "int".to_owned(),
                                name: None,
                            },
                            FunctionParameter {
                                c_type: "int".to_owned(),
                                name: None,
                            },
                        ],
                        variadic: false,
                    }),
                ],
            }
        );
    }

    #[test]
    fn parses_function_definitions_and_variadic_signatures() {
        let unit = translation_unit(
            "int log_value(const char *format, ...); \
             int add(int left, int right) { return left + right; }",
        );
        assert!(matches!(
            &unit.declarations[0],
            ExternalDeclaration::FunctionPrototype(function)
                if function.name == "log_value"
                    && function.variadic
                    && function.parameters.len() == 1
                    && function.parameters[0].c_type == "const char*"
                    && function.parameters[0].name.as_deref() == Some("format")
        ));
        assert!(matches!(
            &unit.declarations[1],
            ExternalDeclaration::FunctionDefinition(definition)
                if definition.declaration.name == "add"
                    && definition.declaration.parameters.len() == 2
                    && matches!(definition.body.kind, StatementKind::Compound(_))
        ));
    }

    #[test]
    fn parses_multiple_external_declarators_and_arrays() {
        let unit = translation_unit(
            "static int first, second[12]; long *pointer, value; int main(void) { return 0; }",
        );
        assert!(matches!(
            &unit.declarations[..3],
            [
                ExternalDeclaration::Global(Declaration { name: first, .. }),
                ExternalDeclaration::Global(Declaration { name: second, array_length: Some(12), .. }),
                ExternalDeclaration::Global(Declaration { name: pointer, c_type, .. }),
            ] if first == "first" && second == "second" && pointer == "pointer" && c_type == "long*"
        ));
        assert!(matches!(
            unit.declarations.last(),
            Some(ExternalDeclaration::FunctionDefinition(definition))
                if definition.declaration.name == "main"
        ));
    }

    #[test]
    fn reports_unsupported_function_pointer_declarators_without_accepting_them() {
        let error = TranslationUnitParser::from_source("int (*callback)(int);", "bad.c")
            .unwrap()
            .parse()
            .unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("bad.c:1: expected an external identifier")
        );
    }

    #[test]
    fn evaluates_integer_constant_expressions_for_array_bounds() {
        let unit = translation_unit(
            "int table[2 + 3 * 4]; unsigned char bytes[0x10U]; \
             int main(void) { int local[(1 << 3) - 2]; return 0; }",
        );
        assert!(matches!(
            &unit.declarations[..2],
            [
                ExternalDeclaration::Global(Declaration {
                    name: first,
                    array_length: Some(14),
                    ..
                }),
                ExternalDeclaration::Global(Declaration {
                    name: second,
                    array_length: Some(16),
                    ..
                }),
            ] if first == "table" && second == "bytes"
        ));
        let ExternalDeclaration::FunctionDefinition(main) = &unit.declarations[2] else {
            panic!("expected main definition");
        };
        let StatementKind::Compound(statements) = &main.body.kind else {
            panic!("expected main body");
        };
        assert!(matches!(
            &statements[0].kind,
            StatementKind::Declaration(declarations)
                if declarations[0].array_length == Some(6)
        ));
    }

    #[test]
    fn rejects_nonconstant_or_invalid_array_bounds() {
        for (source, expected) in [
            (
                "int values[size];",
                "bounds.c:1: expected an integer constant expression",
            ),
            (
                "int values[4 / 0];",
                "bounds.c:1: expected an integer constant expression",
            ),
            (
                "int values[-1];",
                "bounds.c:1: array length must be a non-negative integer constant",
            ),
            (
                "int values[09];",
                "bounds.c:1: expected an integer constant expression",
            ),
        ] {
            let error = TranslationUnitParser::from_source(source, "bounds.c")
                .unwrap()
                .parse()
                .unwrap_err();
            assert_eq!(error.to_string(), expected, "source: {source}");
        }
    }

    #[test]
    fn compiles_and_executes_minimal_integer_main_in_rust() {
        let image = compile_minimal_program("int main(void) { return 2 + 3 * 4; }", "minimal.c")
            .unwrap()
            .expect("supported minimal program");
        assert_eq!(image.entry_point, 0);
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(32).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 14);
    }

    #[test]
    fn leaves_nontrivial_c_programs_for_the_full_compiler() {
        assert!(
            compile_minimal_program(
                "int main(void) { return helper(); }",
                "needs-full-compiler.c"
            )
            .unwrap()
            .is_none()
        );
        assert!(
            compile_minimal_program(
                "int helper(int value) { return value; } int main(void) { return helper(); }",
                "needs-full-compiler.c",
            )
            .unwrap()
            .is_none()
        );
        assert!(
            compile_minimal_program(
                "int main(void) { int values[2] = 1; return values[0]; }",
                "needs-full-compiler.c",
            )
            .unwrap()
            .is_none()
        );
        assert!(
            compile_minimal_program("int main(void) { return 70000; }", "needs-full-compiler.c",)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn compiles_local_integers_and_runtime_expressions() {
        let image = compile_minimal_program(
            "int main(void) { int left = 7; int right = 5; return left + right * 2; }",
            "locals.c",
        )
        .unwrap()
        .expect("supported local-integer program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(32).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 17);
    }

    #[test]
    fn compiles_initialized_and_zero_initialized_global_integers() {
        let image = compile_minimal_program(
            "int total = 3 * 2; int cleared; int main(void) { total += 4; cleared = total; return cleared; }",
            "globals.c",
        )
        .unwrap()
        .expect("supported global-integer program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(128).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 10);
        assert_eq!(cpu.read_u16(0x80000), 10);
        assert_eq!(cpu.read_u16(0x80002), 10);
    }

    #[test]
    fn local_integer_can_shadow_a_global() {
        let image = compile_minimal_program(
            "int value = 10; int main(void) { int value = 3; return value; }",
            "shadow.c",
        )
        .unwrap()
        .expect("supported shadowing program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(64).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 3);
    }

    #[test]
    fn compiles_indexed_global_integer_arrays() {
        let image = compile_minimal_program(
            "int values[4]; int main(void) { values[1] = 5; values[2] = values[1] + 2; values[1]++; values[2] += values[1]; return values[2]; }",
            "global-array.c",
        )
        .unwrap()
        .expect("supported global-array program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 13);
        assert_eq!(cpu.read_u16(0x80002), 6);
        assert_eq!(cpu.read_u16(0x80004), 13);
    }

    #[test]
    fn compiles_switch_dispatch_fallthrough_default_and_break() {
        for (source, expected) in [
            (
                "int main(void) { int value = 2; int result = 0; switch (value) { case 1: result = 10; break; case 2: result = 20; case 3: result += 3; break; default: result = 99; } return result; }",
                23,
            ),
            (
                "int main(void) { int value = 9; switch (value) { case 1: return 10; default: return 42; } return 0; }",
                42,
            ),
            (
                "int main(void) { int value = 9; switch (value) { case 1: value = 10; } return value; }",
                9,
            ),
        ] {
            let image = compile_minimal_program(source, "switch.c")
                .unwrap()
                .expect("supported switch program");
            let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
            cpu.load_program(&image.segments, image.entry_point)
                .unwrap();
            cpu.run(256).unwrap();
            assert!(!cpu.running);
            assert_eq!(cpu.registers[0], expected, "source: {source}");
        }
    }

    #[test]
    fn compiles_initialized_locals_with_unary_and_bitwise_operations() {
        let image = compile_minimal_program(
            "int main(void) { int value = 0x0f; return ~(value ^ 0x03) & 0xff; }",
            "bitwise.c",
        )
        .unwrap()
        .expect("supported bitwise program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(32).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 0xf3);
    }

    #[test]
    fn compiles_local_assignments_and_signed_division() {
        let image = compile_minimal_program(
            "int main(void) { int value = 20; value -= 2; value /= 3; return value * 10 + 20 % 3; }",
            "assignment.c",
        )
        .unwrap()
        .expect("supported assignment program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(64).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 62);
    }

    #[test]
    fn compiles_signed_integer_comparisons() {
        for (source, expected) in [
            ("int main(void) { return 3 == 3; }", 1),
            ("int main(void) { return 3 != 3; }", 0),
            ("int main(void) { return -3 < 2; }", 1),
            ("int main(void) { return 2 <= -3; }", 0),
            ("int main(void) { return 4 > 3; }", 1),
            ("int main(void) { return 4 >= 5; }", 0),
        ] {
            let image = compile_minimal_program(source, "comparison.c")
                .unwrap()
                .expect("supported comparison program");
            let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
            cpu.load_program(&image.segments, image.entry_point)
                .unwrap();
            cpu.run(64).unwrap();
            assert!(!cpu.running);
            assert_eq!(cpu.registers[0], expected, "source: {source}");
        }
    }

    #[test]
    fn compiles_if_else_return_paths() {
        let image = compile_minimal_program(
            "int main(void) { int value = -2; if (value < 0) return 11; else return 22; }",
            "if-else.c",
        )
        .unwrap()
        .expect("supported if/else program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(64).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 11);
    }

    #[test]
    fn compiles_while_loops_with_local_updates() {
        let image = compile_minimal_program(
            "int main(void) { int value = 1; while (value < 6) { value += 1; } return value; }",
            "while.c",
        )
        .unwrap()
        .expect("supported while-loop program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 6);
    }

    #[test]
    fn compiles_for_loops_with_scoped_integer_counters() {
        let image = compile_minimal_program(
            "int main(void) { int sum = 0; for (int i = 0; i < 5; i++) { sum += i; } return sum; }",
            "for.c",
        )
        .unwrap()
        .expect("supported for-loop program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(512).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 10);
    }

    #[test]
    fn compiles_shift_logical_and_conditional_expressions() {
        for (source, expected) in [
            ("int main(void) { return (3 << 2) | (16 >> 2); }", 12),
            ("int main(void) { return !0 + !5; }", 1),
            ("int main(void) { return 0 && (1 / 0); }", 0),
            ("int main(void) { return 1 || (1 / 0); }", 1),
            ("int main(void) { return 0 ? 12 : 7; }", 7),
            ("int main(void) { return 4 ? 12 : 7; }", 12),
        ] {
            let image = compile_minimal_program(source, "expressions.c")
                .unwrap()
                .expect("supported expression program");
            let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
            cpu.load_program(&image.segments, image.entry_point)
                .unwrap();
            cpu.run(128).unwrap();
            assert!(!cpu.running);
            assert_eq!(cpu.registers[0], expected, "source: {source}");
        }
    }

    #[test]
    fn compiles_do_while_and_increment_statements() {
        let image = compile_minimal_program(
            "int main(void) { int count = 0; do { ++count; } while (count < 4); return count; }",
            "do-while.c",
        )
        .unwrap()
        .expect("supported do/while program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 4);
    }

    #[test]
    fn compiles_local_declarations_in_nested_blocks() {
        let image = compile_minimal_program(
            "int main(void) { int sum = 0; { int value = 6; sum = value + 4; } return sum; }",
            "block-locals.c",
        )
        .unwrap()
        .expect("supported nested block locals");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(128).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 10);
    }

    #[test]
    fn compiles_shift_assignment_and_signed_right_shift() {
        let image = compile_minimal_program(
            "int main(void) { int value = -16; value >>= 2; return value; }",
            "shift.c",
        )
        .unwrap()
        .expect("supported shift assignment program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(64).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0] as u16, 0xfffc);
    }

    #[test]
    fn compiles_break_and_continue_in_for_loops() {
        let image = compile_minimal_program(
            "int main(void) { int sum = 0; for (int i = 0; i < 10; i++) { if (i == 3) continue; if (i == 7) break; sum += i; } return sum; }",
            "loop-control.c",
        )
        .unwrap()
        .expect("supported loop-control program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(512).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 18, "registers: {:?}", cpu.registers);
    }

    #[test]
    fn compiles_multiple_integer_functions_and_recursive_calls() {
        let source = "int factorial(int n) { if (n <= 1) return 1; return n * factorial(n - 1); } \
                      int add(int left, int right) { return left + right; } \
                      int main(void) { return add(factorial(5), 2); }";
        let image = compile_minimal_program(source, "functions.c")
            .unwrap()
            .expect("supported recursive multi-function program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(10_000).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 122);
    }
}
