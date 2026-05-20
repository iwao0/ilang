//! `const` inlining pass.
//!
//! Replace every reference to a top-level `const NAME = expr` with
//! the (constant-folded) value of `expr`. Runs after module merging
//! so cross-module consts resolve. Failure yields `LoadError::
//! ConstFold` carrying the source-span diagnostic.

use std::collections::HashMap;

use ilang_ast::{
    BinOp, Block, ClassDecl, Expr, ExprKind, Item, LogicalOp, Program, Span, Stmt, StmtKind,
    Symbol, Type, UnOp,
};

use super::LoadError;

// ─── const substitution ────────────────────────────────────────────────

/// Walk the merged program collecting every `Item::Const`, then
/// replace `Var(const_name)` references everywhere with the literal
/// RHS. Removes the Item::Const entries from the output. Consts are
/// allowed to reference module-prefixed names (e.g. `math.pi` after
/// the loader's mangling) since the substitution happens by exact
/// name match.
pub(super) fn inline_constants(prog: Program) -> Result<Program, LoadError> {
    // Walk items in declaration order and fold each `const`'s RHS to a
    // literal, using already-folded consts as known bindings. The
    // result becomes the substitution value for every `Var(name)`
    // reference in the rest of the program.
    let mut consts: HashMap<Symbol, Expr> = HashMap::new();
    // Annotated types — looked up at substitution time so each
    // substituted reference carries the const's declared type via
    // a wrapping `Cast`. Unannotated consts (`const N = 5`) leave
    // their entry absent and substitute as the bare literal (i64
    // for ints, the natural literal type otherwise).
    let mut const_types: HashMap<Symbol, ilang_ast::Type> = HashMap::new();
    let mut items_no_const: Vec<Item> = Vec::new();
    // Top-level `const NAME = expr` whose RHS isn't a compile-time
    // constant get demoted to a runtime `let` (with `is_const:
    // true` so the type checker still rejects reassignment).
    // Collected here in declaration order; prepended to
    // `prog.stmts` at the end so initialisation runs before any
    // user code that references the name.
    let mut runtime_consts: Vec<ilang_ast::Stmt> = Vec::new();
    // Helper: fold + register a single ConstDecl. The body of the
    // original `Item::Const` arm was duplicated here so the same
    // logic runs for both top-level consts and consts hoisted out
    // of `@extern(C) { ... }` blocks.
    fn process_const(
        c: ilang_ast::ConstDecl,
        consts: &mut HashMap<Symbol, Expr>,
        const_types: &mut HashMap<Symbol, ilang_ast::Type>,
        runtime_consts: &mut Vec<ilang_ast::Stmt>,
    ) -> Result<(), LoadError> {
        let fold_result = fold_const_expr(&c.value, consts);
        let folded = match fold_result {
            Ok(f) => f,
            Err(_reason) => {
                let span = c.value.span;
                let const_mod = c
                    .name
                    .as_str()
                    .rfind('.')
                    .map(|i| Symbol::intern(&c.name.as_str()[..i]));
                runtime_consts.push(ilang_ast::Stmt {
                    kind: ilang_ast::StmtKind::Let {
                        is_pub: c.is_pub,
                        is_const: true,
                        name: c.name.clone(),
                        ty: c.ty.clone(),
                        value: c.value,
                    },
                    span,
                    source_module: const_mod,
                });
                return Ok(());
            }
        };
        if let Some(ty) = &c.ty {
            let wrappable = matches!(
                &folded.kind,
                ExprKind::Int(_) | ExprKind::Float(_)
            );
            if wrappable {
                if let ExprKind::Int(n) = &folded.kind {
                    if !int_literal_fits(*n, ty) {
                        return Err(LoadError::BadConst {
                            name: c.name.clone(),
                            reason: format!(
                                "literal value {n} doesn't fit declared type {ty}"
                            ),
                            span: c.value.span,
                        });
                    }
                }
                const_types.insert(c.name.clone(), ty.clone());
            }
        }
        consts.insert(c.name, folded);
        Ok(())
    }

    for item in prog.items {
        match item {
            Item::ExternC(mut b) => {
                // Pull consts out of the block and fold them as if
                // they were top-level — they were declared inside
                // `@extern(C) { ... }` to make raw-pointer / C-only
                // types legal in the annotation / RHS, not to
                // bundle them with the rest of the FFI items.
                let extern_consts = std::mem::take(&mut b.consts);
                for c in extern_consts.into_vec() {
                    process_const(c, &mut consts, &mut const_types, &mut runtime_consts)?;
                }
                items_no_const.push(Item::ExternC(b));
            }
            Item::Const(c) => {
                process_const(c, &mut consts, &mut const_types, &mut runtime_consts)?;
            }
            other => items_no_const.push(other),
        }
    }
    // Fold each class's static-field initializers using the same
    // rules. The folded literal sits on the AST until the
    // interpreter / JIT pulls it for storage init. Array initialisers
    // are left untouched — the JIT allocates an empty array at
    // `__main` startup, so the AST value isn't read for them.
    for item in items_no_const.iter_mut() {
        if let Item::Class(c) = item {
            let class_name = c.name.clone();
            for sf in c.static_fields.iter_mut() {
                if matches!(sf.value.kind, ExprKind::Array(_)) {
                    continue;
                }
                // String-typed static fields always go through the
                // runtime-init path: the slot's 8 bytes hold a heap
                // pointer that gets filled in at program startup
                // (the static-data section can't carry a literal
                // string). Fall through to the demote branch even if
                // the value is a literal.
                let force_runtime = matches!(sf.ty, ilang_ast::Type::Str);
                let fold_result = if force_runtime {
                    Err(String::new())
                } else {
                    fold_const_expr(&sf.value, &consts)
                };
                match fold_result {
                    Ok(folded) => {
                        sf.value = folded;
                    }
                    Err(_reason) => {
                        // Non-foldable initializer — emit a runtime
                        // assignment that fills in the real value at
                        // program startup. `is_init: true` exempts
                        // the synthetic write from the
                        // "cannot assign to const static field"
                        // rule. We clone the original expression for
                        // the synthetic write but leave `sf.value`
                        // alone so hover / pretty-printers still
                        // show the user's source expression. The
                        // MIR lower picks a typed zero default for
                        // non-literal slot inits.
                        let span = sf.value.span;
                        let value_expr = sf.value.clone();
                        // Tag the synthetic init with the class's
                        // own module so the type checker judges
                        // the AssignField (and any non-pub fns the
                        // RHS calls) from inside that module — not
                        // from the entry, which would falsely
                        // report Class.field as module-private.
                        let class_mod = class_name
                            .as_str()
                            .rfind('.')
                            .map(|i| Symbol::intern(&class_name.as_str()[..i]));
                        runtime_consts.push(ilang_ast::Stmt {
                            kind: ilang_ast::StmtKind::Expr(Expr::new(
                                ExprKind::AssignField {
                                    obj: Box::new(Expr::new(
                                        ExprKind::Var(class_name.clone()),
                                        span,
                                    )),
                                    field: sf.name.clone(),
                                    value: Box::new(value_expr),
                                    is_init: true,
                                },
                                span,
                            )),
                            span,
                            source_module: class_mod,
                        });
                    }
                }
            }
        }
    }
    // Combine runtime-const initialisers (front) with the user's
    // existing top-level statements so constants run before
    // anything that might reference them.
    let mut combined_stmts: Vec<ilang_ast::Stmt> =
        Vec::with_capacity(runtime_consts.len() + prog.stmts.len());
    combined_stmts.extend(runtime_consts.into_iter());
    combined_stmts.extend(prog.stmts.into_iter());
    if consts.is_empty() {
        return Ok(Program {
            items: items_no_const.into(),
            stmts: combined_stmts,
            tail: prog.tail,
        });
    }
    let ctx = SubstCtx { consts: &consts, types: &const_types };
    Ok(Program {
        items: items_no_const
            .into_iter()
            .map(|i| subst_const_item(i, &ctx))
            .collect(),
        stmts: combined_stmts
            .into_iter()
            .map(|s| subst_const_stmt(s, &ctx))
            .collect(),
        tail: prog.tail.map(|e| subst_const_expr(e, &ctx)),
    })
}

