/// Scrollable single-column list of string items. Win32:
/// `LISTBOX`; Cocoa: `NSTableView` (single column) inside an
/// `NSScrollView`. `onSelect` fires whenever the highlighted row
/// changes.

use std.events { Signal }
use events { SelectEvent, FocusEvent, BlurEvent, MouseEvent, KeyEvent }
use window { Window, Control, Container }
use gui_impl {
    createNativeList, readListSelectedIndex,
    setOnListSelect, addListItem, clearListItems,
    setListSelectedIndex, setListFrame,
    setListEnabled, setListVisible,
    setListTooltip,
    setOnListFocus, setOnListBlur,
    setOnListMouseEnter, setOnListMouseLeave,
    setOnListMouseDown, setOnListMouseUp, setOnListContextMenu,
    setOnListMouseWheel,
    setOnListKeyDown, setOnListKeyUp,
    focusList, blurList,
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
    pub onKeyDown:     Signal<KeyEvent>
    pub onKeyUp:       Signal<KeyEvent>

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
        this.onKeyDown     = new Signal<KeyEvent>()
        this.onKeyUp       = new Signal<KeyEvent>()
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

    /// Hover tooltip. Empty string clears it.
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setListTooltip(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeList(
            parent.nativeChildHandle(),
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
        setOnListFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnListBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnListMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnListMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnListMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnListMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnListContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnListMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        setOnListKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnListKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        if this._tooltip != "" { setListTooltip(this.handle, this._tooltip) }
    }

    pub focus() { if this.handle >= 0 { focusList(this.handle) } }
    pub blur()  { if this.handle >= 0 { blurList(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setListFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
