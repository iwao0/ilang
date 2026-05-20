// Common Win32 / COM null-pointer constant.
//
// Most Windows API entry points accept a `NULL` HANDLE / lpParam /
// hWnd / hInstance / hIcon / hCursor / hMenu / lpMenuName etc.
// where the meaning is "no value provided". Spelling it `0 as *void`
// at every call site is noisy; this binding exposes a typed `NULL`
// that flows into any `*T` slot through the void-pointer coercion
// — no explicit `as *HWND` / `as *HMENU` at the call site.
//
// Declared inside `@extern(C) { ... }` because `*void` is a C-only
// type that's only nameable in FFI scope. Use sites in user code
// follow the same rule (they're inside `@extern(C)` already to
// call Win32 functions), so the substitution `0 as *void` lands
// on a legal cast.

@extern(C) {
    /// Win32 / COM null pointer. Pass directly to any `*T`
    /// parameter (HWND / HMENU / lpParam / ...) — the `*void`
    /// → `*T` coercion at the FFI boundary handles the retag.
    pub const NULL: *void = 0 as *void
}
