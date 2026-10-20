use crate::image::ProgramImage;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

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
    pub array_dimensions: Vec<usize>,
    pub initializer: Option<Expression>,
}

const INCOMPLETE_ARRAY_LENGTH: usize = usize::MAX;

fn infer_incomplete_array_length(c_type: &str, initializer: &Expression) -> Option<usize> {
    match &initializer.kind {
        ExpressionKind::InitializerList(values) => Some(values.len()),
        ExpressionKind::Atom(value)
            if matches!(c_type.trim(), "char" | "signed char" | "unsigned char") =>
        {
            decode_string_token(value)?.len().checked_add(1)
        }
        _ => None,
    }
}

fn flatten_initializer_values(expression: &Expression) -> Option<Vec<&Expression>> {
    match &expression.kind {
        ExpressionKind::InitializerList(values) => {
            values.iter().try_fold(Vec::new(), |mut flattened, value| {
                flattened.extend(flatten_initializer_values(value)?);
                Some(flattened)
            })
        }
        _ => Some(vec![scalar_initializer(expression)?]),
    }
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
struct MacroDefinition {
    parameters: Option<Vec<String>>,
    replacement: Vec<Token>,
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
    pub enumerators: HashMap<String, i64>,
}

#[derive(Clone)]
enum NativeLocation {
    Register(u8),
    Constant(i64),
    GlobalScalar,
    GlobalArray {
        length: usize,
        dimensions: Vec<usize>,
        element_size: usize,
        element_signed: bool,
    },
    StackArray {
        offset: usize,
        length: usize,
        dimensions: Vec<usize>,
        element_size: usize,
        element_signed: bool,
    },
}

struct NativeGlobal {
    name: String,
    data: Vec<u8>,
    is_array: bool,
    array_dimensions: Vec<usize>,
    element_size: usize,
    element_signed: bool,
}

const NATIVE_ASSEMBLY_LIBRARIES: [(&str, &str); 2] = [
    ("lib_cgfx.asm", include_str!("../../lib/lib_cgfx.asm")),
    ("lib_text.asm", include_str!("../../lib/lib_text.asm")),
];

fn native_library_for_symbol(symbol: &str) -> Option<&'static str> {
    NATIVE_ASSEMBLY_LIBRARIES
        .iter()
        .find(|(_, source)| {
            source.lines().any(|line| {
                line.trim()
                    .strip_suffix(':')
                    .is_some_and(|label| label == symbol)
            })
        })
        .map(|(name, _)| *name)
}

fn native_argument_is_pointer(function: &str, index: usize) -> bool {
    function == "gfx_text" && index == 2
}

fn is_native_word_type(c_type: &str) -> bool {
    matches!(c_type.trim(), "int" | "unsigned int")
}

pub struct ExpressionParser {
    tokens: Vec<Token>,
    position: usize,
    typedefs: HashMap<String, String>,
}

fn evaluate_integer_constant_with_names(
    expression: &Expression,
    names: &HashMap<String, i64>,
) -> Result<i64, CCompilerError> {
    let fail = || {
        error(
            expression.filename.as_deref().unwrap_or("<source>"),
            expression.line,
            "expected an integer constant expression",
        )
    };
    match &expression.kind {
        ExpressionKind::Atom(value) => parse_integer_constant(value)
            .or_else(|| names.get(value).copied())
            .ok_or_else(fail),
        ExpressionKind::Unary { operator, operand } => {
            let value = evaluate_integer_constant_with_names(operand, names)?;
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
            let left = evaluate_integer_constant_with_names(left, names)?;
            if operator == "&&" && left == 0 {
                return Ok(0);
            }
            if operator == "||" && left != 0 {
                return Ok(1);
            }
            let right = evaluate_integer_constant_with_names(right, names)?;
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
            if evaluate_integer_constant_with_names(condition, names)? != 0 {
                evaluate_integer_constant_with_names(when_true, names)
            } else {
                evaluate_integer_constant_with_names(when_false, names)
            }
        }
        ExpressionKind::InitializerList(_) => Err(fail()),
        _ => Err(fail()),
    }
}

fn scalar_initializer(expression: &Expression) -> Option<&Expression> {
    match &expression.kind {
        ExpressionKind::InitializerList(elements) if elements.len() == 1 => {
            scalar_initializer(&elements[0])
        }
        ExpressionKind::InitializerList(_) => None,
        _ => Some(expression),
    }
}

fn collect_string_literals_expression(expression: &Expression, literals: &mut HashSet<String>) {
    match &expression.kind {
        ExpressionKind::Atom(value) if decode_string_token(value).is_some() => {
            literals.insert(value.clone());
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
            collect_string_literals_expression(operand, literals);
        }
        ExpressionKind::Binary { left, right, .. }
        | ExpressionKind::Assignment {
            target: left,
            value: right,
            ..
        } => {
            collect_string_literals_expression(left, literals);
            collect_string_literals_expression(right, literals);
        }
        ExpressionKind::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            collect_string_literals_expression(condition, literals);
            collect_string_literals_expression(when_true, literals);
            collect_string_literals_expression(when_false, literals);
        }
        ExpressionKind::Call {
            function,
            arguments,
        } => {
            collect_string_literals_expression(function, literals);
            for argument in arguments {
                collect_string_literals_expression(argument, literals);
            }
        }
        ExpressionKind::Index { expression, index } => {
            collect_string_literals_expression(expression, literals);
            collect_string_literals_expression(index, literals);
        }
        ExpressionKind::Member { expression, .. } => {
            collect_string_literals_expression(expression, literals);
        }
        ExpressionKind::InitializerList(values) => {
            for value in values {
                collect_string_literals_expression(value, literals);
            }
        }
        _ => {}
    }
}

