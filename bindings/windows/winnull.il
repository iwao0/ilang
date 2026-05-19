// Common Win32 / COM null-pointer constant.
//
// Most Windows API entry points accept a `NULL` HANDLE / lpParam /
// hWnd / hInstance / hIcon / hCursor / hMenu / lpMenuName etc.
// where the meaning is "no value provided". Spelling it `0 as *void`
// at every call site is noisy; this binding exposes a typed `NULL`
// that the user can re-cast (`NULL as *HWND`) for slot-specific
// pointer types — saving the repeated `0 as ...` boilerplate.
//
// Declared inside `@extern(C) { ... }` because `*void` is a C-only
// type that's only nameable in FFI scope. Use sites in user code
// follow the same rule (they're inside `@extern(C)` already to
// call Win32 functions), so the substitution `0 as *void` lands
// on a legal cast.

@extern(C) {
    /// Win32 / COM null pointer. Cast to the specific handle type
    /// at the call site (`NULL as *HWND`, `NULL as *HMENU`, etc.).
    pub const NULL: *void = 0 as *void
}
