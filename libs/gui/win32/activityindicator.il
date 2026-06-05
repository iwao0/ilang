/// ActivityIndicator — stub. Win32 has no native indeterminate
/// spinner; the typical implementation custom-draws a rotating
/// arc on a timer. Round 1 ships the macOS path only — these
/// stub functions keep the cross-platform surface alive (handle
/// `-1`, every setter a no-op).

@extern(C) {

pub fn createNativeActivityIndicator(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    -1 as i64
}

pub fn startActivityIndicator(handle: i64) {}
pub fn stopActivityIndicator(handle: i64) {}
pub fn setActivityIndicatorFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {}
pub fn setActivityIndicatorVisible(handle: i64, visible: bool) {}
pub fn setActivityIndicatorTooltip(handle: i64, text: string) {}

} // @extern(C)
