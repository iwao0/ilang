use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift_codegen::ir::types::{F32, F64, I16, I32, I64, I8};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use ilang_ast::{
    BinOp, ClassDecl, Expr, ExprKind, FnDecl, Item, LogicalOp, Program, Stmt, StmtKind, Type, UnOp,
};

use crate::error::CodegenError;

// ─── Runtime support ────────────────────────────────────────────────────

/// Heap allocator called from JITed code via FFI. Returns a zeroed
/// `size`-byte block as an `i64`-shaped pointer; the JIT casts it to a
/// raw pointer for field load/store. Memory is intentionally never
/// freed — ARC/deinit is a future addition. Short-lived programs leak.
extern "C" fn ilang_jit_alloc(size: i64) -> i64 {
    let n = (size as usize).max(1);
    let layout = std::alloc::Layout::from_size_align(n, 8).unwrap();
    unsafe { std::alloc::alloc_zeroed(layout) as i64 }
}

// ─── console.log per-type print helpers ────────────────────────────────
// `console.log(a, b, c)` lowers to:
//   ilang_jit_print_<type>(a)
//   ilang_jit_print_space()
//   ilang_jit_print_<type>(b)
//   ilang_jit_print_space()
//   ilang_jit_print_<type>(c)
//   ilang_jit_print_newline()

extern "C" fn ilang_jit_print_i64(n: i64) {
    print!("{n}");
}
extern "C" fn ilang_jit_print_u64(n: u64) {
    print!("{n}");
}
extern "C" fn ilang_jit_print_f64(x: f64) {
    if x.is_finite() && x.fract() == 0.0 {
        print!("{x:.1}");
    } else {
        print!("{x}");
    }
}
extern "C" fn ilang_jit_print_f32(x: f32) {
    ilang_jit_print_f64(x as f64);
}
extern "C" fn ilang_jit_print_bool(b: i8) {
    print!("{}", b != 0);
}
extern "C" fn ilang_jit_print_space() {
    print!(" ");
}
extern "C" fn ilang_jit_print_newline() {
    println!();
}

// ─── JitValue (program result surfaced to the CLI) ──────────────────────

#[derive(Debug, Clone, PartialEq)]
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
    Object { class: String, ptr: i64 },
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
            JitValue::F32(x) => fmt_float(f, *x as f64),
            JitValue::F64(x) => fmt_float(f, *x),
            JitValue::Bool(b) => write!(f, "{b}"),
            JitValue::Object { class, ptr } => write!(f, "<{class} @ {ptr:#x}>"),
            JitValue::Unit => Ok(()),
        }
    }
}

fn fmt_float(f: &mut std::fmt::Formatter<'_>, x: f64) -> std::fmt::Result {
    if x.is_finite() && x.fract() == 0.0 {
        write!(f, "{x:.1}")
    } else {
        write!(f, "{x}")
    }
}

// ─── JIT type tag ───────────────────────────────────────────────────────

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
    /// Heap pointer to a class instance. The id indexes into the
    /// compiler's `class_layouts` / `class_methods` vecs.
    Object(u32),
    Unit,
}

impl JitTy {
    fn from_ast(
        t: &Type,
        span: ilang_ast::Span,
        class_ids: &HashMap<String, u32>,
    ) -> Result<Self, CodegenError> {
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
            Type::Object(name) => {
                let id = class_ids.get(name).copied().ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: format!("unknown class {name:?}"),
                        span,
                    }
                })?;
                JitTy::Object(id)
            }
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
            JitTy::I64 | JitTy::U64 | JitTy::Object(_) => I64,
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
    fn size_bytes(self) -> u32 {
        match self {
            JitTy::I8 | JitTy::U8 | JitTy::Bool => 1,
            JitTy::I16 | JitTy::U16 => 2,
            JitTy::I32 | JitTy::U32 | JitTy::F32 => 4,
            JitTy::I64 | JitTy::U64 | JitTy::F64 | JitTy::Object(_) => 8,
            JitTy::Unit => 0,
        }
    }
}

// ─── Class layout ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ClassLayout {
    name: String,
    fields: HashMap<String, (u32, JitTy)>,
    size: u32,
}

#[derive(Debug, Clone)]
struct MethodInfo {
    id: FuncId,
    /// Parameter types as declared (excludes the implicit `this`).
    params: Vec<JitTy>,
    ret: JitTy,
}

