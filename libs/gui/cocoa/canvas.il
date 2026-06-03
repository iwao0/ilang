/// 2D drawing surface — HTML Canvas-like view. Each Canvas owns
/// a persistent `NSImage` sized to its pixel dimensions; the
/// public drawing primitives rasterise into that image inside a
/// `lockFocus` / `unlockFocus` bracket. `CanvasView.drawRect:`
/// just blits the image into the visible view, so AppKit's
/// repaint after a resize or expose costs nothing in user code.
///
/// Coordinate system: top-left origin, y-down — matches the rest
/// of gui-core. `lockFocusFlipped(true)` flips the image's
/// drawing context to match, so angles passed to `canvasArc` map
/// to the HTML Canvas convention (clockwise from 3 o'clock).

use cocoa {
    NSObject, NSView, NSImage, NSColor, NSBezierPath,
    NSPoint, NSRect, NSCompositingOperation, NSEvent,
    nil,
    makeRect, makePoint, makeSize,
}
use core {
    WINDOWS, noOpKey,
    keyCodeToString, modCtrl, modShift, modAlt, modMeta,
}

/// Custom `NSView` subclass. `drawRect:` blits the slot's
/// persistent bitmap; `isFlipped` keeps the view's coordinate
/// system in the W3C / top-left convention so child positions
/// match the rest of gui-core. Input plumbing mirrors GpuView's
/// pattern — keyboard events arrive once the view takes first
/// responder, mouseMoved fires only while it owns it.
class CanvasView : NSView {
    pub isFlipped(): bool { true }
    /// Take first responder so keyDown:/keyUp: arrive here
    /// instead of falling through to the window's content view.
    pub acceptsFirstResponder(): bool { true }

    pub drawRect(dirty: NSRect) {
        if let some(slot) = CANVASES.get(this.handle) {
            slot.img.drawInRect(this.bounds)
        }
    }