fn collect_string_literals_statement(statement: &Statement, literals: &mut HashSet<String>) {
    match &statement.kind {
        StatementKind::Compound(statements) => {
            for statement in statements {
                collect_string_literals_statement(statement, literals);
            }
        }
        StatementKind::Declaration(declarations) => {
            for declaration in declarations {
                if let Some(initializer) = &declaration.initializer {
                    collect_string_literals_expression(initializer, literals);
                }
            }
        }
        StatementKind::Expression(Some(expression)) | StatementKind::Return(Some(expression)) => {
            collect_string_literals_expression(expression, literals);
        }
        StatementKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_string_literals_expression(condition, literals);
            collect_string_literals_statement(then_branch, literals);
            if let Some(else_branch) = else_branch {
                collect_string_literals_statement(else_branch, literals);
            }
        }
        StatementKind::While { condition, body } | StatementKind::DoWhile { body, condition } => {
            collect_string_literals_expression(condition, literals);
            collect_string_literals_statement(body, literals);
        }
        StatementKind::For {
            initializer,
            condition,
            update,
            body,
        } => {
            if let Some(initializer) = initializer {
                collect_string_literals_statement(initializer, literals);
            }
            if let Some(condition) = condition {
                collect_string_literals_expression(condition, literals);
            }
            if let Some(update) = update {
                collect_string_literals_expression(update, literals);
            }
            collect_string_literals_statement(body, literals);
        }
        StatementKind::Switch { expression, cases } => {
            collect_string_literals_expression(expression, literals);
            for (value, statements) in cases {
                if let Some(value) = value {
                    collect_string_literals_expression(value, literals);
                }
                for statement in statements {
                    collect_string_literals_statement(statement, literals);
                }
            }
        }
        StatementKind::Label { statement, .. } => {
            collect_string_literals_statement(statement, literals);
        }
        StatementKind::Expression(None)
        | StatementKind::Return(None)
        | StatementKind::Break
        | StatementKind::Continue
        | StatementKind::Goto(_) => {}
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
    let mut parser = TranslationUnitParser::from_source(source, filename)?;
    let unit = parser.parse()?;
    let mut functions = Vec::new();
    let mut globals: Vec<NativeGlobal> = Vec::new();
    let mut static_data_size = 0usize;
    let mut names = HashSet::new();
    let mut global_names = HashSet::new();
    let mut prototypes = HashMap::new();
    for declaration in &unit.declarations {
        match declaration {
            ExternalDeclaration::FunctionDefinition(function) => {
                if !matches!(
                    function.declaration.return_type.trim(),
                    "int" | "unsigned int" | "void"
                ) || function.declaration.variadic
                    || function.declaration.parameters.len() > 3
                    || global_names.contains(function.declaration.name.as_str())
                    || function
                        .declaration
                        .parameters
                        .iter()
                        .any(|parameter| !is_native_word_type(&parameter.c_type))
                    || !names.insert(function.declaration.name.as_str())
                {
                    return Ok(None);
                }
                functions.push(function);
            }
            ExternalDeclaration::FunctionPrototype(function) => {
                let is_library_function = native_library_for_symbol(&function.name).is_some();
                if (!matches!(function.return_type.trim(), "int" | "void" | "unsigned int")
                    && !is_library_function)
                    || function.variadic
                    || global_names.contains(function.name.as_str())
                    || function.parameters.len() > if is_library_function { 8 } else { 3 }
                    || (!is_library_function
                        && function
                            .parameters
                            .iter()
                            .any(|parameter| !is_native_word_type(&parameter.c_type)))
                {
                    return Ok(None);
                }
                if prototypes
                    .insert(
                        function.name.as_str(),
                        (
                            function.parameters.len(),
                            function.return_type.trim() == "void",
                        ),
                    )
                    .is_some_and(|previous| {
                        previous
                            != (
                                function.parameters.len(),
                                function.return_type.trim() == "void",
                            )
                    })
                {
                    return Ok(None);
                }
            }
            ExternalDeclaration::Global(global) => {
                let (element_size, element_signed) = match global.c_type.trim() {
                    "int" | "unsigned int" => (2, false),
                    "unsigned char" => (1, false),
                    "char" | "signed char" => (1, true),
                    _ => return Ok(None),
                };
                if (element_size == 1 && global.array_length.is_none())
                    || prototypes.contains_key(global.name.as_str())
                    || !global_names.insert(global.name.as_str())
                    || !names.insert(global.name.as_str())
                {
                    return Ok(None);
                }
                let is_array = global.array_length.is_some();
                let element_count = global.array_length.unwrap_or(1);
                let Some(byte_count) = element_count.checked_mul(element_size) else {
                    return Ok(None);
                };
                let Some(total_data_size) = static_data_size.checked_add(byte_count) else {
                    return Ok(None);
                };
                if element_count == 0 || total_data_size > 0x1000000 - 0x80000 {
                    return Ok(None);
                }
                static_data_size = total_data_size;
                let mut data = vec![0; byte_count];
                if let Some(initializer) = &global.initializer {
                    if let ExpressionKind::Atom(value) = &initializer.kind
                        && let Some(bytes) = decode_string_token(value)
                    {
                        if !is_array
                            || global.array_dimensions.len() != 1
                            || element_size != 1
                            || bytes.len() > element_count
                        {
                            return Ok(None);
                        }
                        data[..bytes.len()].copy_from_slice(&bytes);
                    } else {
                        let values = if is_array {
                            if !matches!(&initializer.kind, ExpressionKind::InitializerList(_)) {
                                return Ok(None);
                            }
                            let Some(values) = flatten_initializer_values(initializer) else {
                                return Ok(None);
                            };
                            if values.len() > element_count {
                                return Ok(None);
                            }
                            Some(values)
                        } else {
                            scalar_initializer(initializer).map(|value| vec![value])
                        };
                        let Some(values) = values else {
                            return Ok(None);
                        };
                        for (index, expression) in values.into_iter().enumerate() {
                            let Ok(value) =
                                evaluate_integer_constant_with_names(expression, &unit.enumerators)
                            else {
                                return Ok(None);
                            };
                            if element_size == 1 {
                                data[index] = value as u8;
                            } else {
                                let value = value as u16;
                                data[index * 2] = (value >> 8) as u8;
                                data[index * 2 + 1] = value as u8;
                            }
                        }
                    }
                }
                globals.push(NativeGlobal {
                    name: global.name.clone(),
                    data,
                    is_array,
                    array_dimensions: global.array_dimensions.clone(),
                    element_size,
                    element_signed,
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
    if unit.enumerators.keys().any(|enumerator| {
        names.contains(enumerator.as_str()) || prototypes.contains_key(enumerator.as_str())
    }) {
        return Ok(None);
    }
    if !main.declaration.parameters.is_empty() || main.declaration.return_type.trim() != "int" {
        return Ok(None);
    }
    if functions.iter().any(|function| {
        prototypes
            .get(function.declaration.name.as_str())
            .is_some_and(|signature| {
                *signature
                    != (
                        function.declaration.parameters.len(),
                        function.declaration.return_type.trim() == "void",
                    )
            })
    }) {
        return Ok(None);
    }
    let mut defined_functions: HashMap<String, (usize, bool)> = functions
        .iter()
        .map(|function| {
            (
                function.declaration.name.clone(),
                (
                    function.declaration.parameters.len(),
                    function.declaration.return_type.trim() == "void",
                ),
            )
        })
        .collect();
    for (name, (arity, returns_void)) in &prototypes {
        if native_library_for_symbol(name).is_some() {
            defined_functions.insert((*name).to_owned(), (*arity, *returns_void));
        }
    }
    if functions
        .iter()
        .any(|function| !statement_calls_are_defined(&function.body, &defined_functions))
    {
        return Ok(None);
    }
    let linked_libraries: HashSet<&str> = prototypes
        .keys()
        .filter_map(|name| native_library_for_symbol(name))
        .collect();
    let mut string_literals = HashSet::new();
    for function in &functions {
        collect_string_literals_statement(&function.body, &mut string_literals);
    }
    let mut assembly = String::from(
        ".address 0\n\
         __c_entry:\n\
         \x20\x20\x20\x20JMP __c_boot\n\
         .address 10\n\
         __c_boot:\n\
         \x20\x20\x20\x20CALLX __c_function_main\n\
         \x20\x20\x20\x20HALT\n",
    );
    for (library_name, library_source) in NATIVE_ASSEMBLY_LIBRARIES {
        if linked_libraries.contains(library_name) {
            assembly.push_str(library_source);
            assembly.push('\n');
        }
    }
    assembly.push_str(".address 0x4800\n");
    let mut label_counter = 0usize;
    for function in functions {
        let StatementKind::Compound(statements) = &function.body.kind else {
            return Ok(None);
        };
        let mut function_labels = HashSet::new();
        let mut label_references = HashSet::new();
        if !collect_native_labels(&function.body, &mut function_labels, &mut label_references)
            || !label_references.is_subset(&function_labels)
        {
            return Ok(None);
        }
        let has_local_array = statement_has_local_array(&function.body);
        if has_local_array && function.declaration.parameters.len() > 2 {
            return Ok(None);
        }
        let mut locals = HashMap::new();
        for (index, parameter) in function.declaration.parameters.iter().enumerate() {
            let Some(name) = &parameter.name else {
                return Ok(None);
            };
            if locals
                .insert(name.clone(), NativeLocation::Register(index as u8 + 1))
                .is_some()
            {
                return Ok(None);
            }
        }
        let mut next_local_register = function.declaration.parameters.len() as u8 + 1;
        let mut frame_size = 0;
        if !collect_native_locals(
            statements,
            &mut locals,
            &mut next_local_register,
            &mut frame_size,
            has_local_array,
        ) {
            return Ok(None);
        }
        for global in &globals {
            locals
                .entry(global.name.clone())
                .or_insert(if global.is_array {
                    NativeLocation::GlobalArray {
                        length: global.data.len() / global.element_size,
                        dimensions: global.array_dimensions.clone(),
                        element_size: global.element_size,
                        element_signed: global.element_signed,
                    }
                } else {
                    NativeLocation::GlobalScalar
                });
        }
        for (name, value) in &unit.enumerators {
            locals
                .entry(name.clone())
                .or_insert(NativeLocation::Constant(*value));
        }
        assembly.push_str(&format!("__c_function_{}:\n", function.declaration.name));
        for (index, _) in function.declaration.parameters.iter().enumerate() {
            assembly.push_str(&format!("    LDWS R{}, {}\n", index + 1, 4 + index * 2));
        }
        emit_native_stack_adjustment(frame_size, true, &mut assembly);
        if has_local_array {
            assembly.push_str("    MOVSP R3\n");
        }
        let returns_void = function.declaration.return_type.trim() == "void";
        let mut loop_labels = Vec::new();
        let mut returned = false;
        for statement in statements {
            let Some(statement_returns) = emit_native_statement(
                statement,
                &locals,
                &function.declaration.name,
                returns_void,
                &mut label_counter,
                &mut loop_labels,
                &mut assembly,
            ) else {
                return Ok(None);
            };
            if !returned {
                returned = statement_returns;
            }
        }
        if !returned {
            if returns_void {
                assembly.push_str(&format!(
                    "    JMPX __c_return_{}\n",
                    function.declaration.name
                ));
            } else {
                return Ok(None);
            }
        }
        assembly.push_str(&format!("__c_return_{}:\n", function.declaration.name));
        emit_native_stack_adjustment(frame_size, false, &mut assembly);
        assembly.push_str("    RET\n");
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
    let mut string_literals = string_literals.into_iter().collect::<Vec<_>>();
    string_literals.sort();
    for literal in string_literals {
        let Some(label) = native_string_label(&literal) else {
            continue;
        };
        let Some(mut bytes) = decode_string_token(&literal) else {
            continue;
        };
        bytes.push(0);
        assembly.push_str(&format!("{label}:\n"));
        for chunk in bytes.chunks(16) {
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
    defined_functions: &HashMap<String, (usize, bool)>,
    allow_void_call: bool,
) -> bool {
    match &expression.kind {
        ExpressionKind::Call {
            function,
            arguments,
        } => {
            let valid_target = match &function.kind {
                ExpressionKind::Atom(name) => {
                    defined_functions.get(name).is_some_and(|(arity, is_void)| {
                        *arity == arguments.len() && (allow_void_call || !is_void)
                    })
                }
                _ => false,
            };
            valid_target
                && arguments.iter().all(|argument| {
                    expression_calls_are_defined(argument, defined_functions, false)
                })
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
            expression_calls_are_defined(operand, defined_functions, false)
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
            expression_calls_are_defined(left, defined_functions, false)
                && expression_calls_are_defined(right, defined_functions, false)
        }
        ExpressionKind::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            expression_calls_are_defined(condition, defined_functions, false)
                && expression_calls_are_defined(when_true, defined_functions, false)
                && expression_calls_are_defined(when_false, defined_functions, false)
        }
        ExpressionKind::Member { expression, .. } => {
            expression_calls_are_defined(expression, defined_functions, false)
        }
        ExpressionKind::InitializerList(values) => values
            .iter()
            .all(|value| expression_calls_are_defined(value, defined_functions, false)),
        ExpressionKind::Atom(_) | ExpressionKind::SizeofType(_) => true,
    }
}

fn statement_calls_are_defined(
    statement: &Statement,
    defined_functions: &HashMap<String, (usize, bool)>,
) -> bool {
    let expression_is_defined = |expression: &Expression| {
        expression_calls_are_defined(expression, defined_functions, false)
    };
    match &statement.kind {
        StatementKind::Compound(statements) => statements
            .iter()
            .all(|statement| statement_calls_are_defined(statement, defined_functions)),
        StatementKind::Declaration(declarations) => declarations
            .iter()
            .filter_map(|declaration| declaration.initializer.as_ref())
            .all(expression_is_defined),
        StatementKind::Expression(Some(expression)) => expression_calls_are_defined(
            expression,
            defined_functions,
            matches!(&expression.kind, ExpressionKind::Call { .. }),
        ),
        StatementKind::Expression(None) | StatementKind::Return(None) => true,
        StatementKind::Return(Some(expression)) => expression_is_defined(expression),
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
        StatementKind::Label { statement, .. } => {
            statement_calls_are_defined(statement, defined_functions)
        }
        StatementKind::Break | StatementKind::Continue | StatementKind::Goto(_) => true,
    }
}

fn collect_native_locals(
    statements: &[Statement],
    locals: &mut HashMap<String, NativeLocation>,
    next_register: &mut u8,
    frame_size: &mut usize,
    reserve_frame_register: bool,
) -> bool {
    for statement in statements {
        match &statement.kind {
            StatementKind::Declaration(declarations) => {
                for declaration in declarations {
                    if locals.contains_key(&declaration.name) {
                        return false;
                    }
                    if let Some(length) = declaration.array_length {
                        let (element_size, element_signed) = match declaration.c_type.trim() {
                            "int" | "unsigned int" => (2, false),
                            "unsigned char" => (1, false),
                            "char" | "signed char" => (1, true),
                            _ => return false,
                        };
                        let Some(byte_count) = length.checked_mul(element_size) else {
                            return false;
                        };
                        let Some(new_frame_size) = frame_size.checked_add(byte_count) else {
                            return false;
                        };
                        if length == 0 || new_frame_size > 65_535 {
                            return false;
                        }
                        locals.insert(
                            declaration.name.clone(),
                            NativeLocation::StackArray {
                                offset: *frame_size,
                                length,
                                dimensions: declaration.array_dimensions.clone(),
                                element_size,
                                element_signed,
                            },
                        );
                        *frame_size = new_frame_size;
                    } else {
                        if reserve_frame_register && *next_register == 3 {
                            *next_register += 1;
                        }
                        if !is_native_word_type(&declaration.c_type) || *next_register > 3 {
                            return false;
                        }
                        locals.insert(
                            declaration.name.clone(),
                            NativeLocation::Register(*next_register),
                        );
                        *next_register += 1;
                    }
                }
            }
            StatementKind::Compound(statements) => {
                if !collect_native_locals(
                    statements,
                    locals,
                    next_register,
                    frame_size,
                    reserve_frame_register,
                ) {
                    return false;
                }
            }
            StatementKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                if !collect_native_locals(
                    std::slice::from_ref(then_branch),
                    locals,
                    next_register,
                    frame_size,
                    reserve_frame_register,
                ) || else_branch.as_ref().is_some_and(|branch| {
                    !collect_native_locals(
                        std::slice::from_ref(branch),
                        locals,
                        next_register,
                        frame_size,
                        reserve_frame_register,
                    )
                }) {
                    return false;
                }
            }
            StatementKind::While { body, .. }
            | StatementKind::DoWhile { body, .. }
            | StatementKind::For { body, .. } => {
                if !collect_native_locals(
                    std::slice::from_ref(body),
                    locals,
                    next_register,
                    frame_size,
                    reserve_frame_register,
                ) {
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
                        frame_size,
                        reserve_frame_register,
                    )
                {
                    return false;
                }
            }
            StatementKind::Switch { cases, .. } => {
                for (_, statements) in cases {
                    if !collect_native_locals(
                        statements,
                        locals,
                        next_register,
                        frame_size,
                        reserve_frame_register,
                    ) {
                        return false;
                    }
                }
            }
            StatementKind::Label { statement, .. }
                if !collect_native_locals(
                    std::slice::from_ref(statement),
                    locals,
                    next_register,
                    frame_size,
                    reserve_frame_register,
                ) =>
            {
                return false;
            }
            _ => {}
        }
    }
    true
}

fn statement_has_local_array(statement: &Statement) -> bool {
    match &statement.kind {
        StatementKind::Declaration(declarations) => declarations
            .iter()
            .any(|declaration| declaration.array_length.is_some()),
        StatementKind::Compound(statements) => statements.iter().any(statement_has_local_array),
        StatementKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            statement_has_local_array(then_branch)
                || else_branch
                    .as_ref()
                    .is_some_and(|branch| statement_has_local_array(branch))
        }
        StatementKind::While { body, .. } | StatementKind::DoWhile { body, .. } => {
            statement_has_local_array(body)
        }
        StatementKind::For {
            initializer, body, ..
        } => {
            initializer
                .as_ref()
                .is_some_and(|initializer| statement_has_local_array(initializer))
                || statement_has_local_array(body)
        }
        StatementKind::Switch { cases, .. } => cases
            .iter()
            .any(|(_, statements)| statements.iter().any(statement_has_local_array)),
        StatementKind::Label { statement, .. } => statement_has_local_array(statement),
        _ => false,
    }
}

fn collect_native_labels(
    statement: &Statement,
    labels: &mut HashSet<String>,
    references: &mut HashSet<String>,
) -> bool {
    match &statement.kind {
        StatementKind::Compound(statements) => statements
            .iter()
            .all(|statement| collect_native_labels(statement, labels, references)),
        StatementKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_native_labels(then_branch, labels, references)
                && else_branch
                    .as_ref()
                    .is_none_or(|statement| collect_native_labels(statement, labels, references))
        }
        StatementKind::While { body, .. }
        | StatementKind::DoWhile { body, .. }
        | StatementKind::For { body, .. } => collect_native_labels(body, labels, references),
        StatementKind::Switch { cases, .. } => cases.iter().all(|(_, statements)| {
            statements
                .iter()
                .all(|statement| collect_native_labels(statement, labels, references))
        }),
        StatementKind::Goto(name) => {
            references.insert(name.clone());
            true
        }
        StatementKind::Label { name, statement } => {
            labels.insert(name.clone()) && collect_native_labels(statement, labels, references)
        }
        _ => true,
    }
}

fn native_sizeof_type(c_type: &str) -> Option<u16> {
    let c_type = c_type.trim();
    if c_type.ends_with('*') {
        Some(4)
    } else if matches!(c_type, "char" | "signed char" | "unsigned char") {
        Some(1)
    } else if matches!(
        c_type,
        "int" | "signed" | "signed int" | "unsigned" | "unsigned int"
    ) {
        Some(2)
    } else if matches!(c_type, "long" | "signed long" | "unsigned long") {
        Some(4)
    } else {
        None
    }
}

fn native_sizeof_expression(
    expression: &Expression,
    locals: &HashMap<String, NativeLocation>,
) -> Option<u16> {
    match &expression.kind {
        ExpressionKind::Atom(value) if parse_integer_constant(value).is_some() => Some(2),
        ExpressionKind::Atom(name) => match locals.get(name) {
            Some(NativeLocation::GlobalArray {
                length,
                element_size,
                ..
            }) => u16::try_from(length.checked_mul(*element_size)?).ok(),
            Some(NativeLocation::StackArray {
                length,
                element_size,
                ..
            }) => u16::try_from(length.checked_mul(*element_size)?).ok(),
            Some(_) => Some(2),
            None => None,
        },
        ExpressionKind::Index { expression, .. } => {
            let ExpressionKind::Atom(name) = &expression.kind else {
                return None;
            };
            match locals.get(name) {
                Some(NativeLocation::GlobalArray { element_size, .. }) => {
                    u16::try_from(*element_size).ok()
                }
                Some(NativeLocation::StackArray { element_size, .. }) => {
                    u16::try_from(*element_size).ok()
                }
                _ => None,
            }
        }
        ExpressionKind::Unary { operator, operand }
            if matches!(operator.as_str(), "+" | "-" | "~" | "!") =>
        {
            native_sizeof_expression(operand, locals)
        }
        ExpressionKind::Binary { left, right, .. } => {
            native_sizeof_expression(left, locals)?;
            native_sizeof_expression(right, locals)?;
            Some(2)
        }
        ExpressionKind::Assignment { target, value, .. } => {
            native_sizeof_expression(target, locals)?;
            native_sizeof_expression(value, locals)?;
            Some(2)
        }
        ExpressionKind::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            native_sizeof_expression(condition, locals)?;
            native_sizeof_expression(when_true, locals)?;
            native_sizeof_expression(when_false, locals)?;
            Some(2)
        }
        ExpressionKind::Call { .. } | ExpressionKind::Postfix { .. } => Some(2),
        ExpressionKind::Cast { c_type, .. } => native_sizeof_type(c_type),
        ExpressionKind::SizeofType(c_type) => native_sizeof_type(c_type),
        ExpressionKind::SizeofExpression(expression) => {
            native_sizeof_expression(expression, locals)?;
            Some(2)
        }
        _ => None,
    }
}

fn native_statement_termination(statement: &Statement) -> Option<bool> {
    match &statement.kind {
        StatementKind::Return(_) => Some(true),
        StatementKind::Break => Some(false),
        StatementKind::Compound(statements) => {
            statements.iter().find_map(native_statement_termination)
        }
        StatementKind::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => match (
            native_statement_termination(then_branch),
            native_statement_termination(else_branch),
        ) {
            (Some(true), Some(true)) => Some(true),
            (Some(_), Some(_)) => Some(false),
            _ => None,
        },
        StatementKind::If {
            then_branch,
            else_branch: None,
            ..
        } if native_statement_termination(then_branch) == Some(false) => Some(false),
        StatementKind::Label { statement, .. } => native_statement_termination(statement),
        _ => None,
    }
}

fn emit_native_array_address(
    name: &str,
    index_register: u8,
    locals: &HashMap<String, NativeLocation>,
    assembly: &mut String,
) -> Option<(usize, bool)> {
    let (element_size, element_signed) = match locals.get(name)? {
        NativeLocation::GlobalArray {
            element_size,
            element_signed,
            ..
        } => {
            assembly.push_str(&format!("    LDI30 R7, __c_global_{name}\n"));
            (*element_size, *element_signed)
        }
        NativeLocation::StackArray {
            offset,
            element_size,
            element_signed,
            ..
        } => {
            assembly.push_str("    MOV R7, R3\n");
            if *offset > 0 {
                assembly.push_str(&format!("    ADD R7, {offset}\n"));
            }
            (*element_size, *element_signed)
        }
        _ => return None,
    };
    if element_size == 2 {
        assembly.push_str(&format!("    SHL R{index_register}, 1\n"));
    }
    assembly.push_str(&format!("    ADD R7, R{index_register}\n"));
    Some((element_size, element_signed))
}

fn collect_native_array_indices<'a>(
    expression: &'a Expression,
    indices: &mut Vec<&'a Expression>,
) -> Option<&'a str> {
    match &expression.kind {
        ExpressionKind::Atom(name) => Some(name),
        ExpressionKind::Index {
            expression: base,
            index,
        } => {
            let name = collect_native_array_indices(base, indices)?;
            indices.push(index);
            Some(name)
        }
        _ => None,
    }
}

fn emit_native_array_element_address(
    base: &Expression,
    index: &Expression,
    index_destination: u8,
    temporary_registers: &[u8],
    locals: &HashMap<String, NativeLocation>,
    label_counter: &mut usize,
    assembly: &mut String,
) -> Option<(usize, bool)> {
    let mut indices = Vec::new();
    let name = collect_native_array_indices(base, &mut indices)?;
    indices.push(index);
    let dimensions = match locals.get(name)? {
        NativeLocation::GlobalArray { dimensions, .. }
        | NativeLocation::StackArray { dimensions, .. } => dimensions,
        _ => return None,
    };
    if indices.len() != dimensions.len() || indices.is_empty() {
        return None;
    }
    if !emit_native_expression(
        indices[0],
        index_destination,
        temporary_registers,
        locals,
        label_counter,
        assembly,
    ) {
        return None;
    }
    for (index, dimension) in indices.iter().enumerate().skip(1) {
        let (&index_register, remaining_registers) = temporary_registers.split_first()?;
        assembly.push_str(&format!("    PUSH R{index_destination}\n"));
        if !emit_native_expression(
            dimension,
            index_register,
            remaining_registers,
            locals,
            label_counter,
            assembly,
        ) {
            return None;
        }
        assembly.push_str(&format!(
            "    POP R{index_destination}\n    LDI R7, {}\n\
             MUL R{index_destination}, R7\n    ADD R{index_destination}, R{index_register}\n",
            dimensions[index]
        ));
    }
    emit_native_array_address(name, index_destination, locals, assembly)
}

fn decode_string_token(value: &str) -> Option<Vec<u8>> {
    let hex = value.strip_prefix("<string:")?.strip_suffix('>')?;
    if hex.len() % 2 != 0 {
        return None;
    }
    hex.as_bytes()
        .chunks(2)
        .map(|pair| {
            if pair.len() != 2 {
                return None;
            }
            let digits = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(digits, 16).ok()
        })
        .collect()
}

fn native_string_label(value: &str) -> Option<String> {
    decode_string_token(value)?;
    Some(format!(
        "__c_string_{}",
        value.strip_prefix("<string:")?.strip_suffix('>')?
    ))
}

fn emit_native_stack_adjustment(frame_size: usize, allocate: bool, assembly: &mut String) {
    let mut remaining = frame_size;
    while remaining > 0 {
        let adjustment = remaining.min(127) as i32;
        let adjustment = if allocate { -adjustment } else { adjustment };
        assembly.push_str(&format!("    ADJSP {adjustment}\n"));
        remaining -= adjustment.unsigned_abs() as usize;
    }
}

fn emit_native_expression(
    expression: &Expression,
    destination: u8,
    temporary_registers: &[u8],
    locals: &HashMap<String, NativeLocation>,
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
            } else if let Some(label) = native_string_label(value) {
                assembly.push_str(&format!("    LDI30 R{destination}, {label}\n"));
            } else if let Some(location) = locals.get(value) {
                match location {
                    NativeLocation::Constant(value) => {
                        assembly.push_str(&format!(
                            "    LDI R{destination}, 0x{:04X}\n",
                            *value as u16
                        ));
                    }
                    NativeLocation::GlobalScalar => assembly.push_str(&format!(
                        "    LDI30 R7, __c_global_{value}\n    LDW R{destination}, R7\n"
                    )),
                    NativeLocation::GlobalArray { .. } | NativeLocation::StackArray { .. } => {
                        return false;
                    }
                    NativeLocation::Register(source) if *source != destination => {
                        assembly.push_str(&format!("    MOV R{destination}, R{source}\n"));
                    }
                    NativeLocation::Register(_) => {}
                }
            } else {
                return false;
            }
            true
        }
        ExpressionKind::SizeofType(c_type) => {
            let Some(size) = native_sizeof_type(c_type) else {
                return false;
            };
            assembly.push_str(&format!("    LDI R{destination}, {size}\n"));
            true
        }
        ExpressionKind::SizeofExpression(operand) => {
            let Some(size) = native_sizeof_expression(operand, locals) else {
                return false;
            };
            assembly.push_str(&format!("    LDI R{destination}, {size}\n"));
            true
        }
        ExpressionKind::Unary { operator, .. } if matches!(operator.as_str(), "++" | "--") => {
            let Some(operand) = native_incdec_operand(expression) else {
                return false;
            };
            if !emit_native_update(expression, locals, label_counter, assembly) {
                return false;
            }
            emit_native_expression(
                operand,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            )
        }
        ExpressionKind::Postfix { .. } => {
            let Some(operand) = native_incdec_operand(expression) else {
                return false;
            };
            let ExpressionKind::Atom(name) = &operand.kind else {
                return false;
            };
            if matches!(locals.get(name), Some(NativeLocation::Register(register)) if *register == destination)
            {
                return false;
            }
            if !emit_native_expression(
                operand,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            ) {
                return false;
            }
            assembly.push_str(&format!("    PUSH R{destination}\n"));
            if !emit_native_update(expression, locals, label_counter, assembly) {
                return false;
            }
            assembly.push_str(&format!("    POP R{destination}\n"));
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
            let library_function = native_library_for_symbol(name).is_some();
            if arguments.len() > if library_function { 8 } else { 3 } {
                return false;
            }
            for register in 1..=6 {
                assembly.push_str(&format!("    PUSH R{register}\n"));
            }
            let mut argument_bytes = 0;
            if library_function {
                for (index, argument) in arguments.iter().enumerate().rev() {
                    if !emit_native_expression(
                        argument,
                        4,
                        &[5, 6, 7],
                        locals,
                        label_counter,
                        assembly,
                    ) {
                        return false;
                    }
                    if native_argument_is_pointer(name, index) {
                        assembly.push_str("    MOV R5, R4\n    LDI R6, 16\n    SHR R5, R6\n");
                        assembly.push_str("    PUSH R5\n");
                        argument_bytes += 2;
                    }
                    assembly.push_str("    PUSH R4\n");
                    argument_bytes += 2;
                }
            } else {
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
                argument_bytes = arguments.len() * 2;
            }
            assembly.push_str(&format!(
                "    CALLX {}\n",
                if library_function {
                    name.clone()
                } else {
                    format!("__c_function_{name}")
                }
            ));
            if argument_bytes != 0 {
                assembly.push_str(&format!("    ADJSP {argument_bytes}\n"));
            }
            for register in (1..=6).rev() {
                assembly.push_str(&format!("    POP R{register}\n"));
            }
            if destination != 0 {
                assembly.push_str(&format!("    MOV R{destination}, R0\n"));
            }
            true
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } if operator == "," => {
            emit_native_discarded_expression(
                left,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            ) && emit_native_expression(
                right,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            )
        }
        ExpressionKind::Index {
            expression: base,
            index,
        } => {
            if destination == 7 {
                return false;
            }
            let Some((element_size, element_signed)) = emit_native_array_element_address(
                base,
                index,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            ) else {
                return false;
            };
            assembly.push_str(&format!(
                "    {} R{destination}, R7\n",
                if element_size == 1 { "LD" } else { "LDW" }
            ));
            if element_signed {
                assembly.push_str(&format!(
                    "    SHL R{destination}, 8\n    ASR R{destination}, 8\n"
                ));
            }
            true
        }
        _ => false,
    }
}

