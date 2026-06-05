/// WebView — Microsoft Edge WebView2 backend for the Win32 gui.
///
/// No dependency on the SDK's `WebView2Loader.dll`. We resolve the
/// installed runtime ourselves: read the path from the EdgeUpdate
/// registry entry, `LoadLibraryExW` the runtime's
/// `EmbeddedBrowserWebView.dll`, and `GetProcAddress` the internal
/// entry point `CreateWebViewEnvironmentWithOptionsInternal`.
///
/// All callbacks WebView2 invokes on us — env-completed,
/// controller-completed, and the two NavigationStarting /
/// NavigationCompleted event handlers — are exposed as honest COM
/// objects: a 32-byte vtable (4 fn pointers — QI / AddRef / Release /
/// Invoke) plus a 16-byte instance (vtable pointer + per-instance
/// slot key into our `WEBVIEWS` map). The vtables are HeapAlloc'd
/// once on first use and shared by every webview; the instances are
/// HeapAlloc'd per webview.
///
/// Async lifecycle (single webview):
///   createNativeWebView(handle, x, y, w, h, visible)
///       → mint slot, store in WEBVIEWS keyed by minted handle
///       → ensure loader DLL + entry point
///       → if env is `pending`, queue this slot — env-completed
///         drains the queue
///       → if env is `nil`,    call CreateWebViewEnvironmentWithOptionsInternal
///       → if env is `ready`,  call env.CreateCoreWebView2Controller
///   envCompletedInvoke(_, errorCode, envPtr)
///       → stash env, iterate pending slots, kick CreateCoreWebView2Controller
///   controllerCompletedInvoke(this, errorCode, controllerPtr)
///       → stash controller / webview on the slot
///       → put_Bounds, register navigation handlers, navigate to pendingUrl
///   navStartingInvoke(this, sender, args) → onNavigate(uri)
///   navCompletedInvoke(this, sender, args) → onLoadFinish(uri, isError, msg)

use std.ffi {
    cstrFromString, freeCstr, readU16, readU32, readI64, writeI64, writeU32,
}
use windef { RECT }
use windows {
    HWND, HMODULE, HANDLE,
    GetProcessHeap, HeapAlloc, HeapFree,
    LoadLibraryExW, GetProcAddress, FreeLibrary, GetLastError,
    LoadLibraryFlags,
    GetTempPathW, RegValueType,
    GetWindowLongPtrW, SetWindowLongPtrW, WindowLongIndex,
    SetWindowPos, SetWindowPosFlags,
}
use ole32 { CoTaskMemFree, CoInitializeEx, CoInit, IUnknown }
use webview2 {
    ICoreWebView2Environment, ICoreWebView2Controller, ICoreWebView2,
    ICoreWebView2NavigationStartingEventArgs,
    ICoreWebView2NavigationCompletedEventArgs,
}
use core { windowHwnd }

// ─── Registry: tiny re-binding using i64 HKEY ─────────────────
//
// The `windows.HKEY` `@handle` struct does not round-trip the
// predefined HKEY_* sign-extended literal through ilang's
// PtrIntCast cleanly — RegOpenKeyExW comes back with
// ERROR_INVALID_PARAMETER. Re-bind here with plain i64. We do
// not import the conflicting names from `windows` above.
@extern(C, "advapi32") {
    @lib pub fn RegOpenKeyExW(
        hKey:       i64,
        lpSubKey:   *const u16,
        ulOptions:  u32,
        samDesired: u32,
        phkResult:  *i64
    ): i32

    @lib pub fn RegQueryValueExW(
        hKey:        i64,
        lpValueName: *const u16,
        lpReserved:  *u32,
        lpType:      *u32,
        lpData:      *void,
        lpcbData:    *u32
    ): i32

    @lib pub fn RegCloseKey(hKey: i64): i32
}

// ─── Per-instance webview state ───────────────────────────────
class WebViewSlot {
    pub key:        i64
    pub parentHwnd: i64
    pub x:          f64
    pub y:          f64
    pub width:      f64
    pub height:     f64
    pub visible:    bool

