/// Numeric entry — `EDIT` control paired with `msctls_updown32`
/// via `UDM_SETBUDDY`. We manage the displayed text manually
/// (`f64` value + decimal places) instead of relying on
/// `UDS_SETBUDDYINT`, which only supports integer display.
///
/// `UDN_DELTAPOS` arrives via WM_NOTIFY on the parent;
/// `wnd_proc.il` calls `tryNumericUpDownNotify` to route it
/// here. The user editing the text field directly is **not**
/// committed back to `value` in Round 1 — typing changes the
/// visible string only; the next stepper click reverts to the
/// model. Tracked as a follow-up.

use std.ffi { readI32 }
use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd, WindowMessage,
    EditStyle, EditMessage,
    NMHDR, NMUPDOWN,
    UDS_SETBUDDYINT, UDS_ALIGNRIGHT, UDS_ARROWKEYS, UDS_HOTTRACK, UDS_WRAP,
    UDM_SETBUDDY, UDM_SETRANGE32,
    UDN_DELTAPOS,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SetFocus, GetFocus,
    SendMessageW, SendMessageA,
    SetWindowTextW,
}
use util { applyGuiFont }
use core { windowHwnd, noOp1 }

class NumericSlot {
    pub editHwnd:  HWND
    pub upDnHwnd:  HWND
    pub winHandle: i64
    pub value:     f64
    pub min:       f64
    pub max:       f64
    pub step:      f64
    pub decimalPlaces: i32
    pub wrap:      bool
    pub onChange: fn(f64)
    pub onFocus:  fn()
    pub onBlur:   fn()
    pub init(editHwnd: HWND, upDnHwnd: HWND, winHandle: i64) {
        this.editHwnd = editHwnd
        this.upDnHwnd = upDnHwnd
        this.winHandle = winHandle
        this.value = 0.0
        this.min   = 0.0
        this.max   = 100.0
        this.step  = 1.0
        this.decimalPlaces = 0 as i32
        this.wrap  = false
        this.onChange = numNoOpF64
        this.onFocus  = noOp1
        this.onBlur   = noOp1
    }
}

fn numNoOpF64(v: f64) {}

/// Keyed by the UpDown control's HWND — that's what the
/// WM_NOTIFY hwndFrom field carries.
let NUMERICS = new Map<i64, NumericSlot>()
/// `editHwnd` → `upDnHwnd`, so the public-facing handle (the
/// edit HWND, more useful for setFocus / SetWindowText) can
/// find its slot.
let EDIT_TO_UPDN: Map<i64, i64> = new Map<i64, i64>()

pub fn tryNumericUpDownNotify(hwnd: i64, code: i32, lParam: i64): bool {
    if code != UDN_DELTAPOS { return false }
    match NUMERICS.get(hwnd) {
        some(slot) {
            // NMUPDOWN.iDelta at offset NMHDR(24) + iPos(4) = 28.
            let p = lParam as *const void
            let delta = readI32(p, 28 as i64)
            let proposed = slot.value + (slot.step * (delta as f64))
            let clamped = clampWithWrap(proposed, slot)
            if clamped != slot.value {
                slot.value = clamped
                writeEditText(slot)
                let cb = slot.onChange
                cb(clamped)
            }
            true
        }
        none { false }
    }
}

fn clampWithWrap(v: f64, slot: NumericSlot): f64 {
    if slot.wrap {
        if v > slot.max { return slot.min }
        if v < slot.min { return slot.max }
        return v
    }
    if v < slot.min { return slot.min }
    if v > slot.max { return slot.max }
    v
}

fn writeEditText(slot: NumericSlot) {
    let s = formatValue(slot.value, slot.decimalPlaces)
    let w = s.encodeUtf16()
    let p: *const u16 = w
    SetWindowTextW(slot.editHwnd, p)
}

