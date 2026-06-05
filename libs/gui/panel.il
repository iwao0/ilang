/// Plain rectangular container. Hosts arbitrary children at
/// explicit `x` / `y` positions relative to the panel's own
/// top-left, without applying any auto layout. Useful when the
/// surrounding container is a `VBox` / `HBox` / `Grid` and you
/// want a sub-region that's free-form, or when you just want
/// to group widgets so they move / hide together.
///
/// No visible decoration — the panel is a transparent host. Use
/// `GroupBox` if you want a titled border around the children.
///
///   use gui
///   let p = new gui.Panel()
///   p.x = 20.0; p.y = 20.0
///   p.width = 200.0; p.height = 100.0
///   window.addChild(p)
///
///   let label = new gui.Label()
///   label.text = "inside the panel"
///   label.x = 8.0; label.y = 8.0    // relative to the panel
///   p.addChild(label)

use window { Window, Container, Control }
use gui_impl {
    createNativePanel,
    setPanelFrame, setPanelVisible,
}

pub class Panel : Container {
    pub handle:     i64
    _isVisible:     bool

    pub init() {
        super()
        this.handle     = -1
        this._w         = 100.0
        this._h         = 100.0
        this._isVisible = true
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setPanelFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setPanelFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub get width(): f64 { this._w }
    pub set width(v: f64) {
        this._w = v
        if this.handle >= 0 {
            setPanelFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub get height(): f64 { this._h }
    pub set height(v: f64) {
        this._h = v
        if this.handle >= 0 {
            setPanelFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setPanelVisible(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativePanel(
            parent.nativeChildHandle(),
            this._x, this._y, this._w, this._h,
            this._isVisible
        )
        super.attachTo(parent)
    }

    /// Children parent under the panel's native HWND / NSView /
    /// GtkFixed so their `x` / `y` are relative to it.
    pub override nativeChildHandle(): i64 { this.handle }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._w = w; this._h = h
        if this.handle >= 0 {
            setPanelFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._w }
    pub override frameHeight(): f64 { this._h }
}
