//! Abstract syntax tree for neo-lang (see README.md).

#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub package: Option<String>,
    pub imports: Vec<ImportDecl>,
    pub structs: Vec<StructDecl>,
    pub functions: Vec<FunctionDecl>,
    pub contract: Option<ContractDecl>, // only one contract is allowed
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub name: String,
    pub library: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContractDecl {
    pub attributes: Vec<Attribute>,
    pub name: String,
    pub members: Vec<ContractMember>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContractMember {
    ConstProp(ConstProp),
    Field(ContractField),
    Event(EventDecl),
    Function(FunctionDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstProp {
    pub ty: Type,
    pub name: String,
    pub init: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContractField {
    pub ty: Type,
    pub name: String,
    pub init: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventDecl {
    pub name: String,
    pub params: Vec<Param>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<StructField>,
    pub methods: Vec<FunctionDecl>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub ty: Type,
    pub name: String,
    pub init: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub attributes: Vec<Attribute>,
    pub return_ty: Type,
    pub name: String,
    pub params: Vec<Param>,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub ty: Type,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Var {
        name: String,
        init: Option<Expr>,
    },
    /// Expression statement (`e;`). Assignments are `Expr::Assign` inside.
    Expr(Expr),
    If {
        cond: Expr,
        then_block: Block,
        else_block: Option<Block>,
    },
    ForArray {
        item: String,
        iter: Expr,
        body: Block,
    },
    ForMap {
        key: String,
        value: String,
        map: Expr,
        body: Block,
    },
    While {
        cond: Expr,
        body: Block,
    },
    Return(Option<Expr>),
    Emit {
        name: String,
        args: Vec<Expr>,
    },
    Block(Block),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    ShrAssign,
    ShlAssign,
    AmpAssign,
    PipeAssign,
    CaretAssign,
}

impl AssignOp {
    pub fn to_binary_op(self) -> Option<BinaryOp> {
        match self {
            AssignOp::PlusAssign => Some(BinaryOp::Add),
            AssignOp::MinusAssign => Some(BinaryOp::Sub),
            AssignOp::StarAssign => Some(BinaryOp::Mul),
            AssignOp::SlashAssign => Some(BinaryOp::Div),
            AssignOp::PercentAssign => Some(BinaryOp::Mod),
            AssignOp::ShrAssign => Some(BinaryOp::Shr),
            AssignOp::ShlAssign => Some(BinaryOp::Shl),
            AssignOp::AmpAssign => Some(BinaryOp::BitAnd),
            AssignOp::PipeAssign => Some(BinaryOp::BitOr),
            AssignOp::CaretAssign => Some(BinaryOp::BitXor),
            AssignOp::Assign => None,
        }
    }
}

impl std::fmt::Display for AssignOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssignOp::Assign => write!(f, "="),
            AssignOp::PlusAssign => write!(f, "+="),
            AssignOp::MinusAssign => write!(f, "-="),
            AssignOp::StarAssign => write!(f, "*="),
            AssignOp::SlashAssign => write!(f, "/="),
            AssignOp::PercentAssign => write!(f, "%="),
            AssignOp::ShrAssign => write!(f, ">>="),
            AssignOp::ShlAssign => write!(f, "<<="),
            AssignOp::AmpAssign => write!(f, "&="),
            AssignOp::PipeAssign => write!(f, "|="),
            AssignOp::CaretAssign => write!(f, "^="),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Assign {
        target: Box<Expr>,
        op: AssignOp,
        value: Box<Expr>,
    }, // target = value
    Literal(Literal), // literal value
    Ident(String),    // identifier
    Self_,            // self
    Cast {
        expr: Box<Expr>,
        ty: Type,
    }, // expr as type
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    }, // expr1 op expr2
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Member {
        base: Box<Expr>,
        field: String,
    }, // expr.field
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    }, // expr[index]
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    }, // expr(args1, args2, ...)
    StructLit {
        name: String,
        fields: Vec<(String, Expr)>,
    }, // struct { field1: expr1, field2: expr2, ... }
    /// Map literal: `map[K, V] { k: v, ... }` only (`ty` is always [`Type::Map`]).
    MapLit {
        ty: Type,
        pairs: Vec<(Expr, Expr)>,
    },
    /// Array literal: `ElemType[] { expr1, expr2, ... }` only (`ty` is always [`Type::Array`]).
    ArrayLit {
        ty: Type,
        elements: Vec<Expr>,
    },
    Paren(Box<Expr>), // (expr)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Positive,
    Negative,
    Not,
    BitNot,
}

impl std::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnaryOp::Positive => write!(f, "+"),
            UnaryOp::Negative => write!(f, "-"),
            UnaryOp::Not => write!(f, "!"),
            UnaryOp::BitNot => write!(f, "~"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Mul,
    Div,
    Mod,
    Add,
    Sub,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryOp::Mul => write!(f, "*"),
            BinaryOp::Div => write!(f, "/"),
            BinaryOp::Mod => write!(f, "%"),
            BinaryOp::Add => write!(f, "+"),
            BinaryOp::Sub => write!(f, "-"),
            BinaryOp::Shl => write!(f, "<<"),
            BinaryOp::Shr => write!(f, ">>"),
            BinaryOp::BitAnd => write!(f, "&"),
            BinaryOp::BitOr => write!(f, "|"),
            BinaryOp::BitXor => write!(f, "^"),
            BinaryOp::Eq => write!(f, "=="),
            BinaryOp::Ne => write!(f, "!="),
            BinaryOp::Lt => write!(f, "<"),
            BinaryOp::Le => write!(f, "<="),
            BinaryOp::Gt => write!(f, ">"),
            BinaryOp::Ge => write!(f, ">="),
            BinaryOp::And => write!(f, "&&"),
            BinaryOp::Or => write!(f, "||"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Literal {
    Null,
    Bool(bool),
    Int(String),    // string literal
    String(String), // string literal
    Buffer(String), // string literal
}

impl std::fmt::Display for Literal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Literal::Null => write!(f, "null"),
            Literal::Bool(b) => write!(f, "{}", b),
            Literal::Int(s) => write!(f, "{}", s),
            Literal::String(s) => write!(f, "\"{s}\""),
            Literal::Buffer(s) => write!(f, "b\"{s}\""),
        }
    }
}

