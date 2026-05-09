//! Textual MIR dumper. Used for debugging and golden tests.
//!
//! Format example:
//!
//! ```text
//! func add(v0: i64, v1: i64) -> i64 {
//!   bb0:
//!     v2 = iadd v0, v1
//!     return v2
//! }
//! ```

use std::fmt::Write;

use crate::inst::{
    BinOp, BlockId, CastKind, FuncRef, Inst, MirConst, Terminator, UnOp, ValueId,
};
use crate::program::{Function, FunctionKind, Program};

pub fn print_program(p: &Program) -> String {
    let mut out = String::new();
    for c in &p.classes {
        let _ = writeln!(out, "class #{} {} (parent: {:?})", c.id.0, c.name, c.parent);
        for f in &c.fields {
            let _ = writeln!(out, "  field {} : {}", f.name, f.ty);
        }
        for m in &c.methods {
            let _ = writeln!(
                out,
                "  method {} -> func#{} (slot: {:?})",
                m.name, m.func.0, m.slot
            );
        }
    }
    for e in &p.enums {
        let _ = writeln!(out, "enum #{} {} (repr: {})", e.id.0, e.name, e.repr);
        for v in &e.variants {
            let _ = writeln!(out, "  {} = {}", v.name, v.discriminant);
        }
    }
    for s in &p.statics {
        let _ = writeln!(
            out,
            "static {}.{} : {} = {}",
            p.classes[s.owner.0 as usize].name, s.name, s.ty,
            print_const(&s.init),
        );
    }
    for f in &p.functions {
        out.push_str(&print_function(f));
        out.push('\n');
    }
    let _ = writeln!(out, "entry: func#{}", p.entry.0);
    out
}

pub fn print_function(f: &Function) -> String {
    let mut out = String::new();
    let kind_tag = match &f.kind {
        FunctionKind::Local => "fn",
        FunctionKind::Init { .. } => "init",
        FunctionKind::Drop { .. } => "drop",
        FunctionKind::Extern { .. } => "extern",
        FunctionKind::ExternBody => "extern-fn",
        FunctionKind::Trampoline { .. } => "trampoline",
    };
    let _ = write!(out, "{kind_tag} {}", f.display_name);
    if f.name != f.display_name {
        let _ = write!(out, " [{}]", f.name);
    }
    out.push('(');
    for (i, p) in f.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "{}: {}", fmt_value(p.value), p.ty);
    }
    let _ = writeln!(out, ") -> {} {{", f.ret);
    for (i, blk) in f.blocks.iter().enumerate() {
        let bid = BlockId(i as u32);
        out.push_str(&print_block(f, bid, blk));
    }
    out.push_str("}\n");
    out
}

fn print_block(f: &Function, id: BlockId, blk: &crate::program::Block) -> String {
    let mut out = String::new();
    let _ = write!(out, "  bb{}", id.0);
    if !blk.params.is_empty() {
        out.push('(');
        for (i, p) in blk.params.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            let _ = write!(out, "{}: {}", fmt_value(*p), f.ty_of(*p));
        }
        out.push(')');
    }
    out.push_str(":\n");
    for inst in &blk.insts {
        let _ = writeln!(out, "    {}", fmt_inst(f, inst));
    }
    let _ = writeln!(out, "    {}", fmt_term(&blk.term));
    out
}

fn fmt_value(v: ValueId) -> String {
    format!("v{}", v.0)
}

fn fmt_args(vs: &[ValueId]) -> String {
    vs.iter().map(|v| fmt_value(*v)).collect::<Vec<_>>().join(", ")
}

fn fmt_block_args(target: BlockId, args: &[ValueId]) -> String {
    if args.is_empty() {
        format!("bb{}", target.0)
    } else {
        format!("bb{}({})", target.0, fmt_args(args))
    }
}