    pub pendingUrl: string
    /// Live UTF-16 buffer for the last URL we passed to Navigate.
    /// WebView2 holds the pointer across the async navigation;
    /// without this anchor ARC could drop the array as soon as
    /// `controllerInvoke` returns.
    pub lastUrlW: u16[]

    pub envReady:        bool   // env is created and we're past it
    pub controllerReady: bool

    pub controller: i64
    pub webview:    i64

    pub navStartingToken:  i64
    pub navCompletedToken: i64

    pub controllerHandler: i64   // ptr to our HeapAlloc'd handler
    pub navStartingHandler: i64
    pub navCompletedHandler: i64

    pub onNavigate:   fn(string)
    pub onLoadFinish: fn(string, bool, string)

    pub init(key: i64, parent: i64, x: f64, y: f64, w: f64, h: f64, vis: bool) {
        this.key        = key
        this.parentHwnd = parent
        this.x          = x
        this.y          = y
        this.width      = w
        this.height     = h
        this.visible    = vis
        this.pendingUrl = ""
        this.lastUrlW   = []
        this.envReady   = false
        this.controllerReady = false
        this.controller = 0
        this.webview    = 0
        this.navStartingToken  = 0
        this.navCompletedToken = 0
        this.controllerHandler  = 0
        this.navStartingHandler = 0
        this.navCompletedHandler = 0
        this.onNavigate   = webViewNoOpUrl
        this.onLoadFinish = webViewNoOpFinish
    }
}

fn webViewNoOpUrl(url: string) {}
fn webViewNoOpFinish(url: string, isError: bool, errorMessage: string) {}

// ─── Module state ─────────────────────────────────────────────
let WEBVIEWS = new Map<i64, WebViewSlot>()
let NEXT_KEY: i64 = 1

// 0 = not loaded / load failed
let RUNTIME_HMOD: i64 = 0
let CREATE_FN: i64 = 0

// Env state: 0=cold, 1=pending, 2=ready, 3=failed
const ENV_COLD:    i32 = 0
const ENV_PENDING: i32 = 1
const ENV_READY:   i32 = 2
const ENV_FAILED:  i32 = 3
let ENV_STATE: i32 = ENV_COLD
let ENV_PTR:   i64 = 0

// Single shared env-completed handler instance.
let ENV_HANDLER: i64 = 0

// Module-level vtable storage. `&` of a `pub let` is process-lifetime
// stable, which is what WebView2 needs for the handler vtables we
// pass it. Initialised at module load — the COM impl fns
// (`comQI`, `envInvoke`, ...) are forward-referenced and resolved
// statically.
pub let VT_ENV_STORAGE: EnvVtbl = EnvVtbl {
    QueryInterface: comQI,
    AddRef:         comAddRef,
    Release:        comRelease,
    Invoke:         envInvoke,
}
pub let VT_CTRL_STORAGE: CtrlVtbl = CtrlVtbl {
    QueryInterface: comQI,
    AddRef:         comAddRef,
    Release:        comRelease,
    Invoke:         controllerInvoke,
}
pub let VT_NAV_START_STORAGE: NavStartVtbl = NavStartVtbl {
    QueryInterface: comQI,
    AddRef:         comAddRef,
    Release:        comRelease,
    Invoke:         navStartingInvoke,
}
pub let VT_NAV_DONE_STORAGE: NavDoneVtbl = NavDoneVtbl {
    QueryInterface: comQI,
    AddRef:         comAddRef,
    Release:        comRelease,
    Invoke:         navCompletedInvoke,
}

// ─── Runtime path discovery + DLL load ─────────────────────────

