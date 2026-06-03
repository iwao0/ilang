/// Drop-down list of pre-populated string items. Win32: `COMBOBOX`
/// with `CBS_DROPDOWNLIST` (read-only); Cocoa: `NSPopUpButton`.
/// `onSelect` fires on each selection change.
///
/// Items added with `addItem` before `addChild` are pushed to the
/// native control on realization; calls after `addChild` propagate
/// immediately.

use std.events { Signal }
use events { SelectEvent, FocusEvent, BlurEvent, MouseEvent }
use window { Window, Control }
use gui_impl {
    createNativeCombo, readComboSelectedIndex,
    setOnComboSelect, addComboItem, clearComboItems,
    setComboSelectedIndex, setComboFrame,
    setComboEnabled, setComboVisible,
    setComboTooltip,
    setOnComboFocus, setOnComboBlur,
    setOnComboMouseEnter, setOnComboMouseLeave,
    setOnComboMouseDown, setOnComboMouseUp, setOnComboContextMenu,
    setOnComboMouseWheel,
    focusCombo, blurCombo,
}

pub class ComboBox : Control {
    handle: i64
    _items:     string[]
    _selected:  i32
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string
    pub onSelect: Signal<SelectEvent>
    pub onFocus:  Signal<FocusEvent>
    pub onBlur:   Signal<BlurEvent>
    pub onMouseEnter:  Signal<MouseEvent>
    pub onMouseLeave:  Signal<MouseEvent>
    pub onMouseDown:   Signal<MouseEvent>
    pub onMouseUp:     Signal<MouseEvent>
    pub onContextMenu: Signal<MouseEvent>
    pub onMouseWheel:  Signal<MouseEvent>

    pub init() {
        super()
        this.handle     = -1
        this._items     = []
        this._selected  = -1
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 160.0
        this._height    = 26.0
        this._isEnabled = true
        this._isVisible = true
        this._tooltip   = ""
        this.onSelect   = new Signal<SelectEvent>()
        this.onFocus    = new Signal<FocusEvent>()
        this.onBlur     = new Signal<BlurEvent>()
        this.onMouseEnter  = new Signal<MouseEvent>()
        this.onMouseLeave  = new Signal<MouseEvent>()
        this.onMouseDown   = new Signal<MouseEvent>()
        this.onMouseUp     = new Signal<MouseEvent>()
        this.onContextMenu = new Signal<MouseEvent>()
        this.onMouseWheel  = new Signal<MouseEvent>()
    }

    /// Append `item` to the dropdown.
    pub addItem(item: string) {
        this._items.push(item)
        if this.handle >= 0 { addComboItem(this.handle, item) }
    }
    /// Drop every item.
    pub clear() {
        this._items = []
        this._selected = -1
        if this.handle >= 0 { clearComboItems(this.handle) }
    }
    /// 0-based index of the currently selected item, or `-1` when
    /// nothing is selected.
    pub get selectedIndex(): i32 {
        if this.handle >= 0 { this._selected = readComboSelectedIndex(this.handle) }
        this._selected
    }
    pub set selectedIndex(v: i32) {
        this._selected = v
        if this.handle >= 0 { setComboSelectedIndex(this.handle, v) }
    }
    pub get selectedValue(): string {
        let i = this.selectedIndex
        if i < 0 || (i as i64) >= this._items.length { return "" }
        this._items[i as i64]
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setComboFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setComboFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setComboFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setComboFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setComboEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setComboVisible(this.handle, v) }
    }

    /// Hover tooltip. Empty string clears it.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setComboTooltip(this.handle, v) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeCombo(
            window.handle,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        // Push the cached items / selection.
        let n = this._items.length
        let i: i64 = 0
        while i < n {
            addComboItem(this.handle, this._items[i])
            i = i + 1
        }
        if this._selected >= 0 {
            setComboSelectedIndex(this.handle, this._selected)
        }
        setOnComboSelect(this.handle, fn(idx: i32) {
            this._selected = idx
            let v = if idx >= 0 && (idx as i64) < this._items.length {
                this._items[idx as i64]
            } else {
                ""
            }
            this.onSelect.emit(new SelectEvent(this, idx, v))
        })
        setOnComboFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnComboBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnComboMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnComboMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnComboMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnComboMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnComboContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnComboMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        if this._tooltip != "" { setComboTooltip(this.handle, this._tooltip) }
    }

    pub focus() { if this.handle >= 0 { focusCombo(this.handle) } }
    pub blur()  { if this.handle >= 0 { blurCombo(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setComboFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
