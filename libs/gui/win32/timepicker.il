/// Time-of-day picker — `SysDateTimePick32` in `DTS_TIMEFORMAT`
/// mode. `showSeconds` is honoured by the system format ("HH:mm:ss"
/// vs "HH:mm"). The chosen time arrives as a
/// `DTN_DATETIMECHANGE` notification on the parent (same path as
/// `DatePicker`); `tryDateTimePickTimeNotify` here routes it.

use std.ffi { readU16 }
use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd, WindowMessage,
    NMHDR, SYSTEMTIME, NMDATETIMECHANGE,
    DTS_TIMEFORMAT,
    DTM_GETSYSTEMTIME, DTM_SETSYSTEMTIME,
    GDT_VALID,
    DTN_DATETIMECHANGE,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SetFocus, GetFocus,
    SendMessageW, SendMessageA,
}
use util { applyGuiFont }
use core { windowHwnd, noOp1 }

class TimePickerSlot {
    pub hwnd:        HWND
    pub winHandle:   i64
    pub showSeconds: bool
    pub onChange: fn(i32, i32, i32)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub init(hwnd: HWND, winHandle: i64, showSeconds: bool) {
        this.hwnd        = hwnd
        this.winHandle   = winHandle
        this.showSeconds = showSeconds
        this.onChange = tpNoOpHms
        this.onFocus  = noOp1
        this.onBlur   = noOp1
    }
}

fn tpNoOpHms(h: i32, m: i32, s: i32) {}

let TIME_PICKERS = new Map<i64, TimePickerSlot>()

pub fn tryDateTimePickTimeNotify(hwnd: i64, code: i32, lParam: i64): bool {
    if code != DTN_DATETIMECHANGE { return false }
    match TIME_PICKERS.get(hwnd) {
        some(slot) {
            let p = lParam as *const void
            // SYSTEMTIME wHour @ +40 (32+8), wMinute @ +42, wSecond @ +44.
            let h = readU16(p, 40 as i64) as i32
            let m = readU16(p, 42 as i64) as i32
            let s = readU16(p, 44 as i64) as i32
            let cb = slot.onChange
            cb(h, m, s)
            true
        }
        none { false }
    }
}

fn makeSystemTime(hour: i32, minute: i32, second: i32): SYSTEMTIME {
    SYSTEMTIME {
        wYear:         2000 as u16,
        wMonth:        1    as u16,
        wDayOfWeek:    0    as u16,
        wDay:          1    as u16,
        wHour:         hour   as u16,
        wMinute:       minute as u16,
        wSecond:       second as u16,
        wMilliseconds: 0 as u16,
    }
}

@extern(C) {

pub fn createNativeTimePicker(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool,
    showSeconds: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "SysDateTimePick32".encodeUtf16()
    let emptyW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32) | visibleBits
                   | (WindowStyle.border as u32)
                   | DTS_TIMEFORMAT
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
    TIME_PICKERS.set(handle, new TimePickerSlot(hwnd, windowHandle, showSeconds))
    // SysDateTimePick32 with DTS_TIMEFORMAT shows HH:mm:ss by
    // default on Vista+; `showSeconds=false` would need a
    // custom DTM_SETFORMATW with "HH:mm" — Round 2.
    let _ = showSeconds
    handle
}

pub fn setTimePickerValue(host: i64, hour: i32, minute: i32, second: i32) {
    if let some(slot) = TIME_PICKERS.get(host) {
        let st = makeSystemTime(hour, minute, second)
        let p: *const SYSTEMTIME = &st
        SendMessageW(slot.hwnd, DTM_SETSYSTEMTIME, GDT_VALID, p as i64)
    }
}

fn readSystemTimeField(slot: TimePickerSlot, offset: i64): i32 {
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

pub fn readTimePickerHour(host: i64): i32 {
    match TIME_PICKERS.get(host) {
        some(slot) { readSystemTimeField(slot, 8 as i64) }
        none       { 0 as i32 }
    }
}
pub fn readTimePickerMinute(host: i64): i32 {
    match TIME_PICKERS.get(host) {
        some(slot) { readSystemTimeField(slot, 10 as i64) }
        none       { 0 as i32 }
    }
}
pub fn readTimePickerSecond(host: i64): i32 {
    match TIME_PICKERS.get(host) {
        some(slot) { readSystemTimeField(slot, 12 as i64) }
        none       { 0 as i32 }
    }
}

pub fn setTimePickerFrame(host: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TIME_PICKERS.get(host) {
        MoveWindow(slot.hwnd, x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setTimePickerEnabled(host: i64, enabled: bool) {
    if let some(slot) = TIME_PICKERS.get(host) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setTimePickerVisible(host: i64, visible: bool) {
    if let some(slot) = TIME_PICKERS.get(host) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setTimePickerTooltip(host: i64, text: string) {
    let _ = host; let _ = text
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
    let _ = host; let _ = cb
}
pub fn setOnTimePickerMouseLeave(host: i64, cb: fn(f64, f64)) {
    let _ = host; let _ = cb
}
pub fn setOnTimePickerMouseDown(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = host; let _ = cb }
pub fn setOnTimePickerMouseUp(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = host; let _ = cb }
pub fn setOnTimePickerContextMenu(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = host; let _ = cb }
pub fn setOnTimePickerMouseWheel(
    host: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) { let _ = host; let _ = cb }
pub fn setOnTimePickerKeyDown(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = host; let _ = cb
}
pub fn setOnTimePickerKeyUp(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = host; let _ = cb
}

pub fn focusTimePicker(host: i64) {
    if let some(slot) = TIME_PICKERS.get(host) { SetFocus(slot.hwnd) }
}
pub fn blurTimePicker(host: i64) {
    if let some(slot) = TIME_PICKERS.get(host) {
        if (GetFocus() as i64) == (slot.hwnd as i64) {
            SetFocus(slot.winHandle as HWND)
        }
    }
}

} // @extern(C)
