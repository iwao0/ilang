// kernel32.dll — the bits the minimal-window demo needs.
//
// `GetModuleHandleA(NULL)` returns the HINSTANCE of the current
// EXE; that handle is passed to `CreateWindowExA` /
// `RegisterClassExA`. `GetLastError` is here so failure sites can
// surface a numeric Win32 error code for debugging.

@extern(C, "kernel32") {
    /// `HMODULE GetModuleHandleA(LPCSTR lpModuleName)`.
    /// Pass `0 as *const char` (NULL) to get the EXE's own
    /// HINSTANCE, suitable for `WNDCLASSEXA.hInstance`.
    @lib pub fn GetModuleHandleA(lpModuleName: *const char): *void

    /// `DWORD GetLastError(void)` — last thread-local Win32 error.
    @lib pub fn GetLastError(): u32
}
