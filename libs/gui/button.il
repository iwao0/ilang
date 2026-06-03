/// Push-button widget. Construct unparented, configure with
/// property setters, then attach with `window.addChild(button)`.
/// Properties set before `addChild` are cached on the Button and
/// applied to the native control once it's realized; properties
/// set after `addChild` propagate immediately.
///
///   let b = new gui.Button()
///   b.text = "OK"
///   b.x = 100.0
///   b.y = 50.0
///   b.width  = 80.0
///   b.height = 30.0
///   b.onClick.add(fn(e: gui.MouseEvent) { console.log("clicked") })
///   window.addChild(b)

use std.events { Signal }
use events { MouseEvent, FocusEvent, BlurEvent, KeyEvent }
use window { Window, Control }
use gui_impl {
    createNativeButton,
    setOnButtonClick, setOnButtonFocus, setOnButtonBlur,
    setOnButtonMouseEnter, setOnButtonMouseLeave,
    setOnButtonMouseDown, setOnButtonMouseUp,
    setOnButtonContextMenu,
    setOnButtonKeyDown, setOnButtonKeyUp,
    setButtonText, setButtonFrame, setButtonEnabled, setButtonVisible,
    setButtonDefault, setButtonTooltip, clickButton, focusButton, blurButton,
}

pub class Button : Control {
    // Cached state. `handle` is -1 until `addChild` realizes the
    // native control; before then, setters only update the cache.
    handle: i64
    _text:      string
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _isDefault: bool
    _tooltip:   string
    pub onClick: Signal<MouseEvent>
    pub onFocus: Signal<FocusEvent>
    pub onBlur:  Signal<BlurEvent>
    pub onMouseEnter: Signal<MouseEvent>
    pub onMouseLeave: Signal<MouseEvent>
    pub onMouseDown:  Signal<MouseEvent>
    pub onMouseUp:    Signal<MouseEvent>
    /// Keyboard events on this button while it holds focus. On
    /// macOS the FocusButton subclass's `keyDown:` / `keyUp:`
    /// override forwards here (and re-runs the default
    /// Space → click activation manually, because the ilang
    /// ObjC-super trampoline can't reach `super.keyDown:`). On
    /// Win32 the BUTTON subclass intercepts `WM_KEYDOWN` /
    /// `WM_KEYUP`. On Linux a `GtkEventControllerKey` attached to
    /// the button delivers the GDK `key-pressed` / `key-released`
    /// signals. `e.code` carries the W3C `KeyboardEvent.code`
    /// string (compare against `KeyCode` enum variants).
    pub onKeyDown:    Signal<KeyEvent>
    pub onKeyUp:      Signal<KeyEvent>
    /// Web-style `contextmenu` event. Fires on right-button press
    /// (macOS `rightMouseDown:`, Win32 `WM_RBUTTONDOWN`, GTK click
    /// gesture with `button=3`) — the same instant the OS would
    /// otherwise begin showing the system context menu. Listeners
    /// often pair this with their own popup to replace the default.
    /// Keyboard-triggered context menus (Shift+F10 / the Menu key)
    /// are not surfaced yet — add WM_CONTEXTMENU plumbing if that
    /// becomes a need.
    pub onContextMenu: Signal<MouseEvent>

    pub init() {
        super()
        this.handle      = -1
        this._text       = ""
        this._x          = 0.0
        this._y          = 0.0
        this._width      = 80.0
        this._height     = 30.0
        this._isEnabled  = true
        this._isVisible  = true
        this._isDefault  = false
        this._tooltip    = ""
        this.onClick     = new Signal<MouseEvent>()
        this.onFocus     = new Signal<FocusEvent>()
        this.onBlur      = new Signal<BlurEvent>()
        this.onMouseEnter = new Signal<MouseEvent>()
        this.onMouseLeave = new Signal<MouseEvent>()
        this.onMouseDown  = new Signal<MouseEvent>()
        this.onMouseUp    = new Signal<MouseEvent>()
        this.onContextMenu = new Signal<MouseEvent>()
        this.onKeyDown    = new Signal<KeyEvent>()
        this.onKeyUp      = new Signal<KeyEvent>()
    }

    pub get text(): string { this._text }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setButtonText(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setButtonEnabled(this.handle, v) }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setButtonVisible(this.handle, v) }
    }

    /// Marks this button as the window's default — pressing Return
    /// while the window is focused activates it, and the native
    /// "default" treatment (pulsing blue on macOS, heavy frame on
    /// Win32, GTK default frame) is drawn. Setting `true` on a new
    /// button automatically clears any previous default on the same
    /// window.
    pub get isDefault(): bool { this._isDefault }
    pub set isDefault(v: bool) {
        this._isDefault = v
        if this.handle >= 0 { setButtonDefault(this.handle, v) }
    }

    /// Hover tooltip. Empty string clears it. On macOS the system
    /// hover delay shows the string; on Win32 a TOOLTIPS_CLASS
    /// control owned by the parent window relays the same; on
    /// Linux GTK shows the widget's tooltip text under cursor rest.
    /// Set before or after `addChild` — both paths apply the value.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setButtonTooltip(this.handle, v) }
    }

    /// Realize the native control under `window`. Called via
    /// `Window.addChild(this)` → `Control.attachTo`; not part of
    /// the public surface the user invokes directly.
    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeButton(
            window.handle,
            this._text, this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible, this._isDefault
        )
        if this._tooltip != "" { setButtonTooltip(this.handle, this._tooltip) }
        // Forward native click → onClick. Native pos is (centre, 0)
        // on macOS and (0, 0) on win32; we surface it as a MouseEvent
        // with `sender = this` for downcast.
        setOnButtonClick(this.handle, fn(x: f64, y: f64, b: i32) {
            this.onClick.emit(new MouseEvent(this, x, y, b, false, false, false, false))
        })
        setOnButtonFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnButtonBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnButtonMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnButtonMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnButtonMouseDown(this.handle, fn(x: f64, y: f64, b: i32) {
            this.onMouseDown.emit(new MouseEvent(this, x, y, b, false, false, false, false))
        })
        setOnButtonMouseUp(this.handle, fn(x: f64, y: f64, b: i32) {
            this.onMouseUp.emit(new MouseEvent(this, x, y, b, false, false, false, false))
        })
        setOnButtonContextMenu(this.handle, fn(x: f64, y: f64, b: i32) {
            this.onContextMenu.emit(new MouseEvent(this, x, y, b, false, false, false, false))
        })
        setOnButtonKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnButtonKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
    }

    /// Programmatic click. Animates the native button and runs
    /// the same target/action path a real mouse click would,
    /// so any `onClick` listeners fire. No-op until the button
    /// has been attached to a window.
    pub click() {
        if this.handle >= 0 { clickButton(this.handle) }
    }

    /// Move keyboard focus to this button. The native focus
    /// transition fires `onFocus` here and `onBlur` on whichever
    /// control previously held focus. No-op until the button has
    /// been attached to a window.
    pub focus() {
        if this.handle >= 0 { focusButton(this.handle) }
    }

    /// Remove keyboard focus from this button. Matches the
    /// `HTMLElement.blur()` web semantics — only acts when this
    /// button is the current focus holder. Fires `onBlur` on
    /// this button as a side effect of the responder chain
    /// resigning. No-op until the button has been attached to a
    /// window.
    pub blur() {
        if this.handle >= 0 { blurButton(this.handle) }
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