fn emit_native_discarded_expression(
    expression: &Expression,
    destination: u8,
    temporary_registers: &[u8],
    locals: &HashMap<String, NativeLocation>,
    label_counter: &mut usize,
    assembly: &mut String,
) -> bool {
    match &expression.kind {
        ExpressionKind::Assignment { .. } => emit_native_assignment(
            expression,
            temporary_registers,
            locals,
            label_counter,
            assembly,
        ),
        ExpressionKind::Postfix { operator, .. } | ExpressionKind::Unary { operator, .. }
            if matches!(operator.as_str(), "++" | "--") =>
        {
            emit_native_update(expression, locals, label_counter, assembly)
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } if operator == "," => {
            emit_native_discarded_expression(
                left,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            ) && emit_native_discarded_expression(
                right,
                destination,
                temporary_registers,
                locals,
                label_counter,
                assembly,
            )
        }
        _ => emit_native_expression(
            expression,
            destination,
            temporary_registers,
            locals,
            label_counter,
            assembly,
        ),
    }
}

fn native_incdec_operand(expression: &Expression) -> Option<&Expression> {
    match &expression.kind {
        ExpressionKind::Unary { operator, operand }
        | ExpressionKind::Postfix {
            operator,
            expression: operand,
        } if matches!(operator.as_str(), "++" | "--")
            && matches!(&operand.kind, ExpressionKind::Atom(_)) =>
        {
            Some(operand)
        }
        _ => None,
    }
}

