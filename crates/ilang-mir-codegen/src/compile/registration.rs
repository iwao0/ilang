//! Driver for the per-class / per-enum registrations the runtime
//! needs at startup. JIT setup and the AOT init-body emitter
//! previously each contained hand-copied class / enum loops that
//! walked the same data and called the same `$class.register*` /
//! `$type.register*` entry points; this module captures the walks
//! as single functions that yield registration events to
//! backend-specific sinks.
//!
//! - `ReflectionSink` covers `typeof(x).<member>` (`$type.register*`).
//! - `ClassLayoutSink` covers print info + heap-layout tables
//!   (`$class.registerPrintName` / `$class.registerPrintField` /
//!   `$class.registerStructPrintField` / `$class.registerSize` /
//!   `$class.registerObjectField`).
//!
//! Vtable / drop registrations stay outside this module — they
//! require `FuncId → fn_addr` resolution that JIT does only after
//! `finalize_definitions()`, while AOT emits `func_addr` IR inline.
//! Closure / fn-name / lib-group tables likewise still live in
//! each backend.

use std::collections::HashMap;

use ilang_ast::Symbol;
use ilang_mir::{
    types::ClassId,
    MirTy, Program,
};

use super::{
    mir_ty_to_type_id, parse_class_name_type_args, type_arg_id_by_name,
};

/// Backend-specific receiver for the reflection registration events
/// the driver yields. JIT calls the matching `ilang_runtime::__*`
/// extern directly; AOT lowers each one as a `call $type.register*`
/// IR instruction inside `__ilang_aot_init`.
pub(crate) trait ReflectionSink {
    /// `class.parent` recorded as the parent's global class id (0
    /// when the class has no parent).
    fn type_parent(&mut self, gcid: i64, parent_gcid: i64);
    /// Method name registration. `class_idx` and `method_idx` index
    /// into `prog.classes[class_idx].methods[method_idx]` so AOT can
    /// look up the pre-allocated data symbol for the string body.
    fn type_method(
        &mut self,
        class_idx: usize,
        method_idx: usize,
        gcid: i64,
        method_name: &str,
    );
    /// Method return-type id, paired with the method name lookup.
    fn type_method_return(
        &mut self,
        class_idx: usize,
        method_idx: usize,
        gcid: i64,
        method_name: &str,
        ret_id: i64,
    );
    /// Method parameter type, one call per non-`this` parameter.
    fn type_method_param(
        &mut self,
        class_idx: usize,
        method_idx: usize,
        gcid: i64,
        method_name: &str,
        param_idx: i64,
        param_id: i64,
    );
    /// Declared field's type id, paired with the field name lookup.
    /// Only declared (own) fields fire this; inherited fields are
    /// resolved through the parent chain by the runtime.
    fn type_field_type(
        &mut self,
        class_idx: usize,
        field_idx: usize,
        gcid: i64,
        field_name: &str,
        fty_id: i64,
    );
    /// Number of fields declared on this class itself (no inherited
    /// prefix). Lets `__type_fields` slice the print-info names
    /// down to the declared range.
    fn type_declared_field_count(&mut self, gcid: i64, count: i64);
    /// Generic instance type arg — parsed back out of the
    /// monomorphised class name (`Box<i64>` → `["i64"]`).
    fn type_arg(&mut self, gcid: i64, idx: i64, arg_id: i64);
}