struct SubstCtx<'a> {
    consts: &'a HashMap<Symbol, Expr>,
    types: &'a HashMap<Symbol, ilang_ast::Type>,
}

/// Constant folder. Reduces `e` to a literal `Expr` (Int / Float /
/// Bool / Str), or returns a human-readable failure reason.
/// Supported: literals, references to other consts, unary `- ! ~`,
/// binary arithmetic / comparison / bitwise / logical, `as` casts
/// between numeric types, string `+` (concat) and `==` / `!=`.
/// True iff a folded integer literal `n` fits the declared numeric
/// type `t`. Mirrors the type checker's `int_literal_fits` rule —
/// kept local to the loader because that crate doesn't depend on
/// `ilang-types`. `Type::Float`s and non-numeric types accept any
/// `n` (no narrowing concern).
fn int_literal_fits(n: i64, t: &ilang_ast::Type) -> bool {
    use ilang_ast::Type;
    match t {
        Type::I8 => i8::try_from(n).is_ok(),
        Type::I16 => i16::try_from(n).is_ok(),
        Type::I32 => i32::try_from(n).is_ok(),
        Type::I64 => true,
        Type::U8 => u8::try_from(n).is_ok(),
        Type::U16 => u16::try_from(n).is_ok(),
        Type::U32 => u32::try_from(n).is_ok(),
        Type::U64 => n >= 0,
        _ => true,
    }
}