@extern(C) {
    pub fn regReadStringW(keyRaw: i64, name: string): string {
        let nameW = name.encodeUtf16()
        let needBytes: u32 = 0
        let valType:   u32 = 0
        let rc1 = RegQueryValueExW(
            keyRaw, nameW, 0 as *u32, &valType,
            0 as *void, &needBytes
        )
        if rc1 != 0 as i32 { return "" }
        if valType != (RegValueType.sz as u32) &&
           valType != (RegValueType.expandSz as u32) { return "" }
        if needBytes == 0 as u32 { return "" }

        let heap = GetProcessHeap()
        let buf  = HeapAlloc(heap, 0 as u32, needBytes as u64) as *u16
        let cap  = needBytes
        let rc2  = RegQueryValueExW(
            keyRaw, nameW, 0 as *u32, &valType,
            buf as *void, &cap
        )
        if rc2 != 0 as i32 {
            HeapFree(heap, 0 as u32, buf as *void)
            return ""
        }
        let units = (cap as i64) / 2 as i64
        if units > 0 as i64 {
            let last = readU16(
                buf as *const void,
                (units - 1 as i64) * 2 as i64
            )
            if last == 0 as u16 { units = units - 1 }
        }
        let arr: u16[] = []
        let i = 0 as i64
        while i < units {
            arr.push(readU16(buf as *const void, i * 2 as i64))
            i = i + 1
        }
        HeapFree(heap, 0 as u32, buf as *void)
        string.fromUtf16(arr)
    }
}

@extern(C) {
    /// Resolve `<install location>\<pv>\EBWebView\x64\EmbeddedBrowserWebView.dll`
    /// from the EdgeUpdate registry entry, returns "" if not installed.
    pub fn resolveRuntimeDllPath(): string {
        // HKEY_LOCAL_MACHINE — `(HKEY)(LONG)0x80000002`, sign-extended.
        let HKLM: i64 = 0xFFFFFFFF80000002 as u64 as i64
        let subkey =
            "Software\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\" +
            "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
        let subW = subkey.encodeUtf16()
        let h: i64 = 0
        let rc = RegOpenKeyExW(HKLM, subW, 0 as u32, 0x20019 as u32, &h)
        if rc != 0 as i32 { return "" }
        let location = regReadStringW(h, "location")
        let pv       = regReadStringW(h, "pv")
        RegCloseKey(h)
        if location == "" || pv == "" { return "" }
        location + "\\" + pv + "\\EBWebView\\x64\\EmbeddedBrowserWebView.dll"
    }
}

@extern(C) {
    pub fn loadRuntime(): bool {
        if RUNTIME_HMOD != 0 as i64 { return true }
        // WebView2 dispatches event callbacks back through COM, so
        // the calling thread must own an STA apartment. Idempotent —
        // S_OK on first init, S_FALSE if another component already
        // initialised the apartment, RPC_E_CHANGED_MODE if a worse
        // mode was selected. Only the negative case is fatal.
        let hr = CoInitializeEx(0 as *void, CoInit.apartmentThreaded as u32)
        if hr < 0 as i32 {
            console.log("[webview2] CoInitializeEx hr =", hr as i64)
            return false
        }
        let path = resolveRuntimeDllPath()
        if path == "" {
            console.log("[webview2] runtime registry entry not found")
            return false
        }
        let pathW = path.encodeUtf16()
        let hmod = LoadLibraryExW(
            pathW, 0 as HANDLE,
            (LoadLibraryFlags.searchDllLoadDir as u32) |
            (LoadLibraryFlags.searchSystem32 as u32)
        )
        if (hmod as i64) == 0 as i64 {
            console.log("[webview2] LoadLibraryExW failed, GLE =",
                        GetLastError() as i64)
            return false
        }
        let symName = cstrFromString("CreateWebViewEnvironmentWithOptionsInternal")
        let p = GetProcAddress(hmod, symName)
        freeCstr(symName)
        if (p as i64) == 0 as i64 {
            console.log("[webview2] entry point missing")
            FreeLibrary(hmod)
            return false
        }
        RUNTIME_HMOD = hmod as i64
        CREATE_FN    = p as i64
        true
    }
}

// ─── COM vtable plumbing ─────────────────────────────────────
//
// Every handler we hand to WebView2 is a 16-byte instance whose
// first 8 bytes point at a 32-byte vtable of 4 fn pointers in IUnknown
// order (QueryInterface, AddRef, Release) plus the handler's Invoke.
// The instance's second 8 bytes hold our internal slot key so the
// Invoke body can `WEBVIEWS.get(slotKey)` back to the per-webview
// state. We do not refcount — the lifetime is the lifetime of the
// WebViewSlot, which we keep alive until the user-side ARC drops
// the wrapper.

