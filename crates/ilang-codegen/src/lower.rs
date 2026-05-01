use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{I64, I8};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};
use ilang_ast::{
    BinOp, Expr, ExprKind, FnDecl, Item, LogicalOp, Program, Stmt, StmtKind, Type, UnOp,
};

use crate::error::CodegenError;

/// Result of running a JITed program. Always an `i64` for the MVP — the
/// CLI prints it as the program's value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JitValue {
    Int(i64),
    Bool(bool),
    Unit,
}

impl std::fmt::Display for JitValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JitValue::Int(n) => write!(f, "{n}"),
            JitValue::Bool(b) => write!(f, "{b}"),
            JitValue::Unit => Ok(()),
        }
    }
}

/// Compile every function in `prog` plus a synthetic `__main` that
/// evaluates the top-level statements / tail expression, then jump into
/// `__main` and return its value.
pub fn jit_run(prog: &Program) -> Result<JitValue, CodegenError> {
    let mut compiler = JitCompiler::new()?;

    // Pass 1: declare every user fn so calls can resolve in any order.
    for item in &prog.items {
        if let Item::Fn(f) = item {
            compiler.declare_fn(f)?;
        }
    }
    // Pass 2: define each fn body.
    for item in &prog.items {
        if let Item::Fn(f) = item {
            compiler.define_fn(f)?;
        } else {
            return Err(CodegenError::Unsupported {
                what: "class".into(),
                span: ilang_ast::Span::dummy(),
            });
        }
    }

    let main_ret = compiler.define_main(prog)?;
    compiler.finalize()?;
    let result = compiler.run_main(main_ret);
    Ok(result)
}

/// Logical type of a value at JIT time. Mirrors what we know about the
/// AST node's `Type` once we've narrowed it to the JIT's supported set.
#[derive(Debug, Clone, Copy, PartialEq)]
enum JitTy {
    Int,
    Bool,
    Unit,
}

impl JitTy {
    fn from_ast(t: &Type, span: ilang_ast::Span) -> Result<Self, CodegenError> {
        match t {
            Type::I64 => Ok(JitTy::Int),
            Type::Bool => Ok(JitTy::Bool),
            Type::Unit => Ok(JitTy::Unit),
            other => Err(CodegenError::UnsupportedType {
                ty: other.clone(),
                span,
            }),
        }
    }

    fn cl(self) -> Option<types::Type> {
        match self {
            JitTy::Int => Some(I64),
            JitTy::Bool => Some(I8),
            JitTy::Unit => None,
        }
    }
}

struct JitCompiler {
    module: JITModule,
    ctx: cranelift_codegen::Context,
    builder_ctx: FunctionBuilderContext,
    /// fn name → (FuncId, signature info) for cross-function calls.
    funcs: HashMap<String, (cranelift_module::FuncId, Vec<JitTy>, JitTy)>,
}

