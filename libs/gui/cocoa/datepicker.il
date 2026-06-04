/// Calendar date picker — `NSDatePicker` configured for
/// `yearMonthDay` elements in the `clockAndCalendar` style. The
/// public-side value lives in our own (year, month, day) ints;
/// we convert via `NSDateComponents` + `NSCalendar.dateFromComponents`
/// to drive `NSDatePicker.dateValue`, and back via
/// `componentsFromDate` on read-back.

use cocoa {
    NSObject, NSView,
    NSDate, NSCalendar, NSCalendarUnit, NSDateComponents,
    NSDatePicker, NSDatePickerStyle, NSDatePickerElementFlags,
    makeRect, nsstr, nil, selectorPtr,
}
use core { resolveParentView, noOp1 }

class DatePickerSlot {
    pub picker:  NSDatePicker
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
    pub init(picker: NSDatePicker, handler: NSObject) {
        this.picker = picker
        this.onChange = dpNoOpYmd
        this.onFocus  = noOp1
        this.onBlur   = noOp1
        this.onMouseEnter = dpNoOpEnter
        this.onMouseLeave = dpNoOpEnter
        this.onMouseDown  = dpNoOpClick
        this.onMouseUp    = dpNoOpClick
        this.onContextMenu = dpNoOpClick
        this.onMouseWheel = dpNoOpWheel
        this.onKeyDown = dpNoOpKey
        this.onKeyUp   = dpNoOpKey
        this.handler = handler
    }
}

