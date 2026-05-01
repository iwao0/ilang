use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{F32, F64, I16, I32, I64, I8};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};
use ilang_ast::{
    BinOp, Expr, ExprKind, FnDecl, Item, LogicalOp, Program, Stmt, StmtKind, Type, UnOp,
};

use crate::error::CodegenError;

/// Result of running a JITed program. Covers every JIT-supported scalar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JitValue {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    Bool(bool),
    Unit,
}

impl std::fmt::Display for JitValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JitValue::I8(n) => write!(f, "{n}"),
            JitValue::I16(n) => write!(f, "{n}"),
            JitValue::I32(n) => write!(f, "{n}"),
            JitValue::I64(n) => write!(f, "{n}"),
            JitValue::U8(n) => write!(f, "{n}"),
            JitValue::U16(n) => write!(f, "{n}"),
            JitValue::U32(n) => write!(f, "{n}"),
            JitValue::U64(n) => write!(f, "{n}"),
            JitValue::F32(x) => {
                if x.is_finite() && x.fract() == 0.0 {
                    write!(f, "{x:.1}")
                } else {
                    write!(f, "{x}")
                }
            }
            JitValue::F64(x) => {
                if x.is_finite() && x.fract() == 0.0 {
                    write!(f, "{x:.1}")
                } else {
                    write!(f, "{x}")
                }
            }
            JitValue::Bool(b) => write!(f, "{b}"),
            JitValue::Unit => Ok(()),
        }
    }
}

/// JIT-internal type tag. Mirrors `ilang_ast::Type` for the supported
/// scalar subset. Pairs with a cranelift `Value` to keep signedness and
/// width information that the IR alone doesn't carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JitTy {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Unit,
}

impl JitTy {
    fn from_ast(t: &Type, span: ilang_ast::Span) -> Result<Self, CodegenError> {
        Ok(match t {
            Type::I8 => JitTy::I8,
            Type::I16 => JitTy::I16,
            Type::I32 => JitTy::I32,
            Type::I64 => JitTy::I64,
            Type::U8 => JitTy::U8,
            Type::U16 => JitTy::U16,
            Type::U32 => JitTy::U32,
            Type::U64 => JitTy::U64,
            Type::F32 => JitTy::F32,
            Type::F64 => JitTy::F64,
            Type::Bool => JitTy::Bool,
            Type::Unit => JitTy::Unit,
            other => {
                return Err(CodegenError::UnsupportedType {
                    ty: other.clone(),
                    span,
                });
            }
        })
    }

    fn cl(self) -> Option<types::Type> {
        Some(match self {
            JitTy::I8 | JitTy::U8 | JitTy::Bool => I8,
            JitTy::I16 | JitTy::U16 => I16,
            JitTy::I32 | JitTy::U32 => I32,
            JitTy::I64 | JitTy::U64 => I64,
            JitTy::F32 => F32,
            JitTy::F64 => F64,
            JitTy::Unit => return None,
        })
    }

    fn is_signed_int(self) -> bool {
        matches!(self, JitTy::I8 | JitTy::I16 | JitTy::I32 | JitTy::I64)
    }
    fn is_unsigned_int(self) -> bool {
        matches!(self, JitTy::U8 | JitTy::U16 | JitTy::U32 | JitTy::U64)
    }
    fn is_int(self) -> bool {
        self.is_signed_int() || self.is_unsigned_int()
    }
    fn is_float(self) -> bool {
        matches!(self, JitTy::F32 | JitTy::F64)
    }
    fn int_width(self) -> u32 {
        match self {
            JitTy::I8 | JitTy::U8 => 8,
            JitTy::I16 | JitTy::U16 => 16,
            JitTy::I32 | JitTy::U32 => 32,
            JitTy::I64 | JitTy::U64 => 64,
            _ => 0,
        }
    }
}

