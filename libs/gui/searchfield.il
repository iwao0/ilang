/// Single-line search input — text field specialised for
/// "filter as you type" interactions. Cocoa: `NSSearchField`
/// (magnifying glass + cancel button); Linux: `GtkSearchEntry`
/// (search-changed / activate / stop-search signals); Win32:
/// custom-drawn `IlangGuiSearchField` (no native equivalent).
///
///     let s = new gui.SearchField()
///     s.placeholder = "Search…"
///     s.onChange.add(fn(e: gui.ChangeEvent) {
///         refilter(e.value)
///     })
///     s.onSearch.add(fn(e: gui.ChangeEvent) {
///         submit(e.value)            // Enter pressed
///     })
///     s.onClear.add(fn(e: gui.ChangeEvent) {
///         refilter("")               // cancel button clicked
///     })
///     window.addChild(s)
///
/// `onChange` fires on every keystroke (DOM `oninput`-like).
/// `onSearch` fires when the user presses Return / Enter.
/// `onClear` fires when the user activates the cancel button
/// (Cocoa: clicks the ✕; Linux: presses Esc / "stop-search";
/// Win32: clicks the custom-drawn ✕).

use std.events { Signal }
use events { ChangeEvent, FocusEvent, BlurEvent, KeyEvent }
use window { Window, Control, Container }
use gui_impl {
    createNativeSearchField, readSearchFieldText, setSearchFieldText,
    setSearchFieldPlaceholder,
    setSearchFieldFrame, setSearchFieldEnabled, setSearchFieldVisible,
    setSearchFieldTooltip,
    setOnSearchFieldChange, setOnSearchFieldSearch, setOnSearchFieldClear,
    setOnSearchFieldFocus, setOnSearchFieldBlur,
    setOnSearchFieldKeyDown, setOnSearchFieldKeyUp,
    focusSearchField, blurSearchField,
}

pub class SearchField : Control {
    handle: i64
    _text:        string
    _placeholder: string
    _x:           f64
    _y:           f64
    _width:       f64
    _height:      f64
    _isEnabled:   bool
    _isVisible:   bool
    _tooltip:     string
    pub onChange: Signal<ChangeEvent>
    pub onSearch: Signal<ChangeEvent>
    pub onClear:  Signal<ChangeEvent>
    pub onFocus:  Signal<FocusEvent>
    pub onBlur:   Signal<BlurEvent>
    pub onKeyDown: Signal<KeyEvent>
    pub onKeyUp:   Signal<KeyEvent>

    pub init() {
        super()
        this.handle      = -1
        this._text       = ""
        this._placeholder = ""
        this._x          = 0.0
        this._y          = 0.0
        this._width      = 200.0
        this._height     = 24.0
        this._isEnabled  = true
        this._isVisible  = true
        this._tooltip    = ""
        this.onChange    = new Signal<ChangeEvent>()
        this.onSearch    = new Signal<ChangeEvent>()
        this.onClear     = new Signal<ChangeEvent>()
        this.onFocus     = new Signal<FocusEvent>()
        this.onBlur      = new Signal<BlurEvent>()
        this.onKeyDown   = new Signal<KeyEvent>()
        this.onKeyUp     = new Signal<KeyEvent>()
    }

    pub get text(): string {
        if this.handle >= 0 { this._text = readSearchFieldText(this.handle) }
        this._text
    }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setSearchFieldText(this.handle, v) }
    }

    /// Greyed-out hint shown while the field is empty.
    pub get placeholder(): string { this._placeholder }
    pub set placeholder(v: string) {
        this._placeholder = v
        if this.handle >= 0 { setSearchFieldPlaceholder(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setSearchFieldFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setSearchFieldFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setSearchFieldFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setSearchFieldFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setSearchFieldEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setSearchFieldVisible(this.handle, v) }
    }

    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setSearchFieldTooltip(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeSearchField(
            parent.nativeChildHandle(),
            this._text, this._placeholder,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnSearchFieldChange(this.handle, fn(v: string) {
            this._text = v
            this.onChange.emit(new ChangeEvent(this, v))
        })
        setOnSearchFieldSearch(this.handle, fn(v: string) {
            this._text = v
            this.onSearch.emit(new ChangeEvent(this, v))
        })
        setOnSearchFieldClear(this.handle, fn() {
            this._text = ""
            this.onClear.emit(new ChangeEvent(this, ""))
        })
        setOnSearchFieldFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnSearchFieldBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnSearchFieldKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnSearchFieldKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        if this._tooltip != "" { setSearchFieldTooltip(this.handle, this._tooltip) }
    }

    pub focus() { if this.handle >= 0 { focusSearchField(this.handle) } }
    pub blur()  { if this.handle >= 0 { blurSearchField(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setSearchFieldFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
