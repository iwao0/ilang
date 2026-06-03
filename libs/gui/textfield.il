/// A single-line editable text field. Backed by the `EDIT`
/// control class on win32 and a bezeled `NSTextField` on macOS.
/// `onChange` fires on every keystroke (DOM `oninput` semantics),
/// not just on commit, so live previews work without polling.
///
///   let t = new gui.TextField()
///   t.text   = "hello"
///   t.x      = 120.0
///   t.y      = 20.0
///   t.width  = 200.0
///   t.height = 24.0
///   t.onChange.add(fn(e: gui.ChangeEvent) { console.log(e.value) })
///   window.addChild(t)

use std.events { Signal }
use events { ChangeEvent, FocusEvent, BlurEvent, MouseEvent, KeyEvent }
use window { Window, Control }
use gui_impl {
    createNativeEdit, readEditText,
    setOnEditChange, setOnEditFocus, setOnEditBlur,
    setOnEditMouseEnter, setOnEditMouseLeave,
    setOnEditMouseDown, setOnEditMouseUp, setOnEditContextMenu,
    setOnEditKeyDown, setOnEditKeyUp,
    setOnEditMouseWheel,
    selectAllEdit, setEditSelection,
    getEditSelectionStart, getEditSelectionEnd,
    setEditText, setEditFrame,
    setEditEnabled, setEditVisible, setEditTooltip,
    setEditPlaceholder, setEditReadOnly, setEditMaxLength,
    focusEdit, blurEdit,
}

pub class TextField : Control {
    handle: i64
    _text:      string
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string
    _placeholder: string
    _isReadOnly: bool
    _maxLength:  i32
    pub onChange: Signal<ChangeEvent>
    pub onFocus:  Signal<FocusEvent>
    pub onBlur:   Signal<BlurEvent>
    pub onMouseEnter: Signal<MouseEvent>
    pub onMouseLeave: Signal<MouseEvent>
    pub onMouseDown:  Signal<MouseEvent>
    pub onMouseUp:    Signal<MouseEvent>
    pub onContextMenu: Signal<MouseEvent>
    pub onKeyDown:    Signal<KeyEvent>
    pub onKeyUp:      Signal<KeyEvent>
    pub onMouseWheel: Signal<MouseEvent>

    pub init() {
        super()
        this.handle     = -1
        this._text      = ""
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 160.0
        this._height    = 24.0
        this._isEnabled = true
        this._isVisible = true
        this._tooltip   = ""
        this._placeholder = ""
        this._isReadOnly = false
        this._maxLength  = 0 as i32
        this.onChange   = new Signal<ChangeEvent>()
        this.onFocus    = new Signal<FocusEvent>()
        this.onBlur     = new Signal<BlurEvent>()
        this.onMouseEnter = new Signal<MouseEvent>()
        this.onMouseLeave = new Signal<MouseEvent>()
        this.onMouseDown  = new Signal<MouseEvent>()
        this.onMouseUp    = new Signal<MouseEvent>()
        this.onContextMenu = new Signal<MouseEvent>()
        this.onKeyDown    = new Signal<KeyEvent>()
        this.onKeyUp      = new Signal<KeyEvent>()
        this.onMouseWheel = new Signal<MouseEvent>()
    }

    pub get text(): string {
        // Once attached, the native control is the source of truth
        // — the user may have typed since the last setter call.
        if this.handle >= 0 {
            this._text = readEditText(this.handle)
        }
        this._text
    }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setEditText(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setEditEnabled(this.handle, v) }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setEditVisible(this.handle, v) }
    }

    /// Hover tooltip. Empty string clears it.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setEditTooltip(this.handle, v) }
    }

    /// Placeholder / cue-banner text shown when the field is
    /// empty. Empty string clears it. macOS uses
    /// `NSTextField.placeholderString`; Win32 sends
    /// `EM_SETCUEBANNER`; GTK calls `gtk_entry_set_placeholder_text`.
    pub get placeholder(): string { this._placeholder }
    pub set placeholder(v: string) {
        this._placeholder = v
        if this.handle >= 0 { setEditPlaceholder(this.handle, v) }
    }

    /// Read-only mode. The field still focuses and copies text;
    /// only typed input is suppressed. Distinct from
    /// `isEnabled = false`, which also greys the field out.
    pub get isReadOnly(): bool { this._isReadOnly }
    pub set isReadOnly(v: bool) {
        this._isReadOnly = v
        if this.handle >= 0 { setEditReadOnly(this.handle, v) }
    }

    /// Maximum number of characters the user can type. `0` = no
    /// limit (the default). Pasting and programmatic writes can
    /// still exceed the cap on macOS — the limit is enforced only
    /// for keyboard input; Win32 / GTK honour it for paste too.
    pub get maxLength(): i32 { this._maxLength }
    pub set maxLength(v: i32) {
        this._maxLength = v
        if this.handle >= 0 { setEditMaxLength(this.handle, v) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeEdit(
            window.handle,
            this._text, this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnEditChange(this.handle, fn(v: string) {
            this._text = v
            this.onChange.emit(new ChangeEvent(this, v))
        })
        setOnEditFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnEditBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnEditMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnEditMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnEditMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnEditMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnEditContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnEditKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnEditKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnEditMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        if this._tooltip != "" { setEditTooltip(this.handle, this._tooltip) }
        if this._placeholder != "" { setEditPlaceholder(this.handle, this._placeholder) }
        if this._isReadOnly { setEditReadOnly(this.handle, true) }
        if this._maxLength as i64 > 0 as i64 { setEditMaxLength(this.handle, this._maxLength) }
    }

    /// Move keyboard focus to this text field. The native
    /// transition fires `onFocus` here and `onBlur` on whichever
    /// control previously held focus. No-op until attached.
    pub focus() {
        if this.handle >= 0 { focusEdit(this.handle) }
    }

    /// Release focus iff this field currently holds it (web
    /// `HTMLElement.blur()` semantics). No-op until attached.
    pub blur() {
        if this.handle >= 0 { blurEdit(this.handle) }
    }

    /// Select the entire contents. Grabs focus on macOS (so the
    /// field editor is installed and the selection has somewhere
    /// to live). No-op until the field is attached.
    pub selectAll() {
        if this.handle >= 0 { selectAllEdit(this.handle) }
    }

    /// Set the selection range. `start` / `end` are character
    /// indices into the UTF-16 form of the text (matching the
    /// HTML / DOM convention each backend's native API also
    /// uses). `start == end` collapses the selection to a caret.
    /// Grabs focus on macOS so the field editor exists. No-op
    /// until the field is attached.
    pub setSelection(start: i64, end: i64) {
        if this.handle >= 0 { setEditSelection(this.handle, start, end) }
    }

    /// Current selection start. `0` when the field isn't
    /// focused and no field editor exists yet (macOS) — the
    /// browser DOM exposes the same reset-on-blur behaviour.
    pub get selectionStart(): i64 {
        if this.handle >= 0 { getEditSelectionStart(this.handle) } else { 0 as i64 }
    }
    pub get selectionEnd(): i64 {
        if this.handle >= 0 { getEditSelectionEnd(this.handle) } else { 0 as i64 }
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
