/// Windows implementation of the `gui-core` bridge API.
///
/// Stub today — a real Win32 backend (RegisterClassExA →
/// CreateWindowExA → GetMessage loop, with `SetWindowLongPtrA`
/// to stash per-window callback state) lands once the macOS
/// slice settles. The public API contract (must stay in sync
/// with `libs/gui-cocoa`):
///
///   createNativeWindow(title, w, h): i64
///   setOnClose(handle, cb)
///   runEventLoop()

pub fn createNativeWindow(title: string, width: f64, height: f64): i64 {
    let _ = title
    let _ = width
    let _ = height
    0
}

pub fn setOnClose(handle: i64, cb: fn()) {
    let _ = handle
    let _ = cb
}

pub fn runEventLoop() {}