/// Compute the "common" type for a binary op, mirroring the type
/// checker's `numeric_result`. Mixed signedness is rejected at compile
/// time so we can assume agreement here.
fn common_numeric_ty(l: JitTy, r: JitTy) -> Option<JitTy> {
    if l == r {
        return Some(l);
    }
    if l.is_int() && r.is_int() {
        if l.is_signed_int() != r.is_signed_int() {
            return None;
        }
        return Some(if l.int_width() >= r.int_width() { l } else { r });
    }
    if l.is_float() && r.is_float() {
        return Some(if matches!(l, JitTy::F64) || matches!(r, JitTy::F64) {
            JitTy::F64
        } else {
            JitTy::F32
        });
    }
    let (int_t, float_t) = if l.is_int() { (l, r) } else { (r, l) };
    let needs_f64 = matches!(float_t, JitTy::F64) || int_t.int_width() >= 32;
    Some(if needs_f64 { JitTy::F64 } else { JitTy::F32 })
}

/// A lowered value plus its JIT type tag.
type TV = (Value, JitTy);

pub fn jit_run(prog: &Program) -> Result<JitValue, CodegenError> {
    let mut compiler = JitCompiler::new()?;
    for item in &prog.items {
        if let Item::Fn(f) = item {
            compiler.declare_fn(f)?;
        }
    }
    for item in &prog.items {
        match item {
            Item::Fn(f) => compiler.define_fn(f)?,
            Item::Class(c) => {
                return Err(CodegenError::Unsupported {
                    what: "class".into(),
                    span: c.span,
                });
            }
        }
    }
    let main_ret = compiler.define_main(prog)?;
    compiler.finalize()?;
    Ok(compiler.run_main(main_ret))
}

struct JitCompiler {
    module: JITModule,
    ctx: cranelift_codegen::Context,
    builder_ctx: FunctionBuilderContext,
    /// fn name → (FuncId, param JitTys, ret JitTy)
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
        self.module.clear_context(&mut self.ctx);
        self.ctx.func.signature =
            self.module.declarations().get_function_decl(id).signature.clone();

        let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut env = Env::default();
        for (i, p) in f.params.iter().enumerate() {
            let pty = param_tys[i];
            let var = Variable::new(env.next_var_id());
            builder.declare_var(var, pty.cl().expect("non-unit checked at declare"));
            let v = builder.block_params(entry)[i];
            builder.def_var(var, v);
            env.bindings.insert(p.name.clone(), (var, pty));
        }

