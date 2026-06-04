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
    NSTrackingArea, NSTrackingAreaOptions,
    NSPoint, NSRect, NSSize, NSCompositingOperation, NSEvent,
    NSArray, NSMutableArray, NSString, NSDictionary, NSMutableDictionary,
    NSNumber, NSFont, NSFontDescriptor, NSFontSymbolicTraits,
    NSPasteboard, NSDraggingInfo, FilenamesPboardType,
    nil, nsstr, ilstr,
    makeRect, makePoint, makeSize,
}
use core {
    WINDOWS, resolveParentView, noOp1, noOpKey, noOpEnterLeave,
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
    /// `isEnabled=false` opts out of focus / input dispatch.
    pub acceptsFirstResponder(): bool {
        match CANVASES.get(this.handle) {
            some(slot) { slot.isEnabled }
            none       { true }
        }
    }
    pub becomeFirstResponder(): bool {
        if let some(slot) = CANVASES.get(this.handle) {
            let cb = slot.onFocus
            cb()
        }
        true
    }
    pub resignFirstResponder(): bool {
        if let some(slot) = CANVASES.get(this.handle) {
            let cb = slot.onBlur
            cb()
        }
        true
    }

    pub drawRect(dirty: NSRect) {
        if let some(slot) = CANVASES.get(this.handle) {
            slot.img.drawInRect(this.bounds)
        }
    }

    pub keyDown(event: NSEvent) {
        if let some(slot) = CANVASES.get(this.handle) {
            if !slot.isEnabled { return }
            let flags = event.modifierFlags as u64
            let cb = slot.onKeyDown
            cb(keyCodeToString(event.keyCode),
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub keyUp(event: NSEvent) {
        if let some(slot) = CANVASES.get(this.handle) {
            if !slot.isEnabled { return }
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
        if let some(slot) = CANVASES.get(this.handle) {
            if !slot.isEnabled { return }
            let p = this.convertPointFromView(e.locationInWindow, nil)
            let flags = e.modifierFlags as u64
            let ctx = slot.onContextMenu
            ctx(p.x, p.y, 2,
                modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
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
    pub override mouseEntered(event: NSObject) {
        if let some(slot) = CANVASES.get(this.handle) {
            if !slot.isEnabled { return }
            let e = NSEvent.__wrap_handle_unowned(event.handle)
            let p = this.convertPointFromView(e.locationInWindow, nil)
            let cb = slot.onMouseEnter
            cb(p.x, p.y)
        }
    }
    pub override mouseExited(event: NSObject) {
        if let some(slot) = CANVASES.get(this.handle) {
            if !slot.isEnabled { return }
            let e = NSEvent.__wrap_handle_unowned(event.handle)
            let p = this.convertPointFromView(e.locationInWindow, nil)
            let cb = slot.onMouseLeave
            cb(p.x, p.y)
        }
    }
    pub scrollWheel(event: NSEvent) {
        if let some(slot) = CANVASES.get(this.handle) {
            if !slot.isEnabled { return }
            let p = this.convertPointFromView(event.locationInWindow, nil)
            let flags = event.modifierFlags as u64
            let cb = slot.onMouseWheel
            cb(event.deltaX, event.deltaY, p.x, p.y,
               modCtrl(flags), modShift(flags), modAlt(flags), modMeta(flags))
        }
    }
    pub draggingEntered(sender: NSObject): u64 { 1 as u64 }
    pub performDragOperation(sender: NSObject): bool {
        if let some(slot) = CANVASES.get(this.handle) {
            let info = NSDraggingInfo.__wrap_handle_unowned(sender.handle)
            let pb = info.draggingPasteboard
            let plist = pb.propertyListForType(nsstr(FilenamesPboardType))
            if plist.handle == 0 as i64 { return false }
            let arr = NSArray.__wrap_handle_unowned(plist.handle)
            let n = arr.count
            let files: string[] = []
            let i = 0 as i64
            while i < n {
                let item = arr.objectAtIndex(i)
                let s = NSString.__wrap_handle_unowned(item.handle)
                files.push(ilstr(s))
                i = i + 1 as i64
            }
            let loc = info.draggingLocation
            let p = this.convertPointFromView(loc, nil)
            let cb = slot.onFilesDropped
            cb(files, p.x, p.y)
        }
        true
    }
    /// `kind`: 0=move, 1=down, 2=up. `button`: W3C MouseEvent.button.
    dispatchMouse(event: NSEvent, kind: i32, button: i32) {
        if let some(slot) = CANVASES.get(this.handle) {
            if !slot.isEnabled { return }
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
    pub font:        NSFont
    pub isEnabled:   bool
    pub onFocus:      fn()
    pub onBlur:       fn()
    pub onKeyDown:    fn(string, bool, bool, bool, bool)
    pub onKeyUp:      fn(string, bool, bool, bool, bool)
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseMove:  fn(f64, f64, u32, bool, bool, bool, bool)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub onFilesDropped: fn(string[], f64, f64)

    pub init(view: CanvasView, img: NSImage, w: f64, h: f64) {
        this.view        = view
        this.img         = img
        this.bmpW        = w
        this.bmpH        = h
        this.path        = NSBezierPath.bezierPath()
        this.fillColor   = NSColor.black
        this.strokeColor = NSColor.black
        this.lineWidth   = 1.0
        this.font        = NSFont.systemFontOfSize(12.0)
        this.isEnabled   = true
        this.onFocus      = noOp1
        this.onBlur       = noOp1
        this.onKeyDown    = noOpKey
        this.onKeyUp      = noOpKey
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseMove  = canvasNoOpMove
        this.onMouseDown  = canvasNoOpBtn
        this.onMouseUp    = canvasNoOpBtn
        this.onContextMenu = canvasNoOpBtn
        this.onMouseWheel = canvasNoOpWheel
        this.onFilesDropped = canvasNoOpFiles
    }
}

fn canvasNoOpFiles(files: string[], x: f64, y: f64) {}

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
    x: f64, y: f64, width: f64, height: f64, enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    CanvasView.register()
    let view = CanvasView.alloc().init()
    view.setFrame(makeRect(x, y, width, height))
    view.setHidden(!visible)

    let img = makeBmp(width, height)

    // Tracking area for mouseEntered: / mouseExited:.
    let opts = NSTrackingAreaOptions.mouseEnteredAndExited
             | NSTrackingAreaOptions.activeAlways
             | NSTrackingAreaOptions.inVisibleRect
    let area = NSTrackingArea.alloc().init(
        makeRect(0.0, 0.0, width, height), opts,
        view as NSObject, nil
    )
    (view as NSView).addTrackingArea(area as NSObject)

    parent.addSubview(view as NSView)

    let handle = view.handle
    let slot = new CanvasSlot(view, img, width, height)
    slot.isEnabled = enabled
    CANVASES[handle] = slot
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

/// NSView has no native `enabled` state — track the flag in
/// `CanvasSlot` and gate input dispatch on it instead. Drawing
/// content stays as the caller left it; redraw to a grayed-out
/// look is the caller's responsibility.
pub fn setCanvasEnabled(handle: i64, enabled: bool) {
    if let some(slot) = CANVASES.get(handle) {
        slot.isEnabled = enabled
        if !enabled {
            let win = (slot.view as NSView).window
            if win.firstResponder.handle == (slot.view as NSView).handle {
                let _ = win.makeFirstResponder(nil)
            }
        }
    }
}

pub fn setCanvasTooltip(handle: i64, text: string) {
    if let some(slot) = CANVASES.get(handle) {
        if text == "" {
            (slot.view as NSView).setToolTip(nil)
        } else {
            (slot.view as NSView).setToolTip(nsstr(text) as NSObject)
        }
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

/// Build an `NSFont` for the requested family / size / weight / italic.
/// Cocoa exposes weight as a discrete trait (Bold or not), so weights
/// ≥ 600 snap to bold and the rest stay regular.
fn buildFont(family: string, size: f64, weight: i32, italic: bool): NSFont {
    // Look the family up by name first. `"sans-serif"`, `"serif"`,
    // and `"monospace"` are CSS generic families with no direct
    // Cocoa equivalent, so map them to the system font / common
    // platform faces. Anything else is passed through verbatim.
    let base = if family == "sans-serif" {
        NSFont.systemFontOfSize(size)
    } elif family == "serif" {
        let f = NSFont.fontWithName(nsstr("Times New Roman"), size)
        if f.handle == 0 as i64 { NSFont.systemFontOfSize(size) } else { f }
    } elif family == "monospace" {
        let f = NSFont.fontWithName(nsstr("Menlo"), size)
        if f.handle == 0 as i64 { NSFont.systemFontOfSize(size) } else { f }
    } else {
        let f = NSFont.fontWithName(nsstr(family), size)
        if f.handle == 0 as i64 { NSFont.systemFontOfSize(size) } else { f }
    }
    let traits: u32 =
        (if weight >= 600 as i32 { NSFontSymbolicTraits.bold as u32 } else { 0 as u32 }) |
        (if italic { NSFontSymbolicTraits.italic as u32 } else { 0 as u32 })
    if traits == 0 as u32 { return base }
    let desc = base.fontDescriptor.fontDescriptorWithSymbolicTraits(traits)
    let withTraits = NSFont.fontWithDescriptor(desc, size)
    // fontWithDescriptor returns nil when the platform can't match
    // the requested traits — fall back to the base font so the
    // caller still gets a renderable glyph stream.
    if withTraits.handle == 0 as i64 { base } else { withTraits }
}

pub fn setCanvasFont(
    handle: i64, family: string, size: f64, weight: i32, italic: bool
) {
    if let some(slot) = CANVASES.get(handle) {
        slot.font = buildFont(family, size, weight, italic)
    }
}

/// Build the attribute dictionary every fill / stroke / measure call
/// hands to AppKit's NSStringDrawing category. Constructed on each
/// call — cheap enough at typical text-per-frame rates.
///
/// `stroke`: 0 = solid fill (NSStrokeWidth absent), positive value =
/// outline-only at that NSStrokeWidth (a percentage of font size
/// per AppKit's convention; outline-only requires a positive value).
fn buildAttrs(slot: CanvasSlot, stroke: f64): NSMutableDictionary {
    let attrs = NSMutableDictionary.dictionary()
    attrs.setObject(slot.font as NSObject, nsstr("NSFont") as NSObject)
    if stroke == 0.0 {
        attrs.setObject(slot.fillColor as NSObject,
                        nsstr("NSColor") as NSObject)
    } else {
        // NSStrokeWidth > 0 means "outline only". Use the stroke
        // colour for both `NSColor` (which becomes the outline tint
        // in that mode) and the explicit NSStrokeColor for AppKit
        // versions that honour the latter.
        attrs.setObject(slot.strokeColor as NSObject,
                        nsstr("NSColor") as NSObject)
        attrs.setObject(slot.strokeColor as NSObject,
                        nsstr("NSStrokeColor") as NSObject)
        attrs.setObject(NSNumber.fromF64(stroke) as NSObject,
                        nsstr("NSStrokeWidth") as NSObject)
    }
    attrs
}

pub fn canvasFillText(handle: i64, text: string, x: f64, y: f64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.img.lockFocusFlipped(true)
        let attrs = buildAttrs(slot, 0.0)
        // drawAtPoint puts the upper-left of the layout box at the
        // given point, but the public API takes the baseline. Back
        // up by the font's ascender so the baseline lines up at `y`.
        let pt = makePoint(x, y - slot.font.ascender)
        nsstr(text).drawAtPoint(pt, attrs as NSDictionary)
        slot.img.unlockFocus()
        slot.view.setNeedsDisplay(true)
    }
}

pub fn canvasStrokeText(handle: i64, text: string, x: f64, y: f64) {
    if let some(slot) = CANVASES.get(handle) {
        slot.img.lockFocusFlipped(true)
        // NSStrokeWidth is a percentage of the font's point size in
        // AppKit; translate ilang's pixel-based `lineWidth` so a
        // 1 px stroke at any font size renders at roughly the right
        // visual weight.
        let pct = if slot.font.pointSize > 0.0 {
            slot.lineWidth * 100.0 / slot.font.pointSize
        } else {
            slot.lineWidth
        }
        let attrs = buildAttrs(slot, pct)
        let pt = makePoint(x, y - slot.font.ascender)
        nsstr(text).drawAtPoint(pt, attrs as NSDictionary)
        slot.img.unlockFocus()
        slot.view.setNeedsDisplay(true)
    }
}

pub fn canvasMeasureText(handle: i64, text: string): f64[] {
    let result: f64[] = [0.0, 0.0, 0.0, 0.0]
    if let some(slot) = CANVASES.get(handle) {
        let attrs = buildAttrs(slot, 0.0)
        let sz = nsstr(text).sizeWithAttributes(attrs as NSDictionary)
        let ascent  = slot.font.ascender
        // NSFont.descender is conventionally negative; the public
        // TextMetrics field is the positive distance below baseline.
        let descent = 0.0 - slot.font.descender
        let leading = slot.font.leading
        result[0] = sz.width
        result[1] = ascent
        result[2] = descent
        result[3] = ascent + descent + leading
    }
    result
}

pub fn setOnCanvasFocus(handle: i64, cb: fn()) {
    if let some(slot) = CANVASES.get(handle) { slot.onFocus = cb }
}
pub fn setOnCanvasBlur(handle: i64, cb: fn()) {
    if let some(slot) = CANVASES.get(handle) { slot.onBlur = cb }
}
pub fn focusCanvas(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        let win = (slot.view as NSView).window
        let _ = win.makeFirstResponder(slot.view as NSObject)
    }
}
pub fn blurCanvas(handle: i64) {
    if let some(slot) = CANVASES.get(handle) {
        let win = (slot.view as NSView).window
        if win.firstResponder.handle == (slot.view as NSView).handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}
pub fn setOnCanvasKeyDown(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = CANVASES.get(handle) { slot.onKeyDown = cb }
}
pub fn setOnCanvasKeyUp(handle: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = CANVASES.get(handle) { slot.onKeyUp = cb }
}
pub fn setOnCanvasMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = CANVASES.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnCanvasMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = CANVASES.get(handle) { slot.onMouseLeave = cb }
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
pub fn setOnCanvasContextMenu(
    handle: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = CANVASES.get(handle) { slot.onContextMenu = cb }
}
pub fn setOnCanvasMouseWheel(
    handle: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = CANVASES.get(handle) { slot.onMouseWheel = cb }
}
pub fn setOnCanvasFilesDropped(handle: i64, cb: fn(string[], f64, f64)) {
    if let some(slot) = CANVASES.get(handle) {
        slot.onFilesDropped = cb
        let types = NSMutableArray.alloc().init()
        types.addObject(nsstr(FilenamesPboardType) as NSObject)
        (slot.view as NSView).registerForDraggedTypes(types as NSArray)
    }
}
