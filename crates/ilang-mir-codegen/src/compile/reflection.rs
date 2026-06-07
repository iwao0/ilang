//! Driver for the per-class reflection-metadata registrations that
//! back `typeof(x).<member>`. JIT setup and the AOT init-body
//! emitter previously each contained a hand-copied class loop that
//! walked the same data and called the same eight `$type.register*`
//! entry points; this module captures the walk as a single function
//! that yields the registration events to a backend-specific sink.
//!
//! Print info (`$class.registerPrintName` / `$class.registerPrintField`
//! / `$class.registerStructPrintField`), vtable / drop / class-size /
//! object-field / closure / enum tables intentionally stay outside
//! this module — those weren't part of the reflection refactor and
//! still live in each backend.

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
