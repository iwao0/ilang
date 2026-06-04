/// Time-of-day picker — `NSDatePicker` with the time elements
/// (`hourMinute` or `hourMinuteSecond`) in `clockAndCalendar`
/// style. The view renders the analog clock face only; the
/// date half is suppressed by leaving the date element flags
/// unset.

use cocoa {
    NSObject, NSView,
    NSDate, NSCalendar, NSCalendarUnit, NSDateComponents,
    NSDatePicker, NSDatePickerStyle, NSDatePickerElementFlags,
    makeRect, nsstr, nil, selectorPtr,
}
use core { resolveParentView, noOp1 }

class TimePickerSlot {
    pub picker:  NSDatePicker
    pub showSeconds: bool
    pub onChange: fn(i32, i32, i32)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub onKeyDown: fn(string, bool, bool, bool, bool)
    pub onKeyUp:   fn(string, bool, bool, bool, bool)
    pub handler:  NSObject
    pub init(picker: NSDatePicker, showSeconds: bool, handler: NSObject) {
        this.picker = picker
        this.showSeconds = showSeconds
        this.onChange = tpNoOpHms
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onMouseEnter = tpNoOpEnter
        this.onMouseLeave = tpNoOpEnter
        this.onMouseDown  = tpNoOpClick
        this.onMouseUp    = tpNoOpClick
        this.onContextMenu = tpNoOpClick
        this.onMouseWheel = tpNoOpWheel
        this.onKeyDown = tpNoOpKey
        this.onKeyUp   = tpNoOpKey
        this.handler = handler
    }
}

