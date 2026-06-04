/// Time-of-day picker. Cocoa: `NSDatePicker` with `hourMinute`
/// (or `hourMinuteSecond` when `showSeconds` is true) in
/// `clockAndCalendar` style — the time half renders as the
/// analog clock face; Linux: three `GtkSpinButton`s
/// (hour / minute / [second]) laid out in a row; Win32:
/// `SysDateTimePick32` in `DTS_TIMEFORMAT` mode.
///
/// `value` is a `std.time.DateTime` — only the `hour`,
/// `minute`, and `second` fields are honoured; the date half
/// reads back as the host's "today".

use std.events { Signal }
use std.time { DateTime }
use events {
    TimeChangeEvent,
    FocusEvent, BlurEvent, MouseEvent, KeyEvent,
}
use window { Window, Control, Container }
use gui_impl {
    createNativeTimePicker,
    setTimePickerValue, readTimePickerHour,
    readTimePickerMinute, readTimePickerSecond,
    setTimePickerFrame, setTimePickerEnabled,
    setTimePickerVisible, setTimePickerTooltip,
    setOnTimePickerChange,
    setOnTimePickerFocus, setOnTimePickerBlur,
    setOnTimePickerMouseEnter, setOnTimePickerMouseLeave,
    setOnTimePickerMouseDown, setOnTimePickerMouseUp,
    setOnTimePickerContextMenu, setOnTimePickerMouseWheel,
    setOnTimePickerKeyDown, setOnTimePickerKeyUp,
    focusTimePicker, blurTimePicker,
}

pub class TimePicker : Control {
    handle: i64
    _hour:   i32
    _minute: i32
    _second: i32
    _showSeconds: bool
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string
    pub onChange: Signal<TimeChangeEvent>
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
        this.handle = -1
        let now = DateTime.now()
        this._hour   = now.hour
        this._minute = now.minute
        this._second = now.second
        this._showSeconds = false
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 160.0
        this._height    = 160.0
        this._isEnabled = true
        this._isVisible = true
        this._tooltip   = ""
        this.onChange   = new Signal<TimeChangeEvent>()
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

    /// Whether the seconds field is shown / editable. Honoured
    /// at `attachTo`; mutating after attach is a Round 2
    /// feature.
    pub get showSeconds(): bool { this._showSeconds }
    pub set showSeconds(v: bool) { this._showSeconds = v }

    pub get value(): DateTime {
        if this.handle >= 0 {
            this._hour   = readTimePickerHour(this.handle)
            this._minute = readTimePickerMinute(this.handle)
            this._second = readTimePickerSecond(this.handle)
        }
        let today = DateTime.now()
        new DateTime(
            today.year, today.month, today.day,
            this._hour, this._minute, this._second,
            0 as i32, today.weekday, today.offsetMin
        )
    }
    pub set value(v: DateTime) {
        this._hour = v.hour; this._minute = v.minute; this._second = v.second
        if this.handle >= 0 {
            setTimePickerValue(this.handle, this._hour, this._minute, this._second)
        }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setTimePickerFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setTimePickerFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setTimePickerFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setTimePickerFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setTimePickerEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setTimePickerVisible(this.handle, v) }
    }
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setTimePickerTooltip(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeTimePicker(
            parent.nativeChildHandle(),
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible,
            this._showSeconds
        )
        setTimePickerValue(this.handle, this._hour, this._minute, this._second)

        setOnTimePickerChange(this.handle, fn(h: i32, m: i32, s: i32) {
            this._hour = h; this._minute = m; this._second = s
            this.onChange.emit(new TimeChangeEvent(this, h, m, s))
        })
        setOnTimePickerFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnTimePickerBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnTimePickerMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnTimePickerMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnTimePickerMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnTimePickerMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnTimePickerContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnTimePickerMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        setOnTimePickerKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnTimePickerKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        if this._tooltip != "" { setTimePickerTooltip(this.handle, this._tooltip) }
    }

    pub focus() { if this.handle >= 0 { focusTimePicker(this.handle) } }
    pub blur()  { if this.handle >= 0 { blurTimePicker(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setTimePickerFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
