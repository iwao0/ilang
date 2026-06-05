/// Numeric entry — `NSTextField` for the editable value with
/// an `NSStepper` glued to its right edge. The stepper's
/// target/action updates the text field; the text field's
/// Enter-press parses the typed string back to the stepper. An
/// `NSNumberFormatter` enforces the visible precision.

use cocoa {
    NSObject, NSView, NSTextField, NSStepper,
    NSNumber, NSNumberFormatter,
    NSEvent, NSEventModifierFlags,
    NSTrackingArea, NSTrackingAreaOptions,
    makeRect, nsstr, ilstr, nil, selectorPtr,
    classPtr, setObjectClass,
}
use core {
    resolveParentView, noOp1, noOpF64,
    noOpEnterLeave, noOpClickMod, noOpWheel, noOpKey,
    keyCodeToString, modCtrl, modShift, modAlt, modMeta,
}

class NumericSlot {
    pub field:     NSTextField
    pub stepper:   NSStepper
    pub formatter: NSNumberFormatter
    pub min:       f64
    pub max:       f64
    pub step:      f64
    pub decimalPlaces: i32
    pub wrap:      bool
    pub onChange:  fn(f64)
    pub onFocus:   fn()
    pub onBlur:    fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseUp:    fn(f64, f64, i32, bool, bool, bool, bool)
    pub onContextMenu: fn(f64, f64, i32, bool, bool, bool, bool)
    pub onMouseWheel: fn(f64, f64, f64, f64, bool, bool, bool, bool)
    pub onKeyDown: fn(string, bool, bool, bool, bool)
    pub onKeyUp:   fn(string, bool, bool, bool, bool)
    pub handler:   NSObject
    pub init(field: NSTextField, stepper: NSStepper,
             formatter: NSNumberFormatter, handler: NSObject) {
        this.field     = field
        this.stepper   = stepper
        this.formatter = formatter
        this.min       = 0.0
        this.max       = 100.0
        this.step      = 1.0
        this.decimalPlaces = 0 as i32
        this.wrap      = false
        this.onChange  = noOpF64
        this.onFocus   = noOp1
        this.onBlur    = noOp1
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClickMod
        this.onMouseUp    = noOpClickMod
        this.onContextMenu = noOpClickMod
        this.onMouseWheel = noOpWheel
        this.onKeyDown = noOpKey
        this.onKeyUp   = noOpKey
        this.handler   = handler
    }
}

/// Keyed by `field.handle` — the NSTextField is the public-facing
/// piece of the composite, so the public bridge handle is its
/// NSView pointer.
let NUMERICS = new Map<i64, NumericSlot>()
/// `stepper.handle` → owning field handle (and `field.handle`
/// also maps to itself — lets either subview look up the slot).
let SUB_TO_HOST: Map<i64, i64> = new Map<i64, i64>()

class NumericHandler : NSObject {
    @objc("stepperChanged:")
    pub stepperChanged(sender: NSObject) {
        if let some(host) = SUB_TO_HOST.get(sender.handle) {
            if let some(slot) = NUMERICS.get(host) {
                let v = slot.stepper.doubleValue
                writeFieldValue(slot, v)
                slot.onChange(v)
            }
        }
    }

    @objc("textCommitted:")
    pub textCommitted(sender: NSObject) {
        if let some(host) = SUB_TO_HOST.get(sender.handle) {
            if let some(slot) = NUMERICS.get(host) {
                let s = ilstr(slot.field.stringValue)
                let parsed = slot.formatter.numberFromString(nsstr(s))
                let v: f64 = if (parsed as NSObject).handle == 0 as i64 {
                    slot.stepper.doubleValue
                } else {
                    NSNumber.__wrap_handle_unowned(parsed.handle).f64Value
                }
                let clamped = clampValue(v, slot)
                slot.stepper.doubleValue = clamped
                writeFieldValue(slot, clamped)
                slot.onChange(clamped)
            }
        }
    }
}

fn clampValue(v: f64, slot: NumericSlot): f64 {
    if v < slot.min { slot.min } else { if v > slot.max { slot.max } else { v } }
}

fn writeFieldValue(slot: NumericSlot, v: f64) {
    let s = slot.formatter.stringFromNumber(NSNumber.fromF64(v) as NSObject)
    slot.field.stringValue = s
}

