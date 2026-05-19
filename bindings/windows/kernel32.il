// kernel32.dll — the bits the minimal-window demo needs.
//
// `GetModuleHandleA(NULL)` returns the HINSTANCE of the current
// EXE; that handle is passed to `CreateWindowExA` /
// `RegisterClassExA`. `GetLastError` is here so failure sites can
// surface a numeric Win32 error code for debugging.

@extern(C, "kernel32") {
    /// Generic kernel-object handle (`void *` in the SDK).
    /// Used as the base type for HANDLEs returned by CreateFile,
    /// CreateThread, etc.
    pub struct HANDLE {}

    /// Module / executable image handle (`HMODULE == HINSTANCE` in
    /// the SDK; kept separate here because it lives in a different
    /// DLL binding file). Cast to `*HINSTANCE` when passing to
    /// user32 functions such as `RegisterClassExA`.
    pub struct HMODULE {}

    /// `HMODULE GetModuleHandleA(LPCSTR lpModuleName)`.
    /// Pass `0 as *const char` (NULL) to get the EXE's own
    /// HINSTANCE; cast the result with `as *HINSTANCE` before
    /// passing to `WNDCLASSEXA.hInstance` / `CreateWindowExA`.
    @lib pub fn GetModuleHandleA(lpModuleName: *const char): *HMODULE

    /// `DWORD GetLastError(void)` — last thread-local Win32 error.
    @lib pub fn GetLastError(): u32
}
