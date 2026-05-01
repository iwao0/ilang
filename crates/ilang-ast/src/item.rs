use crate::span::Span;
use crate::stmt::Block;
use crate::types::Type;

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

/// Attribute on a function declaration, e.g. `#[requires(net, file::read)]`.
/// Phase 2 parses these but does not enforce them.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub args: Vec<AttrArg>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttrArg {
    /// A capability path like `net` or `file::read`.
    Path(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub attrs: Vec<Attribute>,
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    /// All methods of the class. The constructor is the method named `init`
    /// (treated as a regular method by the parser; recognised specially by
    /// the type checker and evaluator).
    pub methods: Vec<FnDecl>,
    pub span: Span,
}

/// One variant of an `enum`. Phase 1 supports unit-only variants; Phase 2
/// adds `Tuple` and `Struct` payload kinds.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: String,
    pub payload: VariantPayload,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VariantPayload {
    /// `Color::Red` — no associated data.
    Unit,
    /// `Shape::Circle(f64)` — positional payload.
    Tuple(Vec<Type>),
    /// `Shape::Square { side: f64 }` — named payload.
    Struct(Vec<FieldDecl>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<Variant>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Fn(FnDecl),
    Class(ClassDecl),
    Enum(EnumDecl),
}
