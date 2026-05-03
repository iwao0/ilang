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
    /// A quoted string literal — used by `@extern("libname")` to
    /// name the dynamic library to dlopen at JIT init.
    Str(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub attrs: Vec<Attribute>,
    pub name: String,
    /// Generic type parameters declared on the fn (e.g. `<T, U>`).
    /// Empty for non-generic fns. Inside the body, references to
    /// these names are rewritten to `Type::TypeVar` by the type
    /// checker, and concrete types are inferred from arg types at
    /// each call site.
    pub type_params: Vec<String>,
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
    /// Generic type parameters declared on the class (e.g. `<T, U>`).
    /// Empty for non-generic classes. Inside the class body, references
    /// to these names parse as `Type::TypeVar`.
    pub type_params: Vec<String>,
    pub fields: Vec<FieldDecl>,
    /// All methods of the class. The constructor is the method named `init`
    /// (treated as a regular method by the parser; recognised specially by
    /// the type checker and evaluator).
    pub methods: Vec<FnDecl>,
    /// `get`/`set` accessors. Read/write of `obj.name` is dispatched to
    /// the corresponding accessor instead of a stored field. Both are
    /// optional (read-only or write-only OK), but at least one is set.
    /// Stored separately from `methods` so method-name lookups don't
    /// trip over property accessors.
    pub properties: Vec<PropertyDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDecl {
    pub name: String,
    /// The property's value type. For getters it's the return type; for
    /// setters it's the (single) parameter type. The type checker
    /// enforces that getter ret == setter param == this `ty`.
    pub ty: Type,
    /// Synthetic FnDecl for the getter body: 0 params, returns `ty`.
    /// `name` field of the FnDecl is the property name itself.
    pub getter: Option<FnDecl>,
    /// Synthetic FnDecl for the setter body: 1 param of type `ty`,
    /// returns `()`. `name` field is the property name.
    pub setter: Option<FnDecl>,
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
    /// Generic type parameters declared on the enum (e.g. `<T, E>`).
    /// Empty for non-generic enums. Variant payload types reference
    /// these names; the type checker rewrites them to `Type::TypeVar`
    /// when registering the enum's signature.
    pub type_params: Vec<String>,
    pub variants: Vec<Variant>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Fn(FnDecl),
    Class(ClassDecl),
    Enum(EnumDecl),
    /// `use module` (whole-module namespace import) or
    /// `use module { name1, name2 }` (selective import).
    /// The loader resolves the path and merges items; the AST node
    /// is removed from the Program before type checking.
    Use(UseDecl),
    /// `const NAME [: T] = literal` — top-level immutable binding.
    /// Restricted to literal values (no expressions). After loader
    /// merge, references to the (possibly module-prefixed) name are
    /// substituted with the literal directly, so type checker /
    /// interpreter / JIT never see Item::Const themselves.
    Const(ConstDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub name: String,
    pub ty: Option<crate::types::Type>,
    pub value: crate::expr::Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    /// The module identifier (`utils` resolves to `utils.il` next to
    /// the importing file).
    pub module: String,
    /// `None` for whole-module import (`use utils`); `Some(names)`
    /// for selective import (`use utils { foo, bar }`).
    pub selective: Option<Vec<String>>,
    pub span: Span,
}
