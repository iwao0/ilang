use crate::span::Span;
use crate::stmt::Block;

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}

// AST equality is structural over `kind`; spans are metadata and would
// otherwise force tests to thread exact source positions through every
// expected-tree literal.
impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Var(String),
    /// The implicit receiver `this` inside a method body.
    This,
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// Short-circuit logical operator (`&&`, `||`). Separated from `Binary`
    /// so the evaluator can avoid evaluating the rhs when not needed.
    Logical {
        op: LogicalOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// Free function call: `foo(args)`. Method calls go through MethodCall.
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    /// `obj.field` — field read.
    Field {
        obj: Box<Expr>,
        name: String,
    },
    /// `obj.method(args)`.
    MethodCall {
        obj: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    /// `new ClassName(args)` or `new ClassName<T, U>(args)` for
    /// generic instantiations. `type_args` is empty for non-generic
    /// classes.
    New {
        class: String,
        type_args: Vec<crate::Type>,
        args: Vec<Expr>,
    },
    Block(Block),
    If {
        cond: Box<Expr>,
        then_branch: Block,
        /// `None`, another `If` (for `else if`), or a `Block`.
        else_branch: Option<Box<Expr>>,
    },
    While {
        cond: Box<Expr>,
        body: Block,
    },
    /// `for x in iter { body }` — iterates over an array, binding each
    /// element to `var` for the body. Always evaluates to `Unit`.
    ForIn {
        var: String,
        iter: Box<Expr>,
        body: Block,
    },
    /// Infinite loop. Exits only via `break` (or returning from the
    /// enclosing function once `return` exists). Always evaluates to `Unit`.
    Loop {
        body: Block,
    },
    /// Exit the innermost enclosing `loop`/`while`. Type checker rejects
    /// occurrences outside any loop, including across function boundaries.
    Break,
    /// Skip to the next iteration of the innermost enclosing loop.
    Continue,
    /// `return` (with or without a value) — early exit from the
    /// enclosing function/method. Type checker rejects occurrences
    /// outside any function body.
    Return(Option<Box<Expr>>),
    /// Assignment to an existing variable. Always evaluates to `Unit`.
    Assign {
        target: String,
        value: Box<Expr>,
    },
    /// Assignment to a field: `obj.field = value`. Evaluates to `Unit`.
    AssignField {
        obj: Box<Expr>,
        field: String,
        value: Box<Expr>,
    },
    /// Numeric (or `bool`-to-int) cast: `expr as Type`.
    Cast {
        expr: Box<Expr>,
        ty: crate::types::Type,
    },
    /// Anonymous function expression — `fn(p: T): R { body }`. No
    /// captures (closures); at runtime it's a code pointer with the
    /// statically-known `Type::Fn` signature.
    FnExpr {
        params: Vec<crate::Param>,
        ret: Option<crate::types::Type>,
        body: crate::stmt::Block,
    },
    /// Array literal: `[a, b, c]`.
    Array(Vec<Expr>),
    /// Map literal: `{ "a": 1, "b": 2 }`. Keys must be K-typed
    /// expressions (string / int / bool literals at parse time;
    /// validated against the inferred K by the type checker).
    MapLit(Vec<(Expr, Expr)>),
    /// Index read: `obj[idx]`.
    Index {
        obj: Box<Expr>,
        index: Box<Expr>,
    },
    /// Index write: `obj[idx] = value`.
    AssignIndex {
        obj: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
    },
    /// `none` literal — its concrete `T?` type is determined by context.
    None,
    /// `some(x)` constructor: wraps `x` of type `T` as a `T?` value.
    Some(Box<Expr>),
    /// `if let some(name) = expr { then } else { ... }`. Inside `then`,
    /// `name` is bound to the unwrapped value of type `T` (where the
    /// scrutinee has type `T?`).
    IfLet {
        name: String,
        expr: Box<Expr>,
        then_branch: Block,
        else_branch: Option<Box<Expr>>,
    },
    /// `EnumName::Variant` (Phase 1: unit) or `EnumName::Variant(args)` /
    /// `EnumName::Variant { f: v }` (Phase 2: payload).
    EnumCtor {
        enum_name: String,
        variant: String,
        args: CtorArgs,
    },
    /// `match scrutinee { Pattern => body, ... }`.
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CtorArgs {
    Unit,
    Tuple(Vec<Expr>),
    Struct(Vec<(String, Expr)>),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: crate::span::Span,
}

impl PartialEq for MatchArm {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern && self.body == other.body
    }
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: crate::span::Span,
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    /// `_` — matches anything, binds nothing.
    Wildcard,
    /// `EnumName::Variant` (Phase 1) or with bindings (Phase 2).
    Variant {
        enum_name: String,
        variant: String,
        bindings: PatternBindings,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternBindings {
    /// Unit variant (no inner pattern).
    Unit,
    /// `EnumName::Variant(name1, name2)` — positional bindings (`_`
    /// for "ignore"); the strings are the binding names.
    Tuple(Vec<String>),
    /// `EnumName::Variant { f1: name1, f2 }` (shorthand allowed).
    Struct(Vec<(String, String)>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Pos,
    Not,
    /// Bitwise NOT (`~`). Operand must be `i64`.
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// Bitwise operators. All require `i64` on both sides and produce `i64`.
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}
