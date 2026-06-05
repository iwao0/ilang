/// Calendar date picker — `SysDateTimePick32` in
/// `DTS_SHORTDATEFORMAT` mode. The user clicks the dropdown
/// arrow to open the calendar popup; the chosen date arrives
/// as a `DTN_DATETIMECHANGE` notification carrying a
/// `SYSTEMTIME`.

use std.ffi { readI32, readU16 }
use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd, WindowMessage,
    NMHDR, SYSTEMTIME, NMDATETIMECHANGE,
    DTS_SHORTDATEFORMAT, DTS_RIGHTALIGN,
    DTM_GETSYSTEMTIME, DTM_SETSYSTEMTIME, DTM_SETRANGE,
    GDT_VALID, GDTR_MIN, GDTR_MAX,
    DTN_DATETIMECHANGE,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SetFocus, GetFocus,
    SendMessageW, SendMessageA,
}
use util { applyGuiFont }
use core { windowHwnd, noOp1 }

class DatePickerSlot {
    pub hwnd:      HWND
    pub winHandle: i64
    pub minY:      i32
    pub minM:      i32
    pub minD:      i32
    pub maxY:      i32
    pub maxM:      i32
    pub maxD:      i32
    pub onChange: fn(i32, i32, i32)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.winHandle = winHandle
        this.minY = 1900 as i32; this.minM = 1 as i32; this.minD = 1 as i32
        this.maxY = 2100 as i32; this.maxM = 12 as i32; this.maxD = 31 as i32
        this.onChange = dpNoOpYmd
        this.onFocus  = noOp1
        this.onBlur   = noOp1
    }
}

fn dpNoOpYmd(y: i32, m: i32, d: i32) {}

let DATE_PICKERS = new Map<i64, DatePickerSlot>()

@extern(C) {
pub fn tryDateTimeNotify(hwnd: i64, code: i32, lParam: i64): bool {
    if code != DTN_DATETIMECHANGE { return false }
    match DATE_PICKERS.get(hwnd) {
        some(slot) {
            // NMDATETIMECHANGE.st starts at offset 32 (after
            // NMHDR(24) + dwFlags(4) + _pad(4)). SYSTEMTIME
            // fields are u16:
            //   wYear @ +0, wMonth @ +2, wDayOfWeek @ +4, wDay @ +6
            //   wHour @ +8, wMinute @ +10, wSecond @ +12, wMs @ +14
            let p = lParam as *const void
            let y = readU16(p, 32 as i64) as i32
            let m = readU16(p, 34 as i64) as i32
            let d = readU16(p, 38 as i64) as i32
            slot.onChange(y, m, d)
            true
        }
        none { false }
    }
}

fn makeDateSystemTime(year: i32, month: i32, day: i32): SYSTEMTIME {
    SYSTEMTIME {
        wYear:         year as u16,
        wMonth:        month as u16,
        wDayOfWeek:    0 as u16,
        wDay:          day as u16,
        wHour:         0 as u16,
        wMinute:       0 as u16,
        wSecond:       0 as u16,
        wMilliseconds: 0 as u16,
    }
}

pub fn createNativeDatePicker(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "SysDateTimePick32".encodeUtf16()
    let emptyW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32) | visibleBits
                   | (WindowStyle.border as u32)
                   | DTS_SHORTDATEFORMAT
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW, emptyW, style,
        x as i32, y as i32, width as i32, height as i32,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)
    applyGuiFont(hwnd)
    let handle = hwnd as i64
    DATE_PICKERS.set(handle, new DatePickerSlot(hwnd, windowHandle))
    handle
}

pub fn setDatePickerValue(host: i64, year: i32, month: i32, day: i32) {
    if let some(slot) = DATE_PICKERS.get(host) {
        let st = makeDateSystemTime(year, month, day)
        let p: *const SYSTEMTIME = &st
        SendMessageW(slot.hwnd, DTM_SETSYSTEMTIME, GDT_VALID, p as i64)
    }
}