fn declare_import(
    module: &mut JITModule,
    name: &str,
    params: &[types::Type],
    ret: Option<types::Type>,
) -> Result<FuncId, CodegenError> {
    let mut sig = module.make_signature();
    for p in params {
        sig.params.push(AbiParam::new(*p));
    }
    if let Some(r) = ret {
        sig.returns.push(AbiParam::new(r));
    }
    module
        .declare_function(name, Linkage::Import, &sig)
        .map_err(|e| CodegenError::Module(e.to_string()))
}

fn align_up(offset: u32, align: u32) -> u32 {
    (offset + align - 1) & !(align - 1)
}

fn common_numeric_ty(l: JitTy, r: JitTy) -> Option<JitTy> {
    if l == r {
        return Some(l);
    }
    if matches!(l, JitTy::Object(_)) || matches!(r, JitTy::Object(_)) {
        return None;
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

type TV = (Value, JitTy);

// ─── Public entry point ─────────────────────────────────────────────────

pub fn jit_run(prog: &Program) -> Result<JitValue, CodegenError> {
    let mut compiler = JitCompiler::new()?;
    // 1. Assign class ids and compute layouts before anything else needs
    //    to look up `Type::Object(name)`.
    for item in &prog.items {
        if let Item::Class(c) = item {
            compiler.declare_class(c)?;
        }
    }
    // 2. Declare every fn / method signature so cross-references resolve.
    for item in &prog.items {
        match item {
            Item::Fn(f) => compiler.declare_fn(f)?,
            Item::Class(c) => compiler.declare_methods(c)?,
        }
    }
    // 3. Define every body.
    for item in &prog.items {
        match item {
            Item::Fn(f) => compiler.define_fn(f)?,
            Item::Class(c) => compiler.define_methods(c)?,
        }
    }
    let main_ret = compiler.define_main(prog)?;
    compiler.finalize()?;
    Ok(compiler.run_main(main_ret))
}

// ─── Compiler driver ────────────────────────────────────────────────────

struct JitCompiler {
    module: JITModule,
    ctx: cranelift_codegen::Context,
    builder_ctx: FunctionBuilderContext,
    funcs: HashMap<String, (FuncId, Vec<JitTy>, JitTy)>,
    class_ids: HashMap<String, u32>,
    class_layouts: Vec<ClassLayout>,
    class_methods: Vec<HashMap<String, MethodInfo>>,
    alloc_id: FuncId,
    /// Per-type FFI print helpers used to lower `console.log(...)`.
    print_i64: FuncId,
    print_u64: FuncId,
    print_f64: FuncId,
    print_f32: FuncId,
    print_bool: FuncId,
    print_space: FuncId,
    print_newline: FuncId,
}

impl JitCompiler {
    fn new() -> Result<Self, CodegenError> {
        let flag_builder = settings::builder();
        let isa_builder = cranelift_native::builder()
            .map_err(|e| CodegenError::Cranelift(format!("isa builder: {e}")))?;
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|e| CodegenError::Cranelift(format!("isa: {e}")))?;
        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        // Expose runtime FFI symbols to the JIT.
        builder.symbol("ilang_jit_alloc", ilang_jit_alloc as *const u8);
        builder.symbol("ilang_jit_print_i64", ilang_jit_print_i64 as *const u8);
        builder.symbol("ilang_jit_print_u64", ilang_jit_print_u64 as *const u8);
        builder.symbol("ilang_jit_print_f64", ilang_jit_print_f64 as *const u8);
        builder.symbol("ilang_jit_print_f32", ilang_jit_print_f32 as *const u8);
        builder.symbol("ilang_jit_print_bool", ilang_jit_print_bool as *const u8);
        builder.symbol("ilang_jit_print_space", ilang_jit_print_space as *const u8);
        builder.symbol(
            "ilang_jit_print_newline",
            ilang_jit_print_newline as *const u8,
        );
        let mut module = JITModule::new(builder);
        let ctx = module.make_context();

        // Declare signatures for every imported runtime function.
        let alloc_id = declare_import(&mut module, "ilang_jit_alloc", &[I64], Some(I64))?;
        let print_i64 = declare_import(&mut module, "ilang_jit_print_i64", &[I64], None)?;
        let print_u64 = declare_import(&mut module, "ilang_jit_print_u64", &[I64], None)?;
        let print_f64 = declare_import(&mut module, "ilang_jit_print_f64", &[F64], None)?;
        let print_f32 = declare_import(&mut module, "ilang_jit_print_f32", &[F32], None)?;
        let print_bool = declare_import(&mut module, "ilang_jit_print_bool", &[I8], None)?;
        let print_space = declare_import(&mut module, "ilang_jit_print_space", &[], None)?;
        let print_newline =
            declare_import(&mut module, "ilang_jit_print_newline", &[], None)?;

        Ok(Self {
            module,
            ctx,
            builder_ctx: FunctionBuilderContext::new(),
            funcs: HashMap::new(),
            class_ids: HashMap::new(),
            class_layouts: Vec::new(),
            class_methods: Vec::new(),
            alloc_id,
            print_i64,
            print_u64,
            print_f64,
            print_f32,
            print_bool,
            print_space,
            print_newline,
        })
    }

    fn declare_class(&mut self, c: &ClassDecl) -> Result<(), CodegenError> {
        let id = self.class_layouts.len() as u32;
        self.class_ids.insert(c.name.clone(), id);
        let mut offset = 0u32;
        let mut max_align = 1u32;
        let mut fields = HashMap::new();
        for field in &c.fields {
            let jty = JitTy::from_ast(&field.ty, field.span, &self.class_ids)?;
            let size = jty.size_bytes();
            let align = size.max(1);
            offset = align_up(offset, align);
            fields.insert(field.name.clone(), (offset, jty));
            offset += size;
            max_align = max_align.max(align);
        }
        let size = align_up(offset.max(1), max_align);
        self.class_layouts.push(ClassLayout {
            name: c.name.clone(),
            fields,
            size,
        });
        self.class_methods.push(HashMap::new());
        Ok(())
    }

    fn declare_fn(&mut self, f: &FnDecl) -> Result<(), CodegenError> {
        let (id, params, ret) = self.declare_fn_signature(&f.name, f, None)?;
        self.funcs.insert(f.name.clone(), (id, params, ret));
        Ok(())
    }

    /// Declare every method of a class as a top-level function with
    /// `this` as the implicit first parameter. Constructor (`init`) is
    /// no different from a regular method here — the special handling
    /// lives in the `new` lowering.
    fn declare_methods(&mut self, c: &ClassDecl) -> Result<(), CodegenError> {
        let class_id = self.class_ids[&c.name];
        for m in &c.methods {
            let symbol = format!("__method_{}_{}", c.name, m.name);
            let (id, params, ret) =
                self.declare_fn_signature(&symbol, m, Some(JitTy::Object(class_id)))?;
            self.class_methods[class_id as usize].insert(
                m.name.clone(),
                MethodInfo { id, params, ret },
            );
        }
        Ok(())
    }

    /// Shared helper for `declare_fn` and `declare_methods`. `this_ty`,
    /// when `Some`, is prepended to the param list so methods get an
    /// implicit `this` pointer.
    fn declare_fn_signature(
        &mut self,
        symbol: &str,
        f: &FnDecl,
        this_ty: Option<JitTy>,
    ) -> Result<(FuncId, Vec<JitTy>, JitTy), CodegenError> {
        let mut params = Vec::with_capacity(f.params.len());
        for p in &f.params {
            params.push(JitTy::from_ast(&p.ty, p.span, &self.class_ids)?);
        }
        let ret = match &f.ret {
            Some(t) => JitTy::from_ast(t, f.span, &self.class_ids)?,
            None => JitTy::Unit,
        };
        let mut sig = self.module.make_signature();
        if let Some(t) = this_ty {
            sig.params.push(AbiParam::new(t.cl().expect("object pointer")));
        }
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
            .declare_function(symbol, Linkage::Local, &sig)
            .map_err(|e| CodegenError::Module(e.to_string()))?;
        Ok((id, params, ret))
    }

    fn define_fn(&mut self, f: &FnDecl) -> Result<(), CodegenError> {
        let (id, param_tys, ret_ty) = self.funcs[&f.name].clone();
        self.define_function_body(id, f, &param_tys, ret_ty, None)
    }

    fn define_methods(&mut self, c: &ClassDecl) -> Result<(), CodegenError> {
        let class_id = self.class_ids[&c.name];
        for m in &c.methods {
            let info = self.class_methods[class_id as usize][&m.name].clone();
            self.define_function_body(info.id, m, &info.params, info.ret, Some(class_id))?;
        }
        Ok(())
    }

    fn define_function_body(
        &mut self,
        id: FuncId,
        f: &FnDecl,
        param_tys: &[JitTy],
        ret_ty: JitTy,
        this_class: Option<u32>,
    ) -> Result<(), CodegenError> {
        self.module.clear_context(&mut self.ctx);
        self.ctx.func.signature =
            self.module.declarations().get_function_decl(id).signature.clone();

        let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut env = Env::default();
        let mut block_param_idx = 0usize;

        // Bind `this` first, if this is a method.
        let this = match this_class {
            Some(class_id) => {
                let var = Variable::new(env.next_var_id());
                builder.declare_var(var, I64);
                let v = builder.block_params(entry)[block_param_idx];
                builder.def_var(var, v);
                block_param_idx += 1;
                Some((var, class_id))
            }
            None => None,
        };

        for (i, p) in f.params.iter().enumerate() {
            let pty = param_tys[i];
            let var = Variable::new(env.next_var_id());
            builder.declare_var(var, pty.cl().expect("non-unit checked at declare"));
            let v = builder.block_params(entry)[block_param_idx + i];
            builder.def_var(var, v);
            env.bindings.insert(p.name.clone(), (var, pty));
        }

        let mut lc = LowerCtx {
            funcs: &self.funcs,
            class_layouts: &self.class_layouts,
            class_methods: &self.class_methods,
            alloc_id: self.alloc_id,
            print: PrintFns {
                i64: self.print_i64,
                u64: self.print_u64,
                f64: self.print_f64,
                f32: self.print_f32,
                bool: self.print_bool,
                space: self.print_space,
                newline: self.print_newline,
            },
            module: &mut self.module,
            env: &mut env,
            loops: Vec::new(),
            this,
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
        let mut tc = ilang_types::TypeChecker::new();
        let prog_ty = tc.check(prog).map_err(|e| CodegenError::Cranelift(e.to_string()))?;
        let ret_ty = JitTy::from_ast(&prog_ty, ilang_ast::Span::dummy(), &self.class_ids)?;

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
            class_layouts: &self.class_layouts,
            class_methods: &self.class_methods,
            alloc_id: self.alloc_id,
            print: PrintFns {
                i64: self.print_i64,
                u64: self.print_u64,
                f64: self.print_f64,
                f32: self.print_f32,
                bool: self.print_bool,
                space: self.print_space,
                newline: self.print_newline,
            },
            module: &mut self.module,
            env: &mut env,
            loops: Vec::new(),
            this: None,
        };
        for s in &prog.stmts {
            lower_stmt(&mut builder, &mut lc, s)?;
        }
        let body = match &prog.tail {
            // A unit-typed tail (e.g. `console.log(...)`) is fine — we'll
            // emit a bare `return` and won't try to coerce a value.
            Some(t) => lower_expr(&mut builder, &mut lc, t)?,
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
                JitTy::Object(id) => {
                    let p = (std::mem::transmute::<_, extern "C" fn() -> i64>(ptr))();
                    JitValue::Object {
                        class: self.class_layouts[id as usize].name.clone(),
                        ptr: p,
                    }
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

// ─── Lowering context ───────────────────────────────────────────────────

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

struct PrintFns {
    i64: FuncId,
    u64: FuncId,
    f64: FuncId,
    f32: FuncId,
    bool: FuncId,
    space: FuncId,
    newline: FuncId,
}

struct LowerCtx<'a> {
    funcs: &'a HashMap<String, (FuncId, Vec<JitTy>, JitTy)>,
    class_layouts: &'a [ClassLayout],
    class_methods: &'a [HashMap<String, MethodInfo>],
    alloc_id: FuncId,
    print: PrintFns,
    module: &'a mut JITModule,
    env: &'a mut Env,
    loops: Vec<(Block, Block)>,
    /// `(this var, class id)` while compiling a method body.
    this: Option<(Variable, u32)>,
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
                Some(t) => JitTy::from_ast(t, s.span, &class_ids_from(lc))?,
                None => vt,
            };
            let coerced = coerce(b, (val, vt), bind_ty, s.span)?;
            let var = Variable::new(lc.env.next_var_id());
            b.declare_var(
                var,
                bind_ty.cl().ok_or_else(|| CodegenError::Unsupported {
                    what: "unit-typed let binding".into(),
                    span: s.span,
                })?,
            );
            b.def_var(var, coerced);
            lc.env.bindings.insert(name.clone(), (var, bind_ty));
        }
        StmtKind::Expr(e) => {
            let _ = lower_expr(b, lc, e)?;
        }
    }
    Ok(())
}

/// `class_ids` reverse-lookup so the lowering paths can resolve
/// annotations like `let x: Foo = ...` without a full TypeChecker.
fn class_ids_from(lc: &LowerCtx) -> HashMap<String, u32> {
    lc.class_layouts
        .iter()
        .enumerate()
        .map(|(i, l)| (l.name.clone(), i as u32))
        .collect()
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
        ExprKind::This => match lc.this {
            Some((var, class_id)) => Ok(Some((b.use_var(var), JitTy::Object(class_id)))),
            None => Err(CodegenError::Unsupported {
                what: "`this` outside a method body".into(),
                span: e.span,
            }),
        },
        ExprKind::Var(name) => {
            if let Some(&(var, vt)) = lc.env.bindings.get(name) {
                return Ok(Some((b.use_var(var), vt)));
            }
            // Implicit-`this` field access inside a method body.
            if let Some((this_var, class_id)) = lc.this {
                let layout = &lc.class_layouts[class_id as usize];
                if let Some(&(offset, fty)) = layout.fields.get(name) {
                    let this = b.use_var(this_var);
                    let v = b.ins().load(
                        fty.cl().expect("non-unit field"),
                        MemFlags::trusted(),
                        this,
                        offset as i32,
                    );
                    return Ok(Some((v, fty)));
                }
            }
            Err(CodegenError::Unsupported {
                what: format!("unknown variable {name:?}"),
                span: e.span,
            })
        }
        ExprKind::Cast { expr, ty } => {
            let inner = lower_expr(b, lc, expr)?.ok_or_else(|| CodegenError::Unsupported {
                what: "cast on unit".into(),
                span: e.span,
            })?;
            let target = JitTy::from_ast(ty, e.span, &class_ids_from(lc))?;
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
            // Ordinary local first; then implicit-`this` field write.
            if let Some(&(var, var_ty)) = lc.env.bindings.get(target) {
                let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                    CodegenError::Unsupported {
                        what: "assigning unit".into(),
                        span: e.span,
                    }
                })?;
                let coerced = coerce(b, (val, vt), var_ty, e.span)?;
                b.def_var(var, coerced);
                return Ok(None);
            }
            if let Some((this_var, class_id)) = lc.this {
                let layout = &lc.class_layouts[class_id as usize];
                if let Some(&(offset, fty)) = layout.fields.get(target) {
                    let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                        CodegenError::Unsupported {
                            what: "assigning unit".into(),
                            span: e.span,
                        }
                    })?;
                    let coerced = coerce(b, (val, vt), fty, e.span)?;
                    let this = b.use_var(this_var);
                    b.ins()
                        .store(MemFlags::trusted(), coerced, this, offset as i32);
                    return Ok(None);
                }
            }
            Err(CodegenError::Unsupported {
                what: format!("unknown variable {target:?}"),
                span: e.span,
            })
        }
        ExprKind::AssignField { obj, field, value } => {
            let (obj_v, obj_t) = lower_expr(b, lc, obj)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "field assignment receiver is unit".into(),
                    span: obj.span,
                }
            })?;
            let class_id = match obj_t {
                JitTy::Object(id) => id,
                _ => {
                    return Err(CodegenError::Unsupported {
                        what: "field assignment on non-object".into(),
                        span: obj.span,
                    });
                }
            };
            let layout = &lc.class_layouts[class_id as usize];
            let (offset, fty) = *layout.fields.get(field).ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("unknown field {field:?}"),
                    span: e.span,
                }
            })?;
            let (val, vt) = lower_expr(b, lc, value)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "field value is unit".into(),
                    span: e.span,
                }
            })?;
            let coerced = coerce(b, (val, vt), fty, e.span)?;
            b.ins()
                .store(MemFlags::trusted(), coerced, obj_v, offset as i32);
            Ok(None)
        }
        ExprKind::Field { obj, name } => {
            let (obj_v, obj_t) = lower_expr(b, lc, obj)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "field receiver is unit".into(),
                    span: obj.span,
                }
            })?;
            let class_id = match obj_t {
                JitTy::Object(id) => id,
                _ => {
                    return Err(CodegenError::Unsupported {
                        what: "field access on non-object".into(),
                        span: obj.span,
                    });
                }
            };
            let layout = &lc.class_layouts[class_id as usize];
            let (offset, fty) = *layout.fields.get(name).ok_or_else(|| {
                CodegenError::Unsupported {
                    what: format!("unknown field {name:?}"),
                    span: e.span,
                }
            })?;
            let v = b.ins().load(
                fty.cl().expect("non-unit field"),
                MemFlags::trusted(),
                obj_v,
                offset as i32,
            );
            Ok(Some((v, fty)))
        }
        ExprKind::MethodCall { obj, method, args } => {
            // Intercept the built-in `console.log(...)`. The receiver
            // expression is `console`, which has type Object("Console") at
            // the type-checker level but no class layout in the JIT — we
            // never need its value.
            if let ExprKind::Var(name) = &obj.kind {
                if name == "console" && method == "log" {
                    return lower_console_log(b, lc, args).map(|_| None);
                }
            }
            let (obj_v, obj_t) = lower_expr(b, lc, obj)?.ok_or_else(|| {
                CodegenError::Unsupported {
                    what: "method receiver is unit".into(),
                    span: obj.span,
                }
            })?;
            let class_id = match obj_t {
                JitTy::Object(id) => id,
                _ => {
                    return Err(CodegenError::Unsupported {
                        what: "method call on non-object".into(),
                        span: obj.span,
                    });
                }
            };
            call_method(b, lc, class_id, method, obj_v, args, e.span)
        }
        ExprKind::Call { callee, args } => {
            // Free function first.
            if let Some(entry) = lc.funcs.get(callee).cloned() {
                let (id, param_tys, ret_ty) = entry;
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
                    return Ok(None);
                }
                return Ok(Some((b.inst_results(call)[0], ret_ty)));
            }
            // Implicit method call on `this`.
            if let Some((this_var, class_id)) = lc.this {
                if lc.class_methods[class_id as usize].contains_key(callee) {
                    let this_v = b.use_var(this_var);
                    return call_method(b, lc, class_id, callee, this_v, args, e.span);
                }
            }
            Err(CodegenError::Unsupported {
                what: format!("unknown function {callee:?}"),
                span: e.span,
            })
        }
        ExprKind::New { class, args } => {
            let class_id = *lc
                .class_layouts
                .iter()
                .enumerate()
                .find(|(_, l)| l.name == *class)
                .map(|(i, _)| i)
                .map(|i| i as u32)
                .as_ref()
                .ok_or_else(|| CodegenError::Unsupported {
                    what: format!("unknown class {class:?}"),
                    span: e.span,
                })?;
            let size = lc.class_layouts[class_id as usize].size as i64;
            // ptr = ilang_jit_alloc(size)
            let alloc_ref = lc.module.declare_func_in_func(lc.alloc_id, b.func);
            let size_v = b.ins().iconst(I64, size);
            let alloc_call = b.ins().call(alloc_ref, &[size_v]);
            let ptr = b.inst_results(alloc_call)[0];
            // If init exists, call it.
            if lc.class_methods[class_id as usize].contains_key("init") {
                let _ = call_method(b, lc, class_id, "init", ptr, args, e.span)?;
            } else if !args.is_empty() {
                return Err(CodegenError::Unsupported {
                    what: format!("no `init` for class {class}, but args were given"),
                    span: e.span,
                });
            }
            Ok(Some((ptr, JitTy::Object(class_id))))
        }
        _ => Err(CodegenError::Unsupported {
            what: format!("expression {:?}", std::mem::discriminant(&e.kind)),
            span: e.span,
        }),
    }
}