fn fold_const_expr(e: &Expr, consts: &HashMap<Symbol, Expr>) -> Result<Expr, String> {
    let span = e.span;
    let lit = |k: ExprKind| Expr { kind: k, span };
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) => {
            Ok(e.clone())
        }
        ExprKind::Var(name) => consts
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unknown identifier `{name}` in const expression")),
        ExprKind::Unary { op, expr } => {
            let v = fold_const_expr(expr, consts)?;
            match (op, &v.kind) {
                (UnOp::Neg, ExprKind::Int(n)) => Ok(lit(ExprKind::Int(n.wrapping_neg()))),
                (UnOp::Neg, ExprKind::Float(x)) => Ok(lit(ExprKind::Float(-x))),
                (UnOp::Not, ExprKind::Bool(b)) => Ok(lit(ExprKind::Bool(!b))),
                (UnOp::BitNot, ExprKind::Int(n)) => Ok(lit(ExprKind::Int(!n))),
                _ => Err(format!("unary {op:?} not supported in const expression")),
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let l = fold_const_expr(lhs, consts)?;
            let r = fold_const_expr(rhs, consts)?;
            fold_binary(*op, &l, &r, span)
        }
        ExprKind::Logical { op, lhs, rhs } => {
            let l = fold_const_expr(lhs, consts)?;
            let lb = match l.kind {
                ExprKind::Bool(b) => b,
                _ => return Err("logical operands must be bool".into()),
            };
            // Short-circuit, like the runtime would.
            match op {
                LogicalOp::And if !lb => Ok(lit(ExprKind::Bool(false))),
                LogicalOp::Or if lb => Ok(lit(ExprKind::Bool(true))),
                _ => {
                    let r = fold_const_expr(rhs, consts)?;
                    match r.kind {
                        ExprKind::Bool(b) => Ok(lit(ExprKind::Bool(b))),
                        _ => Err("logical operands must be bool".into()),
                    }
                }
            }
        }
        ExprKind::Cast { expr, ty } => {
            let v = fold_const_expr(expr, consts)?;
            cast_const(&v, ty, span)
        }
        // Anything else (calls, fields, control flow, ...) is not a
        // constant expression. Be specific in the error so the user
        // knows what to fix.
        other => Err(format!(
            "expression {} is not allowed in `const`",
            describe_expr_kind(other)
        )),
    }
}