/// Walk every class in `prog` and emit the reflection-meta calls
/// each one needs. The driver is responsible for the bookkeeping
/// (skipping `this` on instance methods, dropping inherited fields
/// from the declared-fields count, parsing generic instance args
/// from the mangled class name); sinks just forward each event to
/// their backend's registration channel.
pub(crate) fn emit_reflection_registrations<S: ReflectionSink>(
    prog: &Program,
    class_global: &[u32],
    sink: &mut S,
) {
    let class_name_to_id: HashMap<Symbol, ClassId> =
        prog.classes.iter().map(|c| (c.name, c.id)).collect();
    let global_cid_fn = |c: u32| class_global[c as usize];

    for (class_idx, class) in prog.classes.iter().enumerate() {
        let gcid = class_global[class.id.0 as usize] as i64;

        // Parent.
        let parent_id = class
            .parent
            .map(|p| class_global[p.0 as usize] as i64)
            .unwrap_or(0);
        sink.type_parent(gcid, parent_id);

        // Methods (name + return + each non-`this` parameter).
        for (method_idx, m) in class.methods.iter().enumerate() {
            let mname = m.name.as_str();
            sink.type_method(class_idx, method_idx, gcid, mname);
            let func = &prog.functions[m.func.0 as usize];
            let ret_id = mir_ty_to_type_id(&func.ret, &global_cid_fn);
            sink.type_method_return(
                class_idx, method_idx, gcid, mname, ret_id,
            );
            for (pi, p) in func.params.iter().enumerate() {
                if !m.is_static && pi == 0 {
                    continue;
                }
                let pid = mir_ty_to_type_id(&p.ty, &global_cid_fn);
                sink.type_method_param(
                    class_idx,
                    method_idx,
                    gcid,
                    mname,
                    pi as i64,
                    pid,
                );
            }
        }

        // Declared fields' types + a count for the runtime slicer.
        // MIR's `class.fields` prepends every inherited field for
        // layout reasons; reflection only reports the names declared
        // on this class itself.
        let parent_field_count = class
            .parent
            .map(|p| prog.classes[p.0 as usize].fields.len())
            .unwrap_or(0);
        for (field_idx, f) in
            class.fields.iter().enumerate().skip(parent_field_count)
        {
            let fty_id = mir_ty_to_type_id(&f.ty, &global_cid_fn);
            sink.type_field_type(
                class_idx,
                field_idx,
                gcid,
                f.name.as_str(),
                fty_id,
            );
        }
        sink.type_declared_field_count(
            gcid,
            (class.fields.len() - parent_field_count) as i64,
        );

        // Generic instance args — parsed from the monomorphised
        // class name, since the post-monomorph `ClassLayout` doesn't
        // carry the original `<T, U>` substitutions explicitly.
        let arg_names = parse_class_name_type_args(class.name.as_str());
        for (idx, arg) in arg_names.iter().enumerate() {
            let aid = type_arg_id_by_name(
                arg,
                &class_name_to_id,
                &global_cid_fn,
            );
            sink.type_arg(gcid, idx as i64, aid);
        }
    }

    // Silence unused-import warnings when the file's only use of
    // these is inside `mir_ty_to_type_id`'s call site (the type is
    // still part of the public-by-crate surface elsewhere).
    let _ = std::marker::PhantomData::<MirTy>;
}

/// JIT-side sink — dispatches each event to the matching
/// `ilang_runtime::__register_type_*` extern. Method / field names
/// reach the runtime through `leak_cstring`'s persistent buffers
/// (the runtime's table owns the resulting +1 rc, which is fine
/// because each class registers exactly once per JIT session).
#[allow(non_camel_case_types)]
pub(crate) struct ReflectionSink_JIT;

impl ReflectionSink for ReflectionSink_JIT {
    fn type_parent(&mut self, gcid: i64, parent_gcid: i64) {
        ilang_runtime::__register_type_parent(gcid, parent_gcid);
    }
    fn type_method(
        &mut self,
        _class_idx: usize,
        method_idx: usize,
        gcid: i64,
        method_name: &str,
    ) {
        let ptr = ilang_runtime::leak_cstring(method_name.to_string());
        ilang_runtime::__register_type_method(
            gcid,
            method_idx as i64,
            ptr,
        );
    }
    fn type_method_return(
        &mut self,
        _class_idx: usize,
        _method_idx: usize,
        gcid: i64,
        method_name: &str,
        ret_id: i64,
    ) {
        let ptr = ilang_runtime::leak_cstring(method_name.to_string());
        ilang_runtime::__register_type_method_return(gcid, ptr, ret_id);
    }
    fn type_method_param(
        &mut self,
        _class_idx: usize,
        _method_idx: usize,
        gcid: i64,
        method_name: &str,
        param_idx: i64,
        param_id: i64,
    ) {
        let ptr = ilang_runtime::leak_cstring(method_name.to_string());
        ilang_runtime::__register_type_method_param(
            gcid, ptr, param_idx, param_id,
        );
    }
    fn type_field_type(
        &mut self,
        _class_idx: usize,
        _field_idx: usize,
        gcid: i64,
        field_name: &str,
        fty_id: i64,
    ) {
        let ptr = ilang_runtime::leak_cstring(field_name.to_string());
        ilang_runtime::__register_type_field_type(gcid, ptr, fty_id);
    }
    fn type_declared_field_count(&mut self, gcid: i64, count: i64) {
        ilang_runtime::__register_type_declared_field_count(gcid, count);
    }
    fn type_arg(&mut self, gcid: i64, idx: i64, arg_id: i64) {
        ilang_runtime::__register_type_arg(gcid, idx, arg_id);
    }
}