/// Lower a `console.log(a, b, c, ...)` call: dispatch each argument to
/// the FFI print function for its type, separated by spaces, with a
/// trailing newline. Object args are unsupported for now and surface a
/// clear error.
fn lower_console_log(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    args: &[Expr],
) -> Result<(), CodegenError> {
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            let r = lc.module.declare_func_in_func(lc.print.space, b.func);
            b.ins().call(r, &[]);
        }
        let (av, at) = lower_expr(b, lc, a)?.ok_or_else(|| CodegenError::Unsupported {
            what: "console.log argument is unit".into(),
            span: a.span,
        })?;
        // Promote each scalar to the matching FFI signature, then call.
        let (id, arg) = match at {
            JitTy::I8 | JitTy::I16 | JitTy::I32 | JitTy::I64 => {
                let v = coerce(b, (av, at), JitTy::I64, a.span)?;
                (lc.print.i64, v)
            }
            JitTy::U8 | JitTy::U16 | JitTy::U32 | JitTy::U64 => {
                let v = coerce(b, (av, at), JitTy::U64, a.span)?;
                (lc.print.u64, v)
            }
            JitTy::F32 => (lc.print.f32, av),
            JitTy::F64 => (lc.print.f64, av),
            JitTy::Bool => (lc.print.bool, av),
            other => {
                return Err(CodegenError::Unsupported {
                    what: format!("console.log of {other:?}"),
                    span: a.span,
                });
            }
        };
        let r = lc.module.declare_func_in_func(id, b.func);
        b.ins().call(r, &[arg]);
    }
    let r = lc.module.declare_func_in_func(lc.print.newline, b.func);
    b.ins().call(r, &[]);
    Ok(())
}