fn fold_binary(op: BinOp, l: &Expr, r: &Expr, span: Span) -> Result<Expr, String> {
    let lit = |k: ExprKind| Expr { kind: k, span };
    use ExprKind::*;
    match (&l.kind, &r.kind) {
        (Int(a), Int(b)) => Ok(lit(match op {
            BinOp::Add => Int(a.wrapping_add(*b)),
            BinOp::Sub => Int(a.wrapping_sub(*b)),
            BinOp::Mul => Int(a.wrapping_mul(*b)),
            BinOp::Div => {
                if *b == 0 {
                    return Err("division by zero in const expression".into());
                }
                // `wrapping_div` so `i64::MIN / -1` doesn't panic;
                // matches the wrapping behaviour of `+` / `-` / `*`.
                Int(a.wrapping_div(*b))
            }
            BinOp::Rem => {
                if *b == 0 {
                    return Err("modulo by zero in const expression".into());
                }
                Int(a.wrapping_rem(*b))
            }
            BinOp::BitAnd => Int(a & b),
            BinOp::BitOr => Int(a | b),
            BinOp::BitXor => Int(a ^ b),
            BinOp::Shl => {
                if *b < 0 || *b >= 64 {
                    return Err(format!(
                        "shift amount {b} out of range 0..64 in const expression"
                    ));
                }
                Int(a.wrapping_shl(*b as u32))
            }
            BinOp::Shr => {
                if *b < 0 || *b >= 64 {
                    return Err(format!(
                        "shift amount {b} out of range 0..64 in const expression"
                    ));
                }
                Int(a.wrapping_shr(*b as u32))
            }
            BinOp::Eq => Bool(a == b),
            BinOp::Ne => Bool(a != b),
            BinOp::Lt => Bool(a < b),
            BinOp::Le => Bool(a <= b),
            BinOp::Gt => Bool(a > b),
            BinOp::Ge => Bool(a >= b),
        })),
        (Float(a), Float(b)) => Ok(lit(match op {
            BinOp::Add => Float(a + b),
            BinOp::Sub => Float(a - b),
            BinOp::Mul => Float(a * b),
            BinOp::Div => Float(a / b),
            BinOp::Eq => Bool(a == b),
            BinOp::Ne => Bool(a != b),
            BinOp::Lt => Bool(a < b),
            BinOp::Le => Bool(a <= b),
            BinOp::Gt => Bool(a > b),
            BinOp::Ge => Bool(a >= b),
            _ => return Err(format!("operator {op:?} not supported on float in const")),
        })),
        (Str(a), Str(b)) => Ok(lit(match op {
            BinOp::Add => Str(format!("{a}{b}")),
            BinOp::Eq => Bool(a == b),
            BinOp::Ne => Bool(a != b),
            _ => return Err(format!("operator {op:?} not supported on string in const")),
        })),
        (Bool(a), Bool(b)) => Ok(lit(match op {
            BinOp::Eq => Bool(a == b),
            BinOp::Ne => Bool(a != b),
            BinOp::BitAnd => Bool(a & b),
            BinOp::BitOr => Bool(a | b),
            BinOp::BitXor => Bool(a ^ b),
            _ => return Err(format!("operator {op:?} not supported on bool in const")),
        })),
        _ => Err(format!(
            "type mismatch in const binary {op:?} ({} vs {})",
            describe_expr_kind(&l.kind),
            describe_expr_kind(&r.kind)
        )),
    }
}

fn cast_const(v: &Expr, ty: &Type, span: Span) -> Result<Expr, String> {
    let lit = |k: ExprKind| Expr { kind: k, span };
    use ExprKind::*;
    match (&v.kind, ty) {
        // int → int: truncate / zero-extend to match the runtime
        // `as` cast. The AST stores `i64` so we round-trip through
        // the target width to discard high bits, then re-extend.
        // `i64` / `u64` are no-ops at this width.
        (Int(n), Type::I8) => Ok(lit(Int((*n as i8) as i64))),
        (Int(n), Type::I16) => Ok(lit(Int((*n as i16) as i64))),
        (Int(n), Type::I32) => Ok(lit(Int((*n as i32) as i64))),
        (Int(n), Type::I64) => Ok(lit(Int(*n))),
        (Int(n), Type::U8) => Ok(lit(Int((*n as u8) as i64))),
        (Int(n), Type::U16) => Ok(lit(Int((*n as u16) as i64))),
        (Int(n), Type::U32) => Ok(lit(Int((*n as u32) as i64))),
        (Int(n), Type::U64) => Ok(lit(Int(*n))),
        (Int(n), Type::F32 | Type::F64) => Ok(lit(Float(*n as f64))),
        (Float(x), Type::F32 | Type::F64) => Ok(lit(Float(*x))),
        (Float(x), Type::I8 | Type::I16 | Type::I32 | Type::I64
            | Type::U8 | Type::U16 | Type::U32 | Type::U64) => Ok(lit(Int(*x as i64))),
        (Bool(b), Type::I8 | Type::I16 | Type::I32 | Type::I64
            | Type::U8 | Type::U16 | Type::U32 | Type::U64) => {
            Ok(lit(Int(if *b { 1 } else { 0 })))
        }
        // Integer literal → raw C pointer (`0 as *void`, `0 as
        // *const GUID`). Keep the value as `Int(n)` — the
        // surrounding const declaration's type annotation lets
        // the inline-substitution step re-wrap each reference as
        // `n as <ty>`, which the type checker validates under
        // the @extern(C) scope where the use site sits.
        (Int(n), Type::RawPtr { .. }) => Ok(lit(Int(*n))),
        _ => Err(format!("cast to {ty} not supported in const expression")),
    }
}