/// Minimal f64 → decimal string with a fixed fraction length.
/// We don't have access to a printf-style formatter in the
/// shared runtime; this hand-rolls the same conversion
/// `%.<digits>f` produces, which is plenty for the small ranges
/// a Spinner addresses.
fn formatValue(v: f64, digits: i32): string {
    let neg = v < 0.0
    let av: f64 = if neg { -v } else { v }
    let d: i32 = if digits < 0 as i32 { 0 as i32 } else { digits }
    // Multiply, round to nearest integer, then split.
    let scale: f64 = pow10(d)
    let scaled: f64 = av * scale + 0.5
    let asI64: i64 = scaled as i64
    let intPart: i64 = asI64 / (pow10i(d))
    let fracPart: i64 = asI64 % (pow10i(d))
    let intStr = i64ToString(intPart)
    let signStr = if neg { "-" } else { "" }
    if d == 0 as i32 {
        signStr + intStr
    } else {
        // Pad fracPart to `d` digits with leading zeros.
        let fracStr = padLeftZero(i64ToString(fracPart), d as i64)
        signStr + intStr + "." + fracStr
    }
}

fn pow10(d: i32): f64 {
    let n: i32 = if d < 0 as i32 { 0 as i32 } else { d }
    let acc: f64 = 1.0
    let i: i32 = 0 as i32
    let acc2: f64 = acc
    let i2: i32 = i
    while i2 < n {
        acc2 = acc2 * 10.0
        i2 = i2 + (1 as i32)
    }
    acc2
}

fn pow10i(d: i32): i64 {
    let n: i32 = if d < 0 as i32 { 0 as i32 } else { d }
    let acc: i64 = 1 as i64
    let i: i32 = 0 as i32
    let acc2: i64 = acc
    let i2: i32 = i
    while i2 < n {
        acc2 = acc2 * (10 as i64)
        i2 = i2 + (1 as i32)
    }
    acc2
}

fn i64ToString(n: i64): string {
    let digits: string[] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]
    if n == 0 as i64 { return "0" }
    let neg = n < 0 as i64
    let acc: i64 = if neg { -n } else { n }
    let buf: string[] = []
    while acc > 0 as i64 {
        let dgt = acc % (10 as i64)
        buf.unshift(digits[dgt])
        acc = acc / (10 as i64)
    }
    let out = if neg { "-" } else { "" }
    let m = buf.length
    let i: i64 = 0
    while i < m {
        out = out + buf[i]
        i = i + 1
    }
    out
}

fn padLeftZero(s: string, width: i64): string {
    let n = s.length
    if n >= width { return s }
    let out = s
    let need = width - n
    let i: i64 = 0
    let prefix = ""
    while i < need {
        prefix = prefix + "0"
        i = i + 1
    }
    prefix + out
}

@extern(C) {

pub fn createNativeNumericUpDown(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let editClassW = "EDIT".encodeUtf16()
    let updnClassW = "msctls_updown32".encodeUtf16()
    let emptyW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let editStyle: u32 = (WindowStyle.child as u32) | visibleBits
                      | (EditStyle.left as u32)
                      | (WindowStyle.border as u32)
                      | (EditStyle.autoHScroll as u32)
    let hInstance = GetModuleHandleA(0 as *const char)
    let editHwnd = CreateWindowExW(
        0,
        editClassW, emptyW, editStyle,
        x as i32, y as i32, width as i32, height as i32,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(editHwnd, enabledFlag)
    applyGuiFont(editHwnd)

    let upStyle: u32 = (WindowStyle.child as u32) | visibleBits
                    | UDS_ALIGNRIGHT | UDS_ARROWKEYS | UDS_HOTTRACK
    let upDnHwnd = CreateWindowExW(
        0,
        updnClassW, emptyW, upStyle,
        0, 0, 0, 0,
        parent, 0 as HMENU, hInstance as HINSTANCE, 0 as *void
    )
    // UDM_SETBUDDY: align the spinner against the edit control.
    SendMessageW(upDnHwnd, UDM_SETBUDDY, editHwnd as u64, 0 as i64)
    // Wide initial integer range so single-step UDN_DELTAPOS
    // events never bottom or top out on the OS side — we apply
    // our own min/max clamp on top.
    let lo: i32 = -2147483647 as i32
    let hi: i32 =  2147483647 as i32
    SendMessageW(upDnHwnd, UDM_SETRANGE32, lo as u64, hi as i64)

    let key = upDnHwnd as i64
    let slot = new NumericSlot(editHwnd, upDnHwnd, windowHandle)
    NUMERICS.set(key, slot)
    EDIT_TO_UPDN.set(editHwnd as i64, key)
    writeEditText(slot)
    // Public handle: the edit HWND. It's the one we surface to
    // focusing / framing / tooltips since it's the visible
    // input area; the slot map keys off the UpDown handle for
    // WM_NOTIFY routing, with EDIT_TO_UPDN mapping the public
    // side back.
    editHwnd as i64
}

pub fn setNumericValue(host: i64, value: f64) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) {
            slot.value = clampWithWrap(value, slot)
            writeEditText(slot)
        }
    }
}