fn readSystemTimeField(slot: DatePickerSlot, offset: i64): i32 {
    let st = SYSTEMTIME {
        wYear:         0 as u16, wMonth:        0 as u16,
        wDayOfWeek:    0 as u16, wDay:          0 as u16,
        wHour:         0 as u16, wMinute:       0 as u16,
        wSecond:       0 as u16, wMilliseconds: 0 as u16,
    }
    let p: *const SYSTEMTIME = &st
    SendMessageW(slot.hwnd, DTM_GETSYSTEMTIME, 0 as u64, p as i64)
    readU16(p as *const void, offset) as i32
}

pub fn readDatePickerYear(host: i64): i32 {
    match DATE_PICKERS.get(host) {
        some(slot) { readSystemTimeField(slot, 0 as i64) }
        none       { 1970 as i32 }
    }
}
pub fn readDatePickerMonth(host: i64): i32 {
    match DATE_PICKERS.get(host) {
        some(slot) { readSystemTimeField(slot, 2 as i64) }
        none       { 1 as i32 }
    }
}
pub fn readDatePickerDay(host: i64): i32 {
    match DATE_PICKERS.get(host) {
        some(slot) { readSystemTimeField(slot, 6 as i64) }
        none       { 1 as i32 }
    }
}

pub fn setDatePickerMin(host: i64, year: i32, month: i32, day: i32) {
    if let some(slot) = DATE_PICKERS.get(host) {
        slot.minY = year; slot.minM = month; slot.minD = day
        applyRange(slot)
    }
}
pub fn setDatePickerMax(host: i64, year: i32, month: i32, day: i32) {
    if let some(slot) = DATE_PICKERS.get(host) {
        slot.maxY = year; slot.maxM = month; slot.maxD = day
        applyRange(slot)
    }
}

fn applyRange(slot: DatePickerSlot) {
    // DTM_SETRANGE wants a SYSTEMTIME[2] (min, max). We use
    // two adjacent struct literals — the layout in ilang is
    // identical to the C contract.
    let lo = makeDateSystemTime(slot.minY, slot.minM, slot.minD)
    let hi = makeDateSystemTime(slot.maxY, slot.maxM, slot.maxD)
    let pair: SYSTEMTIME[] = [lo, hi]
    let p: *const SYSTEMTIME = pair
    SendMessageW(slot.hwnd, DTM_SETRANGE, GDTR_MIN | GDTR_MAX, p as i64)
}

pub fn setDatePickerFrame(host: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = DATE_PICKERS.get(host) {
        MoveWindow(slot.hwnd, x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setDatePickerEnabled(host: i64, enabled: bool) {
    if let some(slot) = DATE_PICKERS.get(host) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setDatePickerVisible(host: i64, visible: bool) {
    if let some(slot) = DATE_PICKERS.get(host) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setDatePickerTooltip(host: i64, text: string) {
    let _ = host; let _ = text
    // No-op on Round 1 — same plumbing pattern as ListBox tooltip,
    // deferred.
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

// Mouse / key stubs — Round 1.
pub fn setOnDatePickerMouseEnter(host: i64, cb: fn(f64, f64)) {
    let _ = host; let _ = cb
}
pub fn setOnDatePickerMouseLeave(host: i64, cb: fn(f64, f64)) {
    let _ = host; let _ = cb
}
pub fn setOnDatePickerMouseDown(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = host; let _ = cb }
pub fn setOnDatePickerMouseUp(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = host; let _ = cb }
pub fn setOnDatePickerContextMenu(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = host; let _ = cb }
pub fn setOnDatePickerMouseWheel(
    host: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) { let _ = host; let _ = cb }
pub fn setOnDatePickerKeyDown(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = host; let _ = cb
}
pub fn setOnDatePickerKeyUp(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = host; let _ = cb
}

pub fn focusDatePicker(host: i64) {
    if let some(slot) = DATE_PICKERS.get(host) { SetFocus(slot.hwnd) }
}
pub fn blurDatePicker(host: i64) {
    if let some(slot) = DATE_PICKERS.get(host) {
        if (GetFocus() as i64) == (slot.hwnd as i64) {
            SetFocus(slot.winHandle as HWND)
        }
    }
}

} // @extern(C)
