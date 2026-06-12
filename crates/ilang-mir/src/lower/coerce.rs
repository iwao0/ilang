//! Numeric / heap coercion + binary-operand unification on
//! `BodyCx`.
//!
//! - `coerce(v, from, to)` materialises the right `Inst::Cast` /
//!   `Retain` / etc. so a value typed `from` can flow into a slot
//!   typed `to`. Integer widening / narrowing, float widening,
//!   primitive→`Optional<T>` boxing, `Object`→`Weak` downgrade,
//!   `Object`→subclass refinement, and `&self`-shaped no-ops all
//!   route through here.
//! - `unify_numeric(lv, lty, rv, rty)` is the binary-operand pre-pass
//!   used by `lower_binary` / `lower_logical`: it promotes whichever
//!   side is narrower so the resulting `Inst::BinOp` sees matching
//!   operand widths.

use ilang_ast::{Span, Symbol};

use crate::inst::{FuncRef, Inst, ValueId};
use crate::types::MirTy;

use super::utils::retain_if_heap;
use super::{BodyCx, LowerError};

impl<'a> BodyCx<'a> {
    pub(super) fn coerce(
        &mut self,
        v: ValueId,
        from: &MirTy,
        to: &MirTy,
        _span: Span,
    ) -> Result<ValueId, LowerError> {
        if from == to {
            return Ok(v);
        }
        // `MirTy::CReprEnum(eid)` and `MirTy::Enum(eid)` over the
        // same `eid` are the same value at the SSA / clif level —
        // the codegen-side `LoadField` for a CReprEnum slot returns
        // a heap-box pointer, and `StoreField` extracts the
        // discriminant before writing the inline integer. The MirTy
        // distinction exists only for the rc-slot predicate
        // (`is_heap` / `is_arc_slot`). Treat the pair as a no-op
        // coerce so an `AssignField` with `fty = CReprEnum` lower
        // path doesn't reject the enum-typed rhs.
        match (from, to) {
            (MirTy::Enum(a), MirTy::CReprEnum(b))
            | (MirTy::CReprEnum(a), MirTy::Enum(b))
            | (MirTy::CReprEnum(a), MirTy::CReprEnum(b))
                if a == b =>
            {
                return Ok(v);
            }
            _ => {}
        }
        use crate::inst::CastKind;
        // Same-signed integer resize.
        if (from.is_signed_int() && to.is_signed_int())
            || (from.is_unsigned_int() && to.is_unsigned_int())
        {
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Cast { dst, kind: CastKind::IntResize, src: v });
            return Ok(dst);
        }
        // Sign-cross.
        if from.is_int() && to.is_int() {
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Cast { dst, kind: CastKind::IntSignCross, src: v });
            return Ok(dst);
        }
        // Integer → Enum: build a unit-variant heap enum [tag] from
        // the integer (matches the heap layout `Inst::NewEnum` uses).
        // Used by `value as EnumName` casts whose discriminant only
        // becomes known at runtime.
        if from.is_int() && matches!(to, MirTy::Enum(_)) {
            // Widen to i64 first so the box always sees the canonical
            // integer width.
            let i64_v = if matches!(from, MirTy::I64 | MirTy::U64) {
                v
            } else {
                let widened = self.fb.new_value(MirTy::I64);
                let kind = if from.is_signed_int() {
                    CastKind::IntResize
                } else {
                    CastKind::IntResize
                };
                self.fb.push_inst(Inst::Cast { dst: widened, kind, src: v });
                widened
            };
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Call {
                dst: Some(dst),
                callee: FuncRef::Builtin(Symbol::intern("$enum.box")),
                args: Box::new([i64_v]),
            });
            return Ok(dst);
        }
        // Enum → string: only valid when the enum's repr is `string`.
        // `Inst::EnumDiscStr` carries the enum id so the codegen can
        // call `__enum_disc_str(global, tag)` directly.
        if let MirTy::Enum(eid) = &from {
            if matches!(to, MirTy::Str)
                && matches!(self.enums[eid.0 as usize].repr, MirTy::Str)
            {
                let dst = self.fb.new_value(MirTy::Str);
                self.fb.push_inst(Inst::EnumDiscStr { dst, enum_id: *eid, value: v });
                return Ok(dst);
            }
        }
        // Enum → Integer: read the tag at offset 0, then resize to
        // the requested int width.
        if matches!(from, MirTy::Enum(_)) && to.is_int() {
            let tag = self.fb.new_value(MirTy::I64);
            self.fb.push_inst(Inst::EnumTag { dst: tag, value: v });
            if matches!(to, MirTy::I64) {
                return Ok(tag);
            }
            let dst = self.fb.new_value(to.clone());
            let kind = if to.is_signed_int() {
                CastKind::IntResize
            } else {
                CastKind::IntResize
            };
            self.fb.push_inst(Inst::Cast { dst, kind, src: tag });
            return Ok(dst);
        }
        // `T → T?` Optional auto-wrap — must precede the i64-heap
        // bit-erasure paths below; otherwise `let x: i64? = 7`
        // would treat the literal `7` as a raw pointer.
        if let MirTy::Optional(inner) = to {
            if **inner == *from || matches!(**inner, MirTy::Unit) {
                // For a heap-typed inner the new Optional cell owns a
                // share of `v`; without bumping rc here, the source
                // binding's eventual release would drop the only
                // backing object before the Optional's cascade had a
                // chance to. Matches the explicit `some(x)` path in
                // `lower_expr`, which retains via `needs_retain`.
                // Caught by ASan as a UAF in `host_release_object`
                // during teardown of `recursive_method_optional_tree`,
                // where calls like `new Tree(10, l, r)` rely on this
                // coercion to wrap the local heap arguments.
                let v = match self.copy_fixed_for_cell(v, inner) {
                    // Fixed-of-arc inner: the cell owns a value
                    // copy (no rc to share).
                    Some(copy) => copy,
                    None => {
                        retain_if_heap(&mut self.fb, v, inner);
                        v
                    }
                };
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::NewOptional { dst, value: v });
                return Ok(dst);
            }
        }
        // `@handle pub struct H {}` ↔ `i64` / `*void` — pointer-sized
        // opaque, retag via PtrIntCast (identity at the clif level) so
        // the print-kind machinery sees I64 instead of Object(H). Must
        // precede the heap-erasure rule below, which would otherwise
        // return the Object-tagged ValueId unchanged.
        let is_handle = |t: &MirTy| match t {
            MirTy::Object(cid) => self.classes[cid.0 as usize].is_handle,
            _ => false,
        };
        let is_void_ptr_pre = |t: &MirTy| matches!(
            t,
            MirTy::RawPtr { inner, .. } if matches!(**inner, MirTy::CVoid)
        );
        if (matches!(from, MirTy::I64 | MirTy::U64) && is_handle(to))
            || (is_handle(from) && matches!(to, MirTy::I64 | MirTy::U64))
            || (is_void_ptr_pre(from) && is_handle(to))
            || (is_handle(from) && is_void_ptr_pre(to))
            || (is_handle(from) && is_handle(to))
        {
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: v });
            return Ok(dst);
        }
        // Heap-typed value → `i64` cell. This shows up when a heap
        // value flows into a slot whose declared MirTy is i64 (e.g.
        // the built-in `Result<T, E>` payload, where T / E erase to
        // i64 cells). The runtime layout of all heap pointers is i64.
        if from.is_heap() && matches!(to, MirTy::I64 | MirTy::U64) {
            return Ok(v);
        }
        // Same in reverse — sometimes a generic-erased i64 cell flows
        // back out into a heap-typed slot. Let the consumer deal with
        // the bit pattern.
        if matches!(from, MirTy::I64 | MirTy::U64) && to.is_heap() {
            return Ok(v);
        }
        // Subclass collections — `Child[]` / `Child?` / tuples-of-Child
        // flow into a slot typed for the parent. Heap layout matches
        // (objects are i64 pointers regardless of class), so this is
        // identity at the value level.
        if let (
            MirTy::Array { elem: e1, .. },
            MirTy::Array { elem: e2, .. },
        ) = (from, to)
        {
            if matches!((&**e1, &**e2), (MirTy::Object(_), MirTy::Object(_))) {
                return Ok(v);
            }
        }
        if let (MirTy::Optional(i1), MirTy::Optional(i2)) = (from, to) {
            // Both Optional<Object> and Optional<Array<Object>> share
            // the same heap rep, so all object-shaped Optionals are
            // bit-compatible.
            let is_obj_shape = |t: &MirTy| -> bool {
                matches!(
                    t,
                    MirTy::Object(_)
                        | MirTy::Array { .. }
                        | MirTy::Tuple(_)
                        | MirTy::Map { .. }
                        | MirTy::Optional(_)
                )
            };
            if is_obj_shape(&**i1) && is_obj_shape(&**i2) {
                return Ok(v);
            }
        }
        if let (MirTy::Tuple(_), MirTy::Tuple(_)) = (from, to) {
            return Ok(v);
        }
        // `none`-typed `Optional<Unit>` → `Optional<T>` for any T.
        // The MIR's none literal is a null pointer; widening the
        // declared inner type is a no-op at the bit level.
        if let (MirTy::Optional(inner), MirTy::Optional(_)) = (from, to) {
            if matches!(**inner, MirTy::Unit) {
                return Ok(v);
            }
        }
        // Dynamic array ↔ fixed-length array — same runtime layout
        // (3-i64 header + data), so this is an identity coerce.
        if let (
            MirTy::Array { .. },
            MirTy::Array { .. },
        ) = (from, to)
        {
            // Same runtime layout — type checker has already vetted
            // element compatibility (subtyping / variance).
            return Ok(v);
        }
        // Object (incl. CRepr struct) → *T  — used when an
        // @extern(C) fn takes a `*MyStruct` arg and the caller passes
        // the ilang-side instance.
        if matches!(from, MirTy::Object(_)) {
            if let MirTy::RawPtr { .. } = to {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrCast, src: v });
                return Ok(dst);
            }
        }
        // Array → *T (passes the array's data pointer, not the
        // header).
        //
        // Two layouts:
        //   * `len: Some(_)` — fixed-length, header-less. The SSA
        //     value is already the buffer pointer; the cast is a
        //     bit-level no-op.
        //   * `len: None`   — dynamic, with the 48-byte header.
        //     Load `data_ptr` from offset 16 via __array_data_ptr.
        if let (MirTy::Array { len, .. }, MirTy::RawPtr { .. }) = (from, to) {
            if len.is_some() {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast {
                    dst,
                    kind: CastKind::PtrCast,
                    src: v,
                });
                return Ok(dst);
            }
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Call {
                dst: Some(dst),
                callee: FuncRef::Builtin(Symbol::intern("$array.dataPtr")),
                args: Box::new([v]),
            });
            return Ok(dst);
        }
        // *T → *const T (drop write capability).
        if let (
            MirTy::RawPtr { is_const: false, inner: i1 },
            MirTy::RawPtr { is_const: true, inner: i2 },
        ) = (from, to)
        {
            if i1 == i2 {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrCast, src: v });
                return Ok(dst);
            }
        }
        // Raw pointer reinterprets (within @extern(C)).
        if let (MirTy::RawPtr { .. }, MirTy::RawPtr { .. }) = (from, to) {
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrCast, src: v });
            return Ok(dst);
        }
        // *T ↔ i64.
        if matches!(from, MirTy::RawPtr { .. }) && matches!(to, MirTy::I64 | MirTy::U64) {
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: v });
            return Ok(dst);
        }
        if matches!(from, MirTy::I64 | MirTy::U64) && matches!(to, MirTy::RawPtr { .. }) {
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: v });
            return Ok(dst);
        }
        // Raw pointer → fn(...). The destination type at the AST level
        // is `Type::Fn(...)` (a closure type) but the value is a bare
        // 8-byte fn pointer from `GetProcAddress` / `dlsym`. We tag the
        // MIR-side type as `MirTy::RawFn` so the call lowering knows to
        // skip closure dispatch (no fn_ptr load from offset 0, no
        // trailing env arg).
        if matches!(from, MirTy::RawPtr { .. }) {
            if let MirTy::Fn(ft) = to {
                let raw_ty = MirTy::RawFn(ft.clone());
                let dst = self.fb.new_value(raw_ty);
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrCast, src: v });
                return Ok(dst);
            }
        }
        // fn(...) → raw pointer — extract the underlying fn pointer.
        // Valid for `MirTy::RawFn` (the value already IS an 8-byte
        // address). For a true `MirTy::Fn` closure box this would lose
        // the env, which is intentionally not supported.
        if matches!(from, MirTy::RawFn(_)) && matches!(to, MirTy::RawPtr { .. }) {
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrCast, src: v });
            return Ok(dst);
        }
        // Strong → weak. Same class is the no-op auto-downgrade;
        // a subclass strong reference may also land in a parent
        // weak slot — the value is the same i64 heap pointer and a
        // single StrongToWeak cast preserves the weak-rc protocol.
        if let (MirTy::Object(c1), MirTy::Weak(c2)) = (from, to) {
            let mut is_sub = c1 == c2;
            if !is_sub {
                let mut cur = self.classes[c1.0 as usize].parent;
                while let Some(p) = cur {
                    if p == *c2 {
                        is_sub = true;
                        break;
                    }
                    cur = self.classes[p.0 as usize].parent;
                }
            }
            if is_sub {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::StrongToWeak, src: v });
                return Ok(dst);
            }
        }
        // Subclass → parent (Object subtype → Object supertype).
        if let (MirTy::Object(_c1), MirTy::Object(_c2)) = (from, to) {
            // Subtype check is the type checker's responsibility; we
            // just propagate the value (the runtime layout is the same
            // i64 pointer with a header).
            return Ok(v);
        }
        // `@com interface` ↔ `i64` / `*void` — the interface value is
        // a bare 8-byte COM handle (no ARC header). Treat the cast
        // as identity at the ABI level; PtrIntCast handles the
        // bit-pattern reinterpret on the codegen side.
        let is_com = |t: &MirTy| match t {
            MirTy::Object(cid) => self
                .com_interfaces
                .iter()
                .any(|n| self.classes[cid.0 as usize].name == *n),
            _ => false,
        };
        let is_void_ptr = |t: &MirTy| matches!(
            t,
            MirTy::RawPtr { inner, .. } if matches!(**inner, MirTy::CVoid)
        );
        if (matches!(from, MirTy::I64 | MirTy::U64) && is_com(to))
            || (is_com(from) && matches!(to, MirTy::I64 | MirTy::U64))
            || (is_void_ptr(from) && is_com(to))
            || (is_com(from) && is_void_ptr(to))
        {
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: v });
            return Ok(dst);
        }
        // `@handle pub struct H {}` ↔ `i64` / `*void` / other
        // handle — pointer-sized opaque, bit-identical reinterpret.
        let is_handle = |t: &MirTy| match t {
            MirTy::Object(cid) => self.classes[cid.0 as usize].is_handle,
            _ => false,
        };
        if (matches!(from, MirTy::I64 | MirTy::U64) && is_handle(to))
            || (is_handle(from) && matches!(to, MirTy::I64 | MirTy::U64))
            || (is_void_ptr(from) && is_handle(to))
            || (is_handle(from) && is_void_ptr(to))
            || (is_handle(from) && is_handle(to))
        {
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: v });
            return Ok(dst);
        }
        if from.is_int() && to.is_float() {
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Cast { dst, kind: CastKind::IntToFloat, src: v });
            return Ok(dst);
        }
        if from.is_float() && to.is_int() {
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Cast { dst, kind: CastKind::FloatToInt, src: v });
            return Ok(dst);
        }
        if from.is_float() && to.is_float() {
            let dst = self.fb.new_value(to.clone());
            self.fb.push_inst(Inst::Cast { dst, kind: CastKind::FloatResize, src: v });
            return Ok(dst);
        }
        // `Promise<A>` ↔ `Promise<B>` is structurally compatible at
        // the runtime level (every promise is an i64 pointer to a
        // `ManagedPromise`; the inner type only affects how the
        // value is interpreted at `.then` / settle time, which is
        // driven by an explicit kind tag, not by the static type).
        // Needed for `Promise.__pending()` (returns `Promise<()>` at
        // the MIR layer) feeding into typed bindings the async
        // state-machine desugar emits.
        if matches!(from, MirTy::Promise(_)) && matches!(to, MirTy::Promise(_)) {
            return Ok(v);
        }
        Err(LowerError::Other(format!("no coercion from {from} to {to}")))
    }

    pub(super) fn unify_numeric(
        &mut self,
        lv: ValueId,
        lty: MirTy,
        rv: ValueId,
        rty: MirTy,
    ) -> Result<(ValueId, ValueId, MirTy), LowerError> {
        if lty == rty {
            return Ok((lv, rv, lty));
        }
        // String concat with `+` is its own case in lower_binary.
        if matches!((&lty, &rty), (MirTy::Str, MirTy::Str)) {
            return Ok((lv, rv, MirTy::Str));
        }
        // Cross-class object comparison (Eq / Ne) — both pointers
        // share the same i64 rep, so just pass through with the more
        // specific class on the result side. The caller only uses
        // the unified type to pick the BinOp; for objects we fall
        // through to integer compare logic.
        if matches!((&lty, &rty), (MirTy::Object(_), MirTy::Object(_))) {
            return Ok((lv, rv, MirTy::I64));
        }
        // Enum-vs-int unification: `pub enum E: T { ... }` paired
        // with a value of type T (e.g. `msg == WindowMessage.destroy`
        // where `msg: u32`) reads the enum tag down to the int side
        // so the rest of the numeric unification logic handles it.
        // The opposite direction (`int == enum`) is symmetric.
        if let (MirTy::Enum(_), other) = (&lty, &rty) {
            if other.is_numeric() {
                let lv = self.coerce(lv, &lty, &rty, Span::dummy())?;
                return Ok((lv, rv, rty));
            }
        }
        if let (other, MirTy::Enum(_)) = (&lty, &rty) {
            if other.is_numeric() {
                let rv = self.coerce(rv, &rty, &lty, Span::dummy())?;
                return Ok((lv, rv, lty));
            }
        }
        // Enum-vs-string unification: `pub enum E: string { ... }`
        // paired with a `string` reads the enum's discriminant
        // string so `e.code == KeyCode.keyA` compares against the
        // backend-supplied `"KeyA"` directly. Falls through to
        // the `Str == Str` arm in `bin_result`.
        let enum_is_str_repr = |t: &MirTy| match t {
            MirTy::Enum(eid) => matches!(
                self.enums[eid.0 as usize].repr,
                MirTy::Str
            ),
            _ => false,
        };
        if enum_is_str_repr(&lty) && matches!(rty, MirTy::Str) {
            let lv = self.coerce(lv, &lty, &MirTy::Str, Span::dummy())?;
            return Ok((lv, rv, MirTy::Str));
        }
        if matches!(lty, MirTy::Str) && enum_is_str_repr(&rty) {
            let rv = self.coerce(rv, &rty, &MirTy::Str, Span::dummy())?;
            return Ok((lv, rv, MirTy::Str));
        }
        if lty.is_numeric() && rty.is_numeric() {
            // Promote to float if either side is float.
            if lty.is_float() || rty.is_float() {
                let target = if matches!(lty, MirTy::F64) || matches!(rty, MirTy::F64) {
                    MirTy::F64
                } else {
                    MirTy::F32
                };
                let lv = self.coerce(lv, &lty, &target, Span::dummy())?;
                let rv = self.coerce(rv, &rty, &target, Span::dummy())?;
                return Ok((lv, rv, target));
            }
            // Two integers: pick the wider of the two same-signedness.
            if lty.is_signed_int() == rty.is_signed_int() {
                let target = if lty.int_width() >= rty.int_width() { lty.clone() } else { rty.clone() };
                let lv = self.coerce(lv, &lty, &target, Span::dummy())?;
                let rv = self.coerce(rv, &rty, &target, Span::dummy())?;
                return Ok((lv, rv, target));
            }
            // Cross-sign integer arithmetic — common in FFI bindings
            // where C `size_t` (unsigned) flows into ilang `i64`
            // arithmetic, or vice-versa. Pick the wider type; on a
            // tie prefer the signed side (closer to the conventional
            // "promote-both-to-i64" C behaviour). Coerce both
            // operands; the bit pattern survives because IntResize
            // chooses uextend/sextend based on the source's
            // signedness (per `lower_cast`).
            let target = if lty.int_width() > rty.int_width() {
                lty.clone()
            } else if rty.int_width() > lty.int_width() {
                rty.clone()
            } else if lty.is_signed_int() {
                lty.clone()
            } else {
                rty.clone()
            };
            let lv = self.coerce(lv, &lty, &target, Span::dummy())?;
            let rv = self.coerce(rv, &rty, &target, Span::dummy())?;
            return Ok((lv, rv, target));
        }
        Err(LowerError::Other(format!(
            "cannot unify {lty} and {rty} in arithmetic context"
        )))
    }
}
