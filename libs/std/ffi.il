/// Built-in `ffi` module — raw-pointer marshalling helpers used by
/// `@extern(C)` bindings. Every declaration carries `@intrinsic(...)`
/// inside an `@extern(C) { ... }` block: the runtime hosts the
/// implementations (see `crates/ilang-runtime/src/raw_mem.rs`,
/// `strings.rs`, `arrays.rs`, `os.rs`), and the `@extern(C)` wrapper
/// is what lets the signatures mention raw pointers / `char` / `void`
/// without tripping the "no C-only types at module top level" rule.
///
/// Reach for these through `use std.ffi { ... }`; calling them
/// outside an `@extern(C) { ... }` block still works for the helpers
/// whose signatures stay within plain ilang types (`errnoCheck`,
/// `errnoCheckI64`), but the pointer-shaped ones require the
/// surrounding extern-C scope at the call site as well.

@extern(C) {
    /// Wrap an ilang `string` as a NUL-terminated C-string pointer.
    /// ilang strings carry their own NUL terminator inline, so the
    /// returned pointer aliases the existing buffer — pair every
    /// call with `freeCstr` for documentation symmetry, but the
    /// runtime never owns a separate C-side allocation.
    @intrinsic("ffi.cstrFromString")
    pub fn cstrFromString(s: string): *const char

    /// Copy a NUL-terminated C string into a fresh ilang `string`.
    /// The caller may free / overwrite `p` after the call — the
    /// returned value owns its own bytes.
    @intrinsic("ffi.stringFromCstr")
    pub fn stringFromCstr(p: *const char): string

    /// Copy `n` bytes from `p` into a fresh owned ilang `string`
    /// (UTF-8, lossily decoded). Companion to `stringFromCstr` for
    /// `(const char *, size_t)` pairs (e.g. `WGPUStringView` payloads
    /// whose length is a real count, not a `WGPU_STRLEN` sentinel).
    /// `p == 0` or `n <= 0` returns `""`.
    @intrinsic("ffi.stringFromBytes")
    pub fn stringFromBytes(p: *const void, n: i64): string

    /// No-op counterpart of `cstrFromString`. Kept so call sites
    /// read symmetrically; the runtime doesn't actually free
    /// anything (ilang owns the buffer).
    @intrinsic("ffi.freeCstr")
    pub fn freeCstr(p: *const char)

    /// Walk a NUL-terminated `char **` ( `{p, ..., null}` ) and
    /// return each entry as an owned ilang `string` in the
    /// resulting `string[]`.
    @intrinsic("ffi.cstrArrayToStrings")
    pub fn cstrArrayToStrings(p: *const *const char): string[]

    /// Copy `n` bytes from `p` into a fresh `u8[]`. The returned
    /// array owns its bytes; `p` is not retained.
    @intrinsic("ffi.bytesFromBuffer")
    pub fn bytesFromBuffer(p: *const void, n: size_t): u8[]

    /// `arrayFromCArray<T>(p, n)` — copy `n` elements of `T` from
    /// the C-side buffer `p` into a fresh `T[]`. The element size
    /// (stride) is inferred from `T` at the call site; MIR lowering
    /// reads `T` off the pointer arg and synthesises stride / kind
    /// arguments for the underlying `$ffi.arrayFromCArray` runtime call.
    /// `T` must be a numeric primitive / bool — heap element types
    /// (string / class) aren't supported.
    @intrinsic("ffi.arrayFromCArray")
    pub fn arrayFromCArray<T>(p: *const T, n: size_t): T[]

    /// Wrap a syscall return into `Optional<i32>`: `some(rc)` when
    /// `rc >= 0`, `none` otherwise. After `none`, read `os.errno()`
    /// for the failure reason.
    @intrinsic("ffi.errnoCheck")
    pub fn errnoCheck(rc: i32): i32?

    /// Same as `errnoCheck` but for syscalls returning `i64`
    /// (e.g. `read` / `write` byte counts).
    @intrinsic("ffi.errnoCheckI64")
    pub fn errnoCheckI64(rc: i64): i64?

    /// `readT(p, off)` — load a `T`-sized cell from `p + off`. No
    /// alignment / bounds check; the caller is responsible.
    @intrinsic("ffi.readI8")  pub fn readI8(p: *const void, off: i64): i8
    @intrinsic("ffi.readI16") pub fn readI16(p: *const void, off: i64): i16
    @intrinsic("ffi.readI32") pub fn readI32(p: *const void, off: i64): i32
    @intrinsic("ffi.readI64") pub fn readI64(p: *const void, off: i64): i64
    @intrinsic("ffi.readU8")  pub fn readU8(p: *const void, off: i64): u8
    @intrinsic("ffi.readU16") pub fn readU16(p: *const void, off: i64): u16
    @intrinsic("ffi.readU32") pub fn readU32(p: *const void, off: i64): u32
    @intrinsic("ffi.readU64") pub fn readU64(p: *const void, off: i64): u64
    @intrinsic("ffi.readF32") pub fn readF32(p: *const void, off: i64): f32
    @intrinsic("ffi.readF64") pub fn readF64(p: *const void, off: i64): f64

    /// `writeT(p, off, v)` — store a `T`-sized cell at `p + off`.
    /// No alignment / bounds check; the caller is responsible.
    @intrinsic("ffi.writeI8")  pub fn writeI8(p: *void, off: i64, v: i8)
    @intrinsic("ffi.writeI16") pub fn writeI16(p: *void, off: i64, v: i16)
    @intrinsic("ffi.writeI32") pub fn writeI32(p: *void, off: i64, v: i32)
    @intrinsic("ffi.writeI64") pub fn writeI64(p: *void, off: i64, v: i64)
    @intrinsic("ffi.writeU8")  pub fn writeU8(p: *void, off: i64, v: u8)
    @intrinsic("ffi.writeU16") pub fn writeU16(p: *void, off: i64, v: u16)
    @intrinsic("ffi.writeU32") pub fn writeU32(p: *void, off: i64, v: u32)
    @intrinsic("ffi.writeU64") pub fn writeU64(p: *void, off: i64, v: u64)
    @intrinsic("ffi.writeF32") pub fn writeF32(p: *void, off: i64, v: f32)
    @intrinsic("ffi.writeF64") pub fn writeF64(p: *void, off: i64, v: f64)
}