fn fmt_inst(f: &Function, inst: &Inst) -> String {
    let dst_pre = |dst: ValueId| {
        format!("{} : {} = ", fmt_value(dst), f.ty_of(dst))
    };
    let opt_dst_pre = |dst: Option<ValueId>| match dst {
        Some(d) => dst_pre(d),
        None => String::new(),
    };
    match inst {
        Inst::Const { dst, value } => format!("{}const {}", dst_pre(*dst), print_const(value)),
        Inst::BinOp { dst, op, lhs, rhs } => {
            format!(
                "{}{} {}, {}",
                dst_pre(*dst),
                fmt_binop(*op),
                fmt_value(*lhs),
                fmt_value(*rhs)
            )
        }
        Inst::UnOp { dst, op, src } => {
            format!("{}{} {}", dst_pre(*dst), fmt_unop(*op), fmt_value(*src))
        }
        Inst::Cast { dst, kind, src } => {
            format!("{}cast.{:?} {}", dst_pre(*dst), kind, fmt_value(*src))
        }
        Inst::Call { dst, callee, args } => {
            format!(
                "{}call {} ({})",
                opt_dst_pre(*dst),
                fmt_funcref(callee),
                fmt_args(args)
            )
        }
        Inst::CallIndirect { dst, callee, args, .. } => {
            format!(
                "{}call_indirect {} ({})",
                opt_dst_pre(*dst),
                fmt_value(*callee),
                fmt_args(args)
            )
        }
        Inst::VirtCall { dst, recv, slot, args } => {
            format!(
                "{}virt_call {}.{} ({})",
                opt_dst_pre(*dst),
                fmt_value(*recv),
                slot.0,
                fmt_args(args)
            )
        }
        Inst::NewObject { dst, class, init_args, init } => {
            format!(
                "{}new_object class#{} init=func#{} ({})",
                dst_pre(*dst),
                class.0,
                init.0,
                fmt_args(init_args)
            )
        }
        Inst::LoadField { dst, obj, field } => format!(
            "{}load_field {}.f{}",
            dst_pre(*dst),
            fmt_value(*obj),
            field.0
        ),
        Inst::StoreField { obj, field, value } => format!(
            "store_field {}.f{} = {}",
            fmt_value(*obj),
            field.0,
            fmt_value(*value)
        ),
        Inst::NewArray { dst, elem, items } => format!(
            "{}new_array<{}> ({})",
            dst_pre(*dst),
            elem,
            fmt_args(items)
        ),
        Inst::NewArrayEmpty { dst, elem, fixed_len } => format!(
            "{}new_array_empty<{}> fixed={:?}",
            dst_pre(*dst),
            elem,
            fixed_len
        ),
        Inst::ArrayLen { dst, arr } => format!("{}array_len {}", dst_pre(*dst), fmt_value(*arr)),
        Inst::ArrayLoad { dst, arr, idx } => format!(
            "{}array_load {}, {}",
            dst_pre(*dst),
            fmt_value(*arr),
            fmt_value(*idx)
        ),
        Inst::ArrayStore { arr, idx, value } => format!(
            "array_store {}, {} = {}",
            fmt_value(*arr),
            fmt_value(*idx),
            fmt_value(*value)
        ),
        Inst::NewMap { dst, key, val, entries } => {
            let pairs: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}: {}", fmt_value(*k), fmt_value(*v)))
                .collect();
            format!(
                "{}new_map<{}, {}> {{{}}}",
                dst_pre(*dst),
                key,
                val,
                pairs.join(", ")
            )
        }
        Inst::MapGet { dst, map, key } => format!(
            "{}map_get {}, {}",
            dst_pre(*dst),
            fmt_value(*map),
            fmt_value(*key)
        ),
        Inst::MapSet { map, key, value } => format!(
            "map_set {}, {} = {}",
            fmt_value(*map),
            fmt_value(*key),
            fmt_value(*value)
        ),
        Inst::NewTuple { dst, items } => {
            format!("{}new_tuple ({})", dst_pre(*dst), fmt_args(items))
        }
        Inst::TupleExtract { dst, tup, idx } => format!(
            "{}tuple_extract {}, {}",
            dst_pre(*dst),
            fmt_value(*tup),
            idx
        ),
        Inst::NewOptional { dst, value } => {
            format!("{}new_optional {}", dst_pre(*dst), fmt_value(*value))
        }
        Inst::OptionalIsSome { dst, opt } => {
            format!("{}optional_is_some {}", dst_pre(*dst), fmt_value(*opt))
        }
        Inst::OptionalUnwrap { dst, opt } => {
            format!("{}optional_unwrap {}", dst_pre(*dst), fmt_value(*opt))
        }
        Inst::NewEnum { dst, enum_id, variant, payload } => format!(
            "{}new_enum enum#{}.{} ({})",
            dst_pre(*dst),
            enum_id.0,
            variant.0,
            fmt_args(payload)
        ),
        Inst::EnumTag { dst, value } => {
            format!("{}enum_tag {}", dst_pre(*dst), fmt_value(*value))
        }
        Inst::EnumPayload { dst, value, variant, idx } => format!(
            "{}enum_payload {}.v{}.f{}",
            dst_pre(*dst),
            fmt_value(*value),
            variant.0,
            idx
        ),
        Inst::MakeClosure { dst, func, captures } => format!(
            "{}make_closure func#{} ({})",
            dst_pre(*dst),
            func.0,
            fmt_args(captures)
        ),
        Inst::LoadCapture { dst, idx } => {
            format!("{}load_capture {}", dst_pre(*dst), idx)
        }
        Inst::Retain { value } => format!("retain {}", fmt_value(*value)),
        Inst::Release { value } => format!("release {}", fmt_value(*value)),
        Inst::WeakRetain { value } => format!("weak_retain {}", fmt_value(*value)),
        Inst::WeakRelease { value } => format!("weak_release {}", fmt_value(*value)),
        Inst::WeakUpgrade { dst, weak } => {
            format!("{}weak_upgrade {}", dst_pre(*dst), fmt_value(*weak))
        }
        Inst::TypeOf { dst, value } => {
            format!("{}typeof {}", dst_pre(*dst), fmt_value(*value))
        }
        Inst::IsInstance { dst, value, class } => format!(
            "{}is_instance {}, class#{}",
            dst_pre(*dst),
            fmt_value(*value),
            class.0
        ),
        Inst::DowncastOrNone { dst, value, class } => format!(
            "{}downcast_or_none {}, class#{}",
            dst_pre(*dst),
            fmt_value(*value),
            class.0
        ),
        Inst::LoadStatic { dst, slot } => {
            format!("{}load_static slot#{}", dst_pre(*dst), slot.0)
        }
        Inst::StoreStatic { slot, value } => {
            format!("store_static slot#{} = {}", slot.0, fmt_value(*value))
        }
        Inst::Panic { msg } => format!("panic {:?}", msg.as_str()),
        Inst::DefLocal { local, value } => {
            format!("def_local %{} = {}", local.0, fmt_value(*value))
        }
        Inst::UseLocal { dst, local } => {
            format!("{}use_local %{}", dst_pre(*dst), local.0)
        }
    }
}