pub fn readNumericValue(host: i64): f64 {
    match EDIT_TO_UPDN.get(host) {
        some(updn) {
            match NUMERICS.get(updn) {
                some(slot) { slot.value }
                none       { 0.0 }
            }
        }
        none { 0.0 }
    }
}

pub fn setNumericRange(host: i64, minV: f64, maxV: f64) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) {
            slot.min = minV
            slot.max = maxV
            // Re-clamp the current value against the new range.
            slot.value = clampWithWrap(slot.value, slot)
            writeEditText(slot)
        }
    }
}

pub fn setNumericStep(host: i64, step: f64) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) { slot.step = step }
    }
}

pub fn setNumericDecimalPlaces(host: i64, n: i32) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) {
            slot.decimalPlaces = n
            writeEditText(slot)
        }
    }
}

pub fn setNumericWrap(host: i64, wrap: bool) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) { slot.wrap = wrap }
    }
}

pub fn setNumericFrame(host: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) {
            MoveWindow(slot.editHwnd, x as i32, y as i32, width as i32, height as i32, 1)
            // UDS_ALIGNRIGHT keeps the up/down aligned with the
            // edit's right edge automatically once SETBUDDY ran;
            // moving the edit triggers a re-snap.
        }
    }
}

pub fn setNumericEnabled(host: i64, enabled: bool) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) {
            let flag: i32 = if enabled { 1 } else { 0 }
            EnableWindow(slot.editHwnd, flag)
            EnableWindow(slot.upDnHwnd, flag)
        }
    }
}

pub fn setNumericVisible(host: i64, visible: bool) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) {
            let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
            ShowWindow(slot.editHwnd, cmd)
            ShowWindow(slot.upDnHwnd, cmd)
        }
    }
}

pub fn setNumericTooltip(host: i64, text: string) {
    let _ = host
    let _ = text
    // Tooltip is no-op on Round 1; the EDIT control supports
    // standard tooltips via the system tooltip mechanism, same
    // surface as ListBox — adding it duplicates the ListBox
    // tooltip plumbing, deferred.
}

pub fn setOnNumericChange(host: i64, cb: fn(f64)) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) { slot.onChange = cb }
    }
}
pub fn setOnNumericFocus(host: i64, cb: fn()) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) { slot.onFocus = cb }
    }
}
pub fn setOnNumericBlur(host: i64, cb: fn()) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) { slot.onBlur = cb }
    }
}

// Mouse / key surface stubs — Round 1 gap.
pub fn setOnNumericMouseEnter(host: i64, cb: fn(f64, f64)) {
    let _ = host; let _ = cb
}
pub fn setOnNumericMouseLeave(host: i64, cb: fn(f64, f64)) {
    let _ = host; let _ = cb
}
pub fn setOnNumericMouseDown(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = host; let _ = cb }
pub fn setOnNumericMouseUp(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = host; let _ = cb }
pub fn setOnNumericContextMenu(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) { let _ = host; let _ = cb }
pub fn setOnNumericMouseWheel(
    host: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) { let _ = host; let _ = cb }
pub fn setOnNumericKeyDown(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = host; let _ = cb
}
pub fn setOnNumericKeyUp(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    let _ = host; let _ = cb
}

pub fn focusNumeric(host: i64) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) { SetFocus(slot.editHwnd) }
    }
}
pub fn blurNumeric(host: i64) {
    if let some(updn) = EDIT_TO_UPDN.get(host) {
        if let some(slot) = NUMERICS.get(updn) {
            if (GetFocus() as i64) == (slot.editHwnd as i64) {
                SetFocus(slot.winHandle as HWND)
            }
        }
    }
}

} // @extern(C)