impl JitCompiler {
    fn new() -> Result<Self, CodegenError> {
        let flag_builder = settings::builder();
        let isa_builder = cranelift_native::builder()
            .map_err(|e| CodegenError::Cranelift(format!("isa builder: {e}")))?;
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|e| CodegenError::Cranelift(format!("isa: {e}")))?;
        let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        let module = JITModule::new(builder);
        let ctx = module.make_context();
        Ok(Self {
            module,
            ctx,
            builder_ctx: FunctionBuilderContext::new(),
            funcs: HashMap::new(),
        })
    }

    fn declare_fn(&mut self, f: &FnDecl) -> Result<(), CodegenError> {
        let mut params = Vec::with_capacity(f.params.len());
        for p in &f.params {
            params.push(JitTy::from_ast(&p.ty, p.span)?);
        }
        let ret = match &f.ret {
            Some(t) => JitTy::from_ast(t, f.span)?,
            None => JitTy::Unit,
        };
        let mut sig = self.module.make_signature();
        for p in &params {
            sig.params.push(AbiParam::new(p.cl().ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "unit-typed parameter".into(),
                    span: f.span,
                }
            })?));
        }
        if let Some(t) = ret.cl() {
            sig.returns.push(AbiParam::new(t));
        }
        let id = self
            .module
            .declare_function(&f.name, Linkage::Local, &sig)
            .map_err(|e| CodegenError::Module(e.to_string()))?;
        self.funcs.insert(f.name.clone(), (id, params, ret));
        Ok(())
    }

    fn define_fn(&mut self, f: &FnDecl) -> Result<(), CodegenError> {
        let (id, param_tys, ret_ty) = self.funcs[&f.name].clone();
        // Reset the context for this function.
        self.module.clear_context(&mut self.ctx);
        self.ctx.func.signature = self.module.declarations().get_function_decl(id).signature.clone();

        let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut env = Env::default();
        for (i, p) in f.params.iter().enumerate() {
            let ty = param_tys[i].cl().expect("non-unit checked at declare");
            let var = Variable::new(env.next_var_id());
            builder.declare_var(var, ty);
            let v = builder.block_params(entry)[i];
            builder.def_var(var, v);
            env.bindings.insert(p.name.clone(), (var, param_tys[i]));
        }

        let mut lc = LowerCtx {
            funcs: &self.funcs,
            module: &mut self.module,
            env: &mut env,
            loops: Vec::new(),
        };
        let body = lower_block_value(&mut builder, &mut lc, &f.body, ret_ty)?;
        match (ret_ty, body) {
            (JitTy::Unit, _) => {
                builder.ins().return_(&[]);
            }
            (_, Some(v)) => {
                builder.ins().return_(&[v]);
            }
            (_, None) => {
                return Err(CodegenError::Unsupported {
                    what: format!("function {:?} body produces no value", f.name),
                    span: f.span,
                });
            }
        }
        builder.finalize();

        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|e| CodegenError::Module(e.to_string()))?;
        Ok(())
    }

    /// Wrap top-level statements / tail expression in a synthetic `__main`.
    /// Returns the JIT type of the program's final value.
    fn define_main(&mut self, prog: &Program) -> Result<JitTy, CodegenError> {
        let ret_ty = prog
            .tail
            .as_ref()
            .map(|t| infer_ty_lite(t, &self.funcs).unwrap_or(JitTy::Unit))
            .unwrap_or(JitTy::Unit);

        let mut sig = self.module.make_signature();
        if let Some(t) = ret_ty.cl() {
            sig.returns.push(AbiParam::new(t));
        }
        let id = self
            .module
            .declare_function("__main", Linkage::Export, &sig)
            .map_err(|e| CodegenError::Module(e.to_string()))?;

        self.module.clear_context(&mut self.ctx);
        self.ctx.func.signature = sig;

        let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut env = Env::default();
        let mut lc = LowerCtx {
            funcs: &self.funcs,
            module: &mut self.module,
            env: &mut env,
            loops: Vec::new(),
        };
        for s in &prog.stmts {
            lower_stmt(&mut builder, &mut lc, s)?;
        }
        match (ret_ty, &prog.tail) {
            (JitTy::Unit, _) => {
                builder.ins().return_(&[]);
            }
            (_, Some(t)) => {
                let v = lower_expr(&mut builder, &mut lc, t)?;
                let v = v.ok_or_else(|| CodegenError::Unsupported {
                    what: "tail expression produces no value".into(),
                    span: t.span,
                })?;
                builder.ins().return_(&[v]);
            }
            _ => return Err(CodegenError::NoTopLevelValue),
        }
        builder.finalize();

        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|e| CodegenError::Module(e.to_string()))?;
        // Stash the main signature alongside fns for run_main lookup.
        self.funcs
            .insert("__main".into(), (id, vec![], ret_ty));
        Ok(ret_ty)
    }

    fn finalize(&mut self) -> Result<(), CodegenError> {
        self.module
            .finalize_definitions()
            .map_err(|e| CodegenError::Module(e.to_string()))?;
        Ok(())
    }

    fn run_main(&mut self, ret: JitTy) -> JitValue {
        let (id, _, _) = self.funcs["__main"];
        let ptr = self.module.get_finalized_function(id);
        unsafe {
            match ret {
                JitTy::Int => {
                    let f: extern "C" fn() -> i64 = std::mem::transmute(ptr);
                    JitValue::Int(f())
                }
                JitTy::Bool => {
                    let f: extern "C" fn() -> i8 = std::mem::transmute(ptr);
                    JitValue::Bool(f() != 0)
                }
                JitTy::Unit => {
                    let f: extern "C" fn() = std::mem::transmute(ptr);
                    f();
                    JitValue::Unit
                }
            }
        }
    }
}

#[derive(Default)]
struct Env {
    bindings: HashMap<String, (Variable, JitTy)>,
    next_id: u32,
}

impl Env {
    fn next_var_id(&mut self) -> usize {
        let id = self.next_id as usize;
        self.next_id += 1;
        id
    }
}

struct LowerCtx<'a> {
    funcs: &'a HashMap<String, (cranelift_module::FuncId, Vec<JitTy>, JitTy)>,
    module: &'a mut JITModule,
    env: &'a mut Env,
    /// Active loops, each as `(continue_target, break_target)`.
    loops: Vec<(Block, Block)>,
}

