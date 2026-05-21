/// C standard `<wchar.h>` — wide-character strings.
///
/// `wchar_t` width varies: 16-bit on Windows (UTF-16 code unit),
/// 32-bit on glibc / macOS (UTF-32 code point). The C surface uses
/// `wchar_t` opaquely so a binding has to pick one. We use `u32` —
/// matches POSIX and is wide enough to hold a Windows surrogate
/// pair if you store one as a `wchar_t` value (the actual Win32 API
/// passes `u16` arrays). When you need true Windows-flavoured wide
/// strings, prefer `*u16` and the kernel32 / shlwapi APIs directly.
///
/// Symbols are `@optional`: the ucrt names are all present on
/// Windows but a few of the C99 entries (`wcstold`, `swscanf_s`)
/// vary by version.

@extern(C, "c") {
    // ── Length / compare ────────────────────────────────────────

    @lib @optional pub fn wcslen(s: *const u32): size_t
    @lib @optional pub fn wcscmp(a: *const u32, b: *const u32): i32
    @lib @optional pub fn wcsncmp(a: *const u32, b: *const u32, n: size_t): i32
    @lib @optional pub fn wcscoll(a: *const u32, b: *const u32): i32

    // ── Copy / concat ───────────────────────────────────────────

    @lib @optional pub fn wcscpy(dst: *u32, src: *const u32): *u32
    @lib @optional pub fn wcsncpy(dst: *u32, src: *const u32, n: size_t): *u32
    @lib @optional pub fn wcscat(dst: *u32, src: *const u32): *u32
    @lib @optional pub fn wcsncat(dst: *u32, src: *const u32, n: size_t): *u32

    // ── Search ──────────────────────────────────────────────────

    @lib @optional pub fn wcschr(s: *const u32, c: u32): *const u32
    @lib @optional pub fn wcsrchr(s: *const u32, c: u32): *const u32
    @lib @optional pub fn wcsstr(haystack: *const u32, needle: *const u32): *const u32
    @lib @optional pub fn wcsspn(s: *const u32, accept: *const u32): size_t
    @lib @optional pub fn wcscspn(s: *const u32, reject: *const u32): size_t
    @lib @optional pub fn wcspbrk(s: *const u32, accept: *const u32): *const u32

    // ── Memory ──────────────────────────────────────────────────

    @lib @optional pub fn wmemcpy(dst: *u32, src: *const u32, n: size_t): *u32
    @lib @optional pub fn wmemmove(dst: *u32, src: *const u32, n: size_t): *u32
    @lib @optional pub fn wmemset(dst: *u32, c: u32, n: size_t): *u32
    @lib @optional pub fn wmemcmp(a: *const u32, b: *const u32, n: size_t): i32
    @lib @optional pub fn wmemchr(s: *const u32, c: u32, n: size_t): *const u32

    // ── Convert to number ───────────────────────────────────────

    @lib @optional pub fn wcstol(s: *const u32, endptr: **u32, base: i32): i64
    @lib @optional pub fn wcstoul(s: *const u32, endptr: **u32, base: i32): u64
    @lib @optional pub fn wcstod(s: *const u32, endptr: **u32): f64

    // ── Format ──────────────────────────────────────────────────

    @lib @optional pub fn wprintf(fmt: *const u32, ...): i32
    @lib @optional pub fn swprintf(buf: *u32, n: size_t, fmt: *const u32, ...): i32
}
