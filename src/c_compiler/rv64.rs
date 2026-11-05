use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::{
    CCompilerError, Declaration, Expression, ExpressionKind, ExternalDeclaration,
    FunctionDeclaration, Statement, StatementKind, TranslationUnitParser, decode_string_token,
    error, lex_unexpanded, scalar_initializer,
};
use crate::kiuas16_abi::{PLAIN_CHAR_IS_UNSIGNED, STACK_ALIGNMENT};

const GLOBAL_BASE: u64 = 0x80010000;
const GLOBAL_BYTES: usize = 0x10000;

#[derive(Clone, Copy)]
enum AtomicOperation {
    Load,
    Store,
    Exchange,
    FetchAdd,
    FetchSub,
    FetchAnd,
    FetchOr,
    FetchXor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PointerBase {
    Bool,
    Char,
    SignedChar,
    Byte,
    Short,
    UShort,
    Int,
    UInt,
    Long,
    ULong,
    Void,
}

impl PointerBase {
    fn from_type(ty: Type) -> Option<Self> {
        Some(match ty {
            Type::Bool => Self::Bool,
            Type::Char => Self::Char,
            Type::SignedChar => Self::SignedChar,
            Type::Byte => Self::Byte,
            Type::Short => Self::Short,
            Type::UShort => Self::UShort,
            Type::Int => Self::Int,
            Type::UInt => Self::UInt,
            Type::Long => Self::Long,
            Type::ULong => Self::ULong,
            Type::Void => Self::Void,
            _ => return None,
        })
    }

