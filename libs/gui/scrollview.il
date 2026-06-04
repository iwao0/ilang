/// Scrollable container — wraps a native scroll view around its
/// children. The scroll area's content size auto-grows with the
/// children's bounding box; horizontal / vertical scrollbars
/// toggle independently. Children are positioned in the content
/// space (top-left = 0, 0), not in the scroll view's frame.
///
///   let s = new gui.ScrollView()
///   s.x = 0.0
///   s.y = 0.0
///   s.width  = 320.0
///   s.height = 240.0
///   window.addChild(s)
///   let b = new gui.Button()
///   b.text = "scrolled button"
///   b.x = 600.0   // outside the visible frame — scroll to reach
///   b.y = 400.0
///   b.width  = 160.0
///   b.height = 30.0
///   s.addChild(b)

use window { Window, Control, Container }
use gui_impl {
    createNativeScrollView, scrollViewDocumentHandle,
    setScrollViewFrame, setScrollViewEnabled, setScrollViewVisible,
    setScrollViewShowHScroll, setScrollViewShowVScroll,
    setScrollViewContentSize,
}

pub class ScrollView : Container {
    pub handle:           i64
    pub _documentHandle:  i64
    _isEnabled:           bool
    _isVisible:           bool
    _showHorizontalScroll: bool
    _showVerticalScroll:   bool

    pub init() {
        super()
        this.handle              = -1
        this._documentHandle     = -1
        // `_w` / `_h` are inherited from Container — seed them
        // with the same defaults every other widget uses.
        this._w                  = 200.0
        this._h                  = 200.0
        this._isEnabled          = true
        this._isVisible          = true
        this._showHorizontalScroll = true
        this._showVerticalScroll   = true
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setScrollViewFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setScrollViewFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub get width(): f64 { this._w }
    pub set width(v: f64) {
        this._w = v
        if this.handle >= 0 {
            setScrollViewFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub get height(): f64 { this._h }
    pub set height(v: f64) {
        this._h = v
        if this.handle >= 0 {
            setScrollViewFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setScrollViewEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setScrollViewVisible(this.handle, v) }
    }

    pub get showHorizontalScroll(): bool { this._showHorizontalScroll }
    pub set showHorizontalScroll(v: bool) {
        this._showHorizontalScroll = v
        if this.handle >= 0 { setScrollViewShowHScroll(this.handle, v) }
    }
    pub get showVerticalScroll(): bool { this._showVerticalScroll }
    pub set showVerticalScroll(v: bool) {
        this._showVerticalScroll = v
        if this.handle >= 0 { setScrollViewShowVScroll(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeScrollView(
            parent.nativeChildHandle(),
            this._x, this._y, this._w, this._h,
            this._isEnabled, this._isVisible,
            this._showHorizontalScroll, this._showVerticalScroll
        )
        if this.handle >= 0 {
            this._documentHandle = scrollViewDocumentHandle(this.handle)
        }
        // Let `Container.attachTo` cascade `attachTo(this)` into
        // every buffered child — `nativeChildHandle()` below now
        // returns the document view so children parent under it.
        super.attachTo(parent)
        this.recomputeContentSize()
    }

    /// Native container handle children parent under — the scroll
    /// view's documentView (Cocoa) / inner pane (Win32) / inner
    /// GtkFixed (GTK).
    pub override nativeChildHandle(): i64 { this._documentHandle }

    pub override addChild(child: Control) {
        super.addChild(child)
        this.recomputeContentSize()
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x
        this._y = y
        this._w = w
        this._h = h
        if this.handle >= 0 {
            setScrollViewFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub override frameWidth():  f64 { this._w }
    pub override frameHeight(): f64 { this._h }

    /// Recompute the scrollable content size as the bounding box
    /// of the children's frames. Called after every `addChild`.
    /// Each child's right-edge is `frameX + frameWidth`, its
    /// bottom-edge `frameY + frameHeight`; the running max across
    /// all children gives the content rectangle.
    recomputeContentSize() {
        if this.handle < 0 { return }
        let maxX = this._w
        let maxY = this._h
        let n = this._children.length
        let i = 0
        while i < n {
            let c = this._children[i]
            let right  = c.frameX() + c.frameWidth()
            let bottom = c.frameY() + c.frameHeight()
            if right  > maxX { maxX = right }
            if bottom > maxY { maxY = bottom }
            i = i + 1
        }
        setScrollViewContentSize(this.handle, maxX, maxY)
    }
}