/// Best-effort guess at a tail expression's JIT type. We can't always be
/// precise because we lower greedily without re-running the type checker,
/// but for the AST shapes we accept this is enough to pick `__main`'s
/// return signature. `funcs` lets us look up user fn return types so a
/// trailing `myfn()` call infers correctly.
fn infer_ty_lite(
    e: &Expr,
    funcs: &HashMap<String, (cranelift_module::FuncId, Vec<JitTy>, JitTy)>,
) -> Option<JitTy> {
    match &e.kind {
        ExprKind::Int(_) => Some(JitTy::Int),
        ExprKind::Bool(_) => Some(JitTy::Bool),
        ExprKind::Unary { expr, op } => match op {
            UnOp::Not => Some(JitTy::Bool),
            _ => infer_ty_lite(expr, funcs),
        },
        ExprKind::Binary { op, lhs, .. } => match op {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                Some(JitTy::Bool)
            }
            _ => infer_ty_lite(lhs, funcs),
        },
        ExprKind::Logical { .. } => Some(JitTy::Bool),
        ExprKind::Call { callee, .. } => funcs.get(callee).map(|(_, _, ret)| *ret),
        ExprKind::If { then_branch, .. } => then_branch
            .tail
            .as_deref()
            .and_then(|t| infer_ty_lite(t, funcs))
            .or(Some(JitTy::Unit)),
        ExprKind::Block(b) => b
            .tail
            .as_deref()
            .and_then(|t| infer_ty_lite(t, funcs))
            .or(Some(JitTy::Unit)),
        ExprKind::While { .. } | ExprKind::Loop { .. } => Some(JitTy::Unit),
        _ => Some(JitTy::Int),
    }
}

fn lower_stmt(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    s: &Stmt,
) -> Result<(), CodegenError> {
    match &s.kind {
        StmtKind::Let { name, ty, value } => {
            let v = lower_expr(b, lc, value)?;
            let v = v.ok_or_else(|| CodegenError::Unsupported {
                what: "let value produces no value".into(),
                span: value.span,
            })?;
            // Prefer the annotation when present; otherwise look at the
            // cranelift type of the lowered value so a `let a = true`
            // gets a bool variable rather than the default i64.
            let bind_ty = match ty {
                Some(t) => JitTy::from_ast(t, s.span)?,
                None => match b.func.dfg.value_type(v) {
                    t if t == I64 => JitTy::Int,
                    t if t == I8 => JitTy::Bool,
                    t => {
                        return Err(CodegenError::Unsupported {
                            what: format!("unsupported inferred type: {t}"),
                            span: s.span,
                        });
                    }
                },
            };
            let var = Variable::new(lc.env.next_var_id());
            b.declare_var(var, bind_ty.cl().expect("non-unit binding"));
            b.def_var(var, v);
            lc.env.bindings.insert(name.clone(), (var, bind_ty));
        }
        StmtKind::Expr(e) => {
            let _ = lower_expr(b, lc, e)?;
        }
    }
    Ok(())
}

/// Lower a `Block` and return its tail value (if any).
fn lower_block_value(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    block: &ilang_ast::Block,
    _expected: JitTy,
) -> Result<Option<Value>, CodegenError> {
    for s in &block.stmts {
        lower_stmt(b, lc, s)?;
    }
    match &block.tail {
        Some(t) => lower_expr(b, lc, t),
        None => Ok(None),
    }
}