    fn scalar(self) -> Type {
        match self {
            Self::Bool => Type::Bool,
            Self::Char => Type::Char,
            Self::SignedChar => Type::SignedChar,
            Self::Byte => Type::Byte,
            Self::Short => Type::Short,
            Self::UShort => Type::UShort,
            Self::Int => Type::Int,
            Self::UInt => Type::UInt,
            Self::Long => Type::Long,
            Self::ULong => Type::ULong,
            Self::Void => Type::Void,
        }
    }
}

fn atomic_operation(name: &str) -> Option<AtomicOperation> {
    match name {
        "__atomic_load_n" => Some(AtomicOperation::Load),
        "__atomic_store_n" => Some(AtomicOperation::Store),
        "__atomic_exchange_n" => Some(AtomicOperation::Exchange),
        "__atomic_fetch_add" => Some(AtomicOperation::FetchAdd),
        "__atomic_fetch_sub" => Some(AtomicOperation::FetchSub),
        "__atomic_fetch_and" => Some(AtomicOperation::FetchAnd),
        "__atomic_fetch_or" => Some(AtomicOperation::FetchOr),
        "__atomic_fetch_xor" => Some(AtomicOperation::FetchXor),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Type {
    Bool,
    Int,
    UInt,
    Short,
    UShort,
    Long,
    ULong,
    Char,
    SignedChar,
    Byte,
    PlainCharPointer,
    PlainBoolPointer,
    PlainSignedCharPointer,
    PlainBytePointer,
    PlainIntPointer,
    PlainUIntPointer,
    PlainShortPointer,
    PlainUShortPointer,
    PlainLongPointer,
    PlainULongPointer,
    PlainVoidPointer,
    CharPointer,
    BoolPointer,
    SignedCharPointer,
    BytePointer,
    IntPointer,
    UIntPointer,
    ShortPointer,
    UShortPointer,
    LongPointer,
    ULongPointer,
    VoidPointer,
    ConstPointer {
        base: PointerBase,
        volatile: bool,
    },
    PointerPointer {
        base: PointerBase,
        volatile: bool,
        read_only: bool,
    },
    Void,
}

impl Type {
    fn parse(text: &str) -> Option<Self> {
        let compact = text.split_whitespace().collect::<String>();
        if compact.ends_with("**") {
            return Self::parse(compact.strip_suffix('*')?)?.pointer();
        }
        if compact.contains("const") {
            let pointee = compact.strip_suffix('*')?;
            if pointee.contains('*') {
                return None;
            }
            let pointer = Self::parse(&format!("{}*", pointee.replace("const", "")))?;
            return Some(Self::ConstPointer {
                base: PointerBase::from_type(pointer.pointee()?)?,
                volatile: pointer.volatile_pointee(),
            });
        }
        Some(match compact.as_str() {
            "_Bool" => Self::Bool,
            "_Bool*" => Self::PlainBoolPointer,
            "volatile_Bool*" | "_Boolvolatile*" => Self::BoolPointer,
            "int" | "signed" | "signedint" => Self::Int,
            "unsigned" | "unsignedint" => Self::UInt,
            "short" | "shortint" | "signedshort" | "signedshortint" => Self::Short,
            "unsignedshort" | "unsignedshortint" | "shortunsigned" | "shortunsignedint" => {
                Self::UShort
            }
            "long" | "longint" | "signedlong" | "signedlongint" => Self::Long,
            "unsignedlong" | "unsignedlongint" | "longunsigned" | "longunsignedint" => Self::ULong,
            "char" => Self::Char,
            "signedchar" => Self::SignedChar,
            "unsignedchar" => Self::Byte,
            "char*" => Self::PlainCharPointer,
            "signedchar*" => Self::PlainSignedCharPointer,
            "unsignedchar*" => Self::PlainBytePointer,
            "int*" | "signed*" | "signedint*" => Self::PlainIntPointer,
            "unsigned*" | "unsignedint*" => Self::PlainUIntPointer,
            "short*" | "shortint*" | "signedshort*" | "signedshortint*" => Self::PlainShortPointer,
            "unsignedshort*" | "unsignedshortint*" | "shortunsigned*" | "shortunsignedint*" => {
                Self::PlainUShortPointer
            }
            "long*" | "longint*" | "signedlong*" | "signedlongint*" => Self::PlainLongPointer,
            "unsignedlong*" | "unsignedlongint*" | "longunsigned*" | "longunsignedint*" => {
                Self::PlainULongPointer
            }
            "volatileunsignedchar*" | "unsignedvolatilechar*" | "unsignedcharvolatile*" => {
                Self::BytePointer
            }
            "volatilechar*" | "charvolatile*" => Self::CharPointer,
            "volatilesignedchar*" | "signedvolatilechar*" | "signedcharvolatile*" => {
                Self::SignedCharPointer
            }
            "volatileint*" | "intvolatile*" => Self::IntPointer,
            "volatileunsigned*"
            | "volatileunsignedint*"
            | "unsignedvolatileint*"
            | "unsignedintvolatile*" => Self::UIntPointer,
            "volatileshort*"
            | "shortvolatile*"
            | "volatileshortint*"
            | "shortintvolatile*"
            | "volatilesignedshort*"
            | "signedvolatileshort*"
            | "signedshortvolatile*"
            | "volatilesignedshortint*"
            | "signedshortintvolatile*" => Self::ShortPointer,
            "volatileunsignedshort*"
            | "unsignedvolatileshort*"
            | "unsignedshortvolatile*"
            | "volatileunsignedshortint*"
            | "unsignedshortintvolatile*" => Self::UShortPointer,
            "volatilelong*" | "longvolatile*" | "volatilelongint*" | "longintvolatile*" => {
                Self::LongPointer
            }
            "volatileunsignedlong*"
            | "unsignedvolatilelong*"
            | "unsignedlongvolatile*"
            | "volatileunsignedlongint*"
            | "unsignedlongintvolatile*" => Self::ULongPointer,
            "void" => Self::Void,
            "void*" => Self::PlainVoidPointer,
            "volatilevoid*" | "voidvolatile*" => Self::VoidPointer,
            _ => return None,
        })
    }

    fn integer(self) -> bool {
        matches!(
            self,
            Self::Bool
                | Self::Int
                | Self::UInt
                | Self::Short
                | Self::UShort
                | Self::Long
                | Self::ULong
                | Self::Char
                | Self::SignedChar
                | Self::Byte
        )
    }

    fn unsigned(self) -> bool {
        matches!(
            self,
            Self::Bool | Self::UInt | Self::UShort | Self::ULong | Self::Byte
        ) || self == Self::Char && PLAIN_CHAR_IS_UNSIGNED
            || self.pointee().is_some()
    }

    fn scalar(self) -> bool {
        self.integer() || self.pointee().is_some()
    }

    fn size(self) -> Option<u64> {
        match self {
            Self::Bool | Self::Char | Self::SignedChar | Self::Byte => Some(1),
            Self::Short | Self::UShort => Some(2),
            Self::Int | Self::UInt => Some(4),
            Self::Long | Self::ULong => Some(8),
            _ if self.pointee().is_some() => Some(8),
            _ => None,
        }
    }

    fn promote(self) -> Self {
        if matches!(
            self,
            Self::Bool | Self::Char | Self::SignedChar | Self::Byte | Self::Short | Self::UShort
        ) {
            Self::Int
        } else {
            self
        }
    }

    fn common(self, other: Self) -> Self {
        let left = self.promote();
        let right = other.promote();
        if left == Self::ULong || right == Self::ULong {
            Self::ULong
        } else if left == Self::Long || right == Self::Long {
            Self::Long
        } else if left == Self::UInt || right == Self::UInt {
            Self::UInt
        } else {
            Self::Int
        }
    }

    fn compatible(self, other: Self) -> bool {
        self == other
            || self.integer() && other.integer()
            || self.pointee().is_some()
                && other.pointee().is_some()
                && (self.pointee() == other.pointee()
                    || self.pointee() == Some(Self::Void)
                    || other.pointee() == Some(Self::Void))
                && (self.volatile_pointee() || !other.volatile_pointee())
                && (self.const_pointee() || !other.const_pointee())
    }

    fn volatile_pointee(self) -> bool {
        if let Self::ConstPointer { volatile, .. } = self {
            return volatile;
        }
        self.pointee()
            .and_then(Self::pointer)
            .is_some_and(|plain| self != plain)
    }

    fn const_pointee(self) -> bool {
        matches!(self, Self::ConstPointer { .. })
    }

    fn qualified_pointer(self, volatile: bool) -> Option<Self> {
        if !volatile {
            return self.pointer();
        }
        Some(match self {
            Self::Bool => Self::BoolPointer,
            Self::Char => Self::CharPointer,
            Self::SignedChar => Self::SignedCharPointer,
            Self::Byte => Self::BytePointer,
            Self::Int => Self::IntPointer,
            Self::UInt => Self::UIntPointer,
            Self::Short => Self::ShortPointer,
            Self::UShort => Self::UShortPointer,
            Self::Long => Self::LongPointer,
            Self::ULong => Self::ULongPointer,
            Self::Void => Self::VoidPointer,
            _ => return None,
        })
    }

    fn common_pointer(self, other: Self) -> Option<Self> {
        let left = self.pointee()?;
        let right = other.pointee()?;
        let pointee = if left == Self::Void || right == Self::Void {
            Self::Void
        } else if left == right {
            left
        } else {
            return None;
        };
        let volatile = self.volatile_pointee() || other.volatile_pointee();
        if self.const_pointee() || other.const_pointee() {
            Some(Self::ConstPointer {
                base: PointerBase::from_type(pointee)?,
                volatile,
            })
        } else {
            pointee.qualified_pointer(volatile)
        }
    }

    fn pointer(self) -> Option<Self> {
        Some(match self {
            Self::Bool => Self::PlainBoolPointer,
            Self::Char => Self::PlainCharPointer,
            Self::SignedChar => Self::PlainSignedCharPointer,
            Self::Byte => Self::PlainBytePointer,
            Self::Int => Self::PlainIntPointer,
            Self::UInt => Self::PlainUIntPointer,
            Self::Short => Self::PlainShortPointer,
            Self::UShort => Self::PlainUShortPointer,
            Self::Long => Self::PlainLongPointer,
            Self::ULong => Self::PlainULongPointer,
            Self::Void => Self::PlainVoidPointer,
            _ if self.pointee().is_some() => Self::PointerPointer {
                base: PointerBase::from_type(self.pointee()?)?,
                volatile: self.volatile_pointee(),
                read_only: self.const_pointee(),
            },
            _ => return None,
        })
    }

    fn pointee(self) -> Option<Self> {
        match self {
            Self::PlainBoolPointer | Self::BoolPointer => Some(Self::Bool),
            Self::ConstPointer { base, .. } => Some(base.scalar()),
            Self::PointerPointer {
                base,
                volatile,
                read_only,
            } => {
                if read_only {
                    Some(Self::ConstPointer { base, volatile })
                } else {
                    base.scalar().qualified_pointer(volatile)
                }
            }
            Self::PlainCharPointer | Self::CharPointer => Some(Self::Char),
            Self::PlainSignedCharPointer | Self::SignedCharPointer => Some(Self::SignedChar),
            Self::PlainBytePointer | Self::BytePointer => Some(Self::Byte),
            Self::PlainIntPointer | Self::IntPointer => Some(Self::Int),
            Self::PlainUIntPointer | Self::UIntPointer => Some(Self::UInt),
            Self::PlainShortPointer | Self::ShortPointer => Some(Self::Short),
            Self::PlainUShortPointer | Self::UShortPointer => Some(Self::UShort),
            Self::PlainLongPointer | Self::LongPointer => Some(Self::Long),
            Self::PlainULongPointer | Self::ULongPointer => Some(Self::ULong),
            Self::PlainVoidPointer | Self::VoidPointer => Some(Self::Void),
            _ => None,
        }
    }
}

fn integer_literal(token: &str) -> Option<(u64, Type)> {
    let suffix_start = token.find(['u', 'U', 'l', 'L']).unwrap_or(token.len());
    let (digits, suffix) = token.split_at(suffix_start);
    let suffix = suffix.to_ascii_lowercase();
    if !matches!(suffix.as_str(), "" | "u" | "l" | "ul" | "lu") {
        return None;
    }
    let (radix, digits) = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (16, hex)
    } else if digits.starts_with('0') {
        (8, digits)
    } else {
        (10, digits)
    };
    let number = u64::from_str_radix(digits, radix).ok()?;
    let candidates: &[Type] = match suffix.as_str() {
        "ul" | "lu" => &[Type::ULong],
        "u" => &[Type::UInt, Type::ULong],
        "l" => &[Type::Long, Type::ULong],
        _ if radix == 10 => &[Type::Int, Type::Long, Type::ULong],
        _ => &[Type::Int, Type::UInt, Type::Long, Type::ULong],
    };
    let ty = candidates.iter().copied().find(|ty| match ty {
        Type::Int => number <= i32::MAX as u64,
        Type::UInt => number <= u32::MAX as u64,
        Type::Long => number <= i64::MAX as u64,
        Type::ULong => true,
        _ => unreachable!("integer literal candidates"),
    })?;
    Some((number, ty))
}

#[derive(Clone, PartialEq, Eq)]
struct Signature {
    result: Type,
    parameters: Vec<Type>,
}

#[derive(Clone)]
struct Value {
    ty: Type,
    kind: ValueKind,
}

impl Value {
    fn modifiable(&self) -> bool {
        match &self.kind {
            ValueKind::Local(_) | ValueKind::Global(_, _) => true,
            ValueKind::Load(pointer) => !pointer.ty.const_pointee(),
            _ => false,
        }
    }

    fn integer_constant_expression(&self) -> bool {
        if !self.ty.integer() {
            return false;
        }
        match &self.kind {
            ValueKind::Constant(_) => true,
            ValueKind::Convert(inner) | ValueKind::Unary(_, inner) => {
                inner.integer_constant_expression()
            }
            ValueKind::Binary(_, left, right) | ValueKind::Logical(_, left, right) => {
                left.integer_constant_expression() && right.integer_constant_expression()
            }
            ValueKind::Conditional(condition, when_true, when_false) => {
                condition.integer_constant_expression()
                    && when_true.integer_constant_expression()
                    && when_false.integer_constant_expression()
            }
            _ => false,
        }
    }

    fn null_pointer_constant(&self) -> bool {
        if self.integer_constant_expression() {
            return self.constant_integer() == Some(0);
        }
        self.ty == Type::PlainVoidPointer
            && matches!(&self.kind, ValueKind::Convert(inner)
                if inner.integer_constant_expression() && inner.constant_integer() == Some(0))
    }

    fn static_pointer(&self, globals: &BTreeMap<String, Global>) -> Option<u64> {
        if self.ty.pointee().is_none() {
            return None;
        }
        match &self.kind {
            ValueKind::GlobalAddress(name, _) => Some(globals.get(name)?.address),
            ValueKind::Convert(inner) if inner.ty.integer() => inner.constant_integer(),
            ValueKind::Convert(inner) => inner.static_pointer(globals),
            ValueKind::Binary(operator, left, right) if matches!(operator.as_str(), "+" | "-") => {
                let (address, offset) = if left.ty.pointee().is_some() {
                    (left.static_pointer(globals)?, right.constant_integer()?)
                } else if operator == "+" {
                    (right.static_pointer(globals)?, left.constant_integer()?)
                } else {
                    return None;
                };
                Some(if operator == "+" {
                    address.wrapping_add(offset)
                } else {
                    address.wrapping_sub(offset)
                })
            }
            ValueKind::Conditional(condition, when_true, when_false) => {
                if condition.constant_integer()? != 0 {
                    when_true.static_pointer(globals)
                } else {
                    when_false.static_pointer(globals)
                }
            }
            _ => None,
        }
    }

    fn binary(ty: Type, operator: &str, left: Self, right: Self) -> Self {
        Self {
            ty,
            kind: ValueKind::Binary(operator.to_owned(), Box::new(left), Box::new(right)),
        }
    }

    fn constant_integer(&self) -> Option<u64> {
        if !self.ty.integer() {
            return None;
        }
        let signed_result = |number: i128| {
            let number = i64::try_from(number).ok()?;
            if self.ty == Type::Int && i32::try_from(number).is_err() {
                return None;
            }
            Some(number as u64)
        };
        let number = match &self.kind {
            ValueKind::Constant(number) => *number,
            ValueKind::Convert(inner) => inner.constant_integer()?,
            ValueKind::Unary(operator, inner) => {
                let number = inner.constant_integer()?;
                match operator.as_str() {
                    "+" => number,
                    "-" if self.ty.unsigned() => number.wrapping_neg(),
                    "-" => signed_result(-(number as i64 as i128))?,
                    "~" => !number,
                    "!" => u64::from(number == 0),
                    _ => return None,
                }
            }
            ValueKind::Binary(operator, left, right) => {
                let ty = left.ty;
                let left = left.constant_integer()?;
                let right = right.constant_integer()?;
                let signed_left = left as i64 as i128;
                let signed_right = right as i64 as i128;
                match operator.as_str() {
                    "+" if ty.unsigned() => left.wrapping_add(right),
                    "+" => signed_result(signed_left + signed_right)?,
                    "-" if ty.unsigned() => left.wrapping_sub(right),
                    "-" => signed_result(signed_left - signed_right)?,
                    "*" if ty.unsigned() => left.wrapping_mul(right),
                    "*" => signed_result(signed_left * signed_right)?,
                    "/" if right == 0 => return None,
                    "/" if ty.unsigned() => left / right,
                    "/" => signed_result(signed_left / signed_right)?,
                    "%" if right == 0 => return None,
                    "%" if ty.unsigned() => left % right,
                    "%" if signed_left == i64::MIN as i128 && signed_right == -1 => return None,
                    "%" if ty == Type::Int
                        && signed_left == i32::MIN as i128
                        && signed_right == -1 =>
                    {
                        return None;
                    }
                    "%" => signed_result(signed_left % signed_right)?,
                    "<<" | ">>" if right >= ty.size()? * 8 => return None,
                    "<<" if ty.unsigned() => left << right,
                    "<<" if signed_left < 0 => return None,
                    "<<" => signed_result(signed_left << right)?,
                    ">>" if ty.unsigned() => left >> right,
                    ">>" => (left as i64 >> right) as u64,
                    "&" => left & right,
                    "|" => left | right,
                    "^" => left ^ right,
                    "==" => u64::from(left == right),
                    "!=" => u64::from(left != right),
                    "<" if ty.unsigned() => u64::from(left < right),
                    "<" => u64::from(signed_left < signed_right),
                    "<=" if ty.unsigned() => u64::from(left <= right),
                    "<=" => u64::from(signed_left <= signed_right),
                    ">" if ty.unsigned() => u64::from(left > right),
                    ">" => u64::from(signed_left > signed_right),
                    ">=" if ty.unsigned() => u64::from(left >= right),
                    ">=" => u64::from(signed_left >= signed_right),
                    _ => return None,
                }
            }
            ValueKind::Logical(operator, left, right) => {
                let left = left.constant_integer()? != 0;
                u64::from(if operator == "&&" {
                    left && right.constant_integer()? != 0
                } else {
                    left || right.constant_integer()? != 0
                })
            }
            ValueKind::Conditional(condition, when_true, when_false) => {
                if condition.constant_integer()? != 0 {
                    when_true.constant_integer()?
                } else {
                    when_false.constant_integer()?
                }
            }
            _ => return None,
        };
        Some(match self.ty {
            Type::Bool => u64::from(number != 0),
            Type::Int => number as i32 as i64 as u64,
            Type::UInt => number as u32 as u64,
            Type::Short => number as i16 as i64 as u64,
            Type::UShort => number as u16 as u64,
            Type::Char if PLAIN_CHAR_IS_UNSIGNED => number as u8 as u64,
            Type::Char | Type::SignedChar => number as i8 as i64 as u64,
            Type::Byte => number as u8 as u64,
            _ => number,
        })
    }
}

#[derive(Clone)]
enum ValueKind {
    Constant(u64),
    GlobalAddress(String, u64),
    Global(String, u64),
    Local(usize),
    Address(usize),
    Convert(Box<Value>),
    Unary(String, Box<Value>),
    Binary(String, Box<Value>, Box<Value>),
    Logical(String, Box<Value>, Box<Value>),
    Conditional(Box<Value>, Box<Value>, Box<Value>),
    Call(String, Vec<Value>),
    Atomic {
        operation: AtomicOperation,
        pointer: Box<Value>,
        operand: Option<Box<Value>>,
        order: u64,
    },
    Load(Box<Value>),
    Assign(Box<Value>, Box<Value>),
    Update {
        target: Box<Value>,
        operand: Box<Value>,
        operator: String,
        operation_type: Type,
        postfix: bool,
    },
}

enum Action {
    Block(Vec<Action>),
    Evaluate(Value),
    InitializeArray {
        slot: usize,
        element: Type,
        values: Vec<(usize, Value)>,
    },
    Return(Option<Value>),
    If(Value, Box<Action>, Option<Box<Action>>),
    Loop {
        condition: Value,
        body: Box<Action>,
        update: Option<Value>,
        post_test: bool,
    },
    Switch {
        selector: Value,
        cases: Vec<(Option<u64>, Vec<Action>)>,
    },
    Break,
    Continue,
    Goto(String),
    Label(String, Box<Action>),
}

struct Local {
    slot: usize,
    ty: Type,
    array_length: Option<usize>,
}

#[derive(Clone)]
struct Global {
    ty: Type,
    array_length: Option<usize>,
    address: u64,
    bytes: usize,
    initializer: Option<Vec<u8>>,
    pointer_initializer: Option<(usize, Vec<(usize, Value)>)>,
}

impl Global {
    fn initialized(&self) -> bool {
        self.initializer.is_some() || self.pointer_initializer.is_some()
    }
}

struct Lower<'a> {
    filename: &'a str,
    signatures: &'a BTreeMap<String, Signature>,
    globals: &'a BTreeMap<String, Global>,
    typedefs: Option<&'a HashMap<String, String>>,
    enumerators: Option<&'a HashMap<String, i64>>,
    scopes: Vec<BTreeMap<String, Local>>,
    slots: usize,
    result: Type,
    loop_depth: usize,
    break_depth: usize,
    labels: BTreeSet<String>,
    jumps: Vec<(String, usize)>,
}

impl Lower<'_> {
    fn fail(&self, line: usize, message: &str) -> CCompilerError {
        error(self.filename, line, message)
    }