@extern(C) {
    // QueryInterface — minimal: hand back our own pointer for any
    // IID. WebView2 only QI's our handler for IUnknown and for the
    // specific handler IID before calling Invoke, and both can
    // legally point at the same vtable since our Invoke slot lines
    // up with the handler interface and our IUnknown slots are the
    // first three. Returning S_OK with *pp = this is the lazy-but-
    // correct minimal answer.
    pub fn comQI(self: i64, riid: i64, pp: i64): i32 {
        if pp == 0 as i64 { return 0x80004003 as u32 as i32 }
        // Compare against IUnknown and the handler-specific IID we
        // stashed at offset 24 of the instance.
        let qLo = readI64(riid as *const void, 0 as i64)
        let qHi = readI64(riid as *const void, 8 as i64)
        // IUnknown IID: 00000000-0000-0000-c000-000000000046 →
        //   low8  = 0x0000000000000000
        //   high8 = 0x46000000000000c0
        let iunkLo: i64 = 0
        let iunkHi: i64 = 0x46000000000000c0 as u64 as i64
        let ownLo = readI64(self as *const void, 24 as i64)
        let ownHi = readI64(self as *const void, 32 as i64)
        let ok = (qLo == iunkLo && qHi == iunkHi) ||
                 (qLo == ownLo && qHi == ownHi)
        if !ok {
            writeI64(pp as *void, 0 as i64, 0 as i64)
            return 0x80004002 as u32 as i32         // E_NOINTERFACE
        }
        writeI64(pp as *void, 0 as i64, self)
        let n = readU32(self as *const void, 16 as i64) + 1 as u32
        writeU32(self as *void, 16 as i64, n)
        0 as i32                                    // S_OK
    }
    pub fn comAddRef(self: i64): u32 {
        let n = readU32(self as *const void, 16 as i64) + 1 as u32
        writeU32(self as *void, 16 as i64, n)
        n
    }
    pub fn comRelease(self: i64): u32 {
        let cur = readU32(self as *const void, 16 as i64)
        if cur == 0 as u32 { return 0 as u32 }
        let n = cur - 1 as u32
        writeU32(self as *void, 16 as i64, n)
        n
    }

    // ─── Invoke bodies ────────────────────────────────────────

    pub fn envInvoke(self: i64, errorCode: i32, env: i64): i32 {
        if errorCode != 0 as i32 {
            console.log("[webview2] env-create HRESULT =", errorCode as i64)
            ENV_STATE = ENV_FAILED
            return 0 as i32
        }
        // AddRef — WebView2 hands ownership to us, but the
        // C++ host sample wraps the raw pointer in `wil::com_ptr`
        // which AddRefs. Mirroring that explicit retain here keeps
        // the env alive past the callback even if the framework
        // releases its own temporary reference.
        (env as *void as IUnknown).AddRef()
        ENV_PTR   = env
        ENV_STATE = ENV_READY
        // Kick every pending slot.
        for slot in WEBVIEWS.values() {
            if !slot.controllerReady && slot.controller == 0 as i64 {
                kickController(slot)
            }
        }
        0 as i32
    }

    pub fn controllerInvoke(self: i64, errorCode: i32, controllerPtr: i64): i32 {
        let slotKey = readI64(self as *const void, 8 as i64)
        let slot = match WEBVIEWS.get(slotKey) {
            some(s) { s }
            none    { return 0 as i32 }
        }
        if errorCode != 0 as i32 {
            console.log("[webview2] controller-create HRESULT =",
                        errorCode as i64)
            return 0 as i32
        }
        let controller = controllerPtr as *void as ICoreWebView2Controller
        // AddRef — see envInvoke for rationale.
        (controllerPtr as *void as IUnknown).AddRef()
        slot.controller = controllerPtr

        // Bounds: client-area rect inside the parent HWND.
        let rc = RECT {
            left:   slot.x as i32,
            top:    slot.y as i32,
            right:  (slot.x + slot.width) as i32,
            bottom: (slot.y + slot.height) as i32,
        }
        controller.put_Bounds(&rc)

        // Visibility.
        controller.put_IsVisible(
            if slot.visible { 1 as i32 } else { 0 as i32 }
        )

        // get_CoreWebView2 — out param **void.
        let raw: *void = 0 as *void
        let hr = controller.get_CoreWebView2(&raw)
        if hr != 0 as i32 || (raw as i64) == 0 as i64 {
            console.log("[webview2] get_CoreWebView2 HRESULT =", hr as i64)
            return 0 as i32
        }
        let webview = raw as ICoreWebView2
        slot.webview = raw as i64

        // Register navigation handlers.
        slot.navStartingHandler  = newHandler(
            navStartVtbl(), slot.key,
            iidNavStartingLo(), iidNavStartingHi()
        )
        slot.navCompletedHandler = newHandler(
            navDoneVtbl(), slot.key,
            iidNavCompletedLo(), iidNavCompletedHi()
        )
        let tStart: i64 = 0
        let tDone:  i64 = 0
        webview.add_NavigationStarting(
            slot.navStartingHandler as *void, &tStart
        )
        webview.add_NavigationCompleted(
            slot.navCompletedHandler as *void, &tDone
        )
        slot.navStartingToken  = tStart
        slot.navCompletedToken = tDone

        slot.controllerReady = true

        if slot.pendingUrl != "" {
            slot.lastUrlW = slot.pendingUrl.encodeUtf16()
            webview.Navigate(slot.lastUrlW)
        }
        0 as i32
    }

    pub fn navStartingInvoke(self: i64, sender: i64, args: i64): i32 {
        let slotKey = readI64(self as *const void, 8 as i64)
        let slot = match WEBVIEWS.get(slotKey) {
            some(s) { s }
            none    { return 0 as i32 }
        }
        let a = args as *void as ICoreWebView2NavigationStartingEventArgs
        let uriPtr: *u16 = 0 as *u16
        if a.get_Uri(&uriPtr) == 0 as i32 && (uriPtr as i64) != 0 as i64 {
            let s = decodeWideOwned(uriPtr as i64)
            slot.onNavigate(s)
        }
        0 as i32
    }

    pub fn navCompletedInvoke(self: i64, sender: i64, args: i64): i32 {
        let slotKey = readI64(self as *const void, 8 as i64)
        let slot = match WEBVIEWS.get(slotKey) {
            some(s) { s }
            none    { return 0 as i32 }
        }
        let a = args as *void as ICoreWebView2NavigationCompletedEventArgs
        let isSuccess: i32 = 0
        let errStatus: i32 = 0
        a.get_IsSuccess(&isSuccess)
        a.get_WebErrorStatus(&errStatus)
        let url = readWebViewUrl(slot.key)
        if isSuccess != 0 as i32 {
            slot.onLoadFinish(url, false, "")
        } else {
            slot.onLoadFinish(url, true,
                "WebView2 error status " + errStatus.toString())
        }
        0 as i32
    }
}

