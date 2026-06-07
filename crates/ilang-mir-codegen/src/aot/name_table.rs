//! Pre-allocated `DataId` table for every string body the AOT init
//! body hands to the runtime registration calls. The names are
//! laid out as the existing `[i64 cap | i64 rc | i64 len | bytes |
//! \0]` runtime string format (via `declare_ilang_string_data`);
//! callers feed each DataId through `ilang_string_body` to get the
//! body pointer the runtime expects.
//!
//! `emit_aot_init` used to grow eight parallel `Vec` tables
//! inline; the sinks then each carried two or three `&[Vec<DataId>]`
//! field references just to look them up. Folding everything into
//! one struct lets every sink hold a single `&NameTable` borrow.

use cranelift_module::DataId;
use cranelift_object::ObjectModule;

use ilang_mir::{MirTy, Program};

use super::declare_ilang_string_data;
use super::AotError;

/// All `DataId`s the AOT init body needs to lookup at registration
/// time. Indexed in `prog.classes` / `prog.enums` / `prog.functions`
/// order so sinks index with the same `_idx` arguments the shared
/// `compile::registration` driver yields.
pub(super) struct NameTable {
    /// Class names — `[class_idx] -> DataId`.
    class: Vec<DataId>,
    /// Field names — `[class_idx][field_idx] -> DataId`.
    class_field: Vec<Vec<DataId>>,
    /// Method names — `[class_idx][method_idx] -> DataId`.
    class_method: Vec<Vec<DataId>>,
    /// Enum names — `[enum_idx] -> DataId`.
    enum_: Vec<DataId>,
    /// Variant names — `[enum_idx][variant_idx] -> DataId`.
    enum_variant: Vec<Vec<DataId>>,
    /// `: string`-repr enum's variant disc strings —
    /// `[enum_idx][variant_idx] -> Option<DataId>`. `None` for
    /// non-string-repr enums and unit variants whose discriminator
    /// stayed as the natural integer.
    enum_variant_disc_str: Vec<Vec<Option<DataId>>>,
    /// User-facing fn names (un-mangled) for `__print_fn`'s
    /// `<fn NAME>` formatter — `[fn_idx] -> Option<DataId>`. `None`
    /// for extern / `$anon.fn_*` / `$main*` (no name worth showing).
    fn_display: Vec<Option<DataId>>,
    /// `@lib(a, b, ...)` fallback group names —
    /// `[group_idx][member_idx] -> DataId`.
    lib_group: Vec<Vec<DataId>>,
}

impl NameTable {
    /// Walk `prog` once and declare every name's data symbol.
    pub(super) fn pre_allocate(
        module: &mut ObjectModule,
        prog: &Program,
    ) -> Result<Self, AotError> {
        let mut class = Vec::with_capacity(prog.classes.len());
        let mut class_field = Vec::with_capacity(prog.classes.len());
        let mut class_method = Vec::with_capacity(prog.classes.len());
        for c in &prog.classes {
            class.push(declare_ilang_string_data(module, c.name.as_str())?);
            let mut fs = Vec::with_capacity(c.fields.len());
            for f in &c.fields {
                fs.push(declare_ilang_string_data(module, f.name.as_str())?);
            }
            class_field.push(fs);
            let mut ms = Vec::with_capacity(c.methods.len());
            for m in &c.methods {
                ms.push(declare_ilang_string_data(module, m.name.as_str())?);
            }
            class_method.push(ms);
        }

        let mut fn_display = Vec::with_capacity(prog.functions.len());
        for func in prog.functions.iter() {
            if matches!(func.kind, ilang_mir::FunctionKind::Extern { .. }) {
                fn_display.push(None);
                continue;
            }
            let name = func.name.as_str();
            if name.starts_with("$anon.fn_") || name.starts_with("$main") {
                fn_display.push(None);
                continue;
            }
            let plain = name.split("__").next().unwrap_or(name);
            fn_display.push(Some(declare_ilang_string_data(module, plain)?));
        }

        let mut enum_ = Vec::with_capacity(prog.enums.len());
        let mut enum_variant = Vec::with_capacity(prog.enums.len());
        let mut enum_variant_disc_str = Vec::with_capacity(prog.enums.len());
        for e in &prog.enums {
            enum_.push(declare_ilang_string_data(module, e.name.as_str())?);
            let is_str_repr = matches!(e.repr, MirTy::Str);
            let mut variants = Vec::with_capacity(e.variants.len());
            let mut disc_strs: Vec<Option<DataId>> =
                Vec::with_capacity(e.variants.len());
            for v in &e.variants {
                variants.push(declare_ilang_string_data(
                    module,
                    v.name.as_str(),
                )?);
                if is_str_repr {
                    if let Some(s) = v.discriminant_str.as_ref() {
                        disc_strs.push(Some(declare_ilang_string_data(
                            module, s,
                        )?));
                    } else {
                        disc_strs.push(None);
                    }
                } else {
                    disc_strs.push(None);
                }
            }
            enum_variant.push(variants);
            enum_variant_disc_str.push(disc_strs);
        }

        let mut lib_group: Vec<Vec<DataId>> = Vec::new();
        for f in prog.functions.iter() {
            if matches!(f.kind, ilang_mir::FunctionKind::Extern { .. })
                && f.libs.len() > 1
            {
                let mut members = Vec::with_capacity(f.libs.len());
                for sym in f.libs.iter() {
                    members.push(declare_ilang_string_data(
                        module,
                        sym.as_str(),
                    )?);
                }
                lib_group.push(members);
            }
        }

        Ok(NameTable {
            class,
            class_field,
            class_method,
            enum_,
            enum_variant,
            enum_variant_disc_str,
            fn_display,
            lib_group,
        })
    }

    pub(super) fn class(&self, class_idx: usize) -> DataId {
        self.class[class_idx]
    }
    pub(super) fn class_field(
        &self,
        class_idx: usize,
        field_idx: usize,
    ) -> DataId {
        self.class_field[class_idx][field_idx]
    }
    pub(super) fn class_method(
        &self,
        class_idx: usize,
        method_idx: usize,
    ) -> DataId {
        self.class_method[class_idx][method_idx]
    }
    pub(super) fn enum_(&self, enum_idx: usize) -> DataId {
        self.enum_[enum_idx]
    }
    pub(super) fn enum_variant(
        &self,
        enum_idx: usize,
        variant_idx: usize,
    ) -> DataId {
        self.enum_variant[enum_idx][variant_idx]
    }
    pub(super) fn enum_variant_disc_str(
        &self,
        enum_idx: usize,
        variant_idx: usize,
    ) -> Option<DataId> {
        self.enum_variant_disc_str[enum_idx][variant_idx]
    }
    pub(super) fn fn_display(&self, fn_idx: usize) -> Option<DataId> {
        self.fn_display[fn_idx]
    }
    pub(super) fn lib_groups(&self) -> &[Vec<DataId>] {
        &self.lib_group
    }
}