// Type is a type definition in neo-lang.
// The primitive types are: bool, int, string, hash160, hash256.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Void,
    Bool,
    Int,
    String,
    Hash160,
    Hash256,
    Buffer,
    Any,
    Named(String),
    Array(Box<Type>),
    Map { key: Box<Type>, value: Box<Type> },
}

impl Type {
    pub(crate) fn can_assign_to(&self, to: &Type) -> bool {
        self == to || matches!(self, Type::Any) || matches!(to, Type::Any)
    }

    /// Allowed `map` key types (NeoVM map keys are scalar-like in neo-lang).
    pub(crate) fn is_valid_map_key_type(&self) -> bool {
        matches!(
            self,
            Type::Bool | Type::Int | Type::String | Type::Hash160 | Type::Hash256 | Type::Buffer
        )
    }

    pub(crate) fn is_primitive(&self) -> bool {
        matches!(
            self,
            Type::Int | Type::Bool | Type::String | Type::Hash160 | Type::Hash256
        )
    }

    pub(crate) fn is_map(&self) -> bool {
        matches!(self, Type::Map { .. })
    }

    pub(crate) fn is_array(&self) -> bool {
        matches!(self, Type::Array(_))
    }
}

// impl to_string for Type
impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Void => write!(f, "void"),
            Type::Bool => write!(f, "bool"),
            Type::Int => write!(f, "int"),
            Type::String => write!(f, "string"),
            Type::Hash160 => write!(f, "hash160"),
            Type::Hash256 => write!(f, "hash256"),
            Type::Buffer => write!(f, "buffer"),
            Type::Any => write!(f, "any"),
            Type::Named(n) => write!(f, "{}", n),
            Type::Array(inner) => write!(f, "{}[]", inner),
            Type::Map { key, value } => write!(f, "map[{}, {}]", key, value),
        }
    }
}