fn emit_native_assignment(
    expression: &Expression,
    temporary_registers: &[u8],
    locals: &HashMap<String, NativeLocation>,
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
        let Some((element_size, element_signed)) = emit_native_array_element_address(
            base,
            index,
            0,
            &[5, 6, 7],
            locals,
            label_counter,
            assembly,
        ) else {
            return false;
        };
        let (load, store) = if element_size == 1 {
            ("LD", "ST")
        } else {
            ("LDW", "STW")
        };
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
            assembly.push_str(&format!("    {load} R0, R7\n"));
            if element_signed {
                assembly.push_str("    SHL R0, 8\n    ASR R0, 8\n");
            }
            assembly.push_str(&format!("    {binary_operator} R0, R{value_register}\n"));
            assembly.push_str(&format!("    {store} R7, R0\n"));
        } else {
            assembly.push_str(&format!("    {store} R7, R{value_register}\n"));
        }
        return true;
    }
    let ExpressionKind::Atom(name) = &target.kind else {
        return false;
    };
    let Some(location) = locals.get(name) else {
        return false;
    };
    let (destination, is_global) = match location {
        NativeLocation::Register(register) => (*register, false),
        NativeLocation::Constant(_) => return false,
        NativeLocation::GlobalScalar => (0, true),
        NativeLocation::GlobalArray { .. } | NativeLocation::StackArray { .. } => return false,
    };
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
    locals: &HashMap<String, NativeLocation>,
    function_name: &str,
    returns_void: bool,
    label_counter: &mut usize,
    loop_labels: &mut Vec<(String, String)>,
    assembly: &mut String,
) -> Option<bool> {
    match &statement.kind {
        StatementKind::Declaration(declarations) => {
            for declaration in declarations {
                if let Some(NativeLocation::StackArray {
                    length,
                    element_size,
                    ..
                }) = locals.get(&declaration.name)
                {
                    if let Some(initializer) = &declaration.initializer {
                        if let ExpressionKind::Atom(string_token) = &initializer.kind
                            && let Some(bytes) = decode_string_token(string_token)
                        {
                            if declaration.array_dimensions.len() != 1
                                || *element_size != 1
                                || bytes.len() > *length
                                || *length > 512
                            {
                                return None;
                            }
                            for index in 0..*length {
                                assembly.push_str(&format!(
                                    "    LDI R4, {}\n",
                                    bytes.get(index).copied().unwrap_or(0)
                                ));
                                assembly.push_str(&format!("    LDI R0, {index}\n"));
                                emit_native_array_address(&declaration.name, 0, locals, assembly)?;
                                assembly.push_str("    ST R7, R4\n");
                            }
                        } else {
                            if !matches!(&initializer.kind, ExpressionKind::InitializerList(_)) {
                                return None;
                            }
                            let values = flatten_initializer_values(initializer)?;
                            if values.len() > *length || *length > 512 {
                                return None;
                            }
                            for index in 0..*length {
                                if let Some(value) = values.get(index) {
                                    if !emit_native_expression(
                                        value,
                                        4,
                                        &[5, 6, 7],
                                        locals,
                                        label_counter,
                                        assembly,
                                    ) {
                                        return None;
                                    }
                                } else {
                                    assembly.push_str("    LDI R4, 0\n");
                                }
                                assembly.push_str(&format!("    LDI R0, {index}\n"));
                                emit_native_array_address(&declaration.name, 0, locals, assembly)?;
                                assembly.push_str(&format!(
                                    "    {} R7, R4\n",
                                    if *element_size == 1 { "ST" } else { "STW" }
                                ));
                            }
                        }
                    }
                } else if let Some(initializer) = &declaration.initializer {
                    let initializer = scalar_initializer(initializer)?;
                    let NativeLocation::Register(destination) = locals.get(&declaration.name)?
                    else {
                        return None;
                    };
                    if !emit_native_expression(
                        initializer,
                        *destination,
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
            if emit_native_discarded_expression(
                expression,
                0,
                &[4, 5, 6, 7],
                locals,
                label_counter,
                assembly,
            ) {
                Some(false)
            } else {
                None
            }
        }
        StatementKind::Return(Some(expression)) => {
            if returns_void {
                return None;
            }
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
            assembly.push_str(&format!("    JMPX __c_return_{function_name}\n"));
            Some(true)
        }
        StatementKind::Return(None) if returns_void => {
            assembly.push_str(&format!("    JMPX __c_return_{function_name}\n"));
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
                let statement_returns = emit_native_statement(
                    statement,
                    locals,
                    function_name,
                    returns_void,
                    label_counter,
                    loop_labels,
                    assembly,
                )?;
                if !branch_returns {
                    branch_returns = statement_returns;
                }
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
                    function_name,
                    returns_void,
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
            let then_returns = emit_native_statement(
                then_branch,
                locals,
                function_name,
                returns_void,
                label_counter,
                loop_labels,
                assembly,
            )?;
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
            emit_native_statement(
                body,
                locals,
                function_name,
                returns_void,
                label_counter,
                loop_labels,
                assembly,
            )?;
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
            emit_native_statement(
                body,
                locals,
                function_name,
                returns_void,
                label_counter,
                loop_labels,
                assembly,
            )?;
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
                emit_native_statement(
                    initializer,
                    locals,
                    function_name,
                    returns_void,
                    label_counter,
                    loop_labels,
                    assembly,
                )?;
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
            emit_native_statement(
                body,
                locals,
                function_name,
                returns_void,
                label_counter,
                loop_labels,
                assembly,
            )?;
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
            let constant_names: HashMap<String, i64> = locals
                .iter()
                .filter_map(|(name, location)| match location {
                    NativeLocation::Constant(value) => Some((name.clone(), *value)),
                    _ => None,
                })
                .collect();
            for (value, _) in cases {
                let dispatch_label = format!("__c_switch_{}", *label_counter);
                *label_counter += 1;
                let body_label = format!("__c_switch_body_{}", *label_counter);
                *label_counter += 1;
                if let Some(value) = value {
                    let Ok(value) = evaluate_integer_constant_with_names(value, &constant_names)
                    else {
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
                    emit_native_statement(
                        statement,
                        locals,
                        function_name,
                        returns_void,
                        label_counter,
                        loop_labels,
                        assembly,
                    )?;
                }
            }
            loop_labels.pop();
            assembly.push_str(&format!("{end_label}:\n"));
            if default_label.is_none() {
                return Some(false);
            }
            let mut next_case_returns = false;
            for (_, statements) in cases.iter().rev() {
                if let Some(returns) = statements.iter().find_map(native_statement_termination) {
                    next_case_returns = returns;
                }
                if !next_case_returns {
                    return Some(false);
                }
            }
            Some(true)
        }
        StatementKind::Goto(name) => {
            assembly.push_str(&format!("    JMPX __c_label_{function_name}_{name}\n"));
            Some(false)
        }
        StatementKind::Label { name, statement } => {
            assembly.push_str(&format!("__c_label_{function_name}_{name}:\n"));
            emit_native_statement(
                statement,
                locals,
                function_name,
                returns_void,
                label_counter,
                loop_labels,
                assembly,
            )
        }
        _ => None,
    }
}

fn emit_native_update(
    expression: &Expression,
    locals: &HashMap<String, NativeLocation>,
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
        let Some((element_size, element_signed)) = emit_native_array_element_address(
            base,
            index,
            0,
            &[5, 6, 7],
            locals,
            label_counter,
            assembly,
        ) else {
            return false;
        };
        let (load, store) = if element_size == 1 {
            ("LD", "ST")
        } else {
            ("LDW", "STW")
        };
        assembly.push_str(&format!("    {load} R0, R7\n"));
        if element_signed {
            assembly.push_str("    SHL R0, 8\n    ASR R0, 8\n");
        }
        assembly.push_str(&format!("    {instruction} R0\n"));
        assembly.push_str(&format!("    {store} R7, R0\n"));
        return true;
    }
    let ExpressionKind::Atom(name) = &expression.kind else {
        return false;
    };
    let Some(location) = locals.get(name) else {
        return false;
    };
    match location {
        NativeLocation::GlobalScalar => assembly.push_str(&format!(
            "    LDI30 R7, __c_global_{name}\n    LDW R0, R7\n\
             {instruction} R0\n    LDI30 R7, __c_global_{name}\n    STW R7, R0\n"
        )),
        NativeLocation::Register(register) => {
            assembly.push_str(&format!("    {instruction} R{register}\n"));
        }
        NativeLocation::Constant(_)
        | NativeLocation::GlobalArray { .. }
        | NativeLocation::StackArray { .. } => return false,
    }
    true
}

pub struct StatementParser {
    expressions: ExpressionParser,
    enum_values: HashMap<String, i64>,
}

pub struct TranslationUnitParser {
    statements: StatementParser,
    enumerators: HashMap<String, i64>,
    anonymous_record_counter: usize,
}

const ASSIGNMENTS: [&str; 11] = [
    "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=",
];
const TYPE_WORDS: [&str; 13] = [
    "void", "char", "short", "int", "long", "float", "double", "signed", "unsigned", "const",
    "volatile", "_Bool", "enum",
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
            typedefs: HashMap::new(),
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
        if self.accept("enum").is_some() {
            if self.token().value != "<eof>"
                && self
                    .token()
                    .value
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_alphabetic() || character == '_')
            {
                self.advance();
            } else {
                return Err(self.fail("expected an enum tag"));
            }
            return Ok("int".to_owned());
        }
        if matches!(self.token().value.as_str(), "struct" | "union") {
            return Err(self.fail("record types are not supported here yet"));
        }
        let mut c_type = if let Some(alias) = self.typedefs.get(&self.token().value).cloned() {
            self.advance();
            alias
        } else {
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
            words.join(" ")
        };
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
            && self.tokens.get(self.position + 1).is_some_and(|token| {
                TYPE_WORDS.contains(&token.value.as_str())
                    || self.typedefs.contains_key(&token.value)
            })
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
                || self.typedefs.contains_key(&token.value)
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
            enum_values: HashMap::new(),
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
            || self
                .expressions
                .typedefs
                .contains_key(&self.expressions.token().value)
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
            && !self
                .expressions
                .typedefs
                .contains_key(&self.expressions.token().value)
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
            return Ok(self
                .expressions
                .make(&open, ExpressionKind::InitializerList(values)));
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
        let base_type = if matches!(self.expressions.token().value.as_str(), "struct" | "union") {
            let kind = self.expressions.advance().value;
            let tag = self.expect_identifier("expected a record tag")?;
            format!("{kind} {}", tag.value)
        } else if self.accept("enum").is_some() {
            self.expect_identifier("expected an enum tag")?;
            "int".to_owned()
        } else if let Some(alias) = self
            .expressions
            .typedefs
            .get(&self.expressions.token().value)
            .cloned()
        {
            self.expressions.advance();
            alias
        } else {
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
            type_words.join(" ")
        };
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
            let mut array_dimensions = self.parse_array_dimensions()?;
            let initializer = if self.accept("=").is_some() {
                Some(self.parse_initializer()?)
            } else {
                None
            };
            if array_dimensions.first() == Some(&INCOMPLETE_ARRAY_LENGTH) {
                if array_dimensions.len() != 1 {
                    return Err(self.expressions.fail(
                        "incomplete outer arrays with additional dimensions are not supported",
                    ));
                }
                let Some(length) = initializer
                    .as_ref()
                    .and_then(|initializer| infer_incomplete_array_length(&c_type, initializer))
                else {
                    return Err(error(
                        name.filename.as_deref().unwrap_or("<source>"),
                        name.line,
                        "incomplete array requires an initializer that determines its size",
                    ));
                };
                if length == 0 {
                    return Err(error(
                        name.filename.as_deref().unwrap_or("<source>"),
                        name.line,
                        "array size must be greater than zero",
                    ));
                }
                array_dimensions[0] = length;
            }
            let array_length = if array_dimensions.is_empty() {
                None
            } else {
                Some(
                    array_dimensions
                        .iter()
                        .try_fold(1usize, |size, dimension| size.checked_mul(*dimension))
                        .ok_or_else(|| self.expressions.fail("array size is too large"))?,
                )
            };
            declarations.push(Declaration {
                c_type,
                name: name.value,
                array_length,
                array_dimensions,
                initializer,
            });
            if self.accept(",").is_none() {
                break;
            }
        }
        self.expect(";")?;
        Ok(self.statement(&start, StatementKind::Declaration(declarations)))
    }

    fn parse_array_dimensions(&mut self) -> Result<Vec<usize>, CCompilerError> {
        let mut dimensions = Vec::new();
        while self.accept("[").is_some() {
            if self.accept("]").is_some() {
                dimensions.push(INCOMPLETE_ARRAY_LENGTH);
                continue;
            }
            let expression = self.expressions.parse_expression(0)?;
            self.expect("]")?;
            let value = evaluate_integer_constant_with_names(&expression, &self.enum_values)?;
            dimensions.push(usize::try_from(value).map_err(|_| {
                error(
                    expression.filename.as_deref().unwrap_or("<source>"),
                    expression.line,
                    "array length must be a non-negative integer constant",
                )
            })?);
        }
        Ok(dimensions)
    }
}