fn fmt_term(t: &Terminator) -> String {
    match t {
        Terminator::Br { dst, args } => format!("br {}", fmt_block_args(*dst, args)),
        Terminator::CondBr { cond, then_block, then_args, else_block, else_args } => format!(
            "cond_br {}, {}, {}",
            fmt_value(*cond),
            fmt_block_args(*then_block, then_args),
            fmt_block_args(*else_block, else_args),
        ),
        Terminator::Switch { scrutinee, cases, default, default_args } => {
            let cs: Vec<String> = cases
                .iter()
                .map(|c| format!("{} -> {}", c.value, fmt_block_args(c.dst, &c.args)))
                .collect();
            format!(
                "switch {} [{}] default {}",
                fmt_value(*scrutinee),
                cs.join(", "),
                fmt_block_args(*default, default_args)
            )
        }
        Terminator::Return { value: Some(v) } => format!("return {}", fmt_value(*v)),
        Terminator::Return { value: None } => "return".into(),
        Terminator::Unreachable => "unreachable".into(),
    }
}

fn fmt_funcref(f: &FuncRef) -> String {
    match f {
        FuncRef::Local(id) => format!("func#{}", id.0),
        FuncRef::Builtin(s) => format!("builtin@{s}"),
        FuncRef::Extern { sym, libs, optional } => format!(
            "extern@{sym}{}(libs={:?})",
            if *optional { "?" } else { "" },
            libs.iter().map(|s| s.as_str()).collect::<Vec<_>>()
        ),
    }
}

fn fmt_binop(op: BinOp) -> &'static str {
    match op {
        BinOp::IAdd => "iadd",
        BinOp::ISub => "isub",
        BinOp::IMul => "imul",
        BinOp::IDivS => "idiv_s",
        BinOp::IDivU => "idiv_u",
        BinOp::IRemS => "irem_s",
        BinOp::IRemU => "irem_u",
        BinOp::IShl => "ishl",
        BinOp::IShrS => "ishr_s",
        BinOp::IShrU => "ishr_u",
        BinOp::IAnd => "iand",
        BinOp::IOr => "ior",
        BinOp::IXor => "ixor",
        BinOp::FAdd => "fadd",
        BinOp::FSub => "fsub",
        BinOp::FMul => "fmul",
        BinOp::FDiv => "fdiv",
        BinOp::IEq => "ieq",
        BinOp::INe => "ine",
        BinOp::ILtS => "ilt_s",
        BinOp::ILeS => "ile_s",
        BinOp::IGtS => "igt_s",
        BinOp::IGeS => "ige_s",
        BinOp::ILtU => "ilt_u",
        BinOp::ILeU => "ile_u",
        BinOp::IGtU => "igt_u",
        BinOp::IGeU => "ige_u",
        BinOp::FEq => "feq",
        BinOp::FNe => "fne",
        BinOp::FLt => "flt",
        BinOp::FLe => "fle",
        BinOp::FGt => "fgt",
        BinOp::FGe => "fge",
        BinOp::StrEq => "str_eq",
        BinOp::StrNe => "str_ne",
        BinOp::StrConcat => "str_concat",
    }
}

fn fmt_unop(op: UnOp) -> &'static str {
    match op {
        UnOp::INeg => "ineg",
        UnOp::FNeg => "fneg",
        UnOp::Not => "bnot",
        UnOp::BoolNot => "lnot",
    }
}

fn print_const(c: &MirConst) -> String {
    match c {
        MirConst::Bool(b) => b.to_string(),
        MirConst::Int(n) => n.to_string(),
        MirConst::F32(bits) => format!("{}f32", f32::from_bits(*bits)),
        MirConst::F64(bits) => format!("{}f64", f64::from_bits(*bits)),
        MirConst::Str(s) => format!("{:?}", s.as_str()),
        MirConst::Unit => "()".into(),
        MirConst::None => "none".into(),
    }
}

#[allow(dead_code)]
fn _force_use(_: CastKind) {} // silence unused-import warnings if any