        let mut lc = LowerCtx {
            funcs: &self.funcs,
            module: &mut self.module,
            env: &mut env,
            loops: Vec::new(),
        };
        let body = lower_block_value(&mut builder, &mut lc, &f.body)?;
        emit_return(&mut builder, ret_ty, body, f.span)?;
        builder.finalize();

        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|e| CodegenError::Module(e.to_string()))?;
        Ok(())
    }

    fn define_main(&mut self, prog: &Program) -> Result<JitTy, CodegenError> {
        // Use the type checker's program-level type. The caller (CLI)
        // will normally have already type-checked, but running it again
        // is cheap and lets us handle the JIT being called directly.
        let mut tc = ilang_types::TypeChecker::new();
        let prog_ty = tc.check(prog).map_err(|e| CodegenError::Cranelift(e.to_string()))?;
        let ret_ty = JitTy::from_ast(&prog_ty, ilang_ast::Span::dummy())?;

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
        let body = match &prog.tail {
            Some(t) => Some(lower_expr(&mut builder, &mut lc, t)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "tail expression produces no value".into(),
                    span: t.span,
                }
            })?),
            None => None,
        };
        emit_return(&mut builder, ret_ty, body, ilang_ast::Span::dummy())?;
        builder.finalize();

        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|e| CodegenError::Module(e.to_string()))?;
        self.funcs.insert("__main".into(), (id, vec![], ret_ty));
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
                JitTy::I8 => JitValue::I8((std::mem::transmute::<_, extern "C" fn() -> i8>(ptr))()),
                JitTy::I16 => {
                    JitValue::I16((std::mem::transmute::<_, extern "C" fn() -> i16>(ptr))())
                }
                JitTy::I32 => {
                    JitValue::I32((std::mem::transmute::<_, extern "C" fn() -> i32>(ptr))())
                }
                JitTy::I64 => {
                    JitValue::I64((std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))())
                }
                JitTy::U8 => JitValue::U8((std::mem::transmute::<_, extern "C" fn() -> u8>(ptr))()),
                JitTy::U16 => {
                    JitValue::U16((std::mem::transmute::<_, extern "C" fn() -> u16>(ptr))())
                }
                JitTy::U32 => {
                    JitValue::U32((std::mem::transmute::<_, extern "C" fn() -> u32>(ptr))())
                }
                JitTy::U64 => {
                    JitValue::U64((std::mem::transmute::<_, extern "C" fn() -> u64>(ptr))())
                }
                JitTy::F32 => {
                    JitValue::F32((std::mem::transmute::<_, extern "C" fn() -> f32>(ptr))())
                }
                JitTy::F64 => {
                    JitValue::F64((std::mem::transmute::<_, extern "C" fn() -> f64>(ptr))())
                }
                JitTy::Bool => {
                    let v = (std::mem::transmute::<_, extern "C" fn() -> i8>(ptr))();
                    JitValue::Bool(v != 0)
                }
                JitTy::Unit => {
                    (std::mem::transmute::<_, extern "C" fn()>(ptr))();
                    JitValue::Unit
                }
            }
        }
    }
}

fn emit_return(
    b: &mut FunctionBuilder,
    ret_ty: JitTy,
    body: Option<TV>,
    span: ilang_ast::Span,
) -> Result<(), CodegenError> {
    match (ret_ty, body) {
        (JitTy::Unit, _) => {
            b.ins().return_(&[]);
        }
        (_, Some((v, vt))) => {
            let v = coerce(b, (v, vt), ret_ty, span)?;
            b.ins().return_(&[v]);
        }
        _ => {
            return Err(CodegenError::Unsupported {
                what: "function body produces no value".into(),
                span,
            });
        }
    }
    Ok(())
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
    loops: Vec<(Block, Block)>,
}

fn lower_stmt(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    s: &Stmt,
) -> Result<(), CodegenError> {
    match &s.kind {
        StmtKind::Let { name, ty, value } => {
            let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "let value produces no value".into(),
                    span: value.span,
                }
            })?;
            let bind_ty = match ty {
                Some(t) => JitTy::from_ast(t, s.span)?,
                None => vt,
            };
            let coerced = coerce(b, (val, vt), bind_ty, s.span)?;
            let var = Variable::new(lc.env.next_var_id());
            b.declare_var(var, bind_ty.cl().ok_or_else(|| CodegenError::Unsupported {
                what: "unit-typed let binding".into(),
                span: s.span,
            })?);
            b.def_var(var, coerced);
            lc.env.bindings.insert(name.clone(), (var, bind_ty));
        }
        StmtKind::Expr(e) => {
            let _ = lower_expr(b, lc, e)?;
        }
    }
    Ok(())
}