fn tpNoOpHms(h: i32, m: i32, s: i32) {}
fn tpNoOpEnter(x: f64, y: f64) {}
fn tpNoOpClick(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn tpNoOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn tpNoOpKey(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {}

let TIME_PICKERS = new Map<i64, TimePickerSlot>()
let TP_HANDLER_TO_PICKER: Map<i64, i64> = new Map<i64, i64>()

class TimePickerHandler : NSObject {
    @objc("timePicked:")
    pub timePicked(sender: NSObject) {
        if let some(host) = TP_HANDLER_TO_PICKER.get(this.handle) {
            if let some(slot) = TIME_PICKERS.get(host) {
                let parts = breakTime(slot.picker.dateValue)
                let cb = slot.onChange
                cb(parts[0] as i32, parts[1] as i32, parts[2] as i32)
            }
        }
    }
}

fn hmsUnits(): NSCalendarUnit {
    NSCalendarUnit.hour | NSCalendarUnit.minute | NSCalendarUnit.second
}

fn breakTime(d: NSDate): i64[] {
    let cal = NSCalendar.currentCalendar()
    let c = cal.componentsFromDate(hmsUnits(), d)
    [c.hour, c.minute, c.second]
}

fn makeTime(hour: i32, minute: i32, second: i32): NSDate {
    let comps = NSDateComponents.alloc().init()
    // Anchor to "today" in the user's calendar so the picker
    // has a real date to back the time of day.
    let cal = NSCalendar.currentCalendar()
    let today = cal.componentsFromDate(
        NSCalendarUnit.year | NSCalendarUnit.month | NSCalendarUnit.day,
        NSDate.now()
    )
    comps.year   = today.year
    comps.month  = today.month
    comps.day    = today.day
    comps.hour   = hour as i64
    comps.minute = minute as i64
    comps.second = second as i64
    cal.dateFromComponents(comps)
}

@extern(C) {

pub fn createNativeTimePicker(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool,
    showSeconds: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let picker = NSDatePicker.alloc().init(makeRect(x, y, width, height))
    picker.setDatePickerStyle(NSDatePickerStyle.clockAndCalendar)
    let elements = if showSeconds {
        NSDatePickerElementFlags.hourMinuteSecond
    } else {
        NSDatePickerElementFlags.hourMinute
    }
    picker.setDatePickerElements(elements)
    picker.setEnabled(enabled)
    (picker as NSView).setHidden(!visible)

    TimePickerHandler.register()
    let h = TimePickerHandler.alloc().init()
    picker.setTarget(h as NSObject)
    picker.setAction(selectorPtr("timePicked:"))
    picker.setContinuous(false)

    parent.addSubview(picker as NSView)
    let host = (picker as NSView).handle
    TIME_PICKERS.set(host, new TimePickerSlot(picker, showSeconds, h as NSObject))
    TP_HANDLER_TO_PICKER.set((h as NSObject).handle, host)
    host
}

pub fn setTimePickerValue(host: i64, hour: i32, minute: i32, second: i32) {
    if let some(slot) = TIME_PICKERS.get(host) {
        slot.picker.dateValue = makeTime(hour, minute, second)
    }
}

pub fn readTimePickerHour(host: i64): i32 {
    match TIME_PICKERS.get(host) {
        some(slot) { breakTime(slot.picker.dateValue)[0] as i32 }
        none       { 0 as i32 }
    }
}
pub fn readTimePickerMinute(host: i64): i32 {
    match TIME_PICKERS.get(host) {
        some(slot) { breakTime(slot.picker.dateValue)[1] as i32 }
        none       { 0 as i32 }
    }
}
pub fn readTimePickerSecond(host: i64): i32 {
    match TIME_PICKERS.get(host) {
        some(slot) { breakTime(slot.picker.dateValue)[2] as i32 }
        none       { 0 as i32 }
    }
}

pub fn setTimePickerFrame(host: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TIME_PICKERS.get(host) {
        (slot.picker as NSView).setFrame(makeRect(x, y, width, height))
    }
}
pub fn setTimePickerEnabled(host: i64, enabled: bool) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.picker.setEnabled(enabled) }
}
pub fn setTimePickerVisible(host: i64, visible: bool) {
    if let some(slot) = TIME_PICKERS.get(host) { (slot.picker as NSView).setHidden(!visible) }
}
pub fn setTimePickerTooltip(host: i64, text: string) {
    if let some(slot) = TIME_PICKERS.get(host) {
        if text == "" {
            (slot.picker as NSView).setToolTip(nil)
        } else {
            (slot.picker as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}

pub fn setOnTimePickerChange(host: i64, cb: fn(i32, i32, i32)) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onChange = cb }
}
pub fn setOnTimePickerFocus(host: i64, cb: fn()) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onFocus = cb }
}
pub fn setOnTimePickerBlur(host: i64, cb: fn()) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onBlur = cb }
}
pub fn setOnTimePickerMouseEnter(host: i64, cb: fn(f64, f64)) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onMouseEnter = cb }
}
pub fn setOnTimePickerMouseLeave(host: i64, cb: fn(f64, f64)) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onMouseLeave = cb }
}
pub fn setOnTimePickerMouseDown(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { if let some(slot) = TIME_PICKERS.get(host) { slot.onMouseDown = cb } }
pub fn setOnTimePickerMouseUp(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { if let some(slot) = TIME_PICKERS.get(host) { slot.onMouseUp = cb } }
pub fn setOnTimePickerContextMenu(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { if let some(slot) = TIME_PICKERS.get(host) { slot.onContextMenu = cb } }
pub fn setOnTimePickerMouseWheel(
    host: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) { if let some(slot) = TIME_PICKERS.get(host) { slot.onMouseWheel = cb } }
pub fn setOnTimePickerKeyDown(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onKeyDown = cb }
}
pub fn setOnTimePickerKeyUp(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = TIME_PICKERS.get(host) { slot.onKeyUp = cb }
}

pub fn focusTimePicker(host: i64) {
    if let some(slot) = TIME_PICKERS.get(host) {
        let win = (slot.picker as NSView).window
        let _ = win.makeFirstResponder(slot.picker as NSObject)
    }
}
pub fn blurTimePicker(host: i64) {
    if let some(slot) = TIME_PICKERS.get(host) {
        let win = (slot.picker as NSView).window
        if win.firstResponder.handle == (slot.picker as NSView).handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}

} // @extern(C)
