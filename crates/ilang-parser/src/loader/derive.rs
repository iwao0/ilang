//! `@derive(Eq, Hash)` expansion. Synthesises `equals` /
//! `hashCode` methods on classes that opt in via the attribute, so
//! `Set<MyClass>` / `Map<MyClass, _>` can drive their value-equality
//! protocol against the auto-generated implementations.
//!
//! Hand-written `equals` / `hashCode` always win — the pass only
//! injects methods the class is missing, so a user can derive one
//! side and customise the other.

use ilang_ast::{
    AttrArg, BinOp, Block, ClassDecl, Expr, ExprKind, FnDecl, Item, LogicalOp, Param, Program,
    Span, Stmt, StmtKind, Symbol, Type,
};

use crate::loader::LoadError;

/// Top-level entry point. Walks `prog.items`, calls the synthesizer
/// on every Item::Class that carries `@derive(...)`, and returns the
/// rewritten program. Items not touched are passed through verbatim.
pub(super) fn expand_derives(mut prog: Program) -> Result<Program, LoadError> {
    let mut new_items: Vec<Item> = Vec::with_capacity(prog.items.len());
    for item in prog.items.drain(..) {
        let new = match item {
            Item::Class(c) => Item::Class(expand_class(c)?),
            other => other,
        };
        new_items.push(new);
    }
    prog.items = new_items;
    Ok(prog)
}

fn expand_class(mut c: ClassDecl) -> Result<ClassDecl, LoadError> {
    let derives = collect_derives(&c.attrs);
    if derives.is_empty() {
        return Ok(c);
    }
    let want_eq = derives.iter().any(|s| s.as_str() == "Eq");
    let want_hash = derives.iter().any(|s| s.as_str() == "Hash");
    if !want_eq && !want_hash {
        return Ok(c);
    }
    let methods: Vec<FnDecl> = c.methods.iter().cloned().collect();
    let has_equals = methods.iter().any(|m| m.name.as_str() == "equals");
    let has_hash = methods.iter().any(|m| m.name.as_str() == "hashCode");
    let mut synthesised: Vec<FnDecl> = methods;
    if want_eq && !has_equals {
        synthesised.push(synth_equals(&c)?);
    }
    if want_hash && !has_hash {
        synthesised.push(synth_hash_code(&c)?);
    }
    c.methods = synthesised.into_boxed_slice();
    Ok(c)
}

/// Collect every `Path` arg of every `@derive(...)` attribute on the
/// class. `@derive(Eq, Hash)` returns `["Eq", "Hash"]`.
fn collect_derives(attrs: &[ilang_ast::Attribute]) -> Vec<Symbol> {
    let mut out = Vec::new();
    for attr in attrs {
        if attr.name.as_str() != "derive" {
            continue;
        }
        for arg in attr.args.iter() {
            if let AttrArg::Path(p) = arg {
                if p.len() == 1 {
                    out.push(p[0]);
                }
            }
        }
    }
    out
}

/// Synthesise `pub fn equals(other: ClassName): bool { this.f1 ==
/// other.f1 && this.f2 == other.f2 && ... }`. An empty-field class
/// returns `true` (every instance is equal to every other).
fn synth_equals(c: &ClassDecl) -> Result<FnDecl, LoadError> {
    let span = c.span;
    let other_sym = Symbol::intern("other");
    let body_expr = if c.fields.is_empty() {
        Expr::new(ExprKind::Bool(true), span)
    } else {
        // Fold field comparisons into a chain of `&&`. Walking
        // right-to-left keeps the first comparison at the top of the
        // tree, which is the cheap-fail shape we want.
        let mut iter = c.fields.iter().rev();
        let last = iter.next().expect("non-empty fields");
        let mut acc = field_eq_expr(c.name, last.name, &last.ty, other_sym, span);
        for f in iter {
            let lhs = field_eq_expr(c.name, f.name, &f.ty, other_sym, span);
            acc = Expr::new(
                ExprKind::Logical {
                    op: LogicalOp::And,
                    lhs: Box::new(lhs),
                    rhs: Box::new(acc),
                },
                span,
            );
        }
        acc
    };
    let body = block_with_tail(body_expr, span);
    Ok(FnDecl {
        attrs: Box::new([]),
        is_pub: true,
        name: Symbol::intern("equals"),
        type_params: Box::new([]),
        params: Box::new([Param {
            name: other_sym,
            ty: Type::Object(c.name),
            default: None,
            span,
        }]),
        ret: Some(Type::Bool),
        body,
        span,
        is_override: false,
        is_async: false,
        intrinsic_name: None,
    })
}

fn field_eq_expr(
    class_name: Symbol,
    field: Symbol,
    field_ty: &Type,
    other: Symbol,
    span: Span,
) -> Expr {
    let _ = class_name;
    // Class-typed fields go through the field's own `equals` so a
    // nested `@derive(Eq, Hash)` value compares structurally
    // instead of by reference. Primitive / string fields fall
    // back to the language's `==` (string is structural, ints /
    // bool / floats are value-compare). `equals` returns bool, so
    // the resulting expression slots into the `&&` chain
    // unchanged.
    match field_ty {
        Type::Object(_) => Expr::new(
            ExprKind::MethodCall {
                obj: Box::new(field_on_this(field, span)),
                method: Symbol::intern("equals"),
                args: Box::new([field_on_var(other, field, span)]),
            },
            span,
        ),
        _ => Expr::new(
            ExprKind::Binary {
                op: BinOp::Eq,
                lhs: Box::new(field_on_this(field, span)),
                rhs: Box::new(field_on_var(other, field, span)),
            },
            span,
        ),
    }
}