fn lower_expr(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    e: &Expr,
) -> Result<Option<Value>, CodegenError> {
    match &e.kind {
        ExprKind::Int(n) => Ok(Some(b.ins().iconst(I64, *n))),
        ExprKind::Bool(v) => Ok(Some(b.ins().iconst(I8, if *v { 1 } else { 0 }))),
        ExprKind::Var(name) => {
            let (var, _) = *lc.env.bindings.get(name).ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("unknown variable {name:?}"),
                    span: e.span,
                }
            })?;
            Ok(Some(b.use_var(var)))
        }
        ExprKind::Unary { op, expr } => {
            let v = lower_expr(b, lc, expr)?.ok_or_else(|| CodegenError::Unsupported {
                what: "unary on unit".into(),
                span: e.span,
            })?;
            let r = match op {
                UnOp::Pos => v,
                UnOp::Neg => b.ins().ineg(v),
                UnOp::Not => {
                    let one = b.ins().iconst(I8, 1);
                    b.ins().bxor(v, one)
                }
                UnOp::BitNot => b.ins().bnot(v),
            };
            Ok(Some(r))
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let l = lower_expr(b, lc, lhs)?.ok_or_else(|| CodegenError::Unsupported {
                what: "binary lhs is unit".into(),
                span: lhs.span,
            })?;
            let r = lower_expr(b, lc, rhs)?.ok_or_else(|| CodegenError::Unsupported {
                what: "binary rhs is unit".into(),
                span: rhs.span,
            })?;
            let v = match op {
                BinOp::Add => b.ins().iadd(l, r),
                BinOp::Sub => b.ins().isub(l, r),
                BinOp::Mul => b.ins().imul(l, r),
                BinOp::Div => b.ins().sdiv(l, r),
                BinOp::Rem => b.ins().srem(l, r),
                BinOp::BitAnd => b.ins().band(l, r),
                BinOp::BitOr => b.ins().bor(l, r),
                BinOp::BitXor => b.ins().bxor(l, r),
                BinOp::Shl => b.ins().ishl(l, r),
                BinOp::Shr => b.ins().sshr(l, r),
                BinOp::Eq => b.ins().icmp(IntCC::Equal, l, r),
                BinOp::Ne => b.ins().icmp(IntCC::NotEqual, l, r),
                BinOp::Lt => b.ins().icmp(IntCC::SignedLessThan, l, r),
                BinOp::Le => b.ins().icmp(IntCC::SignedLessThanOrEqual, l, r),
                BinOp::Gt => b.ins().icmp(IntCC::SignedGreaterThan, l, r),
                BinOp::Ge => b.ins().icmp(IntCC::SignedGreaterThanOrEqual, l, r),
            };
            Ok(Some(v))
        }
        ExprKind::Logical { op, lhs, rhs } => Ok(Some(lower_logical(b, lc, *op, lhs, rhs)?)),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => Ok(lower_if(b, lc, cond, then_branch, else_branch.as_deref())?),
        ExprKind::Block(block) => lower_block_value(b, lc, block, JitTy::Int),
        ExprKind::While { cond, body } => {
            lower_while(b, lc, cond, body)?;
            Ok(None)
        }
        ExprKind::Loop { body } => {
            lower_loop(b, lc, body)?;
            Ok(None)
        }
        ExprKind::Break => {
            let target = lc
                .loops
                .last()
                .ok_or_else(|| CodegenError::Unsupported {
                    what: "break outside loop (should have been caught earlier)".into(),
                    span: e.span,
                })?
                .1;
            b.ins().jump(target, &[]);
            // The block becomes unreachable after this; create a dead block
            // to land subsequent code in.
            let dead = b.create_block();
            b.switch_to_block(dead);
            b.seal_block(dead);
            Ok(None)
        }
        ExprKind::Continue => {
            let target = lc
                .loops
                .last()
                .ok_or_else(|| CodegenError::Unsupported {
                    what: "continue outside loop".into(),
                    span: e.span,
                })?
                .0;
            b.ins().jump(target, &[]);
            let dead = b.create_block();
            b.switch_to_block(dead);
            b.seal_block(dead);
            Ok(None)
        }
        ExprKind::Assign { target, value } => {
            let v = lower_expr(b, lc, value)?.ok_or_else(|| CodegenError::Unsupported {
                what: "assigning unit".into(),
                span: e.span,
            })?;
            let (var, _) = *lc.env.bindings.get(target).ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("unknown variable {target:?}"),
                    span: e.span,
                }
            })?;
            b.def_var(var, v);
            Ok(None)
        }
        ExprKind::Call { callee, args } => {
            let (id, _, ret_ty) = lc
                .funcs
                .get(callee)
                .cloned()
                .ok_or_else(|| CodegenError::Unsupported {
                    what: format!("unknown function {callee:?}"),
                    span: e.span,
                })?;
            let mut arg_vals = Vec::with_capacity(args.len());
            for a in args {
                let v = lower_expr(b, lc, a)?.ok_or_else(|| CodegenError::Unsupported {
                    what: "argument is unit".into(),
                    span: a.span,
                })?;
                arg_vals.push(v);
            }
            let func_ref = lc.module.declare_func_in_func(id, b.func);
            let call = b.ins().call(func_ref, &arg_vals);
            if matches!(ret_ty, JitTy::Unit) {
                Ok(None)
            } else {
                Ok(Some(b.inst_results(call)[0]))
            }
        }
        _ => Err(CodegenError::Unsupported {
            what: format!("expression {:?}", std::mem::discriminant(&e.kind)),
            span: e.span,
        }),
    }
}