@extern(C) {
    /// Decodes a NUL-terminated wide string into a ilang string and
    /// frees the CoTaskMem-allocated buffer. `pRaw` is the wide-char
    /// pointer cast to i64.
    pub fn decodeWideOwned(pRaw: i64): string {
        let p = pRaw as *u16
        let units: i64 = 0
        while readU16(p as *const void, units * 2 as i64) != 0 as u16 {
            units = units + 1
        }
        let arr: u16[] = []
        let i: i64 = 0
        while i < units {
            arr.push(readU16(p as *const void, i * 2 as i64))
            i = i + 1
        }
        CoTaskMemFree(p as *void)
        string.fromUtf16(arr)
    }

    // ─── Vtable / instance construction ──────────────────────
    //
    // Each handler vtable is a 32-byte fixed-layout struct (4 fn
    // pointers in IUnknown order plus Invoke). Storing them as
    // typed struct literals lets us assign `pub fn` names to the
    // fields directly — ilang has no `fn -> *void` cast, so we
    // cannot build the vtable by writing raw bytes.

    pub struct EnvVtbl {
        QueryInterface: fn(i64, i64, i64): i32
        AddRef:         fn(i64): u32
        Release:        fn(i64): u32
        Invoke:         fn(i64, i32, i64): i32
    }
    pub struct CtrlVtbl {
        QueryInterface: fn(i64, i64, i64): i32
        AddRef:         fn(i64): u32
        Release:        fn(i64): u32
        Invoke:         fn(i64, i32, i64): i32
    }
    pub struct NavStartVtbl {
        QueryInterface: fn(i64, i64, i64): i32
        AddRef:         fn(i64): u32
        Release:        fn(i64): u32
        Invoke:         fn(i64, i64, i64): i32
    }
    pub struct NavDoneVtbl {
        QueryInterface: fn(i64, i64, i64): i32
        AddRef:         fn(i64): u32
        Release:        fn(i64): u32
        Invoke:         fn(i64, i64, i64): i32
    }

    /// 40-byte instance:
    ///   [0..8]   vtable*
    ///   [8..16]  slotKey
    ///   [16..20] refCount
    ///   [20..24] pad
    ///   [24..32] IID low  8 bytes  (compared in QueryInterface)
    ///   [32..40] IID high 8 bytes
    /// Initial refCount is 1 — creator holds the ref.
    pub fn newHandler(vt: i64, slotKey: i64,
                      iidLo: i64, iidHi: i64): i64 {
        let heap = GetProcessHeap()
        let inst = HeapAlloc(heap, 0 as u32, 40 as u64)
        writeI64(inst, 0  as i64, vt)
        writeI64(inst, 8  as i64, slotKey)
        writeU32(inst, 16 as i64, 1 as u32)
        writeI64(inst, 24 as i64, iidLo)
        writeI64(inst, 32 as i64, iidHi)
        inst as i64
    }

    pub fn envVtbl(): i64       { &VT_ENV_STORAGE       as *void as i64 }
    pub fn ctrlVtbl(): i64      { &VT_CTRL_STORAGE      as *void as i64 }
    pub fn navStartVtbl(): i64  { &VT_NAV_START_STORAGE as *void as i64 }
    pub fn navDoneVtbl(): i64   { &VT_NAV_DONE_STORAGE  as *void as i64 }

    // IID constants — UUIDs from WebView2.h flattened to two i64
    // little-endian halves so QueryInterface can compare in two
    // 8-byte reads. The byte layout of a Win32 GUID is
    //   { Data1:le32, Data2:le16, Data3:le16, Data4[8]:be },
    // which on x64 lays the first 8 bytes out as one little-endian
    // i64 and the trailing Data4[8] as another.
    pub fn iidEnvCompletedLo(): i64 { 0x4BD2C9D84E8A3389 as u64 as i64 }
    pub fn iidEnvCompletedHi(): i64 { 0x4DC16CEE4F12B5B6 as u64 as i64 }

    pub fn iidCtrlCompletedLo(): i64 { 0x4260C9B76C4819F3 as u64 as i64 }
    pub fn iidCtrlCompletedHi(): i64 { 0x8CF6E7BDF5C92781 as u64 as i64 }

    pub fn iidNavStartingLo(): i64 { 0x432BF36D9ADBE429 as u64 as i64 }
    pub fn iidNavStartingHi(): i64 { 0xE376BD1F88F8DC9D as u64 as i64 }

    pub fn iidNavCompletedLo(): i64 { 0x4F981C49D33A35BF as u64 as i64 }
    pub fn iidNavCompletedHi(): i64 { 0x1CFE33056E00AB93 as u64 as i64 }

    // ─── Env creation kickoff ────────────────────────────────

    pub fn ensureEnvCreation() {
        if ENV_STATE != ENV_COLD { return }
        ENV_STATE = ENV_PENDING
        if ENV_HANDLER == 0 as i64 {
            ENV_HANDLER = newHandler(
                envVtbl(), 0 as i64,
                iidEnvCompletedLo(), iidEnvCompletedHi()
            )
        }
        // User-data folder under %TEMP%\ilang_webview2.
        let tempBufBytes: u64 = 1024 as u64 * 2 as u64
        let heap = GetProcessHeap()
        let tempBuf = HeapAlloc(heap, 0 as u32, tempBufBytes) as *u16
        let tempLen = GetTempPathW(1024 as u32, tempBuf)
        let userDir = ""
        if tempLen > 0 as u32 {
            let arr: u16[] = []
            let i: i64 = 0
            while i < (tempLen as i64) {
                arr.push(readU16(tempBuf as *const void, i * 2 as i64))
                i = i + 1
            }
            userDir = string.fromUtf16(arr) + "ilang_webview2"
        } else {
            userDir = ".\\ilang_webview2"
        }
        HeapFree(heap, 0 as u32, tempBuf as *void)
        let userDirW = userDir.encodeUtf16()

        let entry = CREATE_FN as *void as fn(
            i32, *const u16, *const u16, *void, *void
        ): i32
        let hr = entry(
            0 as i32,                       // sandbox=false
            0 as *const u16,                // default browser dir
            userDirW,
            0 as *void,                     // no options
            ENV_HANDLER as *void
        )
        if hr != 0 as i32 {
            console.log("[webview2] CreateWebViewEnvironmentWithOptionsInternal hr =",
                        hr as i64)
            ENV_STATE = ENV_FAILED
        }
    }
}