    pub keyDown(event: NSEvent) {
        if let some(slot) = CANVASES.get(this.handle) {
            let flags = event.modifierFlags as u64
            let cb = slot.onKeyDown
            cb(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub keyUp(event: NSEvent) {
        if let some(slot) = CANVASES.get(this.handle) {
            let flags = event.modifierFlags as u64
            let cb = slot.onKeyUp
            cb(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub mouseMoved(event: NSEvent)        { this.dispatchMouse(event, 0, 0) }
    pub mouseDragged(event: NSEvent)      { this.dispatchMouse(event, 0, 0) }
    pub rightMouseDragged(event: NSEvent) { this.dispatchMouse(event, 0, 2) }
    pub otherMouseDragged(event: NSEvent) { this.dispatchMouse(event, 0, 1) }
    /// mouseDown / mouseUp's binding is `NSObject` (see GuiView's
    /// note in core.il) — override has to match. Rebind via
    /// __wrap_handle_unowned.
    pub override mouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        this.dispatchMouse(e, 1, 0)
    }
    pub override mouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        this.dispatchMouse(e, 2, 0)
    }
    pub override rightMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        this.dispatchMouse(e, 1, 2)
    }
    pub override rightMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        this.dispatchMouse(e, 2, 2)
    }
    pub override otherMouseDown(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        this.dispatchMouse(e, 1, 1)
    }
    pub override otherMouseUp(event: NSObject) {
        let e = NSEvent.__wrap_handle_unowned(event.handle)
        this.dispatchMouse(e, 2, 1)
    }
    pub scrollWheel(event: NSEvent) {
        if let some(slot) = CANVASES.get(this.handle) {
            let p = this.convertPointFromView(event.locationInWindow, nil)
            let flags = event.modifierFlags as u64
            let cb = slot.onMouseWheel
            cb(event.deltaX, event.deltaY, p.x, p.y,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    /// `kind`: 0=move, 1=down, 2=up. `button`: W3C MouseEvent.button.
    dispatchMouse(event: NSEvent, kind: i32, button: i32) {
        if let some(slot) = CANVASES.get(this.handle) {
            let p = this.convertPointFromView(event.locationInWindow, nil)
            let flags = event.modifierFlags as u64
            match kind {
                0 {
                    let cb = slot.onMouseMove
                    // `buttons` bitfield carries the current button
                    // snapshot; for the first pass we pass 0 and let
                    // future Canvas-side tracking layer on top.
                    cb(p.x, p.y, 0 as u32,
                       modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
                }
                1 {
                    let cb = slot.onMouseDown
                    cb(p.x, p.y, button,
                       modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
                }
                2 {
                    let cb = slot.onMouseUp
                    cb(p.x, p.y, button,
                       modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
                }
                _ {}
            }
        }
    }
}

fn canvasNoOpMove(x: f64, y: f64, b: u32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn canvasNoOpBtn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn canvasNoOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}

/// Per-canvas state. Stored in `CANVASES` keyed by the
/// CanvasView's raw ObjC pointer — same value gui-core sees as
/// the public `handle: i64`.
class CanvasSlot {
    pub view:        CanvasView
    pub img:         NSImage
    pub bmpW:        f64
    pub bmpH:        f64
    pub path:        NSBezierPath
    pub fillColor:   NSColor
    pub strokeColor: NSColor
    pub lineWidth:   f64
    pub onKeyDown:    fn(string, bool, bool, bool, bool)
    pub onKeyUp:      fn(string, bool, bool, bool, bool)
    pub onMouseMove:  fn(f64, f64, u32, bool, bool, bool, bool)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)

    pub init(view: CanvasView, img: NSImage, w: f64, h: f64) {
        this.view        = view
        this.img         = img
        this.bmpW        = w
        this.bmpH        = h
        this.path        = NSBezierPath.bezierPath()
        this.fillColor   = NSColor.black
        this.strokeColor = NSColor.black
        this.lineWidth   = 1.0
        this.onKeyDown    = noOpKey
        this.onKeyUp      = noOpKey
        this.onMouseMove  = canvasNoOpMove
        this.onMouseDown  = canvasNoOpBtn
        this.onMouseUp    = canvasNoOpBtn
        this.onMouseWheel = canvasNoOpWheel
    }
}

let CANVASES = new Map<i64, CanvasSlot>()

/// 180 / π — converts radians (the public API unit) to degrees
/// (what `appendBezierPathWithArc…:` expects).
const RAD_TO_DEG: f64 = 57.29577951308232

fn argbToColor(argb: u32): NSColor {
    let mask: u32 = 0xFF as u32
    let a = (((argb >> 24) & mask) as f64) / 255.0
    let r = (((argb >> 16) & mask) as f64) / 255.0
    let g = (((argb >>  8) & mask) as f64) / 255.0
    let b = (( argb        & mask) as f64) / 255.0
    NSColor.rgba(r, g, b, a)
}

fn makeBmp(w: f64, h: f64): NSImage {
    NSImage.alloc().initWithSize(makeSize(w, h))
}

pub fn createNativeCanvas(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    CanvasView.register()
    let view = CanvasView.alloc().init()
    view.setFrame(makeRect(x, y, width, height))
    view.setHidden(!visible)

    let img = makeBmp(width, height)

    winSlot.win.contentView.addSubview(view as NSView)

    let handle = view.handle
    CANVASES[handle] = new CanvasSlot(view, img, width, height)
    handle
}

pub fn setCanvasFrame(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.view.setFrame(makeRect(x, y, w, h))
        // Pixel dimensions changed — reallocate the bitmap so
        // drawing coords line up with what gets blit'd. Previous
        // contents are discarded; callers that want them
        // preserved should re-render after the resize.
        if w != slot.bmpW || h != slot.bmpH {
            slot.img = makeBmp(w, h)
            slot.bmpW = w
            slot.bmpH = h
            slot.view.setNeedsDisplay(true)
        }
    }
}

pub fn setCanvasVisible(handle: i64, visible: bool) {
    if let some(slot) = CANVASES.get(handle) {
        slot.view.setHidden(!visible)
    }
}

pub fn setCanvasFillColor(handle: i64, argb: u32) {
    if let some(slot) = CANVASES.get(handle) {
        slot.fillColor = argbToColor(argb)
    }
}

pub fn setCanvasStrokeColor(handle: i64, argb: u32) {
    if let some(slot) = CANVASES.get(handle) {
        slot.strokeColor = argbToColor(argb)
    }
}

pub fn setCanvasLineWidth(handle: i64, w: f64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.lineWidth = w
    }
}

pub fn canvasClear(handle: i64, argb: u32) {
    if let some(slot) = CANVASES.get(handle) {
        let bg = argbToColor(argb)
        slot.img.lockFocusFlipped(true)
        bg.setFill()
        let r = NSBezierPath.bezierPathWithRect(
            makeRect(0.0, 0.0, slot.bmpW, slot.bmpH)
        )
        r.fill()
        slot.img.unlockFocus()
        slot.view.setNeedsDisplay(true)
    }
}

pub fn canvasFillRect(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.img.lockFocusFlipped(true)
        slot.fillColor.setFill()
        NSBezierPath.bezierPathWithRect(makeRect(x, y, w, h)).fill()
        slot.img.unlockFocus()
        slot.view.setNeedsDisplay(true)
    }
}

pub fn canvasStrokeRect(handle: i64, x: f64, y: f64, w: f64, h: f64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.img.lockFocusFlipped(true)
        slot.strokeColor.setStroke()
        let p = NSBezierPath.bezierPathWithRect(makeRect(x, y, w, h))
        p.lineWidth = slot.lineWidth
        p.stroke()
        slot.img.unlockFocus()
        slot.view.setNeedsDisplay(true)
    }
}

pub fn canvasBeginPath(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.path = NSBezierPath.bezierPath()
    }
}

pub fn canvasMoveTo(handle: i64, x: f64, y: f64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.path.moveToPoint(makePoint(x, y))
    }
}

pub fn canvasLineTo(handle: i64, x: f64, y: f64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.path.lineToPoint(makePoint(x, y))
    }
}

