/// Row of mutually-exclusive segments. Cocoa: `NSSegmentedControl`
/// in `selectOne` tracking mode; Linux: a `GtkBox` row of
/// `GtkToggleButton`s grouped via `gtk_toggle_button_set_group` so
/// exactly one stays active; Win32: custom-drawn `IlangGuiSegmented`
/// window — Win32 has no native segmented widget.
///
///     let s = new gui.SegmentedControl()
///     s.x = 20.0; s.y = 20.0
///     s.width = 240.0; s.height = 26.0
///     s.addSegment("Day")
///     s.addSegment("Week")
///     s.addSegment("Month")
///     s.selectedIndex = 0
///     s.onChange.add(fn(e: gui.SelectEvent) {
///         console.log("picked", e.value, "@", e.index)
///     })
///     window.addChild(s)
///
/// `onChange` fires whenever the selection moves, regardless of
/// whether the user clicked or `selectedIndex` was set in code.

use std.events { Signal }
use events { SelectEvent }
use window { Window, Control, Container }
use gui_impl {
    createNativeSegmented, setSegmentedFrame,
    setSegmentedEnabled, setSegmentedVisible, setSegmentedTooltip,
    setOnSegmentedChange,
    segmentedAddSegment, segmentedClearSegments,
    segmentedSetTitle, segmentedGetTitle,
    segmentedGetSelectedIndex, segmentedSetSelectedIndex,
    segmentedGetCount,
}

pub class SegmentedControl : Control {
    handle: i64
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string
    // Titles cached on the ilang side so `addSegment` works
    // pre-attach. Flushed into the native control during
    // `attachTo`.
    _pendingTitles: string[]
    _pendingSelected: i32
    pub onChange: Signal<SelectEvent>

    pub init() {
        super()
        this.handle     = -1
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 240.0
        this._height    = 26.0
        this._isEnabled = true
        this._isVisible = true
        this._tooltip   = ""
        this._pendingTitles = []
        this._pendingSelected = -1 as i32
        this.onChange   = new Signal<SelectEvent>()
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setSegmentedFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setSegmentedFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setSegmentedFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setSegmentedFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setSegmentedEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setSegmentedVisible(this.handle, v) }
    }

    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setSegmentedTooltip(this.handle, v) }
    }

    /// Append a segment labelled `title`. Works both before and
    /// after attach.
    pub addSegment(title: string) {
        if this.handle >= 0 {
            segmentedAddSegment(this.handle, title)
        } else {
            this._pendingTitles.push(title)
        }
    }

    /// Drop every segment.
    pub clearSegments() {
        if this.handle >= 0 {
            segmentedClearSegments(this.handle)
        } else {
            this._pendingTitles = []
        }
    }

    pub setSegmentTitle(index: i32, title: string) {
        if this.handle >= 0 {
            segmentedSetTitle(this.handle, index, title)
        } elif (index as i64) < this._pendingTitles.length {
            this._pendingTitles[index as i64] = title
        }
    }

    pub getSegmentTitle(index: i32): string {
        if this.handle >= 0 {
            segmentedGetTitle(this.handle, index)
        } elif (index as i64) < this._pendingTitles.length {
            this._pendingTitles[index as i64]
        } else {
            ""
        }
    }

    pub get segmentCount(): i32 {
        if this.handle >= 0 {
            segmentedGetCount(this.handle)
        } else {
            this._pendingTitles.length as i32
        }
    }

    /// Currently-selected 0-based segment index, or `-1` when no
    /// segment is selected.
    pub get selectedIndex(): i32 {
        if this.handle >= 0 {
            segmentedGetSelectedIndex(this.handle)
        } else {
            this._pendingSelected
        }
    }
    pub set selectedIndex(v: i32) {
        if this.handle >= 0 {
            segmentedSetSelectedIndex(this.handle, v)
        } else {
            this._pendingSelected = v
        }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeSegmented(
            parent.nativeChildHandle(),
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        let i = 0 as i64
        let n = this._pendingTitles.length
        while i < n {
            segmentedAddSegment(this.handle, this._pendingTitles[i])
            i = i + 1
        }
        if this._pendingSelected >= 0 as i32 {
            segmentedSetSelectedIndex(this.handle, this._pendingSelected)
        }
        setOnSegmentedChange(this.handle, fn(index: i32) {
            let title = if index >= 0 as i32 {
                segmentedGetTitle(this.handle, index)
            } else {
                ""
            }
            this.onChange.emit(new SelectEvent(this, index, title))
        })
        if this._tooltip != "" { setSegmentedTooltip(this.handle, this._tooltip) }
    }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setSegmentedFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
