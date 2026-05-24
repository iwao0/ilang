/// Drop-down list of pre-populated string items. Win32: `COMBOBOX`
/// with `CBS_DROPDOWNLIST` (read-only); Cocoa: `NSPopUpButton`.
/// `onSelect` fires on each selection change.
///
/// Items added with `addItem` before `addChild` are pushed to the
/// native control on realization; calls after `addChild` propagate
/// immediately.

use std.events { Signal }
use event { SelectEvent }
use window { Window, Control }
use gui_impl {
    createNativeCombo, readComboSelectedIndex,
    setOnComboSelect, addComboItem, clearComboItems,
    setComboSelectedIndex, setComboFrame,
    setComboEnabled, setComboVisible,
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
    pub onSelect: Signal<SelectEvent>

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
        this.onSelect   = new Signal<SelectEvent>()
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
    }
}
