/// Scrollable single-column list of string items. Win32:
/// `LISTBOX`; Cocoa: `NSTableView` (single column) inside an
/// `NSScrollView`. `onSelect` fires whenever the highlighted row
/// changes.

use std.events { Signal }
use events { SelectEvent }
use window { Window, Control }
use gui_impl {
    createNativeList, readListSelectedIndex,
    setOnListSelect, addListItem, clearListItems,
    setListSelectedIndex, setListFrame,
    setListEnabled, setListVisible,
}

pub class ListBox : Control {
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
        this._width     = 200.0
        this._height    = 120.0
        this._isEnabled = true
        this._isVisible = true
        this.onSelect   = new Signal<SelectEvent>()
    }

    pub addItem(item: string) {
        this._items.push(item)
        if this.handle >= 0 { addListItem(this.handle, item) }
    }
    pub clear() {
        this._items = []
        this._selected = -1
        if this.handle >= 0 { clearListItems(this.handle) }
    }
    pub get selectedIndex(): i32 {
        if this.handle >= 0 { this._selected = readListSelectedIndex(this.handle) }
        this._selected
    }
    pub set selectedIndex(v: i32) {
        this._selected = v
        if this.handle >= 0 { setListSelectedIndex(this.handle, v) }
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
            setListFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setListFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setListFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setListFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setListEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setListVisible(this.handle, v) }
    }

    pub override attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeList(
            window.handle,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        let n = this._items.length
        let i: i64 = 0
        while i < n {
            addListItem(this.handle, this._items[i])
            i = i + 1
        }
        if this._selected >= 0 {
            setListSelectedIndex(this.handle, this._selected)
        }
        setOnListSelect(this.handle, fn(idx: i32) {
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