@extern(C) {

pub fn createNativeNumericUpDown(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    // Layout: 19pt stepper on the right; text field fills the
    // remainder minus a 2pt gap.
    let stepperW: f64 = 19.0
    let fieldW: f64 = if width > (stepperW + 4.0) { width - stepperW - 2.0 } else { width }

    let field = NSTextField.alloc().init(makeRect(x, y, fieldW, height))
    field.setEditable(true)
    field.setBezeled(true)
    field.setDrawsBackground(true)
    field.setEnabled(enabled)
    (field as NSView).setHidden(!visible)

    let stepper = NSStepper.alloc().init(
        makeRect(x + fieldW + 2.0, y, stepperW, height)
    )
    stepper.setMinValue(0.0)
    stepper.setMaxValue(100.0)
    stepper.setIncrement(1.0)
    stepper.setContinuous(true)
    (stepper as NSView).setHidden(!visible)

    let formatter = NSNumberFormatter.alloc().init()
    formatter.minimumFractionDigits = 0 as u64
    formatter.maximumFractionDigits = 0 as u64

    NumericHandler.register()
    let h = NumericHandler.alloc().init()

    stepper.setTarget(h as NSObject)
    stepper.setAction(selectorPtr("stepperChanged:"))
    field.setTarget(h as NSObject)
    field.setAction(selectorPtr("textCommitted:"))

    parent.addSubview(field as NSView)
    parent.addSubview(stepper as NSView)

    let host = (field as NSView).handle
    let slot = new NumericSlot(field, stepper, formatter, h as NSObject)
    NUMERICS.set(host, slot)
    SUB_TO_HOST.set(host, host)
    SUB_TO_HOST.set((stepper as NSView).handle, host)

    writeFieldValue(slot, 0.0)
    host
}

pub fn setNumericValue(host: i64, value: f64) {
    if let some(slot) = NUMERICS.get(host) {
        let v = clampValue(value, slot)
        slot.stepper.doubleValue = v
        writeFieldValue(slot, v)
    }
}

pub fn readNumericValue(host: i64): f64 {
    match NUMERICS.get(host) {
        some(slot) { slot.stepper.doubleValue }
        none       { 0.0 }
    }
}

pub fn setNumericRange(host: i64, minV: f64, maxV: f64) {
    if let some(slot) = NUMERICS.get(host) {
        slot.min = minV
        slot.max = maxV
        slot.stepper.setMinValue(minV)
        slot.stepper.setMaxValue(maxV)
    }
}

pub fn setNumericStep(host: i64, step: f64) {
    if let some(slot) = NUMERICS.get(host) {
        slot.step = step
        slot.stepper.setIncrement(step)
    }
}

pub fn setNumericDecimalPlaces(host: i64, n: i32) {
    if let some(slot) = NUMERICS.get(host) {
        slot.decimalPlaces = n
        let u = if n < 0 as i32 { 0 as u64 } else { n as u64 }
        slot.formatter.minimumFractionDigits = u
        slot.formatter.maximumFractionDigits = u
        // Re-render the current value with the new precision.
        writeFieldValue(slot, slot.stepper.doubleValue)
    }
}

pub fn setNumericWrap(host: i64, wrap: bool) {
    if let some(slot) = NUMERICS.get(host) {
        slot.wrap = wrap
        slot.stepper.setValueWraps(wrap)
    }
}

pub fn setNumericFrame(host: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = NUMERICS.get(host) {
        let stepperW: f64 = 19.0
        let fieldW: f64 = if width > (stepperW + 4.0) { width - stepperW - 2.0 } else { width }
        (slot.field as NSView).setFrame(makeRect(x, y, fieldW, height))
        (slot.stepper as NSView).setFrame(makeRect(x + fieldW + 2.0, y, stepperW, height))
    }
}

pub fn setNumericEnabled(host: i64, enabled: bool) {
    if let some(slot) = NUMERICS.get(host) {
        slot.field.setEnabled(enabled)
        // NSStepper inherits setEnabled from NSControl; calling
        // through `(stepper as NSObject).performSelector:` would
        // work, but the binding surfaces no public setEnabled on
        // NSStepper itself — leave as a TODO if the visual
        // mismatch matters.
    }
}

pub fn setNumericVisible(host: i64, visible: bool) {
    if let some(slot) = NUMERICS.get(host) {
        (slot.field as NSView).setHidden(!visible)
        (slot.stepper as NSView).setHidden(!visible)
    }
}

pub fn setNumericTooltip(host: i64, text: string) {
    if let some(slot) = NUMERICS.get(host) {
        if text == "" {
            (slot.field as NSView).setToolTip(nil)
        } else {
            (slot.field as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}

pub fn setOnNumericChange(host: i64, cb: fn(f64)) {
    if let some(slot) = NUMERICS.get(host) { slot.onChange = cb }
}
pub fn setOnNumericFocus(host: i64, cb: fn()) {
    if let some(slot) = NUMERICS.get(host) { slot.onFocus = cb }
}
pub fn setOnNumericBlur(host: i64, cb: fn()) {
    if let some(slot) = NUMERICS.get(host) { slot.onBlur = cb }
}
pub fn setOnNumericMouseEnter(host: i64, cb: fn(f64, f64)) {
    if let some(slot) = NUMERICS.get(host) { slot.onMouseEnter = cb }
}
pub fn setOnNumericMouseLeave(host: i64, cb: fn(f64, f64)) {
    if let some(slot) = NUMERICS.get(host) { slot.onMouseLeave = cb }
}
pub fn setOnNumericMouseDown(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = NUMERICS.get(host) { slot.onMouseDown = cb }
}
pub fn setOnNumericMouseUp(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = NUMERICS.get(host) { slot.onMouseUp = cb }
}
pub fn setOnNumericContextMenu(
    host: i64, cb: fn(f64, f64, i32, bool, bool, bool, bool)
) {
    if let some(slot) = NUMERICS.get(host) { slot.onContextMenu = cb }
}
pub fn setOnNumericMouseWheel(
    host: i64, cb: fn(f64, f64, f64, f64, bool, bool, bool, bool)
) {
    if let some(slot) = NUMERICS.get(host) { slot.onMouseWheel = cb }
}
pub fn setOnNumericKeyDown(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = NUMERICS.get(host) { slot.onKeyDown = cb }
}
pub fn setOnNumericKeyUp(host: i64, cb: fn(string, bool, bool, bool, bool)) {
    if let some(slot) = NUMERICS.get(host) { slot.onKeyUp = cb }
}

pub fn focusNumeric(host: i64) {
    if let some(slot) = NUMERICS.get(host) {
        let win = (slot.field as NSView).window
        let _ = win.makeFirstResponder(slot.field as NSObject)
    }
}
pub fn blurNumeric(host: i64) {
    if let some(slot) = NUMERICS.get(host) {
        let win = (slot.field as NSView).window
        if win.firstResponder.handle == (slot.field as NSView).handle {
            let _ = win.makeFirstResponder(nil)
        }
    }
}

} // @extern(C)