fn call_method(
    b: &mut FunctionBuilder,
    lc: &mut LowerCtx,
    class_id: u32,
    method: &str,
    this_v: Value,
    args: &[Expr],
    span: ilang_ast::Span,
) -> Result<Option<TV>, CodegenError> {
    let info = lc.class_methods[class_id as usize]
        .get(method)
        .cloned()
        .ok_or_else(|| CodegenError::Unsupported {
            what: format!(
                "method {method:?} not found on class {:?}",
                lc.class_layouts[class_id as usize].name
            ),
            span,
        })?;
    let mut arg_vals = Vec::with_capacity(args.len() + 1);
    arg_vals.push(this_v);
    for (i, a) in args.iter().enumerate() {
        let (av, at) = lower_expr(b, lc, a)?.ok_or_else(|| CodegenError::Unsupported {
            what: "argument is unit".into(),
            span: a.span,
        })?;
        arg_vals.push(coerce(b, (av, at), info.params[i], a.span)?);
    }
    let func_ref = lc.module.declare_func_in_func(info.id, b.func);
    let call = b.ins().call(func_ref, &arg_vals);
    if matches!(info.ret, JitTy::Unit) {
        Ok(None)
    } else {
        Ok(Some((b.inst_results(call)[0], info.ret)))
    }
}

