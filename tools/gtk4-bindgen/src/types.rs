//! GIR type / declaration model + GIR → ilang type mapping.

#[derive(Debug, Default)]
pub struct Repo {
    pub namespaces: Vec<Namespace>,
}

#[derive(Debug)]
pub struct Namespace {
    pub name: String,
    pub enums: Vec<EnumDef>,
    pub classes: Vec<ClassDef>,
    /// Free functions at namespace level.
    pub funcs: Vec<FunDef>,
    /// Class-like `<record>` definitions — emitted as opaque handles.
    pub records: Vec<RecordDef>,
}

#[derive(Debug)]
pub struct EnumDef {
    pub name: String,
    pub c_type: String,
    pub is_flags: bool,
    pub members: Vec<EnumMember>,
    pub deprecated: bool,
}

#[derive(Debug)]
pub struct EnumMember {
    pub name: String,
    pub value: i64,
}

#[derive(Debug)]
pub struct ClassDef {
    pub name: String,
    pub c_type: String,
    pub constructors: Vec<FunDef>,
    pub methods: Vec<FunDef>,
    pub funcs: Vec<FunDef>,
    pub deprecated: bool,
}

#[derive(Debug)]
pub struct RecordDef {
    pub name: String,
    pub c_type: String,
    pub deprecated: bool,
}

#[derive(Debug)]
pub struct FunDef {
    /// GIR-side function name (used only for diagnostics; the emitter
    /// always goes through `c_identifier`).
    #[allow(dead_code)]
    pub name: String,
    pub c_identifier: String,
    pub params: Vec<Param>,
    /// `None` for void.
    pub return_type: Option<TypeRef>,
    pub deprecated: bool,
    pub variadic: bool,
    pub throws: bool,
    /// `Some(class_type)` when this function takes the class as its
    /// first (instance) parameter — methods. None for constructors
    /// / static fns.
    pub instance: Option<TypeRef>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
    pub direction: ParamDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamDir {
    In,
    Out,
    InOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    /// Recognised primitive (already in ilang form, e.g. "i32").
    Primitive(String),
    /// Reference to a class / record / enum / bitfield in some
    /// namespace. The string is the canonical GIR name with namespace
    /// prefix, e.g. `"Gtk.Widget"` or `"Orientation"` (intra-ns).
    Named(String),
    /// `utf8` or `filename`.
    String,
    /// `gpointer` / `void*`.
    VoidPtr,
    /// Used for skipped / unsupported types so the entire function
    /// can be dropped.
    Unsupported(String),
}

/// Map a GIR `<type name="..."/>` to its ilang form. Returns
/// `Unsupported` for shapes the first-cut emitter doesn't handle.
pub fn map_primitive(gir_name: &str) -> Option<TypeRef> {
    use TypeRef::*;
    Some(match gir_name {
        "gint" | "gint32" | "int" => Primitive("i32".into()),
        "guint" | "guint32" | "unsigned int" => Primitive("u32".into()),
        "gint64" | "int64" => Primitive("i64".into()),
        "guint64" | "uint64" => Primitive("u64".into()),
        "gint8" | "gchar" | "char" => Primitive("i8".into()),
        "guint8" | "guchar" => Primitive("u8".into()),
        "gint16" => Primitive("i16".into()),
        "guint16" => Primitive("u16".into()),
        // `glong` is platform-dependent (32-bit on Windows, 64-bit on
        // 64-bit Unix). The C ABI on the targets we care about
        // (macOS arm64 / x86_64 Linux) makes 64 bits safe.
        "glong" | "gssize" | "time_t" => Primitive("i64".into()),
        "gulong" | "gsize" => Primitive("u64".into()),
        "gdouble" | "double" => Primitive("f64".into()),
        "gfloat" | "float" => Primitive("f32".into()),
        "gboolean" => Primitive("i32".into()),
        "GType" => Primitive("u64".into()),
        "utf8" | "filename" => String,
        "gpointer" | "gconstpointer" => VoidPtr,
        "none" => return None,
        _ => return None,
    })
}

/// Render a `TypeRef` as the ilang surface form that goes into an
/// `@extern(C)` block. Class / record / enum references resolve to
/// their C type name (`GtkWidget`, `GdkRGBA`, …) via the c_type
/// map — that's globally unique even when two namespaces share an
/// unqualified GIR `name`.
pub fn render_type(t: &TypeRef, c_type_map: &std::collections::HashMap<String, String>) -> String {
    match t {
        TypeRef::Primitive(s) => s.clone(),
        TypeRef::String => "*const char".to_string(),
        TypeRef::VoidPtr => "*void".to_string(),
        TypeRef::Named(qual) => {
            c_type_map
                .get(qual)
                .cloned()
                .unwrap_or_else(|| qual.replace('.', "_"))
        }
        TypeRef::Unsupported(_) => "/*unsupported*/".to_string(),
    }
}
