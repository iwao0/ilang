/// Tabbed selector — Win32: `SysTabControl32`; Cocoa: `NSTabView`;
/// GTK: `GtkNotebook`. Round 1 surfaces the tab strip only — the
/// selected tab's content area is the caller's responsibility.
/// Listen on `onChange` and flip your widgets' `isVisible` (or
/// move them off-screen) based on `selectedIndex`.
///
///   let tv = new gui.TabView()
///   tv.x = 10.0; tv.y = 10.0; tv.width = 460.0; tv.height = 30.0
///   let one = tv.addTab("One")
///   let two = tv.addTab("Two")
///   tv.onChange.add(fn(e: gui.TabChangeEvent) {
///       console.log("selected", e.index)
///   })
///   window.addChild(tv)

use std.events { Signal }
use events { TabChangeEvent }
use window { Window, Control, Container }
use gui_impl {
    createNativeTabView, setTabViewFrame,
    setTabViewEnabled, setTabViewVisible, setTabViewTooltip,
    setOnTabViewChange,
    tabViewAddTab, tabViewRemoveTab,
    tabViewGetSelectedIndex, tabViewSetSelectedIndex,
    tabViewGetTabCount,
    tabItemSetTitle, tabItemSetEnabled,
}

/// One row in a `TabView`. Returned by `addTab` so callers can
/// mutate the tab's label / enabled state without re-adding it.
pub class TabItem {
    /// Opaque native identifier. cocoa: NSTabViewItem ObjC ptr;
    /// win32: 0-based insert index at creation time; linux:
    /// GtkBox child handle.
    pub handle:     i64
    pub viewHandle: i64
    _title:     string
    _isEnabled: bool

    pub init(handle: i64, viewHandle: i64, title: string) {
        this.handle     = handle
        this.viewHandle = viewHandle
        this._title     = title
        this._isEnabled = true
    }

    pub get title(): string { this._title }
    pub set title(v: string) {
        this._title = v
        tabItemSetTitle(this.viewHandle, this.handle, v)
    }

    /// `false` greys the tab and blocks selection. cocoa / linux
    /// honour this; win32 grays the label but can't strictly
    /// block clicks on `SysTabControl32` (it sends TCN_SELCHANGE
    /// regardless — Round 2 may filter it in user space).
    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        tabItemSetEnabled(this.viewHandle, this.handle, v)
    }
}

pub class TabView : Control {
    handle: i64
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string
    pub onChange: Signal<TabChangeEvent>

    pub init() {
        super()
        this.handle     = -1
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 240.0
        this._height    = 28.0
        this._isEnabled = true
        this._isVisible = true
        this._tooltip   = ""
        this.onChange   = new Signal<TabChangeEvent>()
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setTabViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setTabViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setTabViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setTabViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setTabViewEnabled(this.handle, v) }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setTabViewVisible(this.handle, v) }
    }

    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setTabViewTooltip(this.handle, v) }
    }

    /// Append a tab labelled `title`. Returns the freshly-added
    /// `TabItem` so the caller can mutate its label / enabled
    /// state later. No-op until attached — call after `addChild`.
    pub addTab(title: string): TabItem {
        if this.handle < 0 { return new TabItem(0 as i64, 0 as i64, title) }
        let id = tabViewAddTab(this.handle, title)
        new TabItem(id, this.handle, title)
    }

    /// Remove the tab at `index`. No-op if `index` is out of range.
    pub removeTab(index: i32) {
        if this.handle >= 0 { tabViewRemoveTab(this.handle, index) }
    }

    /// Currently-selected 0-based tab index. `-1` when there are
    /// no tabs.
    pub get selectedIndex(): i32 {
        if this.handle >= 0 { tabViewGetSelectedIndex(this.handle) } else { -1 as i32 }
    }
    pub set selectedIndex(v: i32) {
        if this.handle >= 0 { tabViewSetSelectedIndex(this.handle, v) }
    }

    pub get tabCount(): i32 {
        if this.handle >= 0 { tabViewGetTabCount(this.handle) } else { 0 as i32 }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeTabView(
            parent.nativeChildHandle(),
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnTabViewChange(this.handle, fn(index: i32) {
            this.onChange.emit(new TabChangeEvent(this, index))
        })
        if this._tooltip != "" { setTabViewTooltip(this.handle, this._tooltip) }
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setTabViewFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
