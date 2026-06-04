/// Scrollable container — Win32 stub. The cross-platform
/// `libs/gui/scrollview.il` surface compiles against this file
/// but the native side is deferred: `createNativeScrollView`
/// returns `-1` (every public setter then no-ops because the
/// `ScrollView.handle` guard fails), so any program using the
/// ScrollView API on Windows still builds but the widget itself
/// is not rendered. Round 2 will graduate this to a real
/// `WS_CHILD | WS_HSCROLL | WS_VSCROLL` inner pane driven by
/// `WM_VSCROLL` / `WM_HSCROLL` + `ScrollWindowEx`.

@extern(C) {

pub fn createNativeScrollView(
    parentHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool,
    showHScroll: bool, showVScroll: bool
): i64 {
    -1 as i64
}

pub fn scrollViewDocumentHandle(handle: i64): i64 { -1 as i64 }
pub fn setScrollViewFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {}
pub fn setScrollViewVisible(handle: i64, visible: bool) {}
pub fn setScrollViewEnabled(handle: i64, enabled: bool) {}
pub fn setScrollViewShowHScroll(handle: i64, show: bool) {}
pub fn setScrollViewShowVScroll(handle: i64, show: bool) {}
pub fn setScrollViewContentSize(handle: i64, w: f64, h: f64) {}

} // @extern(C)
