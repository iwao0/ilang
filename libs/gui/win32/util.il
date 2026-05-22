/// Win32-side gui_impl helpers shared by every widget module:
///
/// - `toWideUtf8` — UTF-8 string → fresh `HeapAlloc`'d NUL-
///   terminated UTF-16 buffer suitable for `CreateWindowExW` /
///   `SetWindowTextW`. Caller `HeapFree`s after the Win32 call
///   returns (or leaks intentionally for class-name / theme-name
///   buffers whose lifetime has to outlast a single call).
///
/// - `enableComctl32V6` — install a side-by-side activation
///   context for comctl32 6.0 (themed controls) and register the
///   v6 control classes. Idempotent; the first call wins.
///
/// - `EXPLORER_THEME` — cached wide "Explorer" string used by
///   the per-widget modules' `SetWindowTheme` calls. Read after
///   `enableComctl32V6` has populated it (slot is `0` before
///   then, which the call sites skip).

use windows {
    HINSTANCE, ACTCTXW,
    CreateActCtxW, ActivateActCtx, GetTempPathA,
    CreateFileA, WriteFile, CloseHandle, HANDLE,
    FileAccess, FileCreation, FileAttribute,
    HeapAlloc, HeapFree, GetProcessHeap,
    MultiByteToWideChar,
    SetWindowTheme,
    INITCOMMONCONTROLSEX, InitCommonControlsEx, Icc,
}

// True once `enableComctl32V6` has run; subsequent calls bail
// out so the activation context stays installed at most once
// per process.
let SXS_INITIALIZED: bool[] = [false]

// ─── shared no-op callbacks ──────────────────────────────────
//
// Per-widget files use these as the initial value for a slot's
// callback so `tryXxx` dispatchers never see a null fn pointer
// before the user wires `setOnXxx`. Centralised here because the
// umbrella `pub use *` re-export prefixes private helpers under
// `gui_impl.*` and duplicate per-widget definitions would
// collide on that namespace.

pub fn noOpClick(x: f64, y: f64, b: i32) {
    let _ = x
    let _ = y
    let _ = b
}
pub fn noOpBool(b: bool) { let _ = b }
pub fn noOpText(s: string) { let _ = s }
pub fn noOpF64(v: f64) { let _ = v }
pub fn noOpI32(i: i32) { let _ = i }

/// Wide "Explorer" string (UTF-16 NUL-terminated), set by
/// `enableComctl32V6`. Widget modules feed this into
/// `SetWindowTheme` so buttons / edits pick up the modern
/// theme. The slot is `0` until v6 has been activated; widget
/// modules guard with `if EXPLORER_THEME[0] != 0`.
///
/// Stored as `i64` (raw pointer) because ilang top-level let
/// arrays can't hold `*u16` outside `@extern(C)` blocks.
pub let EXPLORER_THEME: i64[] = [0 as i64]