fn lower_block_value(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    block: &ilang_ast::Block,
) -> Result<Option<TV>, CodegenError> {
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
) -> Result<Option<TV>, CodegenError> {
    match &e.kind {
        ExprKind::Int(n) => Ok(Some((b.ins().iconst(I64, *n), JitTy::I64))),
        ExprKind::Float(f) => Ok(Some((b.ins().f64const(*f), JitTy::F64))),
        ExprKind::Bool(v) => Ok(Some((b.ins().iconst(I8, if *v { 1 } else { 0 }), JitTy::Bool))),
        ExprKind::Var(name) => {
            let (var, vt) = *lc.env.bindings.get(name).ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("unknown variable {name:?}"),
                    span: e.span,
                }
            })?;
            Ok(Some((b.use_var(var), vt)))
        }
        ExprKind::Cast { expr, ty } => {
            let inner = lower_expr(b, lc, expr)?.ok_or_else(|| CodegenError::Unsupported {
                what: "cast on unit".into(),
                span: e.span,
            })?;
            let target = JitTy::from_ast(ty, e.span)?;
            let v = coerce(b, inner, target, e.span)?;
            Ok(Some((v, target)))
        }
        ExprKind::Unary { op, expr } => lower_unary(b, lc, *op, expr, e.span),
        ExprKind::Binary { op, lhs, rhs } => lower_binary(b, lc, *op, lhs, rhs),
        ExprKind::Logical { op, lhs, rhs } => Ok(Some((
            lower_logical(b, lc, *op, lhs, rhs)?,
            JitTy::Bool,
        ))),
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => lower_if(b, lc, cond, then_branch, else_branch.as_deref()),
        ExprKind::Block(block) => lower_block_value(b, lc, block),
        ExprKind::While { cond, body } => {
            lower_while(b, lc, cond, body)?;
            Ok(None)
        }
        ExprKind::Loop { body } => {
            lower_loop(b, lc, body)?;
            Ok(None)
        }
        ExprKind::Break => {
            let target = lc.loops.last().ok_or_else(|| CodegenError::Unsupported {
                what: "break outside loop".into(),
                span: e.span,
            })?.1;
            b.ins().jump(target, &[]);
            let dead = b.create_block();
            b.switch_to_block(dead);
            b.seal_block(dead);
            Ok(None)
        }
        ExprKind::Continue => {
            let target = lc.loops.last().ok_or_else(|| CodegenError::Unsupported {
                what: "continue outside loop".into(),
                span: e.span,
            })?.0;
            b.ins().jump(target, &[]);
            let dead = b.create_block();
            b.switch_to_block(dead);
            b.seal_block(dead);
            Ok(None)
        }
        ExprKind::Assign { target, value } => {
            let (var, var_ty) = *lc.env.bindings.get(target).ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("unknown variable {target:?}"),
                    span: e.span,
                }
            })?;
            let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "assigning unit".into(),
                    span: e.span,
                }
            })?;
            let coerced = coerce(b, (val, vt), var_ty, e.span)?;
            b.def_var(var, coerced);
            Ok(None)
        }
        ExprKind::Call { callee, args } => {
            let (id, param_tys, ret_ty) = lc
                .funcs
                .get(callee)
                .cloned()
                .ok_or_else(|| CodegenError::Unsupported {
                    what: format!("unknown function {callee:?}"),
                    span: e.span,
                })?;
            let mut arg_vals = Vec::with_capacity(args.len());
            for (i, a) in args.iter().enumerate() {
                let (av, at) = lower_expr(b, lc, a)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "argument is unit".into(),
                        span: a.span,
                    }
                })?;
                arg_vals.push(coerce(b, (av, at), param_tys[i], a.span)?);
            }
            let func_ref = lc.module.declare_func_in_func(id, b.func);
            let call = b.ins().call(func_ref, &arg_vals);
            if matches!(ret_ty, JitTy::Unit) {
                Ok(None)
            } else {
                Ok(Some((b.inst_results(call)[0], ret_ty)))
            }
        }
        _ => Err(CodegenError::Unsupported {
            what: format!("expression {:?}", std::mem::discriminant(&e.kind)),
            span: e.span,
        }),
    }
}