    fn lookup(&self, name: &str) -> Option<&Local> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn array_size(&self, name: &str) -> Option<u64> {
        if let Some(bytes) = decode_string_token(name) {
            return Some(bytes.len() as u64 + 1);
        }
        let (ty, length) = if let Some(local) = self.lookup(name) {
            (local.ty, local.array_length?)
        } else {
            let global = self.globals.get(name)?;
            (global.ty, global.array_length?)
        };
        Some(ty.size().expect("integer element size") * length as u64)
    }

    fn local(
        &mut self,
        name: &str,
        ty: Type,
        array_length: Option<usize>,
        line: usize,
    ) -> Result<usize, CCompilerError> {
        if atomic_operation(name).is_some() {
            return Err(self.fail(line, "atomic builtin name is reserved"));
        }
        if self
            .typedefs
            .is_some_and(|aliases| aliases.contains_key(name))
        {
            return Err(self.fail(line, "local or parameter name conflicts with a typedef"));
        }
        if self
            .enumerators
            .is_some_and(|names| names.contains_key(name))
        {
            return Err(self.fail(line, "local or parameter name conflicts with an enumerator"));
        }
        let slots = if let Some(length) = array_length {
            if length == 0 || !ty.scalar() || ty.pointer().is_none() {
                return Err(self.fail(
                    line,
                    "arrays require a positive length and supported scalar elements",
                ));
            }
            length
                .checked_mul(ty.size().expect("integer size") as usize)
                .ok_or_else(|| self.fail(line, "array size is too large"))?
                .div_ceil(8)
        } else {
            1
        };
        let scope = self.scopes.last_mut().expect("function scope");
        if scope.contains_key(name) {
            return Err(error(self.filename, line, "duplicate local declaration"));
        }
        if slots > 120 - self.slots {
            return Err(self.fail(
                line,
                "RV64 subset supports at most 120 local/parameter slots",
            ));
        }
        let slot = self.slots + slots - 1;
        scope.insert(
            name.to_owned(),
            Local {
                slot,
                ty,
                array_length,
            },
        );
        self.slots += slots;
        Ok(slot)
    }