fn dpNoOpYmd(y: i32, m: i32, d: i32) {}
fn dpNoOpEnter(x: f64, y: f64) {}
fn dpNoOpClick(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn dpNoOpWheel(dx: f64, dy: f64, x: f64, y: f64, ck: bool, sk: bool, ak: bool, mk: bool) {}
fn dpNoOpKey(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {}

let DATE_PICKERS = new Map<i64, DatePickerSlot>()
/// handler.handle → owning date picker handle.
let DP_HANDLER_TO_PICKER: Map<i64, i64> = new Map<i64, i64>()

class DatePickerHandler : NSObject {
    @objc("datePicked:")
    pub datePicked(sender: NSObject) {
        if let some(host) = DP_HANDLER_TO_PICKER.get(this.handle) {
            if let some(slot) = DATE_PICKERS.get(host) {
                let comps = breakDate(slot.picker.dateValue)
                let cb = slot.onChange
                cb(comps[0] as i32, comps[1] as i32, comps[2] as i32)
            }
        }
    }
}

/// NSCalendarUnit bits for year+month+day. The enum doesn't
/// surface a year/month/day combined constant; OR the three.
fn ymdUnits(): NSCalendarUnit {
    NSCalendarUnit.year | NSCalendarUnit.month | NSCalendarUnit.day
}

fn breakDate(d: NSDate): i64[] {
    let cal = NSCalendar.currentCalendar()
    let c = cal.componentsFromDate(ymdUnits(), d)
    [c.year, c.month, c.day]
}

fn makeDate(year: i32, month: i32, day: i32): NSDate {
    let comps = NSDateComponents.alloc().init()
    comps.year  = year as i64
    comps.month = month as i64
    comps.day   = day as i64
    let cal = NSCalendar.currentCalendar()
    cal.dateFromComponents(comps)
}

@extern(C) {

pub fn createNativeDatePicker(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let picker = NSDatePicker.alloc().init(makeRect(x, y, width, height))
    picker.setDatePickerStyle(NSDatePickerStyle.clockAndCalendar)
    picker.setDatePickerElements(NSDatePickerElementFlags.yearMonthDay)
    picker.setEnabled(enabled)
    (picker as NSView).setHidden(!visible)

    DatePickerHandler.register()
    let h = DatePickerHandler.alloc().init()
    picker.setTarget(h as NSObject)
    picker.setAction(selectorPtr("datePicked:"))
    picker.setContinuous(false)

    parent.addSubview(picker as NSView)
    let host = (picker as NSView).handle
    DATE_PICKERS.set(host, new DatePickerSlot(picker, h as NSObject))
    DP_HANDLER_TO_PICKER.set((h as NSObject).handle, host)
    host
}

pub fn setDatePickerValue(host: i64, year: i32, month: i32, day: i32) {
    if let some(slot) = DATE_PICKERS.get(host) {
        slot.picker.dateValue = makeDate(year, month, day)
    }
}

pub fn readDatePickerYear(host: i64): i32 {
    match DATE_PICKERS.get(host) {
        some(slot) { breakDate(slot.picker.dateValue)[0] as i32 }
        none       { 1970 as i32 }
    }
}
pub fn readDatePickerMonth(host: i64): i32 {
    match DATE_PICKERS.get(host) {
        some(slot) { breakDate(slot.picker.dateValue)[1] as i32 }
        none       { 1 as i32 }
    }
}
pub fn readDatePickerDay(host: i64): i32 {
    match DATE_PICKERS.get(host) {
        some(slot) { breakDate(slot.picker.dateValue)[2] as i32 }
        none       { 1 as i32 }
    }
}

pub fn setDatePickerMin(host: i64, year: i32, month: i32, day: i32) {
    if let some(slot) = DATE_PICKERS.get(host) {
        slot.picker.setMinDate(makeDate(year, month, day))
    }
}
pub fn setDatePickerMax(host: i64, year: i32, month: i32, day: i32) {
    if let some(slot) = DATE_PICKERS.get(host) {
        slot.picker.setMaxDate(makeDate(year, month, day))
    }
}

pub fn setDatePickerFrame(host: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = DATE_PICKERS.get(host) {
        (slot.picker as NSView).setFrame(makeRect(x, y, width, height))
    }
}
pub fn setDatePickerEnabled(host: i64, enabled: bool) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.picker.setEnabled(enabled) }
}
pub fn setDatePickerVisible(host: i64, visible: bool) {
    if let some(slot) = DATE_PICKERS.get(host) { (slot.picker as NSView).setHidden(!visible) }
}
pub fn setDatePickerTooltip(host: i64, text: string) {
    if let some(slot) = DATE_PICKERS.get(host) {
        if text == "" {
            (slot.picker as NSView).setToolTip(nil)
        } else {
            (slot.picker as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}

pub fn setOnDatePickerChange(host: i64, cb: fn(i32, i32, i32)) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onChange = cb }
}
pub fn setOnDatePickerFocus(host: i64, cb: fn()) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onFocus = cb }
}
pub fn setOnDatePickerBlur(host: i64, cb: fn()) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onBlur = cb }
}
pub fn setOnDatePickerMouseEnter(host: i64, cb: fn(f64, f64)) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onMouseEnter = cb }
}
pub fn setOnDatePickerMouseLeave(host: i64, cb: fn(f64, f64)) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onMouseLeave = cb }
}
pub fn setOnDatePickerMouseDown(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { if let some(slot) = DATE_PICKERS.get(host) { slot.onMouseDown = cb } }
pub fn setOnDatePickerMouseUp(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { if let some(slot) = DATE_PICKERS.get(host) { slot.onMouseUp = cb } }
pub fn setOnDatePickerContextMenu(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { if let some(slot) = DATE_PICKERS.get(host) { slot.onContextMenu = cb } }
pub fn setOnDatePickerMouseWheel(
    host: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) { if let some(slot) = DATE_PICKERS.get(host) { slot.onMouseWheel = cb } }
pub fn setOnDatePickerKeyDown(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onKeyDown = cb }
}
pub fn setOnDatePickerKeyUp(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = DATE_PICKERS.get(host) { slot.onKeyUp = cb }
}

pub fn focusDatePicker(host: i64) {
    if let some(slot) = DATE_PICKERS.get(host) {
        let win = (slot.picker as NSView).window
        let _ = win.makeFirstResponder(slot.picker as NSObject)
    }
}
pub fn blurDatePicker(host: i64) {
    if let some(slot) = DATE_PICKERS.get(host) {
        let win = (slot.picker as NSView).window
        if win.firstResponder.handle == (slot.picker as NSView).handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}

} // @extern(C)