impl TranslationUnitParser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            statements: StatementParser::new(tokens),
            enumerators: HashMap::new(),
            anonymous_record_counter: 0,
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
        Ok(TranslationUnit {
            declarations,
            enumerators: self.enumerators.clone(),
        })
    }

    fn parse_external_declaration(&mut self) -> Result<Vec<ExternalDeclaration>, CCompilerError> {
        let token = self.statements.expressions.token().clone();
        let (base_type, storage_class) = self.parse_base_type()?;
        if self.statements.accept(";").is_some() {
            return Ok(Vec::new());
        }
        let (declarator_type, name) = self.parse_declarator(&base_type)?;
        if self.statements.accept("(").is_some() {
            if storage_class.as_deref() == Some("typedef") {
                return Err(self
                    .statements
                    .expressions
                    .fail("function typedefs are not supported"));
            }
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
            self.statements.enum_values = self.enumerators.clone();
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

        let is_typedef = storage_class.as_deref() == Some("typedef");
        let mut globals = Vec::new();
        let mut next = Some((declarator_type, name));
        while let Some((declarator_type, name)) = next.take() {
            let mut array_dimensions = self.parse_array_dimensions()?;
            let initializer = if self.statements.accept("=").is_some() {
                Some(self.statements.parse_initializer()?)
            } else {
                None
            };
            if array_dimensions.first() == Some(&INCOMPLETE_ARRAY_LENGTH) {
                if array_dimensions.len() != 1 {
                    return Err(error(
                        name.filename.as_deref().unwrap_or("<source>"),
                        name.line,
                        "incomplete outer arrays with additional dimensions are not supported",
                    ));
                }
                let Some(length) = initializer.as_ref().and_then(|initializer| {
                    infer_incomplete_array_length(&declarator_type, initializer)
                }) else {
                    return Err(error(
                        name.filename.as_deref().unwrap_or("<source>"),
                        name.line,
                        "incomplete array requires an initializer that determines its size",
                    ));
                };
                if length == 0 {
                    return Err(error(
                        name.filename.as_deref().unwrap_or("<source>"),
                        name.line,
                        "array size must be greater than zero",
                    ));
                }
                array_dimensions[0] = length;
            }
            let array_length = if array_dimensions.is_empty() {
                None
            } else {
                Some(
                    array_dimensions
                        .iter()
                        .try_fold(1usize, |size, dimension| size.checked_mul(*dimension))
                        .ok_or_else(|| {
                            error(
                                name.filename.as_deref().unwrap_or("<source>"),
                                name.line,
                                "array size is too large",
                            )
                        })?,
                )
            };
            if is_typedef {
                if array_length.is_some() || initializer.is_some() {
                    return Err(error(
                        name.filename.as_deref().unwrap_or("<source>"),
                        name.line,
                        "array and initialized typedefs are not supported",
                    ));
                }
                if self.enumerators.contains_key(&name.value)
                    || self
                        .statements
                        .expressions
                        .typedefs
                        .insert(name.value.clone(), declarator_type.clone())
                        .is_some()
                {
                    return Err(error(
                        name.filename.as_deref().unwrap_or("<source>"),
                        name.line,
                        &format!("duplicate typedef name {:?}", name.value),
                    ));
                }
            } else {
                if self
                    .statements
                    .expressions
                    .typedefs
                    .contains_key(&name.value)
                {
                    return Err(error(
                        name.filename.as_deref().unwrap_or("<source>"),
                        name.line,
                        "declaration conflicts with a typedef name",
                    ));
                }
                globals.push(ExternalDeclaration::Global(Declaration {
                    c_type: declarator_type.clone(),
                    name: name.value.clone(),
                    array_length,
                    array_dimensions,
                    initializer,
                }));
            }
            next = if self.statements.accept(",").is_some() {
                Some(self.parse_declarator(&base_type)?)
            } else {
                None
            };
        }
        self.statements.expect(";")?;
        Ok(if is_typedef { Vec::new() } else { globals })
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
        if self.statements.accept("enum").is_some() {
            self.parse_enum_specifier()?;
            let c_type = if qualifiers.is_empty() {
                "int".to_owned()
            } else {
                format!("{} int", qualifiers.join(" "))
            };
            return Ok((c_type, storage_class));
        }
        if matches!(
            self.statements.expressions.token().value.as_str(),
            "struct" | "union"
        ) {
            let kind = self.statements.expressions.advance().value;
            let tag = if self.statements.at("{") {
                None
            } else {
                Some(
                    self.statements
                        .expect_identifier("expected a record tag or body")?
                        .value,
                )
            };
            let record_name = if let Some(tag) = tag {
                tag
            } else {
                self.anonymous_record_counter += 1;
                format!("__anonymous_{}", self.anonymous_record_counter)
            };
            if self.statements.accept("{").is_some() {
                self.parse_record_members()?;
            } else if record_name.starts_with("__anonymous_") {
                return Err(self
                    .statements
                    .expressions
                    .fail("expected a record tag or body"));
            }
            let c_type = format!("{kind} {record_name}");
            return Ok((
                if qualifiers.is_empty() {
                    c_type
                } else {
                    format!("{} {c_type}", qualifiers.join(" "))
                },
                storage_class,
            ));
        }
        if let Some(alias) = self
            .statements
            .expressions
            .typedefs
            .get(&self.statements.expressions.token().value)
            .cloned()
        {
            self.statements.expressions.advance();
            let c_type = if qualifiers.is_empty() {
                alias
            } else {
                format!("{} {alias}", qualifiers.join(" "))
            };
            return Ok((c_type, storage_class));
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

    fn parse_record_members(&mut self) -> Result<(), CCompilerError> {
        while !self.statements.at("}") {
            if self.statements.expressions.is_at_end() {
                return Err(self.statements.expressions.fail("unterminated record body"));
            }
            let member_type = if self.statements.accept("enum").is_some() {
                self.parse_enum_specifier()?;
                "int".to_owned()
            } else if let Some(alias) = self
                .statements
                .expressions
                .typedefs
                .get(&self.statements.expressions.token().value)
                .cloned()
            {
                self.statements.expressions.advance();
                alias
            } else {
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
                        .fail("expected a supported record member type"));
                }
                words.join(" ")
            };
            loop {
                while self.statements.accept("*").is_some() {}
                self.statements
                    .expect_identifier("expected a record member name")?;
                self.statements.parse_array_dimensions()?;
                if self.statements.accept(":").is_some() {
                    return Err(self
                        .statements
                        .expressions
                        .fail("record bit-fields are not supported"));
                }
                if self.statements.accept(",").is_none() {
                    break;
                }
            }
            self.statements.expect(";")?;
            let _ = &member_type;
        }
        self.statements.expect("}")?;
        Ok(())
    }

    fn parse_enum_specifier(&mut self) -> Result<(), CCompilerError> {
        let mut has_tag = false;
        if !self.statements.at("{")
            && self
                .statements
                .expressions
                .token()
                .value
                .chars()
                .next()
                .is_some_and(|character| character.is_alphabetic() || character == '_')
        {
            self.statements.expressions.advance();
            has_tag = true;
        }
        if self.statements.accept("{").is_none() {
            return if has_tag {
                Ok(())
            } else {
                Err(self
                    .statements
                    .expressions
                    .fail("expected an enum tag or body"))
            };
        }
        if self.statements.at("}") {
            return Err(self
                .statements
                .expressions
                .fail("enum body cannot be empty"));
        }
        let mut next_value = 0i64;
        loop {
            let name = self
                .statements
                .expect_identifier("expected an enumerator name")?;
            let value = if self.statements.accept("=").is_some() {
                let expression = self.statements.expressions.parse_expression(1)?;
                evaluate_integer_constant_with_names(&expression, &self.enumerators)?
            } else {
                next_value
            };
            if !(i16::MIN as i64..=u16::MAX as i64).contains(&value) {
                return Err(error(
                    name.filename.as_deref().unwrap_or("<source>"),
                    name.line,
                    "enumerator value is outside the supported int range",
                ));
            }
            if self.enumerators.insert(name.value.clone(), value).is_some() {
                return Err(error(
                    name.filename.as_deref().unwrap_or("<source>"),
                    name.line,
                    &format!("duplicate enumerator {:?}", name.value),
                ));
            }
            next_value = value.checked_add(1).ok_or_else(|| {
                error(
                    name.filename.as_deref().unwrap_or("<source>"),
                    name.line,
                    "enumerator value is outside the supported int range",
                )
            })?;
            if self.statements.accept(",").is_none() || self.statements.at("}") {
                break;
            }
        }
        self.statements.expect("}")?;
        Ok(())
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

    fn parse_array_dimensions(&mut self) -> Result<Vec<usize>, CCompilerError> {
        let mut dimensions = Vec::new();
        while self.statements.accept("[").is_some() {
            if self.statements.accept("]").is_some() {
                dimensions.push(INCOMPLETE_ARRAY_LENGTH);
                continue;
            }
            let expression = self.statements.expressions.parse_expression(0)?;
            self.statements.expect("]")?;
            let length = evaluate_integer_constant_with_names(&expression, &self.enumerators)?;
            dimensions.push(usize::try_from(length).map_err(|_| {
                error(
                    expression.filename.as_deref().unwrap_or("<source>"),
                    expression.line,
                    "array length must be a non-negative integer constant",
                )
            })?);
        }
        Ok(dimensions)
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
        position += 1;
        let fractional_start = position;
        while bytes.get(position).is_some_and(u8::is_ascii_digit) {
            position += 1;
        }
        is_float = digits_before > 0 || position > fractional_start;
        if !is_float {
            return None;
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

enum QuotedCharacter {
    Character(char),
    Byte(u8),
}

fn decode_quoted(value: &str) -> Result<Vec<QuotedCharacter>, &'static str> {
    let characters: Vec<char> = value.chars().collect();
    let mut decoded = Vec::with_capacity(characters.len());
    let mut position = 0;
    while position < characters.len() {
        if characters[position] != '\\' {
            decoded.push(QuotedCharacter::Character(characters[position]));
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
            decoded.push(QuotedCharacter::Character(character));
            continue;
        }

        let radix = match escape {
            'x' => Some(16),
            'u' => Some(16),
            'U' => Some(16),
            _ => None,
        };
        if let Some(radix) = radix {
            let width = match escape {
                'x' => characters[position..]
                    .iter()
                    .take_while(|character| character.is_ascii_hexdigit())
                    .count(),
                'u' => 4,
                'U' => 8,
                _ => unreachable!(),
            };
            let end = position + width;
            let Some(digits) = characters.get(position..end) else {
                return Err("invalid string literal");
            };
            if width == 0 {
                return Err("invalid string literal");
            }
            let digits: String = digits.iter().collect();
            let value =
                u32::from_str_radix(&digits, radix).map_err(|_| "invalid string literal")?;
            if escape == 'x' {
                decoded.push(QuotedCharacter::Byte(
                    u8::try_from(value).map_err(|_| "invalid string literal")?,
                ));
            } else {
                decoded.push(QuotedCharacter::Character(
                    char::from_u32(value).ok_or("invalid string literal")?,
                ));
            }
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
            decoded.push(QuotedCharacter::Byte(
                u8::try_from(value).map_err(|_| "invalid string literal")?,
            ));
            continue;
        }

        decoded.push(QuotedCharacter::Character('\\'));
        decoded.push(QuotedCharacter::Character(escape));
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
    let mut macros = HashMap::new();
    let mut macro_snapshots: HashMap<(String, usize), HashMap<String, MacroDefinition>> =
        HashMap::new();
    let filtered_source =
        preprocess_source(source, filename, &mut macros, &mut macro_snapshots, 0)?;
    let tokens = lex_unexpanded(&filtered_source, filename)?;
    let mut expanded_tokens = Vec::new();
    let mut position = 0;
    while position < tokens.len() {
        let token = &tokens[position];
        if token.value == "<eof>" {
            expanded_tokens.push(token.clone());
            break;
        }
        let key = (
            token
                .filename
                .clone()
                .unwrap_or_else(|| filename.to_owned()),
            token.line,
        );
        let line_macros = macro_snapshots.get(&key);
        let mut end = position + 1;
        while end < tokens.len() && tokens[end].value != "<eof>" {
            let next_key = (
                tokens[end]
                    .filename
                    .clone()
                    .unwrap_or_else(|| filename.to_owned()),
                tokens[end].line,
            );
            let next_macros = macro_snapshots.get(&next_key);
            let snapshots_match = match (line_macros, next_macros) {
                (Some(left), Some(right)) => left == right,
                (None, None) => true,
                _ => false,
            };
            if !snapshots_match {
                break;
            }
            end += 1;
        }
        let line_tokens = &tokens[position..end];
        if let Some(line_macros) = line_macros {
            expanded_tokens.extend(expand_object_macros(
                line_tokens,
                line_macros,
                &mut Vec::new(),
            )?);
        } else {
            expanded_tokens.extend_from_slice(line_tokens);
        }
        position = end;
    }
    Ok(expanded_tokens)
}

fn preprocess_source(
    source: &str,
    filename: &str,
    macros: &mut HashMap<String, MacroDefinition>,
    macro_snapshots: &mut HashMap<(String, usize), HashMap<String, MacroDefinition>>,
    depth: usize,
) -> Result<String, CCompilerError> {
    if depth > 64 {
        return Err(error(
            filename,
            1,
            "include nesting exceeds the supported limit",
        ));
    }
    let mut filtered_source = String::with_capacity(source.len());
    let mut conditionals = Vec::new();
    let mut active = true;
    let mut in_block_comment = false;
    let physical_lines: Vec<_> = source.split_inclusive('\n').collect();
    let mut physical_index = 0;
    while physical_index < physical_lines.len() {
        let line_number = physical_index + 1;
        let mut logical_line = String::new();
        let mut output_newlines = 0;
        loop {
            let physical_line = physical_lines[physical_index];
            physical_index += 1;
            let has_newline = physical_line.ends_with('\n');
            let content = physical_line.strip_suffix('\n').unwrap_or(physical_line);
            let continuation = if has_newline {
                content
                    .strip_suffix("\\\r")
                    .or_else(|| content.strip_suffix('\\'))
            } else {
                None
            };
            if let Some(content) = continuation {
                logical_line.push_str(content);
                output_newlines += 1;
                if physical_index < physical_lines.len() {
                    continue;
                }
                break;
            }
            logical_line.push_str(content);
            output_newlines += usize::from(has_newline);
            break;
        }
        let line_without_newline = logical_line.as_str();
        let trimmed = line_without_newline.trim_start();
        if !in_block_comment && trimmed.starts_with('#') {
            let directive = trimmed[1..].trim_start();
            let directive_end = directive
                .find(char::is_whitespace)
                .unwrap_or(directive.len());
            let directive_name = &directive[..directive_end];
            let argument = directive[directive_end..].trim_start();
            match directive_name {
                "ifdef" | "ifndef" => {
                    let Some((name, trailing)) = split_preprocessor_identifier(argument) else {
                        return Err(error(filename, line_number, "expected a macro name"));
                    };
                    if !trailing.trim().is_empty()
                        && !trailing.trim_start().starts_with("//")
                        && !trailing.trim_start().starts_with("/*")
                    {
                        return Err(error(
                            filename,
                            line_number,
                            "unexpected tokens after conditional macro name",
                        ));
                    }
                    let mut branch_active = macros.contains_key(name);
                    if directive_name == "ifndef" {
                        branch_active = !branch_active;
                    }
                    conditionals.push((active, branch_active, false, line_number));
                    active &= branch_active;
                }
                "if" => {
                    let branch_active = if active {
                        evaluate_preprocessor_condition(argument, macros, filename, line_number)?
                    } else {
                        false
                    };
                    conditionals.push((active, branch_active, false, line_number));
                    active &= branch_active;
                }
                "elif" => {
                    let Some((parent_active, branch_taken, else_seen, _)) = conditionals.last_mut()
                    else {
                        return Err(error(filename, line_number, "#elif without matching #if"));
                    };
                    if *else_seen {
                        return Err(error(filename, line_number, "#elif after #else"));
                    }
                    let branch_active = if *parent_active && !*branch_taken {
                        evaluate_preprocessor_condition(argument, macros, filename, line_number)?
                    } else {
                        false
                    };
                    active = branch_active;
                    *branch_taken |= branch_active;
                }
                "else" => {
                    if !argument.trim().is_empty()
                        && !argument.trim_start().starts_with("//")
                        && !argument.trim_start().starts_with("/*")
                    {
                        return Err(error(
                            filename,
                            line_number,
                            "unexpected tokens after #else",
                        ));
                    }
                    let Some((parent_active, branch_taken, else_seen, _)) = conditionals.last_mut()
                    else {
                        return Err(error(filename, line_number, "#else without matching #if"));
                    };
                    if *else_seen {
                        return Err(error(filename, line_number, "duplicate #else"));
                    }
                    active = *parent_active && !*branch_taken;
                    *branch_taken = true;
                    *else_seen = true;
                }
                "endif" => {
                    if !argument.trim().is_empty()
                        && !argument.trim_start().starts_with("//")
                        && !argument.trim_start().starts_with("/*")
                    {
                        return Err(error(
                            filename,
                            line_number,
                            "unexpected tokens after #endif",
                        ));
                    }
                    let Some((parent_active, _, _, _)) = conditionals.pop() else {
                        return Err(error(filename, line_number, "#endif without matching #if"));
                    };
                    active = parent_active;
                }
                "define" if active => {
                    let definition = argument;
                    let name_end = definition
                        .find(|character: char| character.is_whitespace() || character == '(')
                        .unwrap_or(definition.len());
                    let name = &definition[..name_end];
                    if !is_preprocessor_identifier(name) {
                        return Err(error(filename, line_number, "expected a macro name"));
                    }
                    let tail = &definition[name_end..];
                    let (parameters, replacement) = if let Some(tail) = tail.strip_prefix('(') {
                        let Some(close) = tail.find(')') else {
                            return Err(error(
                                filename,
                                line_number,
                                "unterminated function-like macro parameter list",
                            ));
                        };
                        let parameter_text = &tail[..close];
                        let mut parameters = Vec::new();
                        if !parameter_text.trim().is_empty() {
                            for parameter in parameter_text.split(',') {
                                let parameter = parameter.trim();
                                if !is_preprocessor_identifier(parameter)
                                    || parameters.iter().any(|existing| existing == parameter)
                                {
                                    return Err(error(
                                        filename,
                                        line_number,
                                        "invalid function-like macro parameter list",
                                    ));
                                }
                                parameters.push(parameter.to_owned());
                            }
                        }
                        (Some(parameters), tail[close + 1..].trim_start())
                    } else {
                        (None, tail.trim_start())
                    };
                    let mut tokens = lex_unexpanded(replacement, filename)?;
                    tokens.pop();
                    let definition = MacroDefinition {
                        parameters,
                        replacement: tokens,
                    };
                    if macros
                        .get(name)
                        .is_some_and(|previous| !macros_equivalent(previous, &definition))
                    {
                        return Err(error(
                            filename,
                            line_number,
                            &format!("incompatible redefinition of macro {name}"),
                        ));
                    }
                    macros.entry(name.to_owned()).or_insert(definition);
                }
                "undef" if active => {
                    let Some((name, trailing)) = split_preprocessor_identifier(argument) else {
                        return Err(error(filename, line_number, "expected a macro name"));
                    };
                    if !trailing.trim().is_empty()
                        && !trailing.trim_start().starts_with("//")
                        && !trailing.trim_start().starts_with("/*")
                    {
                        return Err(error(
                            filename,
                            line_number,
                            "unexpected tokens after macro name",
                        ));
                    }
                    macros.remove(name);
                }
                "include" if active => {
                    let (target, quoted) =
                        parse_include_target(argument, macros, filename, line_number)?;
                    let included_path = resolve_include_path(&target, quoted, filename)
                        .ok_or_else(|| {
                            error(
                                filename,
                                line_number,
                                &format!("cannot find include file {target:?}"),
                            )
                        })?;
                    let included = fs::read_to_string(&included_path).map_err(|io_error| {
                        error(
                            filename,
                            line_number,
                            &format!("cannot read include file {target:?}: {io_error}"),
                        )
                    })?;
                    let included_filename = included_path.to_string_lossy();
                    filtered_source.push_str(&format!(
                        "\0SC16_LINE:1:{}\n",
                        hex_encode(included_filename.as_bytes())
                    ));
                    filtered_source.push_str(&preprocess_source(
                        included.strip_prefix('\u{feff}').unwrap_or(&included),
                        &included_filename,
                        macros,
                        macro_snapshots,
                        depth + 1,
                    )?);
                    filtered_source.push_str(&format!(
                        "\n\0SC16_LINE:{}:{}",
                        line_number + 1,
                        hex_encode(filename.as_bytes())
                    ));
                }
                "define" | "undef" => {}
                _ if !active => {}
                _ => {
                    let message =
                        format!("#{directive_name} is not supported by the Rust C preprocessor");
                    return Err(error(filename, line_number, &message));
                }
            }
            for _ in 0..output_newlines {
                filtered_source.push('\n');
            }
        } else {
            if active {
                macro_snapshots.insert((filename.to_owned(), line_number), macros.clone());
                filtered_source.push_str(line_without_newline);
            }
            for _ in 0..output_newlines {
                filtered_source.push('\n');
            }
        }
        update_block_comment_state(line_without_newline, &mut in_block_comment);
    }
    if let Some((_, _, _, opening_line)) = conditionals.last() {
        return Err(error(
            filename,
            *opening_line,
            "unterminated preprocessor conditional",
        ));
    }
    Ok(filtered_source)
}

fn split_preprocessor_identifier(input: &str) -> Option<(&str, &str)> {
    let mut characters = input.char_indices();
    let (_, first) = characters.next()?;
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return None;
    }
    let end = characters
        .find_map(|(index, character)| {
            (!(character == '_' || character.is_ascii_alphanumeric())).then_some(index)
        })
        .unwrap_or(input.len());
    Some((&input[..end], &input[end..]))
}

fn is_preprocessor_identifier(value: &str) -> bool {
    let Some((name, trailing)) = split_preprocessor_identifier(value) else {
        return false;
    };
    trailing.is_empty() && name == value
}

fn macros_equivalent(left: &MacroDefinition, right: &MacroDefinition) -> bool {
    left.parameters == right.parameters
        && left.replacement.len() == right.replacement.len()
        && left
            .replacement
            .iter()
            .zip(&right.replacement)
            .all(|(left, right)| left.value == right.value)
}

fn parse_include_target(
    argument: &str,
    macros: &HashMap<String, MacroDefinition>,
    filename: &str,
    line: usize,
) -> Result<(String, bool), CCompilerError> {
    let mut tokens = lex_unexpanded(argument, filename)?;
    tokens.pop();
    let expanded = expand_object_macros(&tokens, macros, &mut Vec::new())?;
    if expanded.len() == 1
        && let Some(hex) = expanded[0]
            .value
            .strip_prefix("<string:")
            .and_then(|value| value.strip_suffix('>'))
    {
        if hex.len() % 2 != 0 {
            return Err(error(filename, line, "invalid #include filename"));
        }
        let mut bytes = Vec::with_capacity(hex.len() / 2);
        for pair in hex.as_bytes().chunks(2) {
            let pair = std::str::from_utf8(pair)
                .map_err(|_| error(filename, line, "invalid #include filename"))?;
            bytes.push(
                u8::from_str_radix(pair, 16)
                    .map_err(|_| error(filename, line, "invalid #include filename"))?,
            );
        }
        let target = String::from_utf8(bytes)
            .map_err(|_| error(filename, line, "#include filename must be valid UTF-8"))?;
        return Ok((target, true));
    }
    if expanded.first().is_some_and(|token| token.value == "<")
        && expanded.last().is_some_and(|token| token.value == ">")
        && expanded.len() > 2
    {
        let target = expanded[1..expanded.len() - 1]
            .iter()
            .map(|token| token.value.as_str())
            .collect::<String>();
        return Ok((target, false));
    }
    Err(error(filename, line, "invalid #include"))
}

fn resolve_include_path(target: &str, quoted: bool, filename: &str) -> Option<PathBuf> {
    let target_path = Path::new(target);
    let mut candidates = Vec::new();
    if target_path.is_absolute() {
        candidates.push(target_path.to_path_buf());
    } else {
        if quoted {
            let source_parent = Path::new(filename)
                .parent()
                .unwrap_or_else(|| Path::new("."));
            candidates.push(source_parent.join(target_path));
        }
        let manifest_directory = Path::new(env!("CARGO_MANIFEST_DIR"));
        candidates.push(manifest_directory.join("lib").join(target_path));
        if let Some(repository_directory) = manifest_directory.parent() {
            candidates.push(repository_directory.join("lib").join(target_path));
        }
    }
    candidates.into_iter().find_map(|candidate| {
        candidate
            .is_file()
            .then(|| candidate.canonicalize().ok())
            .flatten()
    })
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn evaluate_preprocessor_condition(
    source: &str,
    macros: &HashMap<String, MacroDefinition>,
    filename: &str,
    line: usize,
) -> Result<bool, CCompilerError> {
    let mut tokens = lex_unexpanded(source, filename)?;
    tokens.pop();
    let mut defined_tokens = Vec::new();
    let mut position = 0;
    while position < tokens.len() {
        if tokens[position].value != "defined" {
            defined_tokens.push(tokens[position].clone());
            position += 1;
            continue;
        }
        let directive_token = &tokens[position];
        position += 1;
        let parenthesized = tokens.get(position).is_some_and(|token| token.value == "(");
        if parenthesized {
            position += 1;
        }
        let Some(name) = tokens.get(position) else {
            return Err(error(filename, line, "expected a macro name after defined"));
        };
        if !is_preprocessor_identifier(&name.value) {
            return Err(error(filename, line, "expected a macro name after defined"));
        }
        let is_defined = macros.contains_key(&name.value);
        position += 1;
        if parenthesized {
            if !tokens.get(position).is_some_and(|token| token.value == ")") {
                return Err(error(
                    filename,
                    line,
                    "expected closing parenthesis after defined",
                ));
            }
            position += 1;
        }
        defined_tokens.push(Token {
            value: i32::from(is_defined).to_string(),
            line: directive_token.line,
            filename: directive_token.filename.clone(),
        });
    }
    let mut expanded = expand_object_macros(&defined_tokens, macros, &mut Vec::new())?;
    for token in &mut expanded {
        if is_preprocessor_identifier(&token.value) {
            token.value = "0".to_owned();
        }
        token.line = line;
        token.filename = Some(filename.to_owned());
    }
    expanded.push(Token {
        value: "<eof>".to_owned(),
        line,
        filename: Some(filename.to_owned()),
    });
    let mut parser = ExpressionParser::new(expanded);
    let expression = parser.parse_expression(0).map_err(|_| {
        error(
            filename,
            line,
            "invalid integer constant expression in preprocessor condition",
        )
    })?;
    if !parser.is_at_end() {
        return Err(error(
            filename,
            line,
            "invalid integer constant expression in preprocessor condition",
        ));
    }
    let value =
        evaluate_integer_constant_with_names(&expression, &HashMap::new()).map_err(|_| {
            error(
                filename,
                line,
                "invalid integer constant expression in preprocessor condition",
            )
        })?;
    Ok(value != 0)
}

fn update_block_comment_state(line: &str, in_block_comment: &mut bool) {
    let mut position = 0;
    let mut quote = None;
    let mut escaped = false;
    while position < line.len() {
        let remaining = &line[position..];
        let character = remaining.chars().next().expect("valid character boundary");
        if *in_block_comment {
            if remaining.starts_with("*/") {
                *in_block_comment = false;
                position += 2;
            } else {
                position += character.len_utf8();
            }
        } else if let Some(quoted) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quoted {
                quote = None;
            }
            position += character.len_utf8();
        } else if remaining.starts_with("//") {
            break;
        } else if remaining.starts_with("/*") {
            *in_block_comment = true;
            position += 2;
        } else {
            if matches!(character, '"' | '\'') {
                quote = Some(character);
            }
            position += character.len_utf8();
        }
    }
}

fn expand_object_macros(
    tokens: &[Token],
    macros: &HashMap<String, MacroDefinition>,
    expansion_stack: &mut Vec<String>,
) -> Result<Vec<Token>, CCompilerError> {
    let mut expanded = Vec::new();
    let mut position = 0;
    while position < tokens.len() {
        let token = &tokens[position];
        let Some(definition) = macros.get(&token.value) else {
            expanded.push(token.clone());
            position += 1;
            continue;
        };
        if expansion_stack.contains(&token.value) {
            expanded.push(token.clone());
            position += 1;
            continue;
        }
        if expansion_stack.len() >= 128 {
            return Err(error(
                token.filename.as_deref().unwrap_or("<source>"),
                token.line,
                "macro expansion exceeds the supported nesting limit",
            ));
        }
        let mut raw_arguments = HashMap::new();
        let mut invocation_end = position;
        if let Some(parameters) = &definition.parameters {
            if !tokens
                .get(position + 1)
                .is_some_and(|next| next.value == "(")
            {
                expanded.push(token.clone());
                position += 1;
                continue;
            }
            let mut arguments = vec![Vec::new()];
            let mut nesting = 0usize;
            let mut cursor = position + 2;
            while cursor < tokens.len() {
                match tokens[cursor].value.as_str() {
                    "(" => {
                        nesting += 1;
                        arguments
                            .last_mut()
                            .expect("argument exists")
                            .push(tokens[cursor].clone());
                    }
                    ")" if nesting == 0 => break,
                    ")" => {
                        nesting -= 1;
                        arguments
                            .last_mut()
                            .expect("argument exists")
                            .push(tokens[cursor].clone());
                    }
                    "," if nesting == 0 => arguments.push(Vec::new()),
                    _ => arguments
                        .last_mut()
                        .expect("argument exists")
                        .push(tokens[cursor].clone()),
                }
                cursor += 1;
            }
            if cursor == tokens.len() {
                return Err(error(
                    token.filename.as_deref().unwrap_or("<source>"),
                    token.line,
                    "unterminated function-like macro invocation",
                ));
            }
            if parameters.is_empty() && arguments.len() == 1 && arguments[0].is_empty() {
                arguments.clear();
            }
            if arguments.len() != parameters.len() {
                return Err(error(
                    token.filename.as_deref().unwrap_or("<source>"),
                    token.line,
                    &format!(
                        "macro {} expects {} argument(s), got {}",
                        token.value,
                        parameters.len(),
                        arguments.len()
                    ),
                ));
            }
            raw_arguments.extend(parameters.iter().cloned().zip(arguments));
            invocation_end = cursor;
        }
        expansion_stack.push(token.value.clone());
        let mut substituted = Vec::new();
        let mut replacement_index = 0;
        while replacement_index < definition.replacement.len() {
            let replacement_token = &definition.replacement[replacement_index];
            if replacement_token.value == "#"
                && let Some(parameter) = definition.replacement.get(replacement_index + 1)
                && let Some(argument) = raw_arguments.get(&parameter.value)
            {
                let spelling = argument
                    .iter()
                    .map(token_spelling)
                    .collect::<Vec<_>>()
                    .join(" ");
                let escaped = spelling.replace('\\', "\\\\").replace('"', "\\\"");
                let mut string_tokens = lex_unexpanded(&format!("\"{escaped}\""), "<macro>")?;
                string_tokens.pop();
                for string_token in &mut string_tokens {
                    string_token.line = token.line;
                    string_token.filename = token.filename.clone();
                }
                substituted.extend(string_tokens);
                replacement_index += 2;
                continue;
            }
            if let Some(argument) = raw_arguments.get(&replacement_token.value) {
                let paste_adjacent = definition
                    .replacement
                    .get(replacement_index.wrapping_sub(1))
                    .is_some_and(|previous| previous.value == "##")
                    || definition
                        .replacement
                        .get(replacement_index + 1)
                        .is_some_and(|next| next.value == "##");
                let mut argument = if paste_adjacent {
                    argument.clone()
                } else {
                    expand_object_macros(argument, macros, expansion_stack)?
                };
                for argument_token in &mut argument {
                    argument_token.line = token.line;
                    argument_token.filename = token.filename.clone();
                }
                substituted.extend(argument);
            } else {
                substituted.push(Token {
                    value: replacement_token.value.clone(),
                    line: token.line,
                    filename: token.filename.clone(),
                });
            }
            replacement_index += 1;
        }
        let mut rescanned = expand_object_macros(
            &paste_macro_tokens(substituted, token)?,
            macros,
            expansion_stack,
        )?;
        expansion_stack.pop();
        let suffix = &tokens[invocation_end + 1..];
        if let Some(last) = rescanned.last()
            && last.value != token.value
            && macros
                .get(&last.value)
                .is_some_and(|macro_definition| macro_definition.parameters.is_some())
            && suffix.first().is_some_and(|next| next.value == "(")
        {
            let mut bridge = vec![rescanned.pop().expect("last token exists")];
            bridge.extend_from_slice(suffix);
            expanded.extend(expand_object_macros(&bridge, macros, expansion_stack)?);
            return Ok(expanded);
        }
        expanded.extend(rescanned);
        position = invocation_end + 1;
    }
    Ok(expanded)
}

fn token_spelling(token: &Token) -> String {
    if let Some(hex) = token
        .value
        .strip_prefix("<string:")
        .and_then(|value| value.strip_suffix('>'))
        && hex.len() % 2 == 0
    {
        let bytes = hex
            .as_bytes()
            .chunks(2)
            .filter_map(|pair| std::str::from_utf8(pair).ok())
            .filter_map(|pair| u8::from_str_radix(pair, 16).ok())
            .collect::<Vec<_>>();
        if let Ok(string) = String::from_utf8(bytes) {
            return format!("{string:?}");
        }
    }
    token.value.clone()
}

fn paste_macro_tokens(
    tokens: Vec<Token>,
    invocation: &Token,
) -> Result<Vec<Token>, CCompilerError> {
    let mut pasted = Vec::new();
    let mut position = 0;
    while position < tokens.len() {
        if tokens[position].value != "##" {
            pasted.push(tokens[position].clone());
            position += 1;
            continue;
        }
        if pasted.is_empty() || position + 1 >= tokens.len() {
            return Err(error(
                invocation.filename.as_deref().unwrap_or("<source>"),
                invocation.line,
                "invalid ## in macro replacement",
            ));
        }
        let left = pasted.pop().expect("checked non-empty");
        let spelling = format!(
            "{}{}",
            token_spelling(&left),
            token_spelling(&tokens[position + 1])
        );
        let mut joined = lex_unexpanded(&spelling, "<macro>")?;
        joined.pop();
        if joined.len() != 1 {
            return Err(error(
                invocation.filename.as_deref().unwrap_or("<source>"),
                invocation.line,
                "## does not form one token",
            ));
        }
        let mut joined = joined.pop().expect("one token");
        joined.line = invocation.line;
        joined.filename = invocation.filename.clone();
        pasted.push(joined);
        position += 2;
    }
    Ok(pasted)
}

fn lex_unexpanded(source: &str, filename: &str) -> Result<Vec<Token>, CCompilerError> {
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
                        let mut bytes = Vec::new();
                        for character in decoded {
                            match character {
                                QuotedCharacter::Character(character) => {
                                    let mut encoded = [0; 4];
                                    bytes.extend_from_slice(
                                        character.encode_utf8(&mut encoded).as_bytes(),
                                    );
                                }
                                QuotedCharacter::Byte(byte) => bytes.push(byte),
                            }
                        }
                        format!(
                            "<string:{}>",
                            bytes
                                .iter()
                                .map(|byte| format!("{byte:02x}"))
                                .collect::<String>()
                        )
                    } else if decoded.len() == 1 {
                        match decoded[0] {
                            QuotedCharacter::Character(character) => (character as u32).to_string(),
                            QuotedCharacter::Byte(byte) => byte.to_string(),
                        }
                    } else {
                        return Err(error(
                            &token_filename,
                            start_line,
                            "character constants must contain one character",
                        ));
                    };
                    if quote == '"'
                        && let Some(previous) = tokens.last_mut()
                        && previous.value.starts_with("<string:")
                        && previous.value.ends_with('>')
                    {
                        previous.value.pop();
                        previous
                            .value
                            .push_str(value.strip_prefix("<string:").unwrap_or_default());
                        closed = true;
                        break;
                    }
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
    fn expands_nested_object_like_macros_and_preserves_invocation_locations() {
        let tokens = lex_raw(
            "#define BASE 4\n#define TOTAL (BASE + 3)\nint result = TOTAL;\n",
            "macros.c",
        )
        .unwrap();
        let result = tokens
            .iter()
            .find(|token| token.value == "4")
            .expect("expanded base constant");
        assert_eq!(result.line, 3);
        assert_eq!(result.filename.as_deref(), Some("macros.c"));
        assert_eq!(
            tokens
                .iter()
                .map(|token| token.value.as_str())
                .collect::<Vec<_>>(),
            ["int", "result", "=", "(", "4", "+", "3", ")", ";", "<eof>"]
        );
    }

    #[test]
    fn undef_removes_object_like_macros_and_strings_are_not_expanded() {
        assert_eq!(
            values("#define TEXT token\nconst char *text = \"TEXT\";\n#undef TEXT\nTEXT"),
            [
                "const",
                "char",
                "*",
                "text",
                "=",
                "<string:54455854>",
                ";",
                "TEXT",
                "<eof>"
            ]
        );
    }

    #[test]
    fn macro_expansion_uses_the_definitions_active_at_each_source_line() {
        assert_eq!(
            values(
                "#define VALUE 1\nint first = VALUE;\n#undef VALUE\n\
                 #define VALUE 2\nint second = VALUE;\n"
            ),
            [
                "int", "first", "=", "1", ";", "int", "second", "=", "2", ";", "<eof>"
            ]
        );
    }

    #[test]
    fn splices_physical_lines_before_preprocessing_and_tokenizing() {
        assert_eq!(
            values("#define VALUE 2 + \\\n3\nint main(void) { ret\\\nurn VALUE; }"),
            [
                "int", "main", "(", "void", ")", "{", "return", "2", "+", "3", ";", "}", "<eof>"
            ]
        );
        let tokens = lex_raw("first\\\nsecond\nthird", "continued.c").unwrap();
        assert_eq!(tokens[0].value, "firstsecond");
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[1].value, "third");
        assert_eq!(tokens[1].line, 3);
    }

    #[test]
    fn expands_function_like_macros_with_nested_and_prescanned_arguments() {
        assert_eq!(
            values(
                "#define VALUE 4\n#define ADD(left, right) ((left) + (right))\n\
                 ADD(VALUE, (2 + 1))"
            ),
            [
                "(", "(", "4", ")", "+", "(", "(", "2", "+", "1", ")", ")", ")", "<eof>"
            ]
        );
    }

    #[test]
    fn rescans_function_macro_invocations_across_lines_and_object_aliases() {
        assert_eq!(
            values(
                "#define ID(value) value\n#define ALIAS ID\n\
                 ALIAS\n(9)"
            ),
            ["9", "<eof>"]
        );
    }

    #[test]
    fn expands_macro_stringification_and_token_pasting() {
        let tokens = values(
            "#define TEXT(value) #value\n#define JOIN(left, right) left ## right\n\
             TEXT(hello world) JOIN(item, 7) JOIN(5, U)",
        );
        assert_eq!(
            tokens,
            ["<string:68656c6c6f20776f726c64>", "item7", "5U", "<eof>"]
        );
    }

    #[test]
    fn reports_function_macro_argument_count_errors() {
        assert_eq!(
            lex_raw(
                "#define PAIR(left, right) left + right\nPAIR(1)\n",
                "macro.c"
            )
            .unwrap_err()
            .to_string(),
            "macro.c:2: macro PAIR expects 2 argument(s), got 1"
        );
    }

    #[test]
    fn reports_unsupported_preprocessor_directives_explicitly() {
        assert_eq!(
            lex_raw("#pragma once\n", "include.c")
                .unwrap_err()
                .to_string(),
            "include.c:1: #pragma is not supported by the Rust C preprocessor"
        );
        assert_eq!(
            lex_raw("#define VALUE 1\n#define VALUE 2\n", "macro.c")
                .unwrap_err()
                .to_string(),
            "macro.c:2: incompatible redefinition of macro VALUE"
        );
    }

    #[test]
    fn ignores_directive_text_inside_block_comments() {
        assert_eq!(
            values("/*\n#define HIDDEN 4\n*/\nHIDDEN"),
            ["HIDDEN", "<eof>"]
        );
    }

    #[test]
    fn resolves_nested_includes_with_shared_include_guards_and_locations() {
        let directory =
            std::env::temp_dir().join(format!("sc16-c-includes-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let root = directory.join("root.c");
        let header = directory.join("values.h");
        let nested = directory.join("nested.h");
        fs::write(
            &header,
            "#ifndef VALUES_H\n#define VALUES_H\n#include \"nested.h\"\n#define ANSWER (BASE + 1)\n#endif\n",
        )
        .unwrap();
        fs::write(&nested, "#define BASE 40\n").unwrap();
        let source = "#include \"values.h\"\n#include \"values.h\"\nint result = ANSWER;\n";
        let tokens = lex_raw(source, &root.to_string_lossy()).unwrap();
        let answer = tokens
            .iter()
            .find(|token| token.value == "40")
            .expect("included nested macro");
        assert_eq!(
            answer.filename.as_deref(),
            Some(root.to_string_lossy().as_ref())
        );
        let result = tokens
            .iter()
            .find(|token| token.value == "result")
            .expect("root source token");
        assert_eq!(
            result.filename.as_deref(),
            Some(root.to_string_lossy().as_ref())
        );
        assert_eq!(result.line, 3);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn compiles_local_header_includes_natively() {
        let directory =
            std::env::temp_dir().join(format!("sc16-c-include-build-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let header = directory.join("answer.h");
        fs::write(&header, "#define ANSWER 42\n").unwrap();
        let image = compile_minimal_program(
            "#include \"answer.h\"\nint main(void) { return ANSWER; }\n",
            &directory.join("main.c").to_string_lossy(),
        )
        .unwrap()
        .expect("supported C source with local header");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(128).unwrap();
        assert_eq!(cpu.registers[0], 42);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn resolves_angle_includes_from_the_repository_library() {
        let image = compile_minimal_program(
            "#include <limits.h>\nint main(void) { return INT_MAX; }\n",
            "angle-include.c",
        )
        .unwrap()
        .expect("supported repository library include");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(128).unwrap();
        assert_eq!(cpu.registers[0], 32767);
    }

    #[test]
    fn expands_function_macros_from_the_integer_standard_header() {
        let image = compile_minimal_program(
            "#include <stdint.h>\n\
             int main(void) { return UINT8_C(7) + UINT16_C(9); }\n",
            "stdint-include.c",
        )
        .unwrap()
        .expect("supported fixed-width integer header macros");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(128).unwrap();
        assert_eq!(cpu.registers[0], 16);
    }

    #[test]
    fn reports_missing_local_header_with_source_location() {
        let directory =
            std::env::temp_dir().join(format!("sc16-c-missing-include-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let result = compile_minimal_program(
            "#include \"missing.h\"\nint main(void) { return 0; }\n",
            &directory.join("main.c").to_string_lossy(),
        );
        assert_eq!(
            result.unwrap_err().to_string(),
            format!(
                "{}:1: cannot find include file \"missing.h\"",
                directory.join("main.c").display()
            )
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn processes_nested_defined_conditionals_and_skips_inactive_directives() {
        let tokens = lex_raw(
            "#define SELECTED 9\n\
             #ifdef SELECTED\n\
             #if defined(SELECTED) && SELECTED * 2 == 18\n\
             int value = SELECTED;\n\
             #elif defined(MISSING)\n\
             #include <inactive.h>\n\
             #else\n\
             #include <inactive.h>\n\
             #endif\n\
             #else\n\
             int wrong = 0;\n\
             #endif\n",
            "conditional.c",
        )
        .unwrap();
        assert_eq!(
            tokens
                .iter()
                .map(|token| token.value.as_str())
                .collect::<Vec<_>>(),
            ["int", "value", "=", "9", ";", "<eof>"]
        );
    }

    #[test]
    fn evaluates_if_elif_integer_constant_expressions() {
        let tokens = lex_raw(
            "#define CHOICE (2 + 3)\n\
             #if CHOICE < 0\nint wrong = 0;\n\
             #elif CHOICE == 5 && !defined(UNKNOWN)\nint selected = CHOICE;\n\
             #elif 1\nint wrong_again = 0;\n\
             #else\nint final_wrong = 0;\n\
             #endif\n",
            "condition-expression.c",
        )
        .unwrap();
        assert_eq!(
            tokens
                .iter()
                .map(|token| token.value.as_str())
                .collect::<Vec<_>>(),
            [
                "int", "selected", "=", "(", "2", "+", "3", ")", ";", "<eof>"
            ]
        );
    }

    #[test]
    fn rejects_malformed_preprocessor_conditionals() {
        assert_eq!(
            lex_raw("#else\n", "conditional.c").unwrap_err().to_string(),
            "conditional.c:1: #else without matching #if"
        );
        assert_eq!(
            lex_raw("#ifdef FEATURE\n", "conditional.c")
                .unwrap_err()
                .to_string(),
            "conditional.c:1: unterminated preprocessor conditional"
        );
        assert_eq!(
            lex_raw("#if 1\n#else\n#elif 0\n#endif\n", "conditional.c")
                .unwrap_err()
                .to_string(),
            "conditional.c:3: #elif after #else"
        );
        assert_eq!(
            lex_raw("#if 1 / 0\n#endif\n", "conditional.c")
                .unwrap_err()
                .to_string(),
            "conditional.c:1: invalid integer constant expression in preprocessor condition"
        );
        assert_eq!(
            lex_raw("#elif 1\n", "conditional.c")
                .unwrap_err()
                .to_string(),
            "conditional.c:1: #elif without matching #if"
        );
    }

    #[test]
    fn encodes_string_and_character_literals_as_compiler_tokens() {
        assert_eq!(
            values(r#""A\n\u03bb" "B" '\x41' '\101' item.field"#),
            [
                "<string:410acebb42>",
                "65",
                "65",
                "item",
                ".",
                "field",
                "<eof>"
            ]
        );
        assert_eq!(
            values(r#""\xff\101\u03bb""#),
            ["<string:ff41cebb>", "<eof>"]
        );
        assert_eq!(values(".5 1. 1e-2"), [".5", "1.", "1e-2", "<eof>"]);
        assert_eq!(
            values("city[i].ladder"),
            ["city", "[", "i", "]", ".", "ladder", "<eof>"]
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
                                array_dimensions: Vec::new(),
                                initializer: Some(located_atom("3", 1, "statement.c")),
                            },
                            Declaration {
                                c_type: "unsigned int*".to_owned(),
                                name: "pointer".to_owned(),
                                array_length: None,
                                array_dimensions: Vec::new(),
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
                        array_dimensions: Vec::new(),
                        initializer: Some(located_atom("3", 1, "unit.c")),
                    }),
                    ExternalDeclaration::Global(Declaration {
                        c_type: "char*".to_owned(),
                        name: "message".to_owned(),
                        array_length: None,
                        array_dimensions: Vec::new(),
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
                enumerators: HashMap::new(),
            }
        );
    }

    #[test]
    fn parses_enumerations_and_integer_constant_expressions() {
        let unit = translation_unit(
            "enum Mode { MODE_ZERO = 2, MODE_ONE, MODE_THREE = MODE_ONE + 4 }; \
             enum Mode current;",
        );
        assert_eq!(
            unit.enumerators,
            HashMap::from([
                ("MODE_ZERO".to_owned(), 2),
                ("MODE_ONE".to_owned(), 3),
                ("MODE_THREE".to_owned(), 7),
            ])
        );
        assert!(matches!(
            &unit.declarations[..],
            [ExternalDeclaration::Global(Declaration {
                c_type,
                name,
                ..
            })] if c_type == "int" && name == "current"
        ));
    }

    #[test]
    fn parses_scalar_typedef_aliases() {
        let unit = translation_unit(
            "typedef int word; typedef word count; \
             count identity(word value) { count result = value; return result; }",
        );
        assert!(matches!(
            &unit.declarations[..],
            [ExternalDeclaration::FunctionDefinition(function)]
                if function.declaration.return_type == "int"
                    && function.declaration.parameters[0].c_type == "int"
                    && function.declaration.name == "identity"
        ));
    }

    #[test]
    fn parses_struct_typedefs_and_record_typed_objects() {
        let unit = translation_unit(
            "typedef struct { int x; int y; unsigned char flags[4]; } Vec2; \
             typedef struct Building { Vec2 position; int inventory; } Building; \
             Building city[3]; \
             int main(void) { Building local; return 0; }",
        );
        assert!(matches!(
            &unit.declarations[..],
            [ExternalDeclaration::Global(Declaration {
                c_type,
                name,
                array_length: Some(3),
                ..
            }), ExternalDeclaration::FunctionDefinition(function)]
                if c_type == "struct Building"
                    && name == "city"
                    && function.declaration.name == "main"
        ));
        let ExternalDeclaration::FunctionDefinition(function) = &unit.declarations[1] else {
            panic!("expected main definition");
        };
        let StatementKind::Compound(statements) = &function.body.kind else {
            panic!("expected function body");
        };
        assert!(matches!(
            &statements[0].kind,
            StatementKind::Declaration(declarations)
                if declarations[0].c_type == "struct Building"
        ));
    }

    #[test]
    fn uses_file_scope_enumerators_in_local_array_bounds() {
        let unit = translation_unit(
            "enum Size { BUFFER_SIZE = 5 }; \
             int main(void) { int buffer[BUFFER_SIZE]; return 0; }",
        );
        let ExternalDeclaration::FunctionDefinition(main) = &unit.declarations[0] else {
            panic!("expected main definition");
        };
        let StatementKind::Compound(statements) = &main.body.kind else {
            panic!("expected main body");
        };
        assert!(matches!(
            &statements[0].kind,
            StatementKind::Declaration(declarations)
                if declarations[0].array_length == Some(5)
        ));
    }

    #[test]
    fn rejects_duplicate_enumerators() {
        let error = TranslationUnitParser::from_source(
            "enum First { SAME }; enum Second { SAME };",
            "duplicate-enum.c",
        )
        .unwrap()
        .parse()
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "duplicate-enum.c:1: duplicate enumerator \"SAME\""
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
    fn parses_multidimensional_array_declarators() {
        let unit = translation_unit(
            "unsigned char map[15][20]; int main(void) { int values[2][3]; return 0; }",
        );
        assert!(matches!(
            &unit.declarations[0],
            ExternalDeclaration::Global(Declaration {
                array_length: Some(300),
                array_dimensions,
                ..
            }) if array_dimensions == &[15, 20]
        ));
        let ExternalDeclaration::FunctionDefinition(main) = &unit.declarations[1] else {
            panic!("expected main definition");
        };
        let StatementKind::Compound(statements) = &main.body.kind else {
            panic!("expected main body");
        };
        assert!(matches!(
            &statements[0].kind,
            StatementKind::Declaration(declarations)
                if declarations[0].array_length == Some(6)
                    && declarations[0].array_dimensions == [2, 3]
        ));
    }

    #[test]
    fn infers_array_bounds_from_initializers() {
        let unit = translation_unit(
            r#"unsigned char bytes[] = { 1, 2, 3 }; char title[] = "SC";
               int main(void) { int local[] = { 4, 5 }; return 0; }"#,
        );
        assert!(matches!(
            &unit.declarations[..2],
            [
                ExternalDeclaration::Global(Declaration {
                    array_length: Some(3),
                    ..
                }),
                ExternalDeclaration::Global(Declaration {
                    array_length: Some(3),
                    ..
                })
            ]
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
                if declarations[0].array_length == Some(2)
        ));
    }

    #[test]
    fn rejects_incomplete_arrays_without_inferable_initializers() {
        for source in [
            "int values[]; int main(void) { return 0; }",
            "int main(void) { int values[]; return 0; }",
            "int values[] = 1; int main(void) { return 0; }",
        ] {
            assert!(
                TranslationUnitParser::from_source(source, "incomplete-array.c")
                    .unwrap()
                    .parse()
                    .is_err(),
                "source unexpectedly parsed: {source}"
            );
        }
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
    fn compiles_enum_constants_in_expressions_initializers_and_switches() {
        let image = compile_minimal_program(
            "enum Mode { MODE_ZERO = 2, MODE_ONE, MODE_THREE = MODE_ONE + 4 }; \
             int values[MODE_THREE] = { MODE_ZERO, MODE_ONE }; \
             int selected = MODE_THREE; \
             int main(void) { enum Mode choice = MODE_ONE; \
                 switch (choice) { case MODE_ZERO: return 9; \
                     case MODE_ONE: return values[1] + selected; \
                     default: return 10; } }",
            "enum.c",
        )
        .unwrap()
        .expect("supported enum program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 10);
        assert_eq!(cpu.read_u16(0x80000), 2);
        assert_eq!(cpu.read_u16(0x80002), 3);
        assert_eq!(cpu.read_u16(0x8000e), 7);
    }

    #[test]
    fn compiles_scalar_typedefs_in_globals_functions_and_type_names() {
        let image = compile_minimal_program(
            "typedef int word; typedef word count; \
             word global = 3; \
             count twice(word value) { count result = (word)value * 2; return result; } \
             int main(void) { word local = twice(global); return local + sizeof(count); }",
            "typedef.c",
        )
        .unwrap()
        .expect("supported scalar typedef program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 8);
        assert_eq!(cpu.read_u16(0x80000), 3);
    }

    #[test]
    fn compiles_object_like_macro_constants_without_python() {
        let image = compile_minimal_program(
            "#define OFFSET 5\n#define ENABLE_RESULT 1\n\
             #if defined(ENABLE_RESULT) && ENABLE_RESULT == 1\n#define RESULT (OFFSET * 2)\n\
             #else\n#error inactive branch should not be processed\n#endif\n\
             int main(void) { return RESULT; }\n",
            "macro.c",
        )
        .unwrap()
        .expect("supported object-macro program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(128).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 10);
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
    fn compiles_unsigned_word_globals_locals_and_function_parameters() {
        let image = compile_minimal_program(
            "unsigned int score = 23; \
             unsigned int identity(unsigned int value) { return value; } \
             int main(void) { unsigned int local; local = identity(score); return local; }",
            "unsigned-word.c",
        )
        .unwrap()
        .expect("supported unsigned-word program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 23);
        assert_eq!(cpu.read_u16(0x80000), 23);
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
    fn compiles_indexed_unsigned_char_global_arrays() {
        let image = compile_minimal_program(
            "unsigned char values[4] = { 1, 255, 3 }; \
             int main(void) { values[1] += 2; values[2]++; values[3] = values[1]; \
                 return sizeof(values) + values[0] + values[1] + values[2] + values[3]; }",
            "byte-array.c",
        )
        .unwrap()
        .expect("supported unsigned-char array program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 11);
        assert_eq!(&cpu.memory[0x80000..0x80004], &[1, 1, 4, 1]);
    }

    #[test]
    fn compiles_signed_char_global_array_loads_and_compound_assignments() {
        let image = compile_minimal_program(
            "char values[2] = { 255, 4 }; \
             int main(void) { values[0] /= 2; return values[0] + values[1]; }",
            "signed-byte-array.c",
        )
        .unwrap()
        .expect("supported signed-char array program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 4);
        assert_eq!(&cpu.memory[0x80000..0x80002], &[0, 4]);
    }

    #[test]
    fn compiles_global_and_local_character_string_initializers() {
        let image = compile_minimal_program(
            "char title[4] = \"Cat\"; \
             int main(void) { char local[4] = \"dog\"; \
                 return title[1] + local[2] + sizeof(title) + sizeof(local); }",
            "string-arrays.c",
        )
        .unwrap()
        .expect("supported character array string initializers");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(1024).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 208);
        assert_eq!(&cpu.memory[0x80000..0x80004], b"Cat\0");
    }

    #[test]
    fn compiles_escaped_exact_fit_and_zero_filled_string_arrays() {
        let image = compile_minimal_program(
            r#"char exact[2] = "\377A"; char padded[4] = "hi";
               int main(void) {
                   unsigned char local[2] = "\x80\0";
                   return exact[1] + local[0] + local[1] + padded[2]
                       + padded[3] + sizeof(local);
               }"#,
            "escaped-string-arrays.c",
        )
        .unwrap()
        .expect("supported escaped and exact-fit character arrays");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(1024).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 195);
        assert_eq!(&cpu.memory[0x80000..0x80002], &[0xff, b'A']);
        assert_eq!(&cpu.memory[0x80002..0x80006], b"hi\0\0");
    }

    #[test]
    fn rejects_string_initializers_that_do_not_fit_character_arrays() {
        for source in [
            "char text[2] = \"cat\"; int main(void) { return 0; }",
            "int values[4] = \"cat\"; int main(void) { return 0; }",
        ] {
            assert!(
                compile_minimal_program(source, "too-small-string.c")
                    .unwrap()
                    .is_none(),
                "source unexpectedly compiled: {source}"
            );
        }
    }

    #[test]
    fn compiles_initialized_local_integer_arrays_with_stack_frames() {
        let image = compile_minimal_program(
            "int helper(int value) { int items[3]; items[0] = value; items[1] = 5; \
                 return items[0] + items[1]; } \
             int main(void) { int values[4] = { 2, 3 }; values[1]++; \
                 values[2] = helper(values[0] + values[1]); \
                 return sizeof(values) + values[2]; }",
            "local-array.c",
        )
        .unwrap()
        .expect("supported local integer array program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(2048).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 19);
    }

    #[test]
    fn compiles_initialized_local_char_arrays_with_byte_indexing() {
        let image = compile_minimal_program(
            "int main(void) { unsigned char values[4] = { 1, 255, 3 }; \
                 values[1] += 2; values[2]++; \
                 return sizeof(values) + values[0] + values[1] + values[2]; }",
            "local-byte-array.c",
        )
        .unwrap()
        .expect("supported local byte array program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(1024).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 10);
    }

    #[test]
    fn compiles_brace_initialized_global_integer_arrays() {
        let image = compile_minimal_program(
            "int values[5] = { 4, { 7 }, 3, 9 }; int main(void) { values[1] += values[2]; return values[0] + values[1] + values[2] + values[3] + values[4]; }",
            "initialized-array.c",
        )
        .unwrap()
        .expect("supported initialized global-array program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 26);
        assert_eq!(cpu.read_u16(0x80000), 4);
        assert_eq!(cpu.read_u16(0x80002), 10);
        assert_eq!(cpu.read_u16(0x80004), 3);
        assert_eq!(cpu.read_u16(0x80006), 9);
        assert_eq!(cpu.read_u16(0x80008), 0);
    }

    #[test]
    fn compiles_global_and_local_arrays_with_inferred_bounds() {
        let image = compile_minimal_program(
            r##"unsigned char values[] = { 1, 255, 3 }; char title[] = "SC";
                int main(void) {
                    int local[] = { 4, 5 };
                    char text[] = "ok";
                    return sizeof(values) + values[1] + sizeof(title) + title[2]
                        + sizeof(local) + local[1] + sizeof(text) + text[2];
                }"##,
            "inferred-array-bounds.c",
        )
        .unwrap()
        .expect("supported inferred array bounds");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(1024).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 273);
        assert_eq!(&cpu.memory[0x80000..0x80003], &[1, 255, 3]);
        assert_eq!(&cpu.memory[0x80003..0x80006], b"SC\0");
    }

    #[test]
    fn compiles_global_and_local_two_dimensional_arrays() {
        let image = compile_minimal_program(
            "unsigned char map[2][3] = { { 1, 2, 3 }, { 4, 5, 6 } }; \
             int main(void) { int values[2][3] = { { 7, 8 }, { 9 } }; \
                 values[1][2] = map[1][1] + values[0][1]; \
                 return sizeof(map) + map[1][2] + values[1][2]; }",
            "two-dimensional-arrays.c",
        )
        .unwrap()
        .expect("supported two-dimensional arrays");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(2048).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 25);
        assert_eq!(&cpu.memory[0x80000..0x80006], &[1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn links_and_executes_native_graphics_library_calls() {
        let image = compile_minimal_program(
            "#include <suomi_gfx.h>\n\
             int main(void) { gfx_rect(1, 2, 2, 2, 7); return 0; }",
            "native-gfx-call.c",
        )
        .unwrap()
        .expect("supported native graphics call");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(4096).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.memory[0x50000 + 2 * 320 + 1], 7);
        assert_eq!(cpu.memory[0x50000 + 3 * 320 + 2], 7);
        assert_eq!(cpu.registers[0], 0);
    }

    #[test]
    fn stores_nul_terminated_string_literals_for_native_graphics_calls() {
        let image = compile_minimal_program(
            "#include <suomi_gfx.h>\n\
             int main(void) { gfx_text(0, 0, \"Hi\", 1); return 0; }",
            "native-gfx-string.c",
        )
        .unwrap()
        .expect("supported string argument to graphics builtin");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(8192).unwrap();
        assert!(!cpu.running);
        assert!(cpu.memory.windows(3).any(|bytes| bytes == b"Hi\0"));
    }

    #[test]
    fn compiles_brace_initialized_global_scalar() {
        let image = compile_minimal_program(
            "int value = { { 12 } }; int main(void) { return value; }",
            "initialized-scalar.c",
        )
        .unwrap()
        .expect("supported braced scalar initializer");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(64).unwrap();
        assert_eq!(cpu.registers[0], 12);
    }

    #[test]
    fn rejects_invalid_integer_array_initializers() {
        for source in [
            "int values[0]; int main(void) { return 0; }",
            "int values[1] = { 1, 2 }; int main(void) { return 0; }",
            "int values[1] = { { 1, 2 } }; int main(void) { return 0; }",
            "int values[1] = 1; int main(void) { return 0; }",
        ] {
            assert!(
                compile_minimal_program(source, "bad-initializer.c")
                    .unwrap()
                    .is_none(),
                "source: {source}"
            );
        }
    }

    #[test]
    fn compiles_brace_initialized_local_scalar() {
        let image = compile_minimal_program(
            "int main(void) { int value = { { 9 } }; return value; }",
            "local-initializer.c",
        )
        .unwrap()
        .expect("supported braced local scalar initializer");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(64).unwrap();
        assert_eq!(cpu.registers[0], 9);
    }

    #[test]
    fn compiles_integer_sizeof_without_evaluating_operands() {
        let image = compile_minimal_program(
            "int values[2]; int main(void) { int value = 5; return sizeof(int) + sizeof(char) + sizeof(int*) + sizeof(unsigned long) + sizeof(value++) + sizeof(values[0]) + sizeof(values) + value; }",
            "sizeof.c",
        )
        .unwrap()
        .expect("supported sizeof expressions");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(128).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 24);
    }

    #[test]
    fn compiles_comma_operator_with_discarded_side_effects() {
        for (source, expected) in [
            (
                "int main(void) { int value = 0; return (value = 3, value + 2); }",
                5,
            ),
            (
                "int main(void) { int value = 0; return (value++, value + 2); }",
                3,
            ),
            (
                "int main(void) { int value = 0; return (value = 3, value++, value + 2); }",
                6,
            ),
        ] {
            let image = compile_minimal_program(source, "comma.c")
                .unwrap()
                .unwrap_or_else(|| panic!("unsupported comma expression: {source}"));
            let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
            cpu.load_program(&image.segments, image.entry_point)
                .unwrap();
            cpu.run(128).unwrap();
            assert!(!cpu.running);
            assert_eq!(cpu.registers[0], expected, "source: {source}");
        }
    }

    #[test]
    fn compiles_prefix_and_postfix_updates_as_values() {
        for (source, expected) in [
            (
                "int main(void) { int value = 3; return ++value + value++; }",
                8,
            ),
            (
                "int value = 3; int main(void) { return value++ + ++value; }",
                8,
            ),
        ] {
            let image = compile_minimal_program(source, "increment-values.c")
                .unwrap()
                .expect("supported increment expression");
            let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
            cpu.load_program(&image.segments, image.entry_point)
                .unwrap();
            cpu.run(128).unwrap();
            assert!(!cpu.running);
            assert_eq!(cpu.registers[0], expected, "source: {source}");
        }
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
    fn compiles_forward_backward_and_function_scoped_goto_labels() {
        let source = "int other(void) { int n = 0; again: n++; if (n < 2) goto again; return n; } int main(void) { goto done; return 99; done: return other(); }";
        let image = compile_minimal_program(source, "goto.c")
            .unwrap()
            .expect("supported goto program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 2);
    }

    #[test]
    fn compiles_functions_that_return_from_every_switch_case() {
        let source = "int choose(int value) { switch (value) { case 1: return 11; case 2: return 22; default: return 33; } } int main(void) { return choose(2); }";
        let image = compile_minimal_program(source, "switch-return.c")
            .unwrap()
            .expect("supported switch-return program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 22);
    }

    #[test]
    fn rejects_switch_functions_with_a_break_path_without_a_return() {
        let source = "int choose(int value) { switch (value) { default: if (value) break; return 1; } } int main(void) { return choose(1); }";
        assert!(
            compile_minimal_program(source, "switch-break-path.c")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_duplicate_or_undefined_goto_labels() {
        for source in [
            "int main(void) { goto missing; return 0; }",
            "int main(void) { again: ; again: return 0; }",
        ] {
            assert!(
                compile_minimal_program(source, "goto.c").unwrap().is_none(),
                "source: {source}"
            );
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

    #[test]
    fn compiles_void_helpers_with_and_without_return_statements() {
        let source = "int result; void set_result(int value); void set_result(int value) { result = value; } void increment(void) { result++; return; } int main(void) { set_result(5); increment(); return result; }";
        let image = compile_minimal_program(source, "void-functions.c")
            .unwrap()
            .expect("supported void-function program");
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 6);
    }

    #[test]
    fn rejects_void_function_results_used_as_values() {
        let source = "void action(void) {} int main(void) { return action(); }";
        assert!(
            compile_minimal_program(source, "void-value.c")
                .unwrap()
                .is_none()
        );
    }
}