fn lower_unary(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    op: UnOp,
    expr: &Expr,
    span: ilang_ast::Span,
) -> Result<Option<TV>, CodegenError> {
    let (v, vt) = lower_expr(b, lc, expr)?.ok_or_else(|| CodegenError::Unsupported {
        what: "unary on unit".into(),
        span,
    })?;
    let r = match op {
        UnOp::Pos => v,
        UnOp::Neg => {
            if vt.is_float() {
                b.ins().fneg(v)
            } else if vt.is_signed_int() {
                b.ins().ineg(v)
            } else {
                return Err(CodegenError::Unsupported {
                    what: format!("unary `-` on {vt:?}"),
                    span,
                });
            }
        }
        UnOp::Not => {
            let one = b.ins().iconst(I8, 1);
            b.ins().bxor(v, one)
        }
        UnOp::BitNot => b.ins().bnot(v),
    };
    Ok(Some((r, vt)))
}

fn lower_binary(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    op: BinOp,
    lhs: &Expr,
    rhs: &Expr,
) -> Result<Option<TV>, CodegenError> {
    let (lv, lt) = lower_expr(b, lc, lhs)?.ok_or_else(|| CodegenError::Unsupported {
        what: "binary lhs is unit".into(),
        span: lhs.span,
    })?;
    let (rv, rt) = lower_expr(b, lc, rhs)?.ok_or_else(|| CodegenError::Unsupported {
        what: "binary rhs is unit".into(),
        span: rhs.span,
    })?;
    let common = common_numeric_ty(lt, rt).ok_or_else(|| CodegenError::Unsupported {
        what: format!("incompatible binary operand types {lt:?} and {rt:?}"),
        span: lhs.span,
    })?;
    let lv = coerce(b, (lv, lt), common, lhs.span)?;
    let rv = coerce(b, (rv, rt), common, rhs.span)?;
    let is_compare = matches!(
        op,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
    );
    if is_compare {
        let v = if common.is_float() {
            let cc = match op {
                BinOp::Eq => FloatCC::Equal,
                BinOp::Ne => FloatCC::NotEqual,
                BinOp::Lt => FloatCC::LessThan,
                BinOp::Le => FloatCC::LessThanOrEqual,
                BinOp::Gt => FloatCC::GreaterThan,
                BinOp::Ge => FloatCC::GreaterThanOrEqual,
                _ => unreachable!(),
            };
            b.ins().fcmp(cc, lv, rv)
        } else {
            let signed = common.is_signed_int() || matches!(common, JitTy::Bool);
            let cc = match (op, signed) {
                (BinOp::Eq, _) => IntCC::Equal,
                (BinOp::Ne, _) => IntCC::NotEqual,
                (BinOp::Lt, true) => IntCC::SignedLessThan,
                (BinOp::Le, true) => IntCC::SignedLessThanOrEqual,
                (BinOp::Gt, true) => IntCC::SignedGreaterThan,
                (BinOp::Ge, true) => IntCC::SignedGreaterThanOrEqual,
                (BinOp::Lt, false) => IntCC::UnsignedLessThan,
                (BinOp::Le, false) => IntCC::UnsignedLessThanOrEqual,
                (BinOp::Gt, false) => IntCC::UnsignedGreaterThan,
                (BinOp::Ge, false) => IntCC::UnsignedGreaterThanOrEqual,
                _ => unreachable!(),
            };
            b.ins().icmp(cc, lv, rv)
        };
        return Ok(Some((v, JitTy::Bool)));
    }
    let v = if common.is_float() {
        match op {
            BinOp::Add => b.ins().fadd(lv, rv),
            BinOp::Sub => b.ins().fsub(lv, rv),
            BinOp::Mul => b.ins().fmul(lv, rv),
            BinOp::Div => b.ins().fdiv(lv, rv),
            BinOp::Rem => {
                return Err(CodegenError::Unsupported {
                    what: "float `%` (cranelift has no frem)".into(),
                    span: lhs.span,
                });
            }
            _ => {
                return Err(CodegenError::Unsupported {
                    what: format!("operator {op:?} on float"),
                    span: lhs.span,
                });
            }
        }
    } else {
        let signed = common.is_signed_int();
        match op {
            BinOp::Add => b.ins().iadd(lv, rv),
            BinOp::Sub => b.ins().isub(lv, rv),
            BinOp::Mul => b.ins().imul(lv, rv),
            BinOp::Div => {
                if signed {
                    b.ins().sdiv(lv, rv)
                } else {
                    b.ins().udiv(lv, rv)
                }
            }
            BinOp::Rem => {
                if signed {
                    b.ins().srem(lv, rv)
                } else {
                    b.ins().urem(lv, rv)
                }
            }
            BinOp::BitAnd => b.ins().band(lv, rv),
            BinOp::BitOr => b.ins().bor(lv, rv),
            BinOp::BitXor => b.ins().bxor(lv, rv),
            BinOp::Shl => b.ins().ishl(lv, rv),
            BinOp::Shr => {
                if signed {
                    b.ins().sshr(lv, rv)
                } else {
                    b.ins().ushr(lv, rv)
                }
            }
            _ => unreachable!("compare handled above"),
        }
    };
    Ok(Some((v, common)))
}

