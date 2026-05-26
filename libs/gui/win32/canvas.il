/// Canvas — Windows stub. The drawing surface only has a real
/// implementation on macOS for now (see `libs/gui/cocoa/canvas.il`).
/// These no-ops let the public `gui.Canvas` class compile against
/// the win32 backend; the widget exists, but draws nothing and
/// stays at handle `-1` so every method on the public side
/// short-circuits.

pub fn createNativeCanvas(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64, visible: bool
): i64 {
    -1
}

pub fn setCanvasFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {}
pub fn setCanvasVisible(handle: i64, visible: bool) {}
pub fn setCanvasFillColor(handle: i64, argb: u32) {}
pub fn setCanvasStrokeColor(handle: i64, argb: u32) {}
pub fn setCanvasLineWidth(handle: i64, w: f64) {}
pub fn canvasClear(handle: i64, argb: u32) {}
pub fn canvasFillRect(handle: i64, x: f64, y: f64, w: f64, h: f64) {}
pub fn canvasStrokeRect(handle: i64, x: f64, y: f64, w: f64, h: f64) {}
pub fn canvasBeginPath(handle: i64) {}
pub fn canvasMoveTo(handle: i64, x: f64, y: f64) {}
pub fn canvasLineTo(handle: i64, x: f64, y: f64) {}
pub fn canvasArc(
    handle: i64, cx: f64, cy: f64, r: f64,
    startAngle: f64, endAngle: f64
) {}
pub fn canvasClosePath(handle: i64) {}
pub fn canvasFill(handle: i64) {}
pub fn canvasStroke(handle: i64) {}