fn describe_expr_kind(k: &ExprKind) -> &'static str {
    match k {
        ExprKind::Int(_) => "int literal",
        ExprKind::Float(_) => "float literal",
        ExprKind::Bool(_) => "bool literal",
        ExprKind::Str(_) => "string literal",
        ExprKind::Var(_) => "identifier",
        ExprKind::Call { .. } => "function call",
        ExprKind::MethodCall { .. } => "method call",
        ExprKind::New { .. } => "object construction",
        ExprKind::Field { .. } => "field access",
        ExprKind::Index { .. } => "index",
        ExprKind::Array(_) => "array literal",
        ExprKind::MapLit(_) => "map literal",
        ExprKind::If { .. } => "if expression",
        ExprKind::IfLet { .. } => "if-let expression",
        ExprKind::Match { .. } => "match",
        ExprKind::Block(_) => "block",
        ExprKind::While { .. } | ExprKind::Loop { .. } | ExprKind::ForIn { .. } => "loop",
        ExprKind::Range { .. } => "range",
        _ => "non-constant expression",
    }
}

fn subst_const_item(item: Item, ctx: &SubstCtx<'_>) -> Item {
    match item {
        Item::Fn(mut f) => {
            f.body = subst_const_block(f.body, ctx);
            Item::Fn(f)
        }
        Item::Class(mut c) => {
            subst_const_class_in_place(&mut c, ctx);
            Item::Class(c)
        }
        Item::ExternC(mut b) => {
            // Recurse into the block's fn / class bodies. Without
            // this, bare `Var(X)` references to module-level consts
            // inside an `@extern(C) {}` wrapper survive into the
            // type-checker and fail to resolve.
            for inner in b.items.iter_mut() {
                match inner {
                    ilang_ast::ExternCItem::FnDef(f) => {
                        let body = std::mem::replace(
                            &mut f.body,
                            Block { stmts: Vec::new(), tail: None },
                        );
                        f.body = subst_const_block(body, ctx);
                    }
                    ilang_ast::ExternCItem::Class(c) => {
                        subst_const_class_in_place(c, ctx);
                    }
                    _ => {}
                }
            }
            Item::ExternC(b)
        }
        other => other,
    }
}

fn subst_const_class_in_place(c: &mut ClassDecl, ctx: &SubstCtx<'_>) {
    for m in c.methods.iter_mut().chain(c.static_methods.iter_mut()) {
        let body = std::mem::replace(
            &mut m.body,
            Block { stmts: Vec::new(), tail: None },
        );
        m.body = subst_const_block(body, ctx);
    }
    for prop in &mut c.properties {
        if let Some(g) = prop.getter.as_mut() {
            let body = std::mem::replace(
                &mut g.body,
                Block { stmts: Vec::new(), tail: None },
            );
            g.body = subst_const_block(body, ctx);
        }
        if let Some(s) = prop.setter.as_mut() {
            let body = std::mem::replace(
                &mut s.body,
                Block { stmts: Vec::new(), tail: None },
            );
            s.body = subst_const_block(body, ctx);
        }
    }
}

fn subst_const_block(b: Block, ctx: &SubstCtx<'_>) -> Block {
    Block {
        stmts: b
            .stmts
            .into_iter()
            .map(|s| subst_const_stmt(s, ctx))
            .collect(),
        tail: b.tail.map(|e| Box::new(subst_const_expr(*e, ctx))),
    }
}

fn subst_const_stmt(s: Stmt, ctx: &SubstCtx<'_>) -> Stmt {
    let kind = match s.kind {
        StmtKind::Let { is_pub, is_const, name, ty, value } => StmtKind::Let {
            is_pub,
            is_const,
            name,
            ty,
            value: subst_const_expr(value, ctx),
        },
        StmtKind::LetTuple { elems, value } => StmtKind::LetTuple {
            elems,
            value: subst_const_expr(value, ctx),
        },
        StmtKind::LetStruct { class, fields, value } => StmtKind::LetStruct {
            class,
            fields,
            value: subst_const_expr(value, ctx),
        },
        StmtKind::Expr(e) => StmtKind::Expr(subst_const_expr(e, ctx)),
    };
    Stmt { kind, span: s.span, source_module: s.source_module.clone() }
}