@extern(C) {
    pub fn kickController(slot: WebViewSlot) {
        if ENV_STATE != ENV_READY { return }
        if slot.controller != 0 as i64 { return }
        if slot.controllerHandler == 0 as i64 {
            slot.controllerHandler = newHandler(
                ctrlVtbl(), slot.key,
                iidCtrlCompletedLo(), iidCtrlCompletedHi()
            )
        }
        let env = ENV_PTR as *void as ICoreWebView2Environment
        let hr = env.CreateCoreWebView2Controller(
            slot.parentHwnd,
            slot.controllerHandler as *void
        )
        if hr != 0 as i32 {
            console.log("[webview2] CreateCoreWebView2Controller hr =",
                        hr as i64)
        }
    }
}

// ─── Public surface (gui-core bridge) ────────────────────────

@extern(C) {

pub fn createNativeWebView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    let hwnd = windowHwnd(windowHandle) as i64
    if hwnd == 0 as i64 {
        console.log("[webview2] no parent hwnd")
        return -1 as i64
    }
    if !loadRuntime() { return -1 as i64 }

    // WebView2 uses DirectComposition to draw inside its host area.
    // The parent window's WM_PAINT must not erase that area, so add
    // WS_CLIPCHILDREN (0x02000000) on first WebView per window.
    // Idempotent — the OR is a no-op on subsequent calls.
    let style = GetWindowLongPtrW(hwnd as HWND, WindowLongIndex.style as i32)
    let WS_CLIPCHILDREN: i64 = 0x02000000 as i64
    if (style & WS_CLIPCHILDREN) == 0 as i64 {
        SetWindowLongPtrW(hwnd as HWND, WindowLongIndex.style as i32,
                          style | WS_CLIPCHILDREN)
        SetWindowPos(
            hwnd as HWND, 0 as HWND, 0, 0, 0, 0,
            (SetWindowPosFlags.noMove as u32) |
            (SetWindowPosFlags.noSize as u32) |
            (SetWindowPosFlags.noZOrder as u32) |
            (SetWindowPosFlags.frameChanged as u32)
        )
    }

    let key = NEXT_KEY
    NEXT_KEY = NEXT_KEY + 1
    let slot = new WebViewSlot(key, hwnd, x, y, width, height, visible)
    WEBVIEWS[key] = slot

    if ENV_STATE == ENV_READY {
        kickController(slot)
    } elif ENV_STATE == ENV_COLD {
        ensureEnvCreation()
    }
    // If pending, env-completed will pick this slot up.
    // If failed, the slot stays inert; setters quietly no-op.
    key
}

