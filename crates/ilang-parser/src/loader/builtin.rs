//! Built-in module registry — `use math` / `use std.events` etc.
//! resolve through the embedded sources here before consulting the
//! filesystem. Also hosts the small allow-lists of FFI helper names
//! and always-bare type names that the loader's prefix pass must
//! never qualify with a module name.

use std::path::{Path, PathBuf};

/// Modules whose source is shipped inside the compiler. `use math`
/// resolves here before consulting the filesystem.
pub fn builtin_module_source(name: &str) -> Option<&'static str> {
    match name {
        "math" => Some(include_str!("../../../../libs/std/math.il")),
        "test" => Some(include_str!("../../../../libs/std/test.il")),
        "os" => Some(include_str!("../../../../libs/std/os.il")),
        "events" => Some(include_str!("../../../../libs/std/events.il")),
        "fs" => Some(include_str!("../../../../libs/std/fs.il")),
        "path" => Some(include_str!("../../../../libs/std/path.il")),
        "regex" => Some(include_str!("../../../../libs/std/regex.il")),
        "time" => Some(include_str!("../../../../libs/std/time.il")),
        "ffi" => Some(include_str!("../../../../libs/std/ffi.il")),
        _ => None,
    }
}

/// A path-shaped key for built-in modules so the rest of the loader
/// can treat them uniformly with on-disk files.
pub(super) fn builtin_path(name: &str) -> PathBuf {
    PathBuf::from(format!("<builtin>/{name}.il"))
}

/// Real on-disk source-tree path for a built-in module, baked in at
/// compile time via `CARGO_MANIFEST_DIR`. Returned only when the
/// file exists at the recorded location — released binaries shipped
/// without the source tree fall back to `None` and the caller keeps
/// the synthetic `<builtin>/M.il` key. F12 / hover-to-definition use
/// this so cursoring on a `use events` / `Signal` jumps into
/// `libs/std/events.il` instead of failing to open a fake path.
pub fn builtin_module_path(name: &str) -> Option<PathBuf> {
    if builtin_module_source(name).is_none() {
        return None;
    }
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.pop(); // repo root
    p.push("libs");
    p.push("std");
    p.push(format!("{name}.il"));
    if p.exists() { Some(p) } else { None }
}

pub(super) fn is_builtin_path(p: &Path) -> Option<&str> {
    let s = p.to_str()?;
    s.strip_prefix("<builtin>/")
        .and_then(|rest| rest.strip_suffix(".il"))
}

/// Names that should never get module-prefixed at Call sites — the
/// FFI marshalling helpers shipped by the type checker (mirrors the
/// `FFI_HELPERS` list in `ilang-types`).
pub(super) fn is_builtin_callee(name: &str) -> bool {
    matches!(
        name,
        "stringFromCstr"
            | "cstrFromString"
            | "freeCstr"
            | "bytesFromBuffer"
            | "readI8"
            | "readI16"
            | "readI32"
            | "readI64"
            | "readU8"
            | "readU16"
            | "readU32"
            | "readU64"
            | "readF32"
            | "readF64"
            | "writeI8"
            | "writeI16"
            | "writeI32"
            | "writeI64"
            | "writeU8"
            | "writeU16"
            | "writeU32"
            | "writeU64"
            | "writeF32"
            | "writeF64"
            | "cstrArrayToStrings"
            | "errnoCheck"
            | "errnoCheckI64"
    )
}

/// Built-in classes / enums that should never get prefixed even
/// when referenced inside a module body.
pub(super) fn is_builtin_type(name: &str) -> bool {
    matches!(name, "Console" | "Map" | "Promise" | "Result" | "ObjCBlock")
}