fn subst_const_expr(e: Expr, ctx: &SubstCtx<'_>) -> Expr {
    let span = e.span;
    let kind = match e.kind {
        // The substitution itself: `Var(name)` whose name is a const.
        // Re-apply the const's span to the literal so error messages
        // point at the use site, not the declaration site.
        ExprKind::Var(ref name) => {
            if let Some(lit) = ctx.consts.get(name) {
                let mut new_lit = lit.clone();
                new_lit.span = span;
                // If the const had an annotated type, wrap the
                // literal in a Cast so the substituted reference
                // carries that type. This lets `const N: u32 = 16`
                // be used in `i32 < N` style sites without a manual
                // `as u32` at every call.
                if let Some(ty) = ctx.types.get(name) {
                    return Expr::new(
                        ExprKind::Cast {
                            expr: Box::new(new_lit),
                            ty: ty.clone(),
                        },
                        span,
                    );
                }
                return new_lit;
            }
            ExprKind::Var(name.clone())
        }
        // Mechanical recursion through every other shape.
        ExprKind::Unary { op, expr } => ExprKind::Unary {
            op,
            expr: Box::new(subst_const_expr(*expr, ctx)),
        },
        ExprKind::Binary { op, lhs, rhs } => ExprKind::Binary {
            op,
            lhs: Box::new(subst_const_expr(*lhs, ctx)),
            rhs: Box::new(subst_const_expr(*rhs, ctx)),
        },
        ExprKind::Logical { op, lhs, rhs } => ExprKind::Logical {
            op,
            lhs: Box::new(subst_const_expr(*lhs, ctx)),
            rhs: Box::new(subst_const_expr(*rhs, ctx)),
        },
        ExprKind::Cast { expr, ty } => ExprKind::Cast {
            expr: Box::new(subst_const_expr(*expr, ctx)),
            ty,
        },
        ExprKind::TypeTest { expr, ty } => ExprKind::TypeTest {
            expr: Box::new(subst_const_expr(*expr, ctx)),
            ty,
        },
        ExprKind::TypeDowncast { expr, ty } => ExprKind::TypeDowncast {
            expr: Box::new(subst_const_expr(*expr, ctx)),
            ty,
        },
        ExprKind::FnExpr { params, ret, body } => ExprKind::FnExpr {
            params,
            ret,
            body: subst_const_block(body, ctx),
        },
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee,
            args: Vec::from(args).into_iter().map(|a| subst_const_expr(a, ctx)).collect(),
        },
        ExprKind::Field { obj, name } => ExprKind::Field {
            obj: Box::new(subst_const_expr(*obj, ctx)),
            name,
        },
        ExprKind::MethodCall { obj, method, args } => ExprKind::MethodCall {
            obj: Box::new(subst_const_expr(*obj, ctx)),
            method,
            args: Vec::from(args).into_iter().map(|a| subst_const_expr(a, ctx)).collect(),
        },
        ExprKind::New { class, type_args, args, init_method } => ExprKind::New {
            class,
            type_args,
            args: Vec::from(args).into_iter().map(|a| subst_const_expr(a, ctx)).collect(),
            init_method,
        },
        ExprKind::Block(b) => ExprKind::Block(subst_const_block(b, ctx)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => ExprKind::If {
            cond: Box::new(subst_const_expr(*cond, ctx)),
            then_branch: subst_const_block(then_branch, ctx),
            else_branch: else_branch.map(|e| Box::new(subst_const_expr(*e, ctx))),
        },
        ExprKind::IfLet {
            name,
            expr,
            then_branch,
            else_branch,
        } => ExprKind::IfLet {
            name,
            expr: Box::new(subst_const_expr(*expr, ctx)),
            then_branch: subst_const_block(then_branch, ctx),
            else_branch: else_branch.map(|e| Box::new(subst_const_expr(*e, ctx))),
        },
        ExprKind::While { cond, body } => ExprKind::While {
            cond: Box::new(subst_const_expr(*cond, ctx)),
            body: subst_const_block(body, ctx),
        },
        ExprKind::Loop { body } => ExprKind::Loop {
            body: subst_const_block(body, ctx),
        },
        ExprKind::ForIn { var, iter, body } => ExprKind::ForIn {
            var,
            iter: Box::new(subst_const_expr(*iter, ctx)),
            body: subst_const_block(body, ctx),
        },
        ExprKind::Range { start, end, inclusive } => ExprKind::Range {
            start: start.map(|s| Box::new(subst_const_expr(*s, ctx))),
            end: end.map(|e| Box::new(subst_const_expr(*e, ctx))),
            inclusive,
        },
        ExprKind::Closure { fn_name, captures } => {
            ExprKind::Closure { fn_name, captures }
        }
        ExprKind::SuperCall { method, args } => ExprKind::SuperCall {
            method,
            args: Vec::from(args).into_iter().map(|a| subst_const_expr(a, ctx)).collect(),
        },
        ExprKind::Return(opt) => {
            ExprKind::Return(opt.map(|e| Box::new(subst_const_expr(*e, ctx))))
        }
        ExprKind::Break(opt) => {
            ExprKind::Break(opt.map(|e| Box::new(subst_const_expr(*e, ctx))))
        }
        ExprKind::Assign { target, value } => ExprKind::Assign {
            target,
            value: Box::new(subst_const_expr(*value, ctx)),
        },
        ExprKind::AssignField { obj, field, value, is_init } => ExprKind::AssignField {
            obj: Box::new(subst_const_expr(*obj, ctx)),
            field,
            value: Box::new(subst_const_expr(*value, ctx)), is_init },
        ExprKind::AssignIndex { obj, index, value } => ExprKind::AssignIndex {
            obj: Box::new(subst_const_expr(*obj, ctx)),
            index: Box::new(subst_const_expr(*index, ctx)),
            value: Box::new(subst_const_expr(*value, ctx)),
        },
        ExprKind::Array(items) => ExprKind::Array(
            Vec::from(items).into_iter().map(|e| subst_const_expr(e, ctx)).collect(),
        ),
        ExprKind::Tuple(items) => ExprKind::Tuple(
            Vec::from(items).into_iter().map(|e| subst_const_expr(e, ctx)).collect(),
        ),
        ExprKind::MapLit(entries) => ExprKind::MapLit(
            entries
                .into_iter()
                .map(|(k, v)| (subst_const_expr(k, ctx), subst_const_expr(v, ctx)))
                .collect(),
        ),
        ExprKind::Index { obj, index } => ExprKind::Index {
            obj: Box::new(subst_const_expr(*obj, ctx)),
            index: Box::new(subst_const_expr(*index, ctx)),
        },
        ExprKind::Some(inner) => ExprKind::Some(Box::new(subst_const_expr(*inner, ctx))),
        ExprKind::Await(inner) => ExprKind::Await(Box::new(subst_const_expr(*inner, ctx))),
        ExprKind::EnumCtor {
            enum_name,
            variant,
            args,
        } => ExprKind::EnumCtor {
            enum_name,
            variant,
            args: match args {
                ilang_ast::CtorArgs::Unit => ilang_ast::CtorArgs::Unit,
                ilang_ast::CtorArgs::Tuple(es) => ilang_ast::CtorArgs::Tuple(
                    Vec::from(es).into_iter().map(|e| subst_const_expr(e, ctx)).collect(),
                ),
                ilang_ast::CtorArgs::Struct(fs) => ilang_ast::CtorArgs::Struct(
                    fs.into_iter()
                        .map(|(n, e)| (n, subst_const_expr(e, ctx)))
                        .collect(),
                ),
            },
        },
        ExprKind::Match { scrutinee, arms } => ExprKind::Match {
            scrutinee: Box::new(subst_const_expr(*scrutinee, ctx)),
            arms: arms
                .into_iter()
                .map(|arm| ilang_ast::MatchArm {
                    pattern: arm.pattern,
                    body: subst_const_expr(arm.body, ctx),
                    span: arm.span,
                })
                .collect(),
        },
        // Trivial nodes pass through.
        other @ (ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::This
        | ExprKind::None
        | ExprKind::Continue) => other,
        ExprKind::StructLit { class, fields, field_name_spans } => ExprKind::StructLit {
            class,
            fields: fields
                .into_iter()
                .map(|(n, e)| (n, subst_const_expr(e, ctx)))
                .collect(),
            field_name_spans,
        },
    };
    Expr { kind, span }
}