pub fn setWebViewUrl(handle: i64, url: string) {
    if let some(slot) = WEBVIEWS.get(handle) {
        slot.pendingUrl = url
        if slot.controllerReady && slot.webview != 0 as i64 {
            // Anchor the UTF-16 buffer on the slot so it outlives
            // the async Navigate — assigning to a local would let
            // ARC drop the array as soon as this fn returns.
            slot.lastUrlW = url.encodeUtf16()
            (slot.webview as *void as ICoreWebView2).Navigate(slot.lastUrlW)
        }
    }
}

pub fn readWebViewUrl(handle: i64): string {
    if let some(slot) = WEBVIEWS.get(handle) {
        if slot.webview != 0 as i64 {
            let wv = slot.webview as *void as ICoreWebView2
            let raw: *u16 = 0 as *u16
            if wv.get_Source(&raw) == 0 as i32 &&
               (raw as i64) != 0 as i64 {
                return decodeWideOwned(raw as i64)
            }
        }
        return slot.pendingUrl
    }
    ""
}

pub fn webViewLoadHtml(handle: i64, html: string, baseUrl: string) {
    // Round 1: not implemented (would require NavigateToString slot 6).
}

pub fn webViewGoBack(handle: i64) {
    if let some(slot) = WEBVIEWS.get(handle) {
        if slot.webview != 0 as i64 {
            // Gate on get_CanGoBack — calling GoBack with no
            // back history queues a no-op navigation that, when
            // hammered, has triggered AVs deep inside the
            // runtime's history queue (observed with rapid
            // forward-button spam).
            let wv = slot.webview as *void as ICoreWebView2
            let can: i32 = 0
            wv.get_CanGoBack(&can)
            if can != 0 as i32 { wv.GoBack() }
        }
    }
}
pub fn webViewGoForward(handle: i64) {
    if let some(slot) = WEBVIEWS.get(handle) {
        if slot.webview != 0 as i64 {
            let wv = slot.webview as *void as ICoreWebView2
            let can: i32 = 0
            wv.get_CanGoForward(&can)
            if can != 0 as i32 { wv.GoForward() }
        }
    }
}
pub fn webViewReload(handle: i64) {
    if let some(slot) = WEBVIEWS.get(handle) {
        if slot.webview != 0 as i64 {
            (slot.webview as *void as ICoreWebView2).Reload()
        }
    }
}
pub fn webViewCanGoBack(handle: i64): bool {
    if let some(slot) = WEBVIEWS.get(handle) {
        if slot.webview != 0 as i64 {
            let v: i32 = 0
            (slot.webview as *void as ICoreWebView2).get_CanGoBack(&v)
            return v != 0 as i32
        }
    }
    false
}
pub fn webViewCanGoForward(handle: i64): bool {
    if let some(slot) = WEBVIEWS.get(handle) {
        if slot.webview != 0 as i64 {
            let v: i32 = 0
            (slot.webview as *void as ICoreWebView2).get_CanGoForward(&v)
            return v != 0 as i32
        }
    }
    false
}