// --------------------------------------------------------------------
// Class layout (print info + class size + heap-field cascade)
// --------------------------------------------------------------------

/// Backend-specific receiver for the class-layout registrations:
/// `$class.registerPrintName` / `$class.registerPrintField` /
/// `$class.registerStructPrintField` / `$class.registerSize` /
/// `$class.registerObjectField`. `class_idx` and `field_idx` index
/// into `prog.classes[class_idx].fields[field_idx]` so AOT sinks
/// can reuse the pre-allocated DataIds for the name bodies.
pub(crate) trait ClassLayoutSink {
    fn class_print_name(&mut self, class_idx: usize, gcid: i64, name: &str);
    fn class_print_field(
        &mut self,
        class_idx: usize,
        field_idx: usize,
        gcid: i64,
        idx: i64,
        name: &str,
        pk: i64,
    );
    fn struct_print_field(
        &mut self,
        class_idx: usize,
        field_idx: usize,
        gcid: i64,
        idx: i64,
        name: &str,
        pk: i64,
        offset: i64,
        nested_cid: i64,
    );
    /// Total heap allocation size in bytes. Not emitted for CRepr /
    /// packed / union classes (their lifetime goes through direct
    /// `__mir_free(ptr, c_size)` instead of the runtime drop path).
    fn class_size(&mut self, gcid: i64, size: i64);
    fn object_field(&mut self, gcid: i64, off: i64, tag: i64);
}

/// Walk every class and emit the layout registrations the runtime
/// needs at startup. `print_kind_id_for_ty(ty)` is folded through
/// the caller because the helpers live in each backend (JIT keeps
/// `print_kind_id` in `compile`, AOT keeps `print_kind_id_for_ty`
/// in `aot::helpers`) — the rules differ slightly for enum-bool
/// reprs, so wiring two functions instead of unifying them keeps
/// each backend honest.
pub(crate) fn emit_class_layout_registrations<S, PK, KT>(
    prog: &Program,
    class_global: &[u32],
    print_kind: PK,
    field_kind_tag: KT,
    sink: &mut S,
) where
    S: ClassLayoutSink,
    PK: Fn(&MirTy) -> i64,
    KT: Fn(&MirTy) -> i64,
{
    for (class_idx, class) in prog.classes.iter().enumerate() {
        let gcid = class_global[class.id.0 as usize] as i64;
        sink.class_print_name(class_idx, gcid, class.name.as_str());
        let is_struct = matches!(
            class.repr,
            ilang_mir::ClassRepr::CRepr
                | ilang_mir::ClassRepr::CPacked
                | ilang_mir::ClassRepr::CUnion
        );
        for (field_idx, f) in class.fields.iter().enumerate() {
            let pk = print_kind(&f.ty);
            sink.class_print_field(
                class_idx,
                field_idx,
                gcid,
                field_idx as i64,
                f.name.as_str(),
                pk,
            );
            if is_struct && f.bit_field.is_none() {
                let off = class
                    .c_field_offsets
                    .get(field_idx)
                    .copied()
                    .unwrap_or(0);
                let nested_cid: i64 = if let MirTy::Object(nc) = &f.ty {
                    let nested = &prog.classes[nc.0 as usize];
                    if matches!(
                        nested.repr,
                        ilang_mir::ClassRepr::CRepr
                            | ilang_mir::ClassRepr::CPacked
                            | ilang_mir::ClassRepr::CUnion
                    ) {
                        class_global[nc.0 as usize] as i64
                    } else {
                        0
                    }
                } else {
                    0
                };
                sink.struct_print_field(
                    class_idx,
                    field_idx,
                    gcid,
                    field_idx as i64,
                    f.name.as_str(),
                    pk,
                    off,
                    nested_cid,
                );
            }
        }
        // Heap-typed field cascade table — one row per field whose
        // type carries a non-`KIND_NONE` cascade tag.
        for (i, f) in class.fields.iter().enumerate() {
            let tag = field_kind_tag(&f.ty);
            if tag == 0 {
                continue;
            }
            let off = 16 + (i as i64) * 8;
            sink.object_field(gcid, off, tag);
        }
        // class_size — skipped for CRepr / packed / union (their
        // lifetime is tracked via direct `__mir_free(ptr, c_size)`
        // emits at codegen time, not the runtime drop path).
        let skip_free = matches!(
            class.repr,
            ilang_mir::ClassRepr::CRepr
                | ilang_mir::ClassRepr::CPacked
                | ilang_mir::ClassRepr::CUnion
        );
        if !skip_free {
            let size = 16 + (class.fields.len() as i64) * 8;
            sink.class_size(gcid, size);
        }
    }
}

