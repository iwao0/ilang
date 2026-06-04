/// Multi-column list. Win32: `SysListView32` in
/// `LVS_REPORT` mode; Cocoa: `NSTableView` with one
/// `NSTableColumn` per logical column, view-based cells; Linux:
/// `GtkColumnView` over a `GListStore` driven by a per-column
/// `GtkSignalListItemFactory`.
///
/// Pre-declare columns once (title + width) and append rows as
/// `string[]` of length = column count:
///
///     let t = new Table()
///     t.addColumn("Name", 160.0)
///     t.addColumn("Size", 80.0)
///     t.addRow(["foo.txt", "12 KB"])
///     t.onSelect.add(fn(e) { console.log(e.row, e.rowData) })
///     t.onActivate.add(fn(e) { console.log("opened", e.row) })
///     window.add(t)
///
/// `onSelect` fires whenever the highlighted row changes;
/// `onActivate` fires on a double-click (macOS / Win32) or the
/// `activate` signal (GTK Enter / double-click).

use std.events { Signal }
use events {
    TableSelectEvent, TableActivateEvent,
    FocusEvent, BlurEvent, MouseEvent, KeyEvent,
}
use window { Window, Control, Container }
use gui_impl {
    createNativeTable, tableAddColumn, tableAddRow,
    clearTableRows, readTableSelectedRow, setTableSelectedRow,
    setTableFrame, setTableEnabled, setTableVisible, setTableTooltip,
    setOnTableSelect, setOnTableActivate,
    setOnTableFocus, setOnTableBlur,
    setOnTableMouseEnter, setOnTableMouseLeave,
    setOnTableMouseDown, setOnTableMouseUp, setOnTableContextMenu,
    setOnTableMouseWheel,
    setOnTableKeyDown, setOnTableKeyUp,
    focusTable, blurTable,
}

pub class Table : Control {
    handle: i64
    _columns:    string[]
    _colWidths:  f64[]
    _rows:       string[][]
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string
    pub onSelect:   Signal<TableSelectEvent>
    pub onActivate: Signal<TableActivateEvent>
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
        this._columns   = []
        this._colWidths = []
        this._rows      = []
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 320.0
        this._height    = 200.0
        this._isEnabled = true
        this._isVisible = true
        this._tooltip   = ""
        this.onSelect   = new Signal<TableSelectEvent>()
        this.onActivate = new Signal<TableActivateEvent>()
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

    /// Add a column with `title` shown in the header row and an
    /// initial pixel `width`. Columns must be declared before
    /// `attachTo`; adding columns post-attach is a Round 2
    /// feature.
    pub addColumn(title: string, width: f64) {
        this._columns.push(title)
        this._colWidths.push(width)
        if this.handle >= 0 {
            tableAddColumn(this.handle, title, width)
        }
    }

    /// Append a row. `cells` should have as many entries as
    /// columns declared; extra entries are ignored, missing
    /// entries render as empty strings.
    pub addRow(cells: string[]) {
        this._rows.push(cells)
        if this.handle >= 0 {
            tableAddRow(this.handle, cells)
        }
    }

    pub clear() {
        this._rows = []
        if this.handle >= 0 { clearTableRows(this.handle) }
    }

    pub get columnCount(): i64 { this._columns.length }
    pub get rowCount():    i64 { this._rows.length }

    pub get selectedRow(): i32 {
        if this.handle >= 0 { readTableSelectedRow(this.handle) } else { -1 as i32 }
    }
    pub set selectedRow(v: i32) {
        if this.handle >= 0 { setTableSelectedRow(this.handle, v) }
    }
    /// Read the highlighted row's cells. Empty array when no
    /// row is selected.
    pub get selectedRowData(): string[] {
        let r = this.selectedRow
        if r < 0 as i32 || (r as i64) >= this._rows.length {
            return []
        }
        this._rows[r as i64]
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setTableFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setTableFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setTableFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setTableFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setTableEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setTableVisible(this.handle, v) }
    }
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setTableTooltip(this.handle, v) }
    }

    _rowDataAt(r: i32): string[] {
        if r < 0 as i32 || (r as i64) >= this._rows.length {
            let empty: string[] = []
            empty
        } else {
            this._rows[r as i64]
        }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeTable(
            parent.nativeChildHandle(),
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        let nc = this._columns.length
        let i: i64 = 0
        while i < nc {
            tableAddColumn(this.handle, this._columns[i], this._colWidths[i])
            i = i + 1
        }
        let nr = this._rows.length
        let j: i64 = 0
        while j < nr {
            tableAddRow(this.handle, this._rows[j])
            j = j + 1
        }
        setOnTableSelect(this.handle, fn(row: i32) {
            this.onSelect.emit(new TableSelectEvent(this, row, this._rowDataAt(row)))
        })
        setOnTableActivate(this.handle, fn(row: i32) {
            this.onActivate.emit(new TableActivateEvent(this, row, this._rowDataAt(row)))
        })
        setOnTableFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnTableBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnTableMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnTableMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnTableMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnTableMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnTableContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnTableMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        setOnTableKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnTableKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        if this._tooltip != "" { setTableTooltip(this.handle, this._tooltip) }
    }

    pub focus() { if this.handle >= 0 { focusTable(this.handle) } }
    pub blur()  { if this.handle >= 0 { blurTable(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setTableFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