pub fn setWebViewFrame(
    handle: i64, x: f64, y: f64, width: f64, height: f64
) {
    if let some(slot) = WEBVIEWS.get(handle) {
        slot.x = x; slot.y = y; slot.width = width; slot.height = height
        if slot.controller != 0 as i64 {
            let rc = RECT {
                left:   x as i32,
                top:    y as i32,
                right:  (x + width)  as i32,
                bottom: (y + height) as i32,
            }
            (slot.controller as *void as ICoreWebView2Controller).put_Bounds(&rc)
        }
    }
}

pub fn setWebViewVisible(handle: i64, visible: bool) {
    if let some(slot) = WEBVIEWS.get(handle) {
        slot.visible = visible
        if slot.controller != 0 as i64 {
            (slot.controller as *void as ICoreWebView2Controller).put_IsVisible(
                if visible { 1 as i32 } else { 0 as i32 }
            )
        }
    }
}

pub fn setOnWebViewNavigate(handle: i64, cb: fn(string)) {
    if let some(slot) = WEBVIEWS.get(handle) { slot.onNavigate = cb }
}
pub fn setOnWebViewLoadFinish(
    handle: i64, cb: fn(string, bool, string)
) {
    if let some(slot) = WEBVIEWS.get(handle) { slot.onLoadFinish = cb }
}

} // @extern(C)