/// `startAngle` / `endAngle` are radians (HTML Canvas convention).
/// In the flipped drawing context, `clockwise:true` matches HTML
/// Canvas's default sweep direction (CW from 3 o'clock, y down).
pub fn canvasArc(
    handle: i64,
    cx: f64, cy: f64, r: f64, startAngle: f64, endAngle: f64
) {
    if let some(slot) = CANVASES.get(handle) {
        slot.path.appendBezierPathWithArcClockwise(
            makePoint(cx, cy), r,
            startAngle * RAD_TO_DEG, endAngle * RAD_TO_DEG,
            true
        )
    }
}

pub fn canvasClosePath(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.path.closePath()
    }
}

pub fn canvasFill(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.img.lockFocusFlipped(true)
        slot.fillColor.setFill()
        slot.path.fill()
        slot.img.unlockFocus()
        slot.view.setNeedsDisplay(true)
    }
}

pub fn canvasStroke(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.img.lockFocusFlipped(true)
        slot.strokeColor.setStroke()
        slot.path.lineWidth = slot.lineWidth
        slot.path.stroke()
        slot.img.unlockFocus()
        slot.view.setNeedsDisplay(true)
    }
}

pub fn setOnCanvasKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = CANVASES.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnCanvasKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = CANVASES.get(handle) { slot.onKeyUp = cb }
}
pub fn setOnCanvasMouseMove(
    handle: i64, cb: fn(f64, f64, u32, bool, bool, bool, bool)
) {
    if let some(slot) = CANVASES.get(handle) { slot.onMouseMove = cb }
}
pub fn setOnCanvasMouseDown(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = CANVASES.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnCanvasMouseUp(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = CANVASES.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnCanvasMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = CANVASES.get(handle) { slot.onMouseWheel = cb }
}