fn lower_logical(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    op: LogicalOp,
    lhs: &Expr,
    rhs: &Expr,
) -> Result<Value, CodegenError> {
    let then_block = b.create_block();
    let else_block = b.create_block();
    let merge = b.create_block();
    b.append_block_param(merge, I8);

    let l = lower_expr(b, lc, lhs)?.ok_or_else(|| CodegenError::Unsupported {
        what: "logical lhs is unit".into(),
        span: lhs.span,
    })?
    .0;
    b.ins().brif(l, then_block, &[], else_block, &[]);

    b.switch_to_block(then_block);
    b.seal_block(then_block);
    let then_val = match op {
        LogicalOp::And => lower_expr(b, lc, rhs)?
            .ok_or_else(|| CodegenError::Unsupported {
                what: "logical rhs is unit".into(),
                span: rhs.span,
            })?
            .0,
        LogicalOp::Or => b.ins().iconst(I8, 1),
    };
    b.ins().jump(merge, &[then_val]);

    b.switch_to_block(else_block);
    b.seal_block(else_block);
    let else_val = match op {
        LogicalOp::And => b.ins().iconst(I8, 0),
        LogicalOp::Or => lower_expr(b, lc, rhs)?
            .ok_or_else(|| CodegenError::Unsupported {
                what: "logical rhs is unit".into(),
                span: rhs.span,
            })?
            .0,
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
) -> Result<Option<TV>, CodegenError> {
    let then_block = b.create_block();
    let else_block = b.create_block();

    let c = lower_expr(b, lc, cond)?.ok_or_else(|| CodegenError::Unsupported {
        what: "if-cond is unit".into(),
        span: cond.span,
    })?
    .0;
    b.ins().brif(c, then_block, &[], else_block, &[]);

    b.switch_to_block(then_block);
    b.seal_block(then_block);
    let then_val = lower_block_value(b, lc, then_branch)?;

    let merge = b.create_block();
    let merge_param = match then_val {
        Some((v, _)) => Some(b.append_block_param(merge, b.func.dfg.value_type(v))),
        None => None,
    };
    if let Some((v, _)) = then_val {
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
    match (then_val, else_val) {
        (Some((_, tt)), Some((ev, _et))) => {
            // Coerce else value to then's type so the merge param matches.
            let ev_coerced = coerce(b, (ev, _et), tt, cond.span)?;
            b.ins().jump(merge, &[ev_coerced]);
            // Track tt for the result.
            b.switch_to_block(merge);
            b.seal_block(merge);
            return Ok(merge_param.map(|p| (p, tt)));
        }
        (Some((_, tt)), None) => {
            let zero = match tt.cl() {
                Some(t) if t.is_float() => b.ins().f64const(0.0),
                Some(t) => b.ins().iconst(t, 0),
                None => unreachable!(),
            };
            b.ins().jump(merge, &[zero]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            return Ok(None);
        }
        (None, _) => {
            b.ins().jump(merge, &[]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            return Ok(None);
        }
    }
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
    let c = lower_expr(b, lc, cond)?.ok_or_else(|| CodegenError::Unsupported {
        what: "while-cond is unit".into(),
        span: cond.span,
    })?
    .0;
    b.ins().brif(c, body_block, &[], after, &[]);

    b.switch_to_block(body_block);
    b.seal_block(body_block);
    lc.loops.push((header, after));
    let _ = lower_block_value(b, lc, body)?;
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
    let _ = lower_block_value(b, lc, body)?;
    lc.loops.pop();
    b.ins().jump(header, &[]);
    b.seal_block(header);
    b.switch_to_block(after);
    b.seal_block(after);
    Ok(())
}

/// Convert a value from one numeric (or bool) type to another using the
/// appropriate cranelift instruction. Panics if either side isn't a
/// supported scalar — callers should validate first.
fn coerce(
    b: &mut FunctionBuilder,
    (v, from): TV,
    to: JitTy,
    span: ilang_ast::Span,
) -> Result<Value, CodegenError> {
    if from == to {
        return Ok(v);
    }
    let v = match (from, to) {
        // Bool / int interchangeable (bool is i8 underneath).
        (JitTy::Bool, t) if t.is_int() => widen_int(b, v, 8, t, false),
        (t, JitTy::Bool) if t.is_int() => narrow_int(b, v, 8, t),
        // Both integers — pick the right widening / narrowing instruction
        // based on relative widths and the source's signedness.
        (a, c) if a.is_int() && c.is_int() => {
            let from_w = a.int_width();
            let to_w = c.int_width();
            if to_w > from_w {
                widen_int(b, v, from_w, c, a.is_signed_int())
            } else if to_w < from_w {
                narrow_int(b, v, to_w, c)
            } else {
                // Same width, different signedness — bit-pattern preserved.
                v
            }
        }
        // Int → Float
        (a, JitTy::F32) if a.is_signed_int() => b.ins().fcvt_from_sint(F32, v),
        (a, JitTy::F32) if a.is_unsigned_int() => b.ins().fcvt_from_uint(F32, v),
        (a, JitTy::F64) if a.is_signed_int() => b.ins().fcvt_from_sint(F64, v),
        (a, JitTy::F64) if a.is_unsigned_int() => b.ins().fcvt_from_uint(F64, v),
        // Float ↔ Float
        (JitTy::F32, JitTy::F64) => b.ins().fpromote(F64, v),
        (JitTy::F64, JitTy::F32) => b.ins().fdemote(F32, v),
        // Float → Int — saturating cast, matches Rust's `as`.
        (a, c) if a.is_float() && c.is_signed_int() => {
            let cl = c.cl().expect("non-unit");
            b.ins().fcvt_to_sint_sat(cl, v)
        }
        (a, c) if a.is_float() && c.is_unsigned_int() => {
            let cl = c.cl().expect("non-unit");
            b.ins().fcvt_to_uint_sat(cl, v)
        }
        _ => {
            return Err(CodegenError::Unsupported {
                what: format!("cannot coerce {from:?} to {to:?}"),
                span,
            });
        }
    };
    Ok(v)
}

fn widen_int(
    b: &mut FunctionBuilder,
    v: Value,
    from_width: u32,
    to: JitTy,
    signed: bool,
) -> Value {
    let to_cl = to.cl().expect("non-unit");
    let _ = from_width;
    if signed {
        b.ins().sextend(to_cl, v)
    } else {
        b.ins().uextend(to_cl, v)
    }
}

fn narrow_int(b: &mut FunctionBuilder, v: Value, _to_width: u32, to: JitTy) -> Value {
    let to_cl = to.cl().expect("non-unit");
    b.ins().ireduce(to_cl, v)
}