    fn convert(&self, value: Value, ty: Type, line: usize) -> Result<Value, CCompilerError> {
        if value.ty == Type::Void
            || ty == Type::Void
            || !(ty.compatible(value.ty)
                || ty == Type::Bool && value.ty.scalar()
                || ty.pointee().is_some() && value.null_pointer_constant())
        {
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

    fn pointer_offset(
        &self,
        index: Value,
        pointer: Type,
        line: usize,
    ) -> Result<Value, CCompilerError> {
        let index = self.convert(index, Type::Long, line)?;
        let size = pointer
            .pointee()
            .and_then(Type::size)
            .ok_or_else(|| self.fail(line, "pointer arithmetic requires a complete object type"))?;
        Ok(Value::binary(
            Type::Long,
            "*",
            index,
            Value {
                ty: Type::Long,
                kind: ValueKind::Constant(size),
            },
        ))
    }

    fn binary(
        &self,
        operator: &str,
        left: Value,
        right: Value,
        line: usize,
    ) -> Result<Value, CCompilerError> {
        let comparison = matches!(operator, "==" | "!=" | "<" | "<=" | ">" | ">=");
        if left.ty.pointee().is_some() || right.ty.pointee().is_some() {
            if matches!(operator, "==" | "!=") {
                if left.ty.pointee().is_some() && right.null_pointer_constant() {
                    let right = self.convert(right, left.ty, line)?;
                    return Ok(Value::binary(Type::Int, operator, left, right));
                }
                if right.ty.pointee().is_some() && left.null_pointer_constant() {
                    let left = self.convert(left, right.ty, line)?;
                    return Ok(Value::binary(Type::Int, operator, left, right));
                }
            }
            if left.ty.pointee().is_some() && right.ty.integer() && matches!(operator, "+" | "-") {
                let ty = left.ty;
                let offset = self.pointer_offset(right, ty, line)?;
                return Ok(Value::binary(ty, operator, left, offset));
            }
            if left.ty.integer() && right.ty.pointee().is_some() && operator == "+" {
                let ty = right.ty;
                let offset = self.pointer_offset(left, ty, line)?;
                return Ok(Value::binary(ty, operator, offset, right));
            }
            let matching_pointees =
                left.ty.pointee().is_some() && left.ty.pointee() == right.ty.pointee();
            if matches!(operator, "==" | "!=") && left.ty.common_pointer(right.ty).is_some() {
                return Ok(Value::binary(Type::Int, operator, left, right));
            }
            if matching_pointees && left.ty.pointee() != Some(Type::Void) && comparison {
                return Ok(Value::binary(Type::Int, operator, left, right));
            }
            if matching_pointees && operator == "-" {
                let size = left.ty.pointee().and_then(Type::size).ok_or_else(|| {
                    self.fail(line, "pointer subtraction requires a complete object type")
                })?;
                return Ok(Value::binary(
                    Type::Long,
                    "/",
                    Value::binary(Type::Long, "-", left, right),
                    Value {
                        ty: Type::Long,
                        kind: ValueKind::Constant(size),
                    },
                ));
            }
            return Err(self.fail(line, "invalid or incompatible RV64 pointer operands"));
        }
        if !left.ty.integer() || !right.ty.integer() {
            return Err(self.fail(line, "RV64 arithmetic requires integer operands"));
        }
        let shift = matches!(operator, "<<" | ">>");
        let ty = if shift {
            left.ty.promote()
        } else {
            left.ty.common(right.ty)
        };
        let right_ty = if shift { right.ty.promote() } else { ty };
        let left = self.convert(left, ty, line)?;
        let right = self.convert(right, right_ty, line)?;
        Ok(Value::binary(
            if comparison { Type::Int } else { ty },
            operator,
            left,
            right,
        ))
    }

    fn value(&mut self, expression: &Expression) -> Result<Value, CCompilerError> {
        let line = expression.line;
        let value = match &expression.kind {
            ExpressionKind::Atom(token) => {
                if let Some(local) = self.lookup(token) {
                    Value {
                        ty: if local.array_length.is_some() {
                            local.ty.pointer().expect("integer array")
                        } else {
                            local.ty
                        },
                        kind: if local.array_length.is_some() {
                            ValueKind::Address(local.slot)
                        } else {
                            ValueKind::Local(local.slot)
                        },
                    }
                } else if let Some(global) = self.globals.get(token) {
                    if global.array_length.is_some() {
                        Value {
                            ty: global.ty.pointer().expect("integer array"),
                            kind: ValueKind::GlobalAddress(token.clone(), global.address),
                        }
                    } else {
                        Value {
                            ty: global.ty,
                            kind: ValueKind::Global(token.clone(), global.address),
                        }
                    }
                } else if let Some((number, ty)) = integer_literal(token) {
                    Value {
                        ty,
                        kind: ValueKind::Constant(number),
                    }
                } else if token.starts_with(|c: char| c.is_ascii_digit()) {
                    return Err(self.fail(
                        line,
                        "invalid or out-of-range LP64 integer literal; supported suffixes are U, L, UL and LU",
                    ));
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
                if ty.pointee().is_some() && value.ty.integer()
                    || ty.integer() && value.ty.pointee().is_some()
                    || ty.common_pointer(value.ty).is_some()
                {
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
                if operator == "&" {
                    match value.kind {
                        ValueKind::Local(slot) => Value {
                            ty: value.ty.pointer().ok_or_else(|| {
                                self.fail(line, "address-of requires a supported scalar object")
                            })?,
                            kind: ValueKind::Address(slot),
                        },
                        ValueKind::Global(name, address) => Value {
                            ty: value.ty.pointer().ok_or_else(|| {
                                self.fail(line, "address-of requires a supported scalar object")
                            })?,
                            kind: ValueKind::GlobalAddress(name, address),
                        },
                        ValueKind::Load(pointer) if value.ty.scalar() => *pointer,
                        _ => return Err(self.fail(line, "address-of requires an lvalue")),
                    }
                } else if matches!(operator.as_str(), "++" | "--") {
                    self.update(
                        value,
                        if operator == "++" { "+" } else { "-" },
                        Value {
                            ty: Type::Int,
                            kind: ValueKind::Constant(1),
                        },
                        false,
                        line,
                    )?
                } else if operator == "*" && value.ty.pointee().is_some() {
                    if value.ty.pointee() == Some(Type::Void) {
                        return Err(self.fail(line, "dereference requires a complete object type"));
                    }
                    Value {
                        ty: value.ty.pointee().expect("checked pointer"),
                        kind: ValueKind::Load(Box::new(value)),
                    }
                } else if operator == "!" && value.ty.scalar() {
                    Value {
                        ty: Type::Int,
                        kind: ValueKind::Unary(operator.clone(), Box::new(value)),
                    }
                } else if matches!(operator.as_str(), "+" | "-" | "~") && value.ty.integer() {
                    let ty = value.ty.promote();
                    let value = self.convert(value, ty, line)?;
                    Value {
                        ty,
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
            } if matches!(operator.as_str(), "&&" | "||") => {
                let left = self.condition(left)?;
                let right = self.condition(right)?;
                Value {
                    ty: Type::Int,
                    kind: ValueKind::Logical(operator.clone(), Box::new(left), Box::new(right)),
                }
            }
            ExpressionKind::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                let condition = self.condition(condition)?;
                let when_true = self.value(when_true)?;
                let when_false = self.value(when_false)?;
                let ty = if when_true.ty.integer() && when_false.ty.integer() {
                    when_true.ty.common(when_false.ty)
                } else if when_true.ty.pointee().is_some() && when_false.null_pointer_constant() {
                    when_true.ty
                } else if when_false.ty.pointee().is_some() && when_true.null_pointer_constant() {
                    when_false.ty
                } else if when_true.ty == when_false.ty {
                    when_true.ty
                } else if let Some(ty) = when_true.ty.common_pointer(when_false.ty) {
                    ty
                } else {
                    return Err(self.fail(line, "incompatible RV64 conditional expression types"));
                };
                let (when_true, when_false) = if ty == Type::Void {
                    (when_true, when_false)
                } else {
                    (
                        self.convert(when_true, ty, line)?,
                        self.convert(when_false, ty, line)?,
                    )
                };
                Value {
                    ty,
                    kind: ValueKind::Conditional(
                        Box::new(condition),
                        Box::new(when_true),
                        Box::new(when_false),
                    ),
                }
            }
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => {
                if !matches!(
                    operator.as_str(),
                    "+" | "-"
                        | "*"
                        | "/"
                        | "%"
                        | "<<"
                        | ">>"
                        | "&"
                        | "|"
                        | "^"
                        | "=="
                        | "!="
                        | "<"
                        | "<="
                        | ">"
                        | ">="
                ) {
                    return Err(self.fail(line, "unsupported RV64 binary operation"));
                }
                let left = self.value(left)?;
                let right = self.value(right)?;
                self.binary(operator, left, right, line)?
            }
            ExpressionKind::Index { expression, index } => {
                let base = self.value(expression)?;
                let index = self.value(index)?;
                let pointer = self.binary("+", base, index, line)?;
                let ty = pointer.ty.pointee().ok_or_else(|| {
                    self.fail(line, "subscript requires one pointer and one integer")
                })?;
                Value {
                    ty,
                    kind: ValueKind::Load(Box::new(pointer)),
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
                if let Some(operation) = atomic_operation(name) {
                    return self.atomic(operation, arguments, line);
                }
                if self
                    .scopes
                    .iter()
                    .rev()
                    .any(|scope| scope.contains_key(name))
                    || self.globals.contains_key(name)
                {
                    return Err(self.fail(line, "called name is an object"));
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
                if !target.modifiable() {
                    return Err(self.fail(line, "assignment requires a modifiable object"));
                }
                let value = self.value(value)?;
                let value = self.convert(value, target.ty, line)?;
                Value {
                    ty: target.ty,
                    kind: ValueKind::Assign(Box::new(target), Box::new(value)),
                }
            }
            ExpressionKind::Assignment {
                operator,
                target,
                value,
            } => {
                let target = self.value(target)?;
                let operand = self.value(value)?;
                let operator = operator
                    .strip_suffix('=')
                    .ok_or_else(|| self.fail(line, "unsupported compound assignment"))?;
                self.update(target, operator, operand, false, line)?
            }
            ExpressionKind::Postfix {
                operator,
                expression,
            } => {
                let target = self.value(expression)?;
                let operator = match operator.as_str() {
                    "++" => "+",
                    "--" => "-",
                    _ => return Err(self.fail(line, "unsupported postfix operation")),
                };
                self.update(
                    target,
                    operator,
                    Value {
                        ty: Type::Int,
                        kind: ValueKind::Constant(1),
                    },
                    true,
                    line,
                )?
            }
            ExpressionKind::SizeofType(text) => {
                let ty = Type::parse(text)
                    .ok_or_else(|| self.fail(line, "unsupported RV64 sizeof type"))?;
                self.sizeof(ty, line)?
            }
            ExpressionKind::SizeofExpression(expression) => {
                if let ExpressionKind::Atom(name) = &expression.kind
                    && let Some(size) = self.array_size(name)
                {
                    Value {
                        ty: Type::ULong,
                        kind: ValueKind::Constant(size),
                    }
                } else {
                    let value = self.value(expression)?;
                    self.sizeof(value.ty, line)?
                }
            }
            _ => return Err(self.fail(line, "unsupported expression in initial RV64 subset")),
        };
        Ok(value)
    }

    fn sizeof(&self, ty: Type, line: usize) -> Result<Value, CCompilerError> {
        Ok(Value {
            ty: Type::ULong,
            kind: ValueKind::Constant(
                ty.size()
                    .ok_or_else(|| self.fail(line, "sizeof requires a complete object type"))?,
            ),
        })
    }

    fn atomic(
        &mut self,
        operation: AtomicOperation,
        arguments: &[Expression],
        line: usize,
    ) -> Result<Value, CCompilerError> {
        let load = matches!(operation, AtomicOperation::Load);
        if arguments.len() != if load { 2 } else { 3 } {
            return Err(self.fail(line, "atomic builtin argument count mismatch"));
        }
        let pointer = self.value(&arguments[0])?;
        if !load && pointer.ty.const_pointee() {
            return Err(self.fail(line, "atomic write requires a modifiable pointee"));
        }
        let ty = pointer
            .ty
            .pointee()
            .filter(|ty| matches!(ty, Type::Int | Type::UInt | Type::Long | Type::ULong))
            .ok_or_else(|| {
                self.fail(
                    line,
                    "atomic builtin requires a pointer to a 32/64-bit integer",
                )
            })?;
        let order = self.value(arguments.last().expect("checked arguments"))?;
        let order = order
            .constant_integer()
            .filter(|order| *order <= 5)
            .ok_or_else(|| {
                self.fail(
                    line,
                    "atomic memory order must be an integer constant from 0 through 5",
                )
            })?;
        if load && matches!(order, 3 | 4)
            || matches!(operation, AtomicOperation::Store) && matches!(order, 1 | 2 | 4)
        {
            return Err(self.fail(line, "invalid memory order for atomic load/store"));
        }
        let operand = if load {
            None
        } else {
            let value = self.value(&arguments[1])?;
            Some(Box::new(self.convert(value, ty, line)?))
        };
        Ok(Value {
            ty: if matches!(operation, AtomicOperation::Store) {
                Type::Void
            } else {
                ty
            },
            kind: ValueKind::Atomic {
                operation,
                pointer: Box::new(pointer),
                operand,
                order,
            },
        })
    }

    fn array_initializers(
        &mut self,
        initializer: &Expression,
        ty: Type,
        length: usize,
    ) -> Result<Vec<(usize, Value)>, CCompilerError> {
        let line = initializer.line;
        if let Some(Expression {
            kind: ExpressionKind::Atom(token),
            ..
        }) = scalar_initializer(initializer)
            && let Some(bytes) = decode_string_token(token)
        {
            if !matches!(ty, Type::Char | Type::SignedChar | Type::Byte) {
                return Err(self.fail(line, "string initializer requires a character array"));
            }
            if bytes.len() > length {
                return Err(self.fail(
                    line,
                    "string initializer is too long for the character array",
                ));
            }
            return (0..length)
                .map(|index| {
                    self.convert(
                        Value {
                            ty: Type::Int,
                            kind: ValueKind::Constant(u64::from(
                                bytes.get(index).copied().unwrap_or(0),
                            )),
                        },
                        ty,
                        line,
                    )
                    .map(|value| (index, value))
                })
                .collect();
        }
        let ExpressionKind::InitializerList(initializers) = &initializer.kind else {
            return Err(self.fail(line, "array initialization requires an initializer list"));
        };
        if initializers.is_empty() {
            return Err(self.fail(line, "array initializer list must not be empty"));
        }
        let mut values = Vec::with_capacity(length);
        let mut initialized = BTreeSet::new();
        let mut next = 0usize;
        for initializer in initializers {
            let (index, initializer) = match &initializer.kind {
                ExpressionKind::ArrayDesignator { index, value } => (*index, value.as_ref()),
                _ => (next, initializer),
            };
            if index >= length {
                return Err(self.fail(
                    initializer.line,
                    "array initializer index is outside the array",
                ));
            }
            if !initialized.insert(index) {
                return Err(self.fail(
                    initializer.line,
                    "repeated array initializer index is not supported",
                ));
            }
            let initializer = scalar_initializer(initializer).ok_or_else(|| {
                self.fail(
                    initializer.line,
                    "array element requires a scalar initializer",
                )
            })?;
            let value = self.value(initializer)?;
            values.push((index, self.convert(value, ty, initializer.line)?));
            next = index + 1;
        }
        for index in 0..length {
            if initialized.contains(&index) {
                continue;
            }
            values.push((
                index,
                Value {
                    ty,
                    kind: if ty.pointee().is_some() {
                        ValueKind::Convert(Box::new(Value {
                            ty: Type::Int,
                            kind: ValueKind::Constant(0),
                        }))
                    } else {
                        ValueKind::Constant(0)
                    },
                },
            ));
        }
        Ok(values)
    }

    fn update(
        &self,
        target: Value,
        operator: &str,
        operand: Value,
        postfix: bool,
        line: usize,
    ) -> Result<Value, CCompilerError> {
        if !target.modifiable() {
            return Err(self.fail(line, "update requires a modifiable scalar object"));
        }
        let ty = target.ty;
        let (operation_type, operand) = if ty.pointee().is_some() {
            if !matches!(operator, "+" | "-") || !operand.ty.integer() {
                return Err(self.fail(line, "pointer update requires integer addition/subtraction"));
            }
            (ty, self.pointer_offset(operand, ty, line)?)
        } else {
            if !ty.integer()
                || !operand.ty.integer()
                || !matches!(
                    operator,
                    "+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "<<" | ">>"
                )
            {
                return Err(self.fail(line, "invalid integer update operands"));
            }
            let shift = matches!(operator, "<<" | ">>");
            let operation_type = if shift {
                ty.promote()
            } else {
                ty.common(operand.ty)
            };
            let operand_type = if shift {
                operand.ty.promote()
            } else {
                operation_type
            };
            (operation_type, self.convert(operand, operand_type, line)?)
        };
        Ok(Value {
            ty,
            kind: ValueKind::Update {
                target: Box::new(target),
                operand: Box::new(operand),
                operator: operator.to_owned(),
                operation_type,
                postfix,
            },
        })
    }

    fn condition(&mut self, expression: &Expression) -> Result<Value, CCompilerError> {
        let value = self.value(expression)?;
        if !value.ty.scalar() {
            return Err(self.fail(
                expression.line,
                "condition requires a scalar in initial RV64 subset",
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
                    if declaration.array_dimensions.len() > 1 {
                        return Err(self.fail(
                            line,
                            "multidimensional arrays are not supported by RV64 subset",
                        ));
                    }
                    let ty = Type::parse(&declaration.c_type)
                        .filter(|ty| *ty != Type::Void)
                        .ok_or_else(|| self.fail(line, "unsupported RV64 local type"))?;
                    let slot = self.local(&declaration.name, ty, declaration.array_length, line)?;
                    if let Some(initializer) = &declaration.initializer {
                        if let Some(length) = declaration.array_length {
                            let values = self.array_initializers(initializer, ty, length)?;
                            actions.push(Action::InitializeArray {
                                slot,
                                element: ty,
                                values,
                            });
                            continue;
                        }
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
            StatementKind::While { condition, body } => Action::Loop {
                condition: self.condition(condition)?,
                body: Box::new(self.loop_body(body)?),
                update: None,
                post_test: false,
            },
            StatementKind::DoWhile { body, condition } => Action::Loop {
                body: Box::new(self.loop_body(body)?),
                condition: self.condition(condition)?,
                update: None,
                post_test: true,
            },
            StatementKind::For {
                initializer,
                condition,
                update,
                body,
            } => {
                self.scopes.push(BTreeMap::new());
                let initializer = initializer.as_ref().map(|s| self.action(s)).transpose()?;
                let condition = condition
                    .as_ref()
                    .map(|e| self.condition(e))
                    .transpose()?
                    .unwrap_or(Value {
                        ty: Type::Int,
                        kind: ValueKind::Constant(1),
                    });
                let update = update.as_ref().map(|e| self.value(e)).transpose()?;
                let body = self.loop_body(body)?;
                self.scopes.pop();
                let mut actions = Vec::new();
                if let Some(initializer) = initializer {
                    actions.push(initializer);
                }
                actions.push(Action::Loop {
                    condition,
                    body: Box::new(body),
                    update,
                    post_test: false,
                });
                Action::Block(actions)
            }
            StatementKind::Switch { expression, cases } => {
                let selector = self.value(expression)?;
                if !selector.ty.integer() {
                    return Err(self.fail(line, "switch requires an integer selector"));
                }
                let ty = selector.ty.promote();
                let selector = self.convert(selector, ty, line)?;
                let mut seen = BTreeSet::new();
                let mut lowered = Vec::with_capacity(cases.len());
                self.scopes.push(BTreeMap::new());
                self.break_depth += 1;
                for (case, statements) in cases {
                    let number = if let Some(case) = case {
                        let value = self.value(case)?;
                        if !value.ty.integer() {
                            return Err(self.fail(case.line, "case requires an integer constant"));
                        }
                        let value = self.convert(value, ty, case.line)?;
                        let number = value.constant_integer().ok_or_else(|| {
                            self.fail(
                                case.line,
                                "case requires an LP64 integer constant without undefined arithmetic",
                            )
                        })?;
                        if !seen.insert(number) {
                            return Err(self.fail(
                                case.line,
                                "duplicate case value after selector type conversion",
                            ));
                        }
                        Some(number)
                    } else {
                        None
                    };
                    let actions = statements
                        .iter()
                        .map(|statement| self.action(statement))
                        .collect::<Result<_, _>>()?;
                    lowered.push((number, actions));
                }
                self.break_depth -= 1;
                self.scopes.pop();
                Action::Switch {
                    selector,
                    cases: lowered,
                }
            }
            StatementKind::Break if self.break_depth != 0 => Action::Break,
            StatementKind::Continue if self.loop_depth != 0 => Action::Continue,
            StatementKind::Goto(name) => {
                self.jumps.push((name.clone(), line));
                Action::Goto(name.clone())
            }
            StatementKind::Label { name, statement } => {
                if !self.labels.insert(name.clone()) {
                    return Err(self.fail(line, "duplicate label in function"));
                }
                if matches!(statement.kind, StatementKind::Declaration(_)) {
                    return Err(self.fail(line, "label requires a statement, not a declaration"));
                }
                Action::Label(name.clone(), Box::new(self.action(statement)?))
            }
            StatementKind::Break | StatementKind::Continue => {
                return Err(self.fail(
                    line,
                    "break requires an enclosing loop/switch; continue requires an enclosing loop",
                ));
            }
        })
    }

    fn loop_body(&mut self, body: &Statement) -> Result<Action, CCompilerError> {
        self.loop_depth += 1;
        self.break_depth += 1;
        let body = self.action(body);
        self.loop_depth -= 1;
        self.break_depth -= 1;
        body
    }
}

struct Emit {
    assembly: String,
    next_label: usize,
    return_label: String,
    continues: Vec<String>,
    breaks: Vec<String>,
    internal_functions: BTreeSet<String>,
}

impl Emit {
    fn function_symbol(&self, name: &str) -> String {
        if self.internal_functions.contains(name) {
            format!(".Lrv64_internal_{name}")
        } else {
            name.to_owned()
        }
    }

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
            Type::Bool => self.line("sltu a0, zero, a0"),
            Type::Int => self.line("addiw a0, a0, 0"),
            Type::UInt => self.line("slli a0, a0, 32\nsrli a0, a0, 32"),
            Type::Short => self.line("slli a0, a0, 48\nsrai a0, a0, 48"),
            Type::UShort => self.line("slli a0, a0, 48\nsrli a0, a0, 48"),
            Type::Char if PLAIN_CHAR_IS_UNSIGNED => self.line("andi a0, a0, 255"),
            Type::Char | Type::SignedChar => self.line("slli a0, a0, 56\nsrai a0, a0, 56"),
            Type::Byte => self.line("andi a0, a0, 255"),
            _ => {}
        }
    }

    fn constant(&mut self, number: u64) {
        if let Ok(value) = i32::try_from(number as i64) {
            self.line(format!("li a0, {value}"));
        } else {
            let low = ((number as i64) << 52) >> 52;
            self.constant(number.wrapping_sub(low as u64) >> 12);
            self.line("slli a0, a0, 12");
            if low != 0 {
                self.line(format!("addi a0, a0, {low}"));
            }
        }
    }

    fn operation(&mut self, operator: &str, ty: Type) {
        self.line(match operator {
            "+" => "add a0, t0, a0",
            "-" => "sub a0, t0, a0",
            "*" => "mul a0, t0, a0",
            "/" if ty.unsigned() => "divu a0, t0, a0",
            "/" => "div a0, t0, a0",
            "%" if ty.unsigned() => "remu a0, t0, a0",
            "%" => "rem a0, t0, a0",
            "<<" => "sll a0, t0, a0",
            ">>" if ty.unsigned() => "srl a0, t0, a0",
            ">>" => "sra a0, t0, a0",
            "&" => "and a0, t0, a0",
            "|" => "or a0, t0, a0",
            "^" => "xor a0, t0, a0",
            "<" if ty.unsigned() => "sltu a0, t0, a0",
            "<" => "slt a0, t0, a0",
            ">" if ty.unsigned() => "sltu a0, a0, t0",
            ">" => "slt a0, a0, t0",
            "<=" if ty.unsigned() => "sltu a0, a0, t0\nxori a0, a0, 1",
            "<=" => "slt a0, a0, t0\nxori a0, a0, 1",
            ">=" if ty.unsigned() => "sltu a0, t0, a0\nxori a0, a0, 1",
            ">=" => "slt a0, t0, a0\nxori a0, a0, 1",
            "==" => "xor a0, t0, a0\nsltiu a0, a0, 1",
            "!=" => "xor a0, t0, a0\nsltu a0, zero, a0",
            _ => unreachable!("checked operator"),
        });
    }

    fn load(&mut self, ty: Type) {
        self.line(match ty {
            Type::Bool => "lbu a0, 0(a0)",
            Type::Char if PLAIN_CHAR_IS_UNSIGNED => "lbu a0, 0(a0)",
            Type::Char | Type::SignedChar => "lb a0, 0(a0)",
            Type::Byte => "lbu a0, 0(a0)",
            Type::Int => "lw a0, 0(a0)",
            Type::UInt => "lwu a0, 0(a0)",
            Type::Short => "lh a0, 0(a0)",
            Type::UShort => "lhu a0, 0(a0)",
            Type::Long | Type::ULong => "ld a0, 0(a0)",
            _ if ty.pointee().is_some() => "ld a0, 0(a0)",
            _ => unreachable!("checked scalar load"),
        });
    }

    fn store(&mut self, ty: Type) {
        self.line(match ty {
            Type::Bool | Type::Char | Type::SignedChar | Type::Byte => "sb a0, 0(t0)",
            Type::Int | Type::UInt => "sw a0, 0(t0)",
            Type::Short | Type::UShort => "sh a0, 0(t0)",
            Type::Long | Type::ULong => "sd a0, 0(t0)",
            _ if ty.pointee().is_some() => "sd a0, 0(t0)",
            _ => unreachable!("checked scalar store"),
        });
    }

    fn value(&mut self, value: &Value) {
        match &value.kind {
            ValueKind::Constant(number) => self.constant(*number),
            ValueKind::GlobalAddress(_, address) => self.constant(*address),
            ValueKind::Global(_, address) => {
                self.constant(*address);
                self.load(value.ty);
            }
            ValueKind::Local(slot) => {
                self.line(format!("addi a0, s0, -{}", 24 + slot * 8));
                self.load(value.ty);
            }
            ValueKind::Address(slot) => self.line(format!("addi a0, s0, -{}", 24 + slot * 8)),
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
                self.operation(operator, left.ty);
            }
            ValueKind::Logical(operator, left, right) => {
                let done = self.label();
                self.value(left);
                self.line("sltu a0, zero, a0");
                self.line(format!(
                    "{} a0, zero, {done}",
                    if operator == "&&" { "beq" } else { "bne" }
                ));
                self.value(right);
                self.line(format!("sltu a0, zero, a0\n{done}:"));
            }
            ValueKind::Conditional(condition, when_true, when_false) => {
                let otherwise = self.label();
                let done = self.label();
                self.value(condition);
                self.line(format!("beq a0, zero, {otherwise}"));
                self.value(when_true);
                self.line(format!("j {done}\n{otherwise}:"));
                self.value(when_false);
                self.line(format!("{done}:"));
            }
            ValueKind::Call(name, arguments) => {
                let stack_bytes = (arguments.len().saturating_sub(8) * 8).div_ceil(STACK_ALIGNMENT)
                    * STACK_ALIGNMENT;
                if stack_bytes != 0 {
                    self.line(format!("addi sp, sp, -{stack_bytes}"));
                }
                for argument in arguments {
                    self.value(argument);
                    if argument.ty == Type::UInt {
                        self.line("addiw a0, a0, 0");
                    }
                    self.push();
                }
                if stack_bytes == 0 {
                    for index in (0..arguments.len()).rev() {
                        self.pop(&format!("a{index}"));
                    }
                } else {
                    let temporary_bytes = arguments.len() * STACK_ALIGNMENT;
                    for index in 0..arguments.len() {
                        let source = (arguments.len() - 1 - index) * STACK_ALIGNMENT;
                        if index < 8 {
                            self.line(format!("ld a{index}, {source}(sp)"));
                        } else {
                            let destination = temporary_bytes + (index - 8) * 8;
                            self.line(format!("ld t0, {source}(sp)\nsd t0, {destination}(sp)"));
                        }
                    }
                    self.line(format!("addi sp, sp, {temporary_bytes}"));
                }
                self.line(format!("call {}", self.function_symbol(name)));
                if stack_bytes != 0 {
                    self.line(format!("addi sp, sp, {stack_bytes}"));
                }
            }
            ValueKind::Load(pointer) => {
                self.value(pointer);
                self.load(value.ty);
            }
            ValueKind::Atomic {
                operation,
                pointer,
                operand,
                order,
            } => {
                let ty = pointer.ty.pointee().expect("checked atomic pointer");
                self.value(pointer);
                if let Some(operand) = operand {
                    self.push();
                    self.value(operand);
                    self.pop("t0");
                } else {
                    self.line("mv t0, a0");
                }
                if matches!(operation, AtomicOperation::FetchSub) {
                    self.line("sub a0, zero, a0");
                }
                if *order == 5 {
                    self.line("fence");
                }
                let suffix = match order {
                    1 | 2 => ".aq",
                    3 => ".rl",
                    4 | 5 => ".aqrl",
                    _ => "",
                };
                let width = if ty.size() == Some(4) { "w" } else { "d" };
                if matches!(operation, AtomicOperation::Load) {
                    let suffix = if *order == 5 { ".aq" } else { suffix };
                    self.line(format!("lr.{width}{suffix} a0, (t0)"));
                } else {
                    let destination = if matches!(operation, AtomicOperation::Store) {
                        "zero"
                    } else {
                        "a0"
                    };
                    let instruction = match operation {
                        AtomicOperation::FetchAdd | AtomicOperation::FetchSub => "amoadd",
                        AtomicOperation::FetchAnd => "amoand",
                        AtomicOperation::FetchOr => "amoor",
                        AtomicOperation::FetchXor => "amoxor",
                        AtomicOperation::Store | AtomicOperation::Exchange => "amoswap",
                        AtomicOperation::Load => unreachable!("load handled above"),
                    };
                    self.line(format!(
                        "{instruction}.{width}{suffix} {destination}, a0, (t0)"
                    ));
                }
                if *order == 5 {
                    self.line("fence");
                }
            }
            ValueKind::Assign(target, value) => match &target.kind {
                ValueKind::Global(_, address) => {
                    self.constant(*address);
                    self.push();
                    self.value(value);
                    self.pop("t0");
                    self.store(target.ty);
                }
                ValueKind::Local(slot) => {
                    self.value(value);
                    self.line(format!("sd a0, -{}(s0)", 24 + slot * 8));
                }
                ValueKind::Load(pointer) => {
                    self.value(pointer);
                    self.push();
                    self.value(value);
                    self.pop("t0");
                    self.store(target.ty);
                }
                _ => unreachable!("checked assignment"),
            },
            ValueKind::Update {
                target,
                operand,
                operator,
                operation_type,
                postfix,
            } => {
                match &target.kind {
                    ValueKind::Local(slot) => self.line(format!("addi a0, s0, -{}", 24 + slot * 8)),
                    ValueKind::Global(_, address) => self.constant(*address),
                    ValueKind::Load(pointer) => self.value(pointer),
                    _ => unreachable!("checked update"),
                }
                self.push();
                let local = matches!(target.kind, ValueKind::Local(_));
                self.load(target.ty);
                self.push();
                self.normalize(*operation_type);
                self.push();
                self.value(operand);
                self.pop("t0");
                self.operation(operator, *operation_type);
                self.normalize(target.ty);
                self.pop("t1");
                self.pop("t0");
                if local {
                    self.line("sd a0, 0(t0)");
                } else {
                    self.store(target.ty);
                }
                if *postfix {
                    self.line("mv a0, t1");
                }
            }
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
            Action::InitializeArray {
                slot,
                element,
                values,
            } => {
                let size = element.size().expect("integer element size") as usize;
                for (index, value) in values {
                    self.value(value);
                    self.line(format!("addi t0, s0, -{}", 24 + slot * 8 - index * size));
                    self.store(*element);
                }
            }
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
            Action::Loop {
                condition,
                body,
                update,
                post_test,
            } => {
                let start = self.label();
                let next = self.label();
                let done = self.label();
                self.continues.push(next.clone());
                self.breaks.push(done.clone());
                self.line(format!("{start}:"));
                if !post_test {
                    self.value(condition);
                    self.line(format!("beq a0, zero, {done}"));
                }
                self.action(body);
                self.line(format!("{next}:"));
                if let Some(update) = update {
                    self.value(update);
                }
                if *post_test {
                    self.value(condition);
                    self.line(format!("bne a0, zero, {start}"));
                } else {
                    self.line(format!("j {start}"));
                }
                self.line(format!("{done}:"));
                self.continues.pop();
                self.breaks.pop();
            }
            Action::Switch { selector, cases } => {
                let done = self.label();
                let labels: Vec<_> = cases.iter().map(|_| self.label()).collect();
                let default = cases
                    .iter()
                    .position(|(number, _)| number.is_none())
                    .map(|index| labels[index].as_str())
                    .unwrap_or(&done);
                self.value(selector);
                self.line("mv t0, a0");
                for ((number, _), label) in cases.iter().zip(&labels) {
                    if let Some(number) = number {
                        self.constant(*number);
                        self.line(format!("beq t0, a0, {label}"));
                    }
                }
                self.line(format!("j {default}"));
                self.breaks.push(done.clone());
                for ((_, actions), label) in cases.iter().zip(&labels) {
                    self.line(format!("{label}:"));
                    for action in actions {
                        self.action(action);
                    }
                }
                self.line(format!("{done}:"));
                self.breaks.pop();
            }
            Action::Break => self.line(format!("j {}", self.breaks.last().expect("checked break"))),
            Action::Continue => self.line(format!(
                "j {}",
                self.continues.last().expect("checked loop")
            )),
            Action::Goto(name) => {
                self.line(format!("j {}_named_{name}", self.return_label));
            }
            Action::Label(name, statement) => {
                self.line(format!("{}_named_{name}:", self.return_label));
                self.action(statement);
            }
        }
    }
}

fn signature(declaration: &FunctionDeclaration) -> Option<Signature> {
    if declaration.variadic || !declaration.has_prototype || declaration.parameters.len() > 32 {
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

fn array_length(expression: &Expression) -> Result<usize, CCompilerError> {
    array_constant(expression, false)
}

fn array_designator(expression: &Expression) -> Result<usize, CCompilerError> {
    array_constant(expression, true)
}

fn array_constant(expression: &Expression, allow_zero: bool) -> Result<usize, CCompilerError> {
    let filename = expression.filename.as_deref().unwrap_or("<source>");
    let signatures = BTreeMap::new();
    let globals = BTreeMap::new();
    let mut lower = Lower {
        filename,
        signatures: &signatures,
        globals: &globals,
        typedefs: None,
        enumerators: None,
        scopes: vec![BTreeMap::new()],
        slots: 0,
        result: Type::Void,
        loop_depth: 0,
        break_depth: 0,
        labels: BTreeSet::new(),
        jumps: Vec::new(),
    };
    let value = lower.value(expression)?;
    value
        .constant_integer()
        .filter(|_| !allow_zero || value.integer_constant_expression())
        .filter(|number| {
            (allow_zero || *number > 0) && (value.ty.unsigned() || (*number as i64) >= 0)
        })
        .and_then(|number| usize::try_from(number).ok())
        .filter(|number| !allow_zero || *number < GLOBAL_BYTES)
        .ok_or_else(|| {
            error(
                filename,
                expression.line,
                if allow_zero {
                    "array designator requires a nonnegative LP64 integer constant below 65536 without undefined arithmetic"
                } else {
                    "array length requires a positive LP64 integer constant without undefined arithmetic"
                },
            )
        })
}

fn enumerator(expression: &Expression) -> Result<i64, CCompilerError> {
    let filename = expression.filename.as_deref().unwrap_or("<source>");
    let signatures = BTreeMap::new();
    let globals = BTreeMap::new();
    let mut lower = Lower {
        filename,
        signatures: &signatures,
        globals: &globals,
        typedefs: None,
        enumerators: None,
        scopes: vec![BTreeMap::new()],
        slots: 0,
        result: Type::Void,
        loop_depth: 0,
        break_depth: 0,
        labels: BTreeSet::new(),
        jumps: Vec::new(),
    };
    let value = lower.value(expression)?;
    let number = value.constant_integer().ok_or_else(|| {
        error(
            filename,
            expression.line,
            "enumerator requires an LP64 integer constant without undefined arithmetic",
        )
    })?;
    let signed = if value.ty.unsigned() {
        i64::try_from(number).ok()
    } else {
        Some(number as i64)
    };
    signed
        .and_then(|number| i32::try_from(number).ok())
        .map(i64::from)
        .ok_or_else(|| {
            error(
                filename,
                expression.line,
                "enumerator value is outside the RV64 signed int range",
            )
        })
}

fn global(
    declaration: &Declaration,
    globals: &BTreeMap<String, Global>,
    filename: &str,
) -> Result<Global, CCompilerError> {
    let line = declaration
        .initializer
        .as_ref()
        .map_or(1, |value| value.line);
    let ty = Type::parse(&declaration.c_type)
        .filter(|ty| ty.scalar())
        .ok_or_else(|| {
            error(
                filename,
                line,
                "RV64 globals require supported scalar types",
            )
        })?;
    if declaration.array_dimensions.len() > 1 {
        return Err(error(
            filename,
            line,
            "multidimensional global arrays are not supported",
        ));
    }
    if declaration.array_length.is_some() && ty.pointer().is_none() {
        return Err(error(
            filename,
            line,
            "global arrays require supported scalar elements",
        ));
    }
    let length = declaration.array_length.unwrap_or(1);
    let bytes = length
        .checked_mul(ty.size().expect("integer size") as usize)
        .filter(|bytes| *bytes > 0 && *bytes <= GLOBAL_BYTES)
        .ok_or_else(|| {
            error(
                filename,
                line,
                "global storage exceeds the 65536-byte RAM budget",
            )
        })?;
    let mut object = Global {
        ty,
        array_length: declaration.array_length,
        address: 0,
        bytes: bytes.div_ceil(8) * 8,
        initializer: None,
        pointer_initializer: None,
    };
    if let Some(initializer) = &declaration.initializer {
        let mut visible = globals.clone();
        visible.insert(declaration.name.clone(), object.clone());
        let signatures = BTreeMap::new();
        let mut lower = Lower {
            filename,
            signatures: &signatures,
            globals: &visible,
            typedefs: None,
            enumerators: None,
            scopes: vec![BTreeMap::new()],
            slots: 0,
            result: Type::Void,
            loop_depth: 0,
            break_depth: 0,
            labels: BTreeSet::new(),
            jumps: Vec::new(),
        };
        let values = if declaration.array_length.is_some() {
            lower.array_initializers(initializer, ty, length)?
        } else {
            let initializer = scalar_initializer(initializer)
                .ok_or_else(|| error(filename, line, "global requires a scalar initializer"))?;
            let value = lower.value(initializer)?;
            vec![(0, lower.convert(value, ty, line)?)]
        };
        if ty.pointee().is_some() {
            object.pointer_initializer = Some((line, values));
            return Ok(object);
        }
        let mut image = vec![0; object.bytes];
        let size = ty.size().expect("integer size") as usize;
        for (index, value) in values {
            let number = value.constant_integer().ok_or_else(|| {
                error(filename, line, "global initializer requires an LP64 integer constant without undefined arithmetic")
            })?;
            image[index * size..(index + 1) * size].copy_from_slice(&number.to_le_bytes()[..size]);
        }
        object.initializer = Some(image);
    }
    Ok(object)
}

pub fn compile_to_assembly(source: &str, filename: &str) -> Result<String, CCompilerError> {
    let tokens = lex_unexpanded(source, filename)?;
    for token in &tokens {
        if matches!(
            token.value.as_str(),
            "struct" | "union" | "register" | "auto" | "restrict" | "#"
        ) {
            return Err(error(
                filename,
                token.line,
                "preprocessing, automatic storage classes and records are not supported by initial RV64 subset",
            ));
        }
    }
    let mut signatures = BTreeMap::new();
    let mut definitions = BTreeSet::new();
    let mut globals: BTreeMap<String, Global> = BTreeMap::new();
    for token in &tokens {
        if let Some(mut bytes) = decode_string_token(&token.value) {
            bytes.push(0);
            let length = bytes.len();
            if length > GLOBAL_BYTES {
                return Err(error(
                    filename,
                    token.line,
                    "string storage exceeds the 65536-byte RAM budget",
                ));
            }
            bytes.resize(length.div_ceil(8) * 8, 0);
            globals.entry(token.value.clone()).or_insert(Global {
                ty: Type::Char,
                array_length: Some(length),
                address: 0,
                bytes: bytes.len(),
                initializer: Some(bytes),
                pointer_initializer: None,
            });
        }
    }
    let mut parser = TranslationUnitParser::new(tokens.clone());
    parser.statements.array_length_evaluator = Some(array_length);
    parser.statements.array_designator_evaluator = Some(array_designator);
    parser.enumerator_evaluator = Some(enumerator);
    parser.statements.expressions.resolved_enumerators = Some(HashMap::new());
    parser.file_scope_storage = Some(Vec::new());
    let unit = parser.parse()?;
    if unit
        .enumerators
        .keys()
        .any(|name| atomic_operation(name).is_some())
        || parser
            .statements
            .expressions
            .typedefs
            .keys()
            .any(|name| atomic_operation(name).is_some())
    {
        return Err(error(filename, 1, "atomic builtin name is reserved"));
    }
    let storage = parser
        .file_scope_storage
        .as_ref()
        .expect("RV64 storage metadata");
    let mut internal_linkage = BTreeMap::new();
    let mut object_definitions = BTreeSet::new();
    for (declaration, storage) in unit.declarations.iter().zip(storage) {
        let (name, function) = match declaration {
            ExternalDeclaration::Global(object) => (object.name.as_str(), false),
            ExternalDeclaration::FunctionPrototype(function) => (function.name.as_str(), true),
            ExternalDeclaration::FunctionDefinition(function) => {
                (function.declaration.name.as_str(), true)
            }
        };
        let previous = internal_linkage.get(name).copied();
        if atomic_operation(name).is_some() {
            return Err(error(
                filename,
                storage.line,
                "atomic builtin name is reserved",
            ));
        }
        let internal = match storage.class.as_deref() {
            Some("static") => true,
            Some("extern") => previous.unwrap_or(false),
            None if function => previous.unwrap_or(false),
            None => false,
            _ => {
                return Err(error(
                    filename,
                    storage.line,
                    "unsupported file-scope storage class",
                ));
            }
        };
        if previous.is_some_and(|previous| previous != internal) {
            return Err(error(
                filename,
                storage.line,
                "conflicting internal and external linkage",
            ));
        }
        if name == "main" && internal {
            return Err(error(
                filename,
                storage.line,
                "RV64 ROM requires external int main(void)",
            ));
        }
        internal_linkage.insert(name.to_owned(), internal);
        if !function && !storage.declaration_only {
            object_definitions.insert(name.to_owned());
        }
    }
    for (name, target) in &parser.statements.expressions.typedefs {
        if unit.enumerators.contains_key(name) {
            return Err(error(
                filename,
                1,
                "typedef name conflicts with an enumerator",
            ));
        }
        if Type::parse(target).is_none() {
            return Err(error(
                filename,
                1,
                &format!("unsupported RV64 typedef target for {name:?}: {target}"),
            ));
        }
    }
    for pair in tokens.windows(2) {
        if matches!(pair[0].value.as_str(), "volatile" | "const" | "restrict")
            && parser
                .statements
                .expressions
                .typedefs
                .get(&pair[1].value)
                .and_then(|target| Type::parse(target))
                .is_some_and(|ty| ty.pointee().is_some())
        {
            return Err(error(
                filename,
                pair[0].line,
                "qualified pointer typedef uses are not supported by RV64 subset",
            ));
        }
    }
    for declaration in &unit.declarations {
        let (function, line, definition) = match declaration {
            ExternalDeclaration::FunctionDefinition(f) => (&f.declaration, f.body.line, true),
            ExternalDeclaration::FunctionPrototype(f) => (f, 1, false),
            ExternalDeclaration::Global(declaration) => {
                if unit.enumerators.contains_key(&declaration.name) {
                    return Err(error(
                        filename,
                        1,
                        "object name conflicts with an enumerator",
                    ));
                }
                if parser
                    .statements
                    .expressions
                    .typedefs
                    .contains_key(&declaration.name)
                {
                    return Err(error(filename, 1, "object name conflicts with a typedef"));
                }
                if declaration.name == "_start" || declaration.name.starts_with(".Lrv64_") {
                    return Err(error(filename, 1, "reserved RV64 startup symbol"));
                }
                let object = global(declaration, &globals, filename)?;
                if let Some(previous) = globals.get_mut(&declaration.name) {
                    if previous.ty != object.ty || previous.array_length != object.array_length {
                        return Err(error(filename, 1, "conflicting global declarations"));
                    }
                    if object.initialized() {
                        if previous.initialized() {
                            return Err(error(
                                filename,
                                1,
                                "duplicate initialized global definition",
                            ));
                        }
                        previous.initializer = object.initializer;
                        previous.pointer_initializer = object.pointer_initializer;
                    }
                } else {
                    globals.insert(declaration.name.clone(), object);
                }
                continue;
            }
        };
        if function.name.starts_with(".Lrv64_") || function.name == "_start" {
            return Err(error(filename, line, "reserved RV64 startup symbol"));
        }
        if unit.enumerators.contains_key(&function.name) {
            return Err(error(
                filename,
                line,
                "function name conflicts with an enumerator",
            ));
        }
        if parser
            .statements
            .expressions
            .typedefs
            .contains_key(&function.name)
        {
            return Err(error(
                filename,
                line,
                "function name conflicts with a typedef",
            ));
        }
        let signature = signature(function)
            .ok_or_else(|| error(filename, line, "unsupported RV64 function signature; use prototypes and at most 32 scalar parameters"))?;
        if let Some(previous) = signatures.insert(function.name.clone(), signature.clone()) {
            if previous != signature {
                return Err(error(filename, line, "conflicting function declarations"));
            }
        }
        if definition && !definitions.insert(function.name.clone()) {
            return Err(error(filename, line, "duplicate function definition"));
        }
    }
    if globals.keys().any(|name| signatures.contains_key(name)) {
        return Err(error(
            filename,
            1,
            "name declared as both object and function",
        ));
    }
    if globals
        .keys()
        .any(|name| !name.starts_with("<string:") && !object_definitions.contains(name))
    {
        return Err(error(
            filename,
            1,
            "every declared object must be defined in the source unit",
        ));
    }
    let mut image = Vec::new();
    let mut storage_bytes = 0;
    for initialized in [true, false] {
        for object in globals
            .values_mut()
            .filter(|object| object.initialized() == initialized)
        {
            if object.bytes > GLOBAL_BYTES - storage_bytes {
                return Err(error(
                    filename,
                    1,
                    "global storage exceeds the 65536-byte RAM budget",
                ));
            }
            object.address = GLOBAL_BASE + storage_bytes as u64;
            storage_bytes += object.bytes;
        }
    }
    for object in globals.values().filter(|object| object.initialized()) {
        if let Some(bytes) = &object.initializer {
            image.extend_from_slice(bytes);
        } else if let Some((line, values)) = &object.pointer_initializer {
            let start = image.len();
            image.resize(start + object.bytes, 0);
            for (index, value) in values {
                let address = value.static_pointer(&globals).ok_or_else(|| {
                    error(filename, *line, "pointer initializer requires a static object address or an explicit integer constant cast")
                })?;
                let offset = start + index * 8;
                image[offset..offset + 8].copy_from_slice(&address.to_le_bytes());
            }
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
        assembly: ".text\n.globl _start\n_start:\nli sp, 1\nslli sp, sp, 31\nli t0, 65536\nadd sp, sp, t0\n".to_owned(),
        next_label: 0,
        return_label: String::new(),
        continues: Vec::new(),
        breaks: Vec::new(),
        internal_functions: internal_linkage
            .iter()
            .filter(|(name, internal)| **internal && signatures.contains_key(*name))
            .map(|(name, _)| name.clone())
            .collect(),
    };
    if !image.is_empty() {
        emit.line("la t0, .Lrv64_data");
        emit.constant(GLOBAL_BASE);
        emit.line("mv t1, a0");
        emit.constant(image.len() as u64);
        emit.line("mv t2, a0\n.Lrv64_copy_data:\nlbu t3, 0(t0)\nsb t3, 0(t1)\naddi t0, t0, 1\naddi t1, t1, 1\naddi t2, t2, -1\nbne t2, zero, .Lrv64_copy_data");
    }
    if storage_bytes > image.len() {
        emit.constant(GLOBAL_BASE + image.len() as u64);
        emit.line("mv t1, a0");
        emit.constant((storage_bytes - image.len()) as u64);
        emit.line("mv t2, a0\n.Lrv64_zero_bss:\nsb zero, 0(t1)\naddi t1, t1, 1\naddi t2, t2, -1\nbne t2, zero, .Lrv64_zero_bss");
    }
    emit.line("call main\n.Lrv64_halt:\nwfi\nj .Lrv64_halt");
    let mut visible = BTreeMap::new();
    let mut visible_globals: BTreeMap<_, _> = globals
        .iter()
        .filter(|(name, _)| name.starts_with("<string:"))
        .map(|(name, object)| (name.clone(), object.clone()))
        .collect();
    for declaration in &unit.declarations {
        let function = match declaration {
            ExternalDeclaration::FunctionDefinition(function) => function,
            ExternalDeclaration::FunctionPrototype(function) => {
                visible.insert(function.name.clone(), signatures[&function.name].clone());
                continue;
            }
            ExternalDeclaration::Global(declaration) => {
                visible_globals
                    .insert(declaration.name.clone(), globals[&declaration.name].clone());
                continue;
            }
        };
        let sig = &signatures[&function.declaration.name];
        visible.insert(function.declaration.name.clone(), sig.clone());
        let mut lower = Lower {
            filename,
            signatures: &visible,
            globals: &visible_globals,
            typedefs: Some(&parser.statements.expressions.typedefs),
            enumerators: Some(&unit.enumerators),
            scopes: vec![BTreeMap::new()],
            slots: 0,
            result: sig.result,
            loop_depth: 0,
            break_depth: 0,
            labels: BTreeSet::new(),
            jumps: Vec::new(),
        };
        for (parameter, ty) in function.declaration.parameters.iter().zip(&sig.parameters) {
            let name = parameter.name.as_ref().ok_or_else(|| {
                error(
                    filename,
                    function.body.line,
                    "function definition requires named parameters",
                )
            })?;
            lower.local(name, *ty, None, function.body.line)?;
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
        for (name, line) in &lower.jumps {
            if !lower.labels.contains(name) {
                return Err(lower.fail(*line, &format!("undefined label {name:?} in function")));
            }
        }
        emit.return_label = emit.label();
        emit.line(format!(
            "{}:\naddi sp, sp, -{frame}\nsd ra, {}(sp)\nsd s0, {}(sp)\naddi s0, sp, {frame}",
            emit.function_symbol(&function.declaration.name),
            frame - 8,
            frame - 16,
        ));
        for (index, ty) in sig.parameters.iter().enumerate() {
            if index < 8 {
                emit.line(format!("mv a0, a{index}"));
            } else {
                emit.line(format!("ld a0, {}(s0)", (index - 8) * 8));
            }
            emit.normalize(*ty);
            emit.line(format!("sd a0, -{}(s0)", 24 + index * 8));
        }
        emit.action(&body);
        if function.declaration.name == "main" {
            emit.line("li a0, 0");
        } else if sig.result != Type::Void {
            emit.line("ebreak");
        }
        emit.line(format!("{}:", emit.return_label));
        if sig.result == Type::UInt {
            emit.line("addiw a0, a0, 0");
        }
        emit.line("ld ra, -8(s0)\nld t0, -16(s0)\nmv sp, s0\nmv s0, t0\nret");
    }
    if !image.is_empty() {
        emit.line(".balign 8\n.Lrv64_data:");
        for bytes in image.chunks(16) {
            emit.line(format!(
                ".byte {}",
                bytes
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    Ok(emit.assembly)
}
