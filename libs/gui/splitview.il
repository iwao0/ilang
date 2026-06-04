/// Resizable split pane — wraps a native splitter around an
/// arbitrary number of child panels. Children are added through
/// the inherited `addChild`; each child becomes one pane.
/// Splitter bars sit between adjacent panes and the user drags
/// them to redistribute space.
///
///   let s = new gui.SplitView()
///   s.x = 0.0
///   s.y = 0.0
///   s.width  = 480.0
///   s.height = 320.0
///   s.orientation = gui.SplitOrientation.horizontal
///   window.addChild(s)
///   s.addChild(left)   // pane 0
///   s.addChild(right)  // pane 1
///   s.setSplitPosition(0 as i32, 200.0)   // first divider 200 px from start
///
/// `SplitOrientation.horizontal` lays panes left-to-right with
/// vertical splitter bars; `.vertical` stacks them top-to-bottom
/// with horizontal bars. The orientation is set before
/// `addChild`; changing it later is supported but redraws the
/// whole splitter.

use window { Window, Control, Container }
use gui_impl {
    createNativeSplitView, splitViewDocumentHandle,
    splitViewBeginNewPane,
    setSplitViewFrame, setSplitViewEnabled, setSplitViewVisible,
    setSplitViewOrientation, setSplitViewPosition,
}

/// Layout direction of a `SplitView`.
pub enum SplitOrientation: i32 {
    /// Panes laid out left-to-right; splitter bars are vertical.
    horizontal = 0
    /// Panes stacked top-to-bottom; splitter bars are horizontal.
    vertical   = 1
}

pub class SplitView : Container {
    pub handle:          i64
    pub _documentHandle: i64
    _isEnabled:          bool
    _isVisible:          bool
    _orientation:        SplitOrientation
    /// Splitter positions queued before `attachTo` runs. Applied
    /// in order once the native splitter is realised; afterward
    /// `setSplitPosition` hits the backend directly.
    _pendingPositions:   f64[]

    pub init() {
        super()
        this.handle           = -1
        this._documentHandle  = -1
        this._w               = 200.0
        this._h               = 200.0
        this._isEnabled       = true
        this._isVisible       = true
        this._orientation     = SplitOrientation.horizontal
        let empty: f64[]      = []
        this._pendingPositions = empty
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setSplitViewFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setSplitViewFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub get width(): f64 { this._w }
    pub set width(v: f64) {
        this._w = v
        if this.handle >= 0 {
            setSplitViewFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub get height(): f64 { this._h }
    pub set height(v: f64) {
        this._h = v
        if this.handle >= 0 {
            setSplitViewFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setSplitViewEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setSplitViewVisible(this.handle, v) }
    }

    pub get orientation(): SplitOrientation { this._orientation }
    pub set orientation(v: SplitOrientation) {
        this._orientation = v
        if this.handle >= 0 {
            setSplitViewOrientation(this.handle, v as i32)
        }
    }

    /// Drag the `index`-th splitter (0 = the one between panes 0
    /// and 1) to `px` pixels from the splitter's origin along its
    /// main axis. Out-of-range indices are no-ops once the
    /// splitter is realised; before attachment positions are
    /// queued and applied in order.
    pub setSplitPosition(index: i32, px: f64) {
        if this.handle >= 0 {
            setSplitViewPosition(this.handle, index, px)
            return
        }
        // Pad the pending list out to `index + 1` so the queued
        // entry lands at the right slot when applied.
        let target = (index as i64) + 1
        while this._pendingPositions.length < target {
            this._pendingPositions.push(0.0)
        }
        this._pendingPositions[index as i64] = px
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeSplitView(
            parent.nativeChildHandle(),
            this._x, this._y, this._w, this._h,
            this._isEnabled, this._isVisible,
            this._orientation as i32
        )
        if this.handle >= 0 {
            this._documentHandle = splitViewDocumentHandle(this.handle)
        }
        super.attachTo(parent)
        // Flush any queued splitter positions now that the native
        // splitter exists. Order matches `setSplitPosition` calls.
        let n = this._pendingPositions.length
        let i = 0 as i64
        while i < n {
            setSplitViewPosition(this.handle, i as i32, this._pendingPositions[i])
            i = i + 1
        }
    }

    pub override nativeChildHandle(): i64 { this._documentHandle }

    /// Allocate a fresh pane container for `child` before
    /// realising it. Backends that need per-pane wrapping (Linux
    /// nested GtkPaned chain / Win32 child HWND) materialise a
    /// new inner container here and return its handle; Cocoa
    /// returns the splitter itself so the child becomes a direct
    /// NSSplitView subview.
    pub override addChild(child: Control) {
        this._children.push(child)
        if let some(_) = this._root {
            if this.handle >= 0 {
                this._documentHandle = splitViewBeginNewPane(this.handle)
            }
            child.attachTo(this)
        }
        this.relayout()
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x
        this._y = y
        this._w = w
        this._h = h
        if this.handle >= 0 {
            setSplitViewFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub override frameWidth():  f64 { this._w }
    pub override frameHeight(): f64 { this._h }
}