/// JIT-side sink for `ClassLayoutSink`. Each event dispatches to the
/// matching `ilang_runtime::__register_class_*` extern; names reach
/// the runtime through `leak_cstring`'s persistent buffer.
#[allow(non_camel_case_types)]
pub(crate) struct ClassLayoutSink_JIT;

impl ClassLayoutSink for ClassLayoutSink_JIT {
    fn class_print_name(
        &mut self,
        _class_idx: usize,
        gcid: i64,
        name: &str,
    ) {
        let ptr = ilang_runtime::leak_cstring(name.to_string());
        ilang_runtime::__register_class_print_name(gcid, ptr);
    }
    fn class_print_field(
        &mut self,
        _class_idx: usize,
        _field_idx: usize,
        gcid: i64,
        idx: i64,
        name: &str,
        pk: i64,
    ) {
        let ptr = ilang_runtime::leak_cstring(name.to_string());
        ilang_runtime::__register_class_print_field(gcid, idx, ptr, pk);
    }
    fn struct_print_field(
        &mut self,
        _class_idx: usize,
        _field_idx: usize,
        gcid: i64,
        idx: i64,
        name: &str,
        pk: i64,
        offset: i64,
        nested_cid: i64,
    ) {
        let ptr = ilang_runtime::leak_cstring(name.to_string());
        ilang_runtime::__register_struct_print_field(
            gcid, idx, ptr, pk, offset, nested_cid,
        );
    }
    fn class_size(&mut self, gcid: i64, size: i64) {
        ilang_runtime::__register_class_size(gcid, size);
    }
    fn object_field(&mut self, gcid: i64, off: i64, tag: i64) {
        ilang_runtime::__register_object_field(gcid, off, tag);
    }
}

// --------------------------------------------------------------------
// Enum print / payload registrations
// --------------------------------------------------------------------

/// Backend-specific receiver for the enum-side registrations:
/// `$enum.registerPrintName` / `…PrintVariantName` /
/// `…PrintVariantPayloadPk` / `$enum.registerPayloadKind` /
/// `$enum.registerDiscStr`, plus the one-shot
/// `$type.registerTypeKindEnumId` for the built-in TypeKind.
///
/// `enum_idx` / `variant_idx` index into
/// `prog.enums[enum_idx].variants[variant_idx]` so AOT sinks can
/// reuse the pre-allocated DataIds for the name / disc-str bodies.
pub(crate) trait EnumRegistrationSink {
    fn enum_print_name(&mut self, enum_idx: usize, gid: i64, name: &str);
    fn enum_print_variant_name(
        &mut self,
        enum_idx: usize,
        variant_idx: usize,
        gid: i64,
        disc: i64,
        name: &str,
    );
    fn enum_print_variant_payload_pk(
        &mut self,
        gid: i64,
        disc: i64,
        slot: i64,
        pk: i64,
    );
    fn enum_payload_kind(
        &mut self,
        gid: i64,
        disc: i64,
        slot: i64,
        cascade_tag: i64,
    );
    fn enum_disc_str(
        &mut self,
        enum_idx: usize,
        variant_idx: usize,
        gid: i64,
        disc: i64,
        disc_str: &str,
    );
    /// One-shot: the built-in `TypeKind` enum's global id, used by
    /// `$type.kind` to box discriminants through `__enum_unit_get`.
    fn typekind_enum_id(&mut self, gid: i64);
}

