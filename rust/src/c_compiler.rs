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
        if matches!(
            self.expressions.token().value.as_str(),
            "switch" | "case" | "default"
        ) {
            return Err(self
                .expressions
                .fail("switch statements are not supported yet"));
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
                if array_length.is_some() && self.at("{") {
                    return Err(self
                        .expressions
                        .fail("array initializer lists are not supported yet"));
                }
                Some(self.expressions.parse_expression(1)?)
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
                if array_length.is_some() && self.statements.at("{") {
                    return Err(self
                        .statements
                        .expressions
                        .fail("array initializer lists are not supported yet"));
                }
                Some(self.statements.expressions.parse_expression(1)?)
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
}
