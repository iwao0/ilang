/// Calendar date picker. Cocoa: `NSDatePicker` in
/// `clockAndCalendar` style with `yearMonthDay` elements; Linux:
/// `GtkCalendar`; Win32: `SysDateTimePick32` in
/// `DTS_SHORTDATEFORMAT` mode.
///
/// `value`, `min`, and `max` use `std.time.DateTime`. Only the
/// `year` / `month` / `day` fields are honoured — the
/// time-of-day fields are zero on read-back. Use `TimePicker`
/// for hour / minute / second.

use std.events { Signal }
use std.time { DateTime }
use events {
    DateChangeEvent,
    FocusEvent, BlurEvent, MouseEvent, KeyEvent,
}
use window { Window, Control, Container }
use gui_impl {
    createNativeDatePicker,
    setDatePickerValue, readDatePickerYear,
    readDatePickerMonth, readDatePickerDay,
    setDatePickerMin, setDatePickerMax,
    setDatePickerFrame, setDatePickerEnabled,
    setDatePickerVisible, setDatePickerTooltip,
    setOnDatePickerChange,
    setOnDatePickerFocus, setOnDatePickerBlur,
    setOnDatePickerMouseEnter, setOnDatePickerMouseLeave,
    setOnDatePickerMouseDown, setOnDatePickerMouseUp,
    setOnDatePickerContextMenu, setOnDatePickerMouseWheel,
    setOnDatePickerKeyDown, setOnDatePickerKeyUp,
    focusDatePicker, blurDatePicker,
}

pub class DatePicker : Control {
    handle: i64
    _year:  i32
    _month: i32
    _day:   i32
    _minYear:  i32
    _minMonth: i32
    _minDay:   i32
    _maxYear:  i32
    _maxMonth: i32
    _maxDay:   i32
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string
    pub onChange: Signal<DateChangeEvent>
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
        this.handle    = -1
        // Default to today (host's local timezone).
        let now = DateTime.now()
        this._year  = now.year
        this._month = now.month
        this._day   = now.day
        this._minYear  = 1900 as i32
        this._minMonth = 1 as i32
        this._minDay   = 1 as i32
        this._maxYear  = 2100 as i32
        this._maxMonth = 12 as i32
        this._maxDay   = 31 as i32
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 220.0
        this._height    = 160.0
        this._isEnabled = true
        this._isVisible = true
        this._tooltip   = ""
        this.onChange   = new Signal<DateChangeEvent>()
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

    pub get value(): DateTime {
        if this.handle >= 0 {
            this._year  = readDatePickerYear(this.handle)
            this._month = readDatePickerMonth(this.handle)
            this._day   = readDatePickerDay(this.handle)
        }
        new DateTime(
            this._year, this._month, this._day,
            0 as i32, 0 as i32, 0 as i32, 0 as i32,
            0 as i32, 0 as i32
        )
    }
    pub set value(v: DateTime) {
        this._year = v.year; this._month = v.month; this._day = v.day
        if this.handle >= 0 {
            setDatePickerValue(this.handle, this._year, this._month, this._day)
        }
    }

    pub set min(v: DateTime) {
        this._minYear = v.year; this._minMonth = v.month; this._minDay = v.day
        if this.handle >= 0 {
            setDatePickerMin(this.handle, this._minYear, this._minMonth, this._minDay)
        }
    }
    pub set max(v: DateTime) {
        this._maxYear = v.year; this._maxMonth = v.month; this._maxDay = v.day
        if this.handle >= 0 {
            setDatePickerMax(this.handle, this._maxYear, this._maxMonth, this._maxDay)
        }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setDatePickerFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setDatePickerFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setDatePickerFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setDatePickerFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setDatePickerEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setDatePickerVisible(this.handle, v) }
    }
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setDatePickerTooltip(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeDatePicker(
            parent.nativeChildHandle(),
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setDatePickerMin(this.handle, this._minYear, this._minMonth, this._minDay)
        setDatePickerMax(this.handle, this._maxYear, this._maxMonth, this._maxDay)
        setDatePickerValue(this.handle, this._year, this._month, this._day)

        setOnDatePickerChange(this.handle, fn(y: i32, m: i32, d: i32) {
            this._year = y; this._month = m; this._day = d
            this.onChange.emit(new DateChangeEvent(this, y, m, d))
        })
        setOnDatePickerFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnDatePickerBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnDatePickerMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnDatePickerMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnDatePickerMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnDatePickerMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnDatePickerContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnDatePickerMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        setOnDatePickerKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnDatePickerKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        if this._tooltip != "" { setDatePickerTooltip(this.handle, this._tooltip) }
    }

    pub focus() { if this.handle >= 0 { focusDatePicker(this.handle) } }
    pub blur()  { if this.handle >= 0 { blurDatePicker(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setDatePickerFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }
}