fn lower_logical(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    op: LogicalOp,
    lhs: &Expr,
    rhs: &Expr,
) -> Result<Value, CodegenError> {
    // `lhs && rhs` lowers as: if lhs { rhs } else { false }
    // `lhs || rhs` lowers as: if lhs { true  } else { rhs }
    let then_block = b.create_block();
    let else_block = b.create_block();
    let merge = b.create_block();
    b.append_block_param(merge, I8);

    let l = lower_expr(b, lc, lhs)?.expect("logical lhs is bool");
    b.ins().brif(l, then_block, &[], else_block, &[]);

    b.switch_to_block(then_block);
    b.seal_block(then_block);
    let then_val = match op {
        LogicalOp::And => lower_expr(b, lc, rhs)?.expect("rhs"),
        LogicalOp::Or => b.ins().iconst(I8, 1),
    };
    b.ins().jump(merge, &[then_val]);

    b.switch_to_block(else_block);
    b.seal_block(else_block);
    let else_val = match op {
        LogicalOp::And => b.ins().iconst(I8, 0),
        LogicalOp::Or => lower_expr(b, lc, rhs)?.expect("rhs"),
    };
    b.ins().jump(merge, &[else_val]);

    b.switch_to_block(merge);
    b.seal_block(merge);
    Ok(b.block_params(merge)[0])
}

fn lower_if(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    cond: &Expr,
    then_branch: &ilang_ast::Block,
    else_branch: Option<&Expr>,
) -> Result<Option<Value>, CodegenError> {
    let then_block = b.create_block();
    let else_block = b.create_block();

    let c = lower_expr(b, lc, cond)?.expect("cond");
    b.ins().brif(c, then_block, &[], else_block, &[]);

    // Lower then-branch first so we can pick the merge block's parameter
    // type from its value (the type checker has already proven both
    // branches agree).
    b.switch_to_block(then_block);
    b.seal_block(then_block);
    let then_val = lower_block_value(b, lc, then_branch, JitTy::Int)?;

    let merge = b.create_block();
    let merge_param = match then_val {
        Some(v) => Some(b.append_block_param(merge, b.func.dfg.value_type(v))),
        None => None,
    };
    if let Some(v) = then_val {
        b.ins().jump(merge, &[v]);
    } else {
        b.ins().jump(merge, &[]);
    }

    b.switch_to_block(else_block);
    b.seal_block(else_block);
    let else_val = match else_branch {
        Some(e) => lower_expr(b, lc, e)?,
        None => None,
    };
    match (then_val.is_some(), else_val) {
        (true, Some(v)) => {
            b.ins().jump(merge, &[v]);
        }
        (true, None) => {
            // then has a value but else doesn't — fill with a zero of the
            // matching type to keep the merge block well-formed.
            let ty = b.func.dfg.value_type(then_val.unwrap());
            let zero = b.ins().iconst(ty, 0);
            b.ins().jump(merge, &[zero]);
        }
        (false, _) => {
            b.ins().jump(merge, &[]);
        }
    }

    b.switch_to_block(merge);
    b.seal_block(merge);
    Ok(if then_val.is_some() && else_branch.is_some() {
        merge_param
    } else {
        None
    })
}

fn lower_while(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    cond: &Expr,
    body: &ilang_ast::Block,
) -> Result<(), CodegenError> {
    let header = b.create_block();
    let body_block = b.create_block();
    let after = b.create_block();

    b.ins().jump(header, &[]);
    b.switch_to_block(header);
    let c = lower_expr(b, lc, cond)?.expect("cond");
    b.ins().brif(c, body_block, &[], after, &[]);

    b.switch_to_block(body_block);
    b.seal_block(body_block);
    lc.loops.push((header, after));
    let _ = lower_block_value(b, lc, body, JitTy::Unit)?;
    lc.loops.pop();
    b.ins().jump(header, &[]);
    b.seal_block(header);

    b.switch_to_block(after);
    b.seal_block(after);
    Ok(())
}

fn lower_loop(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    body: &ilang_ast::Block,
) -> Result<(), CodegenError> {
    let header = b.create_block();
    let after = b.create_block();
    b.ins().jump(header, &[]);
    b.switch_to_block(header);
    lc.loops.push((header, after));
    let _ = lower_block_value(b, lc, body, JitTy::Unit)?;
    lc.loops.pop();
    b.ins().jump(header, &[]);
    b.seal_block(header);
    b.switch_to_block(after);
    b.seal_block(after);
    Ok(())
}