/// Walk every enum and emit the print / payload registrations.
/// `payload_kind` resolves a payload MirTy to its release-cascade
/// tag (`KIND_*`); `print_kind` produces the print PK_* for a
/// payload type. Both helpers stay in the calling backend so the
/// driver doesn't reach across crate boundaries for them.
pub(crate) fn emit_enum_registrations<S, PK, KT>(
    prog: &Program,
    enum_global: &[u32],
    print_kind: PK,
    payload_kind: KT,
    sink: &mut S,
) where
    S: EnumRegistrationSink,
    PK: Fn(&MirTy) -> i64,
    KT: Fn(&MirTy) -> i64,
{
    for (enum_idx, e) in prog.enums.iter().enumerate() {
        let gid = enum_global[e.id.0 as usize] as i64;
        sink.enum_print_name(enum_idx, gid, e.name.as_str());
        if e.name.as_str() == "TypeKind" {
            sink.typekind_enum_id(gid);
        }
        let is_str_repr = matches!(e.repr, MirTy::Str);
        for (variant_idx, v) in e.variants.iter().enumerate() {
            let payload_tys: Vec<MirTy> = match &v.payload {
                ilang_mir::VariantPayload::Unit => Vec::new(),
                ilang_mir::VariantPayload::Tuple(tys) => tys.to_vec(),
                ilang_mir::VariantPayload::Struct(fs) => {
                    fs.iter().map(|(_, t)| t.clone()).collect()
                }
            };
            sink.enum_print_variant_name(
                enum_idx,
                variant_idx,
                gid,
                v.discriminant,
                v.name.as_str(),
            );
            for (i, ty) in payload_tys.iter().enumerate() {
                let cascade = payload_kind(ty);
                if cascade != 0 {
                    sink.enum_payload_kind(
                        gid,
                        v.discriminant,
                        i as i64,
                        cascade,
                    );
                }
                let pk = print_kind(ty);
                sink.enum_print_variant_payload_pk(
                    gid,
                    v.discriminant,
                    i as i64,
                    pk,
                );
            }
            if is_str_repr {
                if let Some(s) = v.discriminant_str.as_ref() {
                    sink.enum_disc_str(
                        enum_idx,
                        variant_idx,
                        gid,
                        v.discriminant,
                        s.as_str(),
                    );
                }
            }
        }
    }
}

/// JIT-side sink for `EnumRegistrationSink`.
#[allow(non_camel_case_types)]
pub(crate) struct EnumRegistrationSink_JIT;

impl EnumRegistrationSink for EnumRegistrationSink_JIT {
    fn enum_print_name(
        &mut self,
        _enum_idx: usize,
        gid: i64,
        name: &str,
    ) {
        let ptr = ilang_runtime::leak_cstring(name.to_string());
        ilang_runtime::__register_enum_print_name(gid, ptr);
    }
    fn enum_print_variant_name(
        &mut self,
        _enum_idx: usize,
        _variant_idx: usize,
        gid: i64,
        disc: i64,
        name: &str,
    ) {
        let ptr = ilang_runtime::leak_cstring(name.to_string());
        ilang_runtime::__register_enum_print_variant_name(gid, disc, ptr);
    }
    fn enum_print_variant_payload_pk(
        &mut self,
        gid: i64,
        disc: i64,
        slot: i64,
        pk: i64,
    ) {
        ilang_runtime::__register_enum_print_variant_payload_pk(
            gid, disc, slot, pk,
        );
    }
    fn enum_payload_kind(
        &mut self,
        gid: i64,
        disc: i64,
        slot: i64,
        cascade_tag: i64,
    ) {
        ilang_runtime::__register_enum_payload_kind(
            gid, disc, slot, cascade_tag,
        );
    }
    fn enum_disc_str(
        &mut self,
        _enum_idx: usize,
        _variant_idx: usize,
        gid: i64,
        disc: i64,
        disc_str: &str,
    ) {
        let ptr = ilang_runtime::leak_cstring(disc_str.to_string());
        ilang_runtime::__register_enum_disc_str(gid, disc, ptr);
    }
    fn typekind_enum_id(&mut self, gid: i64) {
        ilang_runtime::__register_typekind_enum_id(gid);
    }
}
