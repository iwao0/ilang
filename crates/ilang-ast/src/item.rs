use crate::span::Span;
use crate::stmt::Block;
use crate::types::Type;

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub span: Span,
    /// `Some(expr)` when the parameter has a default (e.g.
    /// `mode: string = "r"`). Defaults are valid only on trailing
    /// parameters — once a parameter has one, every later parameter
    /// must too. The type checker fills these in at call sites whose
    /// arity is short of the declared count.
    pub default: Option<crate::expr::Expr>,
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
    /// An integer literal — used by `@bits(N)` to declare a bitfield
    /// width.
    Int(i64),
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
    /// `true` for `override <method>(...)` declarations inside a
    /// class body — the method must replace a same-named one from
    /// an ancestor class (signature must match). Always `false` for
    /// top-level fns and non-override methods.
    pub is_override: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub ty: Type,
    pub span: Span,
    /// Bitfield width in bits, set by `@bits(N)` on the field. Only
    /// valid inside `@repr(C)` classes on unsigned integer types.
    /// `None` means a normal full-width field.
    pub bits: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    /// `Some(libname)` for an opaque-handle class declared as
    /// `@extern("lib") class Foo {}` — the value is a raw C pointer
    /// returned by a native extern fn, not an ilang-managed instance.
    /// `new`, fields, and methods are all rejected by the type
    /// checker for these classes; the type tag exists only to keep
    /// different libraries' handles from being mixed up.
    pub extern_lib: Option<String>,
    /// `true` for `@repr(C) class Foo { ... }` — the class is laid
    /// out with C-compatible field offsets (each field at its
    /// natural alignment, no ilang-specific padding) so native
    /// extern fns can marshal it as a `T *`. Methods, init, and
    /// inheritance are forbidden for these classes; `new ClassName`
    /// (no args) zero-initializes the storage.
    pub is_repr_c: bool,
    /// `@repr(C, packed)` — drop natural alignment so every field
    /// sits at offset = sum-of-prior-sizes (no padding) and the
    /// struct's overall alignment is 1. Mirrors C's
    /// `__attribute__((packed))`. Only meaningful with `is_repr_c`.
    pub is_packed: bool,
    /// `@repr(C, union)` — every field shares the same offset (0)
    /// and the struct size is the maximum field size. C union
    /// semantics: writing one field overwrites the others.
    pub is_union: bool,
    pub name: String,
    /// `class Child extends Parent { ... }` — single-inheritance
    /// parent. `None` for root classes. The parent class must be
    /// declared before the child (no forward references for now).
    pub parent: Option<String>,
    /// Generic type parameters declared on the class (e.g. `<T, U>`).
    /// Empty for non-generic classes. Inside the class body, references
    /// to these names parse as `Type::TypeVar`.
    pub type_params: Vec<String>,
    pub fields: Vec<FieldDecl>,
    /// All methods of the class. The constructor is the method named `init`
    /// (treated as a regular method by the parser; recognised specially by
    /// the type checker and evaluator).
    pub methods: Vec<FnDecl>,
    /// `static` methods — callable via `ClassName.method(args)` with
    /// no `this`. Stored in their own Vec so instance-method lookups
    /// don't trip over them, and so the JIT can register each as a
    /// plain top-level fn (no receiver param).
    pub static_methods: Vec<FnDecl>,
    /// `static` fields — class-level mutable storage. The initial
    /// `value` must fold to a literal at compile time (same rules
    /// as top-level `const`). Allowed types are `i64` / `f64` /
    /// `bool` for now; heap types await a Phase-2 design.
    pub static_fields: Vec<StaticFieldDecl>,
    /// `get`/`set` accessors. Read/write of `obj.name` is dispatched to
    /// the corresponding accessor instead of a stored field. Both are
    /// optional (read-only or write-only OK), but at least one is set.
    /// Stored separately from `methods` so method-name lookups don't
    /// trip over property accessors.
    pub properties: Vec<PropertyDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StaticFieldDecl {
    pub name: String,
    pub ty: crate::types::Type,
    /// Compile-time-evaluable initializer. After the loader's
    /// `inline_constants` pass this is a literal Expr.
    pub value: crate::expr::Expr,
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
    /// Explicit discriminant value (e.g. `Foo = 0x10`). Only allowed
    /// on `Unit` payload variants. `None` means "use the auto-assigned
    /// declaration index".
    pub discriminant: Option<i64>,
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
    /// Optional explicit underlying integer type
    /// (`enum Flag: u32 { ... }`). When `None`, defaults to the
    /// codegen-internal `i32` tag. Numeric primitive types only
    /// (the type checker enforces).
    pub repr_ty: Option<Type>,
    /// `@flags` attribute — bitflag enum. Bitwise ops (`|`, `&`, `^`,
    /// `~`) are allowed between values, and `.has(other)` is generated.
    /// Combined values that don't match any single variant are
    /// represented as raw bits.
    pub flags: bool,
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
    /// `@extern("libname") static name: T` — read/write reference to
    /// a C global variable resolved via dlsym at JIT init. The host
    /// stores the symbol address; reads/writes lower to a load/store
    /// against that address. Type is restricted to numeric / bool.
    ExternStatic(ExternStaticDecl),
    /// `@extern(C) { ... }` — C ABI block. Inside this block raw
    /// pointer types (`*char`, `*void`, `*const T`, etc.) are
    /// nameable, and `struct` / `union` declarations replace `class`.
    /// Items declared here use the C calling convention. Raw pointer
    /// values cannot escape the block — extern fn returns of pointer
    /// type must be wrapped by an in-block helper that converts to
    /// an ilang type.
    ExternC(ExternCBlock),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternCBlock {
    pub items: Vec<ExternCItem>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExternCItem {
    /// C struct (= `@repr(C) class` equivalent inside the block).
    /// Methods / properties are not allowed; only fields. `packed`
    /// and `@bits(N)` are still supported.
    Struct {
        name: String,
        fields: Vec<FieldDecl>,
        is_packed: bool,
        span: Span,
    },
    /// C union — every field at offset 0, size = max field size.
    Union {
        name: String,
        fields: Vec<FieldDecl>,
        span: Span,
    },
    /// `@lib("libname") fn name(...): T` — declaration only, dlsym'd
    /// from the named library. `libs` may have multiple entries (each
    /// tried in order, fallback for soname differences). Empty `libs`
    /// = host-side extern pre-registered via `JITBuilder::symbol`.
    /// `optional = true` (`@optional`) lets the JIT keep going when
    /// the library can't be loaded.
    FnDecl {
        name: String,
        params: Vec<Param>,
        ret: Option<crate::types::Type>,
        libs: Vec<String>,
        optional: bool,
        /// `fn snprintf(buf: *u8, n: size_t, fmt: *const char, ...)`
        /// — trailing `...` marks the C variadic. Extra arguments at
        /// the call site lower with their actual JIT types.
        variadic: bool,
        /// `@symbol("name")` overrides the C symbol used at dlsym
        /// time so the ilang-side fn name can differ from the C one
        /// (e.g. `fn my_sprintf` calling `sprintf`). `None` means use
        /// `name` as both the ilang name and the C symbol.
        c_symbol: Option<String>,
        span: Span,
    },
    /// `fn name(...): T { body }` — ilang-side definition with C ABI.
    /// Used to write callbacks that C will call back into.
    FnDef(FnDecl),
    /// `static name: T` — C global variable. `libs`/`optional` mirror
    /// the FnDecl flags.
    Static {
        name: String,
        ty: crate::types::Type,
        libs: Vec<String>,
        optional: bool,
        span: Span,
    },
    /// `class Foo { ... }` — ilang-side ARC-managed wrapper class
    /// declared next to the FFI bindings it wraps. Method bodies
    /// run in the `@extern(C)` context so they can call the block's
    /// raw extern fns / marshalling helpers / use raw pointer types.
    Class(ClassDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternStaticDecl {
    pub name: String,
    pub ty: crate::types::Type,
    /// `Some("libfoo")` for `@extern("libfoo") static …`. `None` for
    /// host-side `@extern static …` (the host pre-registers the
    /// symbol with `JITBuilder::symbol`).
    pub lib: Option<String>,
    pub span: Span,
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
    /// `@export use mod` — re-export `mod`'s items under the current
    /// module's namespace. Inside the entrypoint program this flag
    /// has no effect (no parent prefix to re-export under).
    pub re_export: bool,
    pub span: Span,
}