@extern(C) {

/// UTF-8 `string` → fresh NUL-terminated UTF-16 buffer. Cast
/// the returned `i64` back with `as *u16` at the call site.
/// Caller owns the buffer; pass to a Win32 entry point and
/// `HeapFree` afterwards.
pub fn toWideUtf8(s: string): i64 {
    let cstr = cstrFromString(s)
    let cap = (s.length as i32 + 1) * 2
    let buf = HeapAlloc(
        GetProcessHeap(),
        8 as u32,
        (cap as u64) * 2 as u64
    )
    let _ = MultiByteToWideChar(
        65001 as u32,           // CP_UTF8
        0 as u32,
        cstr,
        -1 as i32,              // NUL-terminated input
        buf as *u16,
        cap
    )
    buf as i64
}

/// Install a side-by-side activation context that pulls in
/// `Microsoft.Windows.Common-Controls` v6 — the "modern" themed
/// button / edit / listview / tab / progress classes. Without
/// this (or an embedded .manifest resource on the EXE), every
/// CreateWindow hands back the legacy Win95-style controls.
///
/// Writes the manifest to `%TEMP%\ilang_gui_v6.manifest` on the
/// first call, then calls `CreateActCtxW + ActivateActCtx`. The
/// context stays activated for the lifetime of the process (the
/// cookie is leaked on purpose).
pub fn enableComctl32V6() {
    if SXS_INITIALIZED[0] { return }
    SXS_INITIALIZED[0] = true

    let heap = GetProcessHeap()

    // Compose the manifest path under %TEMP%.
    let tempBuf = HeapAlloc(heap, 8 as u32, 1024 as u64)
    let _ = GetTempPathA(1024 as u32, tempBuf as *char)
    let tempStr = stringFromCstr(tempBuf as *const char)
    let manifestPath = tempStr + "ilang_gui_v6.manifest"
    let pathC = cstrFromString(manifestPath)

    // Manifest body — same content as a themed app's embedded
    // .manifest, naming the v6.0.0.0 common-controls assembly.
    let manifest = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<assembly xmlns=\"urn:schemas-microsoft-com:asm.v1\" manifestVersion=\"1.0\"><dependency><dependentAssembly><assemblyIdentity type=\"win32\" name=\"Microsoft.Windows.Common-Controls\" version=\"6.0.0.0\" processorArchitecture=\"*\" publicKeyToken=\"6595b64144ccf1df\" language=\"*\"/></dependentAssembly></dependency></assembly>"
    let manifestC = cstrFromString(manifest)
    let manifestLen = manifest.length as u32

    let hFile = CreateFileA(
        pathC,
        FileAccess.write as u32,
        0 as u32,
        0 as *void,
        FileCreation.createAlways as u32,
        FileAttribute.normal as u32,
        0 as HANDLE
    )
    if (hFile as i64) == -1 as i64 {
        HeapFree(heap, 0 as u32, tempBuf)
        return
    }
    let written: u32 = 0
    let _ = WriteFile(
        hFile,
        manifestC as *const void,
        manifestLen,
        &written,
        0 as *void
    )
    CloseHandle(hFile)

    // ANSI path → wide for CreateActCtxW.
    let wideBuf = HeapAlloc(heap, 8 as u32, 2048 as u64) as *u16
    let _ = MultiByteToWideChar(
        0 as u32,                   // CP_ACP
        0 as u32,
        pathC,
        -1 as i32,                  // NUL-terminated
        wideBuf,
        1024 as i32
    )

    // CreateActCtxW + ActivateActCtx; cookie leaks on purpose so
    // v6 stays active for every later window / button.
    let actCtx = ACTCTXW {
        cbSize:                 56 as u32,
        dwFlags:                0 as u32,
        lpSource:               wideBuf as *const u16,
        wProcessorArchitecture: 0 as u16,
        wLangId:                0 as u16,
        _pad:                   0 as u32,
        lpAssemblyDirectory:    0 as *const u16,
        lpResourceName:         0 as *const u16,
        lpApplicationName:      0 as *const u16,
        hModule:                0 as i64,
    }
    let h = CreateActCtxW(&actCtx)
    if h != -1 as i64 {
        let cookie: u64 = 0
        let _ = ActivateActCtx(h, &cookie)
    }
    HeapFree(heap, 0 as u32, tempBuf)
    // wideBuf intentionally not freed — the activation context
    // keeps a pointer to it.

    // Register v6 common control classes.
    let icc = INITCOMMONCONTROLSEX {
        dwSize: 8 as u32,
        dwICC:  Icc.standardClasses as u32
              | Icc.barClasses as u32      // trackbar / toolbar
              | Icc.progressClass as u32,  // progress bar
    }
    let _ = InitCommonControlsEx(&icc)

    // Cache a wide "Explorer" string for SetWindowTheme.
    let themeBuf = HeapAlloc(heap, 8 as u32, 32 as u64) as *u16
    let themeName = cstrFromString("Explorer")
    let _ = MultiByteToWideChar(
        0 as u32, 0 as u32, themeName, -1 as i32,
        themeBuf, 16 as i32
    )
    EXPLORER_THEME[0] = themeBuf as i64
}

/// Apply the Explorer theme to `hwnd` so buttons / edit / list
/// controls pick up the modern rounded / hover-shaded look. No-op
/// when the v6 activation context isn't live (i.e. the EXE's
/// manifest already pinned an older theme, or the host lacks v6).
pub fn applyExplorerTheme(hwnd: i64) {
    if EXPLORER_THEME[0] != 0 as i64 {
        let _ = SetWindowTheme(
            hwnd,
            EXPLORER_THEME[0] as *const u16,
            0 as *const u16
        )
    }
}

} // @extern(C)