fn field_on_this(field: Symbol, span: Span) -> Expr {
    Expr::new(
        ExprKind::Field {
            obj: Box::new(Expr::new(ExprKind::This, span)),
            name: field,
        },
        span,
    )
}

fn field_on_var(var: Symbol, field: Symbol, span: Span) -> Expr {
    Expr::new(
        ExprKind::Field {
            obj: Box::new(Expr::new(ExprKind::Var(var), span)),
            name: field,
        },
        span,
    )
}

/// Synthesise `pub fn hashCode(): i64 { let h0 = 17; let h1 = h0 *
/// 31 + (this.f1 as i64); ... hN }`. Each field contributes
/// `(field-hash as i64)` to a folded accumulator. Unsupported
/// field types (strings, floats, nested classes without a derived
/// `hashCode`) fail the expansion with an actionable error rather
/// than silently producing a value that doesn't honour the
/// equals contract.
fn synth_hash_code(c: &ClassDecl) -> Result<FnDecl, LoadError> {
    let span = c.span;
    let mut stmts: Vec<Stmt> = Vec::new();
    let mut last_name = Symbol::intern("__derive_h0");
    stmts.push(let_const_i64(last_name, 17, span));
    for (i, f) in c.fields.iter().enumerate() {
        let next_name = Symbol::intern(&format!("__derive_h{}", i + 1));
        let h_expr = combine_step(last_name, f.name, &f.ty, c.name, span)?;
        stmts.push(let_var(next_name, h_expr, span));
        last_name = next_name;
    }
    let tail = Expr::new(ExprKind::Var(last_name), span);
    let body = Block {
        stmts,
        tail: Some(Box::new(tail)),
    };
    Ok(FnDecl {
        attrs: Box::new([]),
        is_pub: true,
        name: Symbol::intern("hashCode"),
        type_params: Box::new([]),
        params: Box::new([]),
        ret: Some(Type::I64),
        body,
        span,
        is_override: false,
        is_async: false,
        intrinsic_name: None,
    })
}

/// `next = prev * 31 + field_as_i64`. The `field_as_i64` lowering
/// depends on the field type; unsupported types are rejected here.
fn combine_step(
    prev: Symbol,
    field: Symbol,
    ty: &Type,
    class_name: Symbol,
    span: Span,
) -> Result<Expr, LoadError> {
    let field_as_i64 = field_as_i64(field, ty, class_name, span)?;
    let prev_v = Expr::new(ExprKind::Var(prev), span);
    let mul = Expr::new(
        ExprKind::Binary {
            op: BinOp::Mul,
            lhs: Box::new(prev_v),
            rhs: Box::new(Expr::new(ExprKind::Int(31), span)),
        },
        span,
    );
    Ok(Expr::new(
        ExprKind::Binary {
            op: BinOp::Add,
            lhs: Box::new(mul),
            rhs: Box::new(field_as_i64),
        },
        span,
    ))
}

fn field_as_i64(
    field: Symbol,
    ty: &Type,
    class_name: Symbol,
    span: Span,
) -> Result<Expr, LoadError> {
    use Type::*;
    let field_expr = field_on_this(field, span);
    // Every primitive (i*/u*/bool/f32/f64), string, and class
    // routes through the same `.hashCode(): i64` call. Primitives
    // and string have their `hashCode` baked into the type checker
    // + MIR lowering; class fields use their own (manual or
    // `@derive(Hash)`-synthesised) method. The type checker fails
    // the call when the class doesn't carry `hashCode`, so the
    // error surfaces at the synthesised method's own call site
    // rather than during this expansion.
    match ty {
        I8 | I16 | I32 | I64 | U8 | U16 | U32 | U64 | Bool | F32 | F64 | Str | Object(_) => {
            Ok(Expr::new(
                ExprKind::MethodCall {
                    obj: Box::new(field_expr),
                    method: Symbol::intern("hashCode"),
                    args: Box::new([]),
                },
                span,
            ))
        }
        _ => Err(LoadError::AsyncLowerError {
            reason: format!(
                "@derive(Hash) on class {class:?}: field {field:?} has type \
                 {ty} which is not supported by auto-derived `hashCode`. \
                 Supported field types: every primitive (i*/u*/bool/f32/f64), \
                 string, enums, and classes that derive (or define) \
                 `hashCode`. Implement `hashCode` manually for this class \
                 to handle the unsupported field type.",
                class = class_name,
            ),
            span,
        }),
    }
}

fn let_const_i64(name: Symbol, value: i64, span: Span) -> Stmt {
    let value_expr = Expr::new(ExprKind::Int(value), span);
    Stmt::new(
        StmtKind::Let {
            is_pub: false,
            is_const: false,
            name,
            ty: Some(Type::I64),
            value: value_expr,
        },
        span,
    )
}

fn let_var(name: Symbol, value: Expr, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::Let {
            is_pub: false,
            is_const: false,
            name,
            ty: Some(Type::I64),
            value,
        },
        span,
    )
}

fn block_with_tail(tail: Expr, span: Span) -> Block {
    let _ = span;
    Block {
        stmts: Vec::new(),
        tail: Some(Box::new(tail)),
    }
}