// ─── Operator lowering (numeric / bool — unchanged from before) ─────────

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
            let ev_coerced = coerce(b, (ev, _et), tt, cond.span)?;
            b.ins().jump(merge, &[ev_coerced]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            Ok(merge_param.map(|p| (p, tt)))
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
            Ok(None)
        }
        (None, _) => {
            b.ins().jump(merge, &[]);
            b.switch_to_block(merge);
            b.seal_block(merge);
            Ok(None)
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
        (JitTy::Bool, t) if t.is_int() => widen_int(b, v, 8, t, false),
        (t, JitTy::Bool) if t.is_int() => narrow_int(b, v, 8, t),
        (a, c) if a.is_int() && c.is_int() => {
            let from_w = a.int_width();
            let to_w = c.int_width();
            if to_w > from_w {
                widen_int(b, v, from_w, c, a.is_signed_int())
            } else if to_w < from_w {
                narrow_int(b, v, to_w, c)
            } else {
                v
            }
        }
        (a, JitTy::F32) if a.is_signed_int() => b.ins().fcvt_from_sint(F32, v),
        (a, JitTy::F32) if a.is_unsigned_int() => b.ins().fcvt_from_uint(F32, v),
        (a, JitTy::F64) if a.is_signed_int() => b.ins().fcvt_from_sint(F64, v),
        (a, JitTy::F64) if a.is_unsigned_int() => b.ins().fcvt_from_uint(F64, v),
        (JitTy::F32, JitTy::F64) => b.ins().fpromote(F64, v),
        (JitTy::F64, JitTy::F32) => b.ins().fdemote(F32, v),
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
    _from_width: u32,
    to: JitTy,
    signed: bool,
) -> Value {
    let to_cl = to.cl().expect("non-unit");
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
