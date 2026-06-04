/// Controls — NSButton / NSTextField / NSSlider / NSStepper /
/// NSPopUpButton / NSComboBox / NSSegmentedControl /
/// NSProgressIndicator / NSDatePicker / NSBox plus
/// NSTextFieldDelegate / NSControlTextEditingDelegate.

use foundation { NSObject, NSString, NSArray, NSDate, NSRect, nsstr, makeRect }
use core { NSView }
use image { NSImage }

/// NSBezelStyle — visual style of a push button. `push` is the
/// standard Mac-style rounded blue button; the rest are
/// historical / niche options.
pub enum NSBezelStyle: u64 {
    push                = 1
    rounded             = 1
    regularSquare       = 2
    disclosure          = 5
    shadowlessSquare    = 6
    circular            = 7
    textured            = 8
    helpButton          = 9
    smallSquare         = 10
    texturedRounded     = 11
    roundedRect         = 12
    recessed            = 13
    roundedDisclosure   = 14
    inline              = 15
}

/// NSProgressIndicatorStyle — bar vs spinner.
pub enum NSProgressIndicatorStyle: u64 {
    bar     = 0
    spinner = 1
}

/// NSSegmentSwitchTracking — selection model of a segmented
/// control: how clicks change which segment(s) are highlighted.
pub enum NSSegmentSwitchTracking: u64 {
    selectOne     = 0
    selectAny     = 1
    momentary     = 2
    momentaryAccelerator = 3
}

/// NSDatePickerStyle — visual layout of the date picker control.
pub enum NSDatePickerStyle: u64 {
    textFieldAndStepper = 0
    clockAndCalendar    = 1
    textField           = 2
}

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    /// NSButton. `alloc + initWithFrame:` follows the standard
    /// two-step pattern. Target/action follows the standard
    /// Cocoa convention: pair `setTarget(handler)` with
    /// `setAction(selectorPtr("clicked:"))` so AppKit invokes
    /// `[handler clicked:button]` whenever the button is clicked.
    @objc pub class NSButton : NSView {
        @objc("alloc")
        pub static alloc(): NSButton
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSButton

        @objc("setTitle:")
        pub setTitle(title: NSString)
        @objc("setBezelStyle:")
        pub setBezelStyle(style: NSBezelStyle)
        /// Resize the button to fit its current title + bezel —
        /// honours the system font metrics rather than the
        /// estimated width a caller might compute by hand.
        @objc("sizeToFit")
        pub sizeToFit()
        /// Set the button's icon (NSImage). Used by status-bar
        /// items / toolbar buttons; left nil on text-only
        /// buttons.
        @objc("setImage:")
        pub setImage(img: NSImage)
        /// Switch to checkbox / radio / push-on-push-off / etc. by
        /// passing the matching `NSButtonType` (= `i64` on macOS).
        /// `3` = `NSButtonTypeSwitch` (checkbox).
        @objc("setButtonType:")
        pub setButtonType(t: i64)
        /// `NSControlStateValueOff = 0`, `Mixed = -1`, `On = 1`.
        @objc("state")
        pub get state(): i64
        @objc("setState:")
        pub set state(v: i64)

        @objc("setTarget:")
        pub setTarget(target: NSObject)
        /// SEL doesn't have a user-facing ilang type today; the
        /// action is passed as the raw SEL pointer cast to `i64`.
        /// Use `selectorPtr("clicked:")` to build one from a name.
        @objc("setAction:")
        pub setAction(sel: i64)
        @objc("setEnabled:")
        pub setEnabled(flag: bool)
        @objc("isEnabled")
        pub get isEnabled(): bool
        /// Setting the key equivalent to `"\r"` (Return) makes this
        /// button the window's default — AppKit then draws the
        /// pulsing-blue treatment and activates the button when the
        /// user presses Return while the window is key. Pass `""` to
        /// clear it.
        @objc("setKeyEquivalent:")
        pub setKeyEquivalent(s: NSString)
        /// Programmatic click. Animates the button briefly and
        /// invokes the configured target/action. Pass `nil` as
        /// `sender` for the canonical "no real sender" case.
        @objc("performClick:")
        pub performClick(sender: NSObject)
        /// Toggle the cell's "is being pressed" state explicitly.
        /// `NSButton` reflects this in the cell's visual treatment
        /// (the bezel sinks in). Useful when a subclass takes over
        /// `mouseDown:` / `mouseUp:` and needs to keep the press
        /// feedback running without bouncing through `super`.
        @objc("setHighlighted:")
        pub setHighlighted(flag: bool)
    }

    /// NSTextField. Doubles as both an editable text input and a
    /// non-editable label (set `setEditable(false)` + `setBordered(false)` +
    /// `setDrawsBackground(false)` for the label look — `makeLabel`
    /// below does that for you).
    @objc pub class NSTextField : NSView {
        @objc("alloc")
        pub static alloc(): NSTextField
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSTextField

        @objc("stringValue")
        pub get stringValue(): NSString
        @objc("setStringValue:")
        pub set stringValue(s: NSString)

        @objc("setEditable:")
        pub setEditable(flag: bool)
        @objc("setBordered:")
        pub setBordered(flag: bool)
        @objc("setBezeled:")
        pub setBezeled(flag: bool)
        @objc("setDrawsBackground:")
        pub setDrawsBackground(flag: bool)
        @objc("setSelectable:")
        pub setSelectable(flag: bool)
        /// Inherited from `NSControl`; included on `NSTextField`
        /// for the common case of greying out an input.
        @objc("setEnabled:")
        pub setEnabled(flag: bool)
        /// Cue-banner text shown when the field is empty. The
        /// AppKit attribute is plain NSString; for styled text use
        /// `setPlaceholderAttributedString:` instead.
        @objc("setPlaceholderString:")
        pub setPlaceholderString(s: NSString)
        /// `nil` when no field editor is attached (i.e. the field
        /// is not the active editing target). Returns the NSText
        /// field editor when the field has focus. Useful as an
        /// "am I focused" probe — the firstResponder of the
        /// containing window is the field editor, not this NSTextField.
        @objc("currentEditor")
        pub get currentEditor(): NSObject
        /// Make the field the first responder, install the shared
        /// field editor, and select all text in one shot. The
        /// `sender` argument is the AppKit convention for who
        /// requested the action; `nil` is the canonical "no real
        /// sender" path.
        @objc("selectText:")
        pub selectText(sender: NSObject)

        /// setDelegate overloads. NSTextField accepts either the
        /// text-field-specific protocol or the more general
        /// NSControlTextEditingDelegate; bind both typed forms plus
        /// an NSObject fallback. Same ObjC selector under the hood.
        @objc("setDelegate:")
        pub setDelegate(d: NSTextFieldDelegate)
        @objc("setDelegate:")
        pub setDelegate(d: NSControlTextEditingDelegate)
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
        /// Target/action — inherited from `NSControl`. Hooked on
        /// `NumericUpDown` so Return-press commits the typed
        /// value back to the stepper.
        @objc("setTarget:")
        pub setTarget(target: NSObject)
        @objc("setAction:")
        pub setAction(sel: i64)
    }

    /// NSSlider — continuous horizontal slider by default. `min`,
    /// `max`, and the current `doubleValue` are f64; the value
    /// reads back via `doubleValue()` (target/action hookups for
    /// live updates follow the NSButton pattern).
    @objc pub class NSSlider : NSView {
        @objc("alloc")
        pub static alloc(): NSSlider
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSSlider

        @objc("setMinValue:")
        pub setMinValue(v: f64)
        @objc("setMaxValue:")
        pub setMaxValue(v: f64)
        @objc("doubleValue")
        pub get doubleValue(): f64
        @objc("setDoubleValue:")
        pub set doubleValue(v: f64)
        /// `true` = fire the action every time the thumb moves;
        /// `false` (default) = only fire on mouse-up.
        @objc("setContinuous:")
        pub setContinuous(flag: bool)

        @objc("setTarget:")
        pub setTarget(target: NSObject)
        @objc("setAction:")
        pub setAction(sel: i64)

        @objc("setNumberOfTickMarks:")
        pub setNumberOfTickMarks(n: i64)
        @objc("setAllowsTickMarkValuesOnly:")
        pub setAllowsTickMarkValuesOnly(flag: bool)
    }

    /// `NSStepper` — pair of up / down arrows over a value with
    /// configurable bounds and increment. Pair with an
    /// `NSTextField` via target/action when you want a numeric
    /// input.
    @objc pub class NSStepper : NSView {
        @objc("alloc")
        pub static alloc(): NSStepper
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSStepper

        @objc("setMinValue:")
        pub setMinValue(v: f64)
        @objc("setMaxValue:")
        pub setMaxValue(v: f64)
        @objc("setIncrement:")
        pub setIncrement(v: f64)
        @objc("doubleValue")
        pub get doubleValue(): f64
        @objc("setDoubleValue:")
        pub set doubleValue(v: f64)
        @objc("setValueWraps:")
        pub setValueWraps(flag: bool)
        /// Target/action — inherited from `NSControl` but bound
        /// here explicitly so `NumericUpDown` can wire the
        /// stepper's click to its own handler.
        @objc("setTarget:")
        pub setTarget(target: NSObject)
        @objc("setAction:")
        pub setAction(sel: i64)
        /// `setContinuous:` — same NSControl path; fires the
        /// action repeatedly while the user holds the button.
        @objc("setContinuous:")
        pub setContinuous(flag: bool)
        @objc("setEnabled:")
        pub setEnabled(flag: bool)
    }

    /// `NSPopUpButton` — drop-down menu button. `pullsDown=true`
    /// turns it into the menu-button shape (title fixed, items
    /// drop down underneath); `false` is the standard pop-up
    /// (title reflects the selected item).
    @objc pub class NSPopUpButton : NSButton {
        @objc("alloc")
        pub static alloc(): NSPopUpButton
        @objc("initWithFrame:pullsDown:")
        pub init(rect: NSRect, pullsDown: bool): NSPopUpButton

        @objc("addItemWithTitle:")
        pub addItemWithTitle(title: NSString)
        @objc("addItemsWithTitles:")
        pub addItemsWithTitles(titles: NSArray)
        @objc("removeAllItems")
        pub removeAllItems()
        @objc("numberOfItems")
        pub get numberOfItems(): i64
        @objc("indexOfSelectedItem")
        pub get indexOfSelectedItem(): i64
        @objc("titleOfSelectedItem")
        pub get titleOfSelectedItem(): NSString
        @objc("selectItemAtIndex:")
        pub selectItemAtIndex(i: i64)
        @objc("selectItemWithTitle:")
        pub selectItemWithTitle(s: NSString)
    }

    /// `NSComboBox` — editable text field with a drop-down list
    /// of suggestions. Reads its current value via `stringValue`
    /// (inherited from `NSTextField`).
    @objc pub class NSComboBox : NSTextField {
        @objc("alloc")
        pub static alloc(): NSComboBox
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSComboBox

        @objc("addItemWithObjectValue:")
        pub addItemWithObjectValue(o: NSObject)
        @objc("removeAllItems")
        pub removeAllItems()
        @objc("numberOfItems")
        pub get numberOfItems(): i64
        @objc("indexOfSelectedItem")
        pub get indexOfSelectedItem(): i64
        @objc("selectItemAtIndex:")
        pub selectItemAtIndex(i: i64)
        @objc("setNumberOfVisibleItems:")
        pub setNumberOfVisibleItems(n: i64)
    }

    /// `NSProgressIndicator` — bar or spinner. Determinate mode
    /// (default for `.bar`) reads `doubleValue` against the
    /// configured `[min, max]` range; indeterminate mode
    /// requires `startAnimation:` / `stopAnimation:` to spin.
    @objc pub class NSProgressIndicator : NSView {
        @objc("alloc")
        pub static alloc(): NSProgressIndicator
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSProgressIndicator

        @objc("setStyle:")
        pub setStyle(s: NSProgressIndicatorStyle)
        @objc("setMinValue:")
        pub setMinValue(v: f64)
        @objc("setMaxValue:")
        pub setMaxValue(v: f64)
        @objc("doubleValue")
        pub get doubleValue(): f64
        @objc("setDoubleValue:")
        pub set doubleValue(v: f64)
        @objc("setIndeterminate:")
        pub setIndeterminate(flag: bool)
        @objc("startAnimation:")
        pub startAnimation(sender: NSObject)
        @objc("stopAnimation:")
        pub stopAnimation(sender: NSObject)
    }

    /// `NSSegmentedControl` — row of buttons with shared
    /// selection. After `setSegmentCount:`, label each slot with
    /// `setLabel:forSegment:`; read the active one via
    /// `selectedSegment`.
    @objc pub class NSSegmentedControl : NSView {
        @objc("alloc")
        pub static alloc(): NSSegmentedControl
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSSegmentedControl

        @objc("segmentCount")
        pub get segmentCount(): i64
        @objc("setSegmentCount:")
        pub set segmentCount(n: i64)
        @objc("setLabel:forSegment:")
        pub setLabel(label: NSString, forSegment: i64)
        @objc("labelForSegment:")
        pub labelForSegment(seg: i64): NSString
        @objc("setSelected:forSegment:")
        pub setSelected(flag: bool, forSegment: i64)
        @objc("isSelectedForSegment:")
        pub isSelectedForSegment(seg: i64): bool
        @objc("selectedSegment")
        pub selectedSegment(): i64
        @objc("setSegmentedTrackingMode:")
        pub setTrackingMode(m: NSSegmentSwitchTracking)
    }

    /// `NSDatePicker` — graphical or text-field date editor.
    /// Defaults to `cocoa.NSDatePickerStyle.textFieldAndStepper`;
    /// switch to `.clockAndCalendar` for the round-clock /
    /// month-calendar combo.
    @objc pub class NSDatePicker : NSView {
        @objc("alloc")
        pub static alloc(): NSDatePicker
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSDatePicker

        @objc("setDatePickerStyle:")
        pub setDatePickerStyle(s: NSDatePickerStyle)
        @objc("dateValue")
        pub get dateValue(): NSDate
        @objc("setDateValue:")
        pub set dateValue(d: NSDate)
        @objc("setMinDate:")
        pub setMinDate(d: NSDate)
        @objc("setMaxDate:")
        pub setMaxDate(d: NSDate)
    }

    /// `NSBox` — titled / bordered container view. Wraps a
    /// `contentView` you set yourself (default is a blank
    /// `NSView`).
    @objc pub class NSBox : NSView {
        @objc("alloc")
        pub static alloc(): NSBox
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSBox

        @objc("setTitle:")
        pub setTitle(s: NSString)
        @objc("setBoxType:")
        pub setBoxType(t: u64)
        @objc("contentView")
        pub get contentView(): NSView
        @objc("setContentView:")
        pub set contentView(v: NSView)
    }

    // ─── Text-field delegate protocols ──────────────────────────

    /// `NSTextFieldDelegate` — text-field-specific edit callbacks.
    /// Inherits the more general
    /// `NSControlTextEditingDelegate` (declared below); use
    /// either protocol depending on whether the methods you care
    /// about are NSTextField-specific or shared with other
    /// NSControl subclasses.
    @objc pub interface NSTextFieldDelegate {
        controlTextDidBeginEditing?(notification: NSObject)
        controlTextDidChange?(notification: NSObject)
        controlTextDidEndEditing?(notification: NSObject)
    }

    /// `NSControlTextEditingDelegate` — generic text-editing
    /// callbacks for any NSControl that wraps a field editor
    /// (NSTextField, NSSearchField, NSComboBox, …).
    /// `control:textShouldBeginEditing:` and friends let the
    /// delegate veto edits, intercept commands, etc.
    @objc pub interface NSControlTextEditingDelegate {
        @objc("control:textShouldBeginEditing:")
        controlTextShouldBeginEditing?(control: NSObject, fieldEditor: NSObject): bool
        @objc("control:textShouldEndEditing:")
        controlTextShouldEndEditing?(control: NSObject, fieldEditor: NSObject): bool
        @objc("control:isValidObject:")
        controlIsValidObject?(control: NSObject, obj: NSObject): bool
        @objc("control:didFailToFormatString:errorDescription:")
        controlDidFailToFormatString?(
            control: NSObject, string: NSString, errorDescription: NSString
        ): bool
        @objc("control:didFailToValidatePartialString:errorDescription:")
        controlDidFailToValidatePartialString?(
            control: NSObject, string: NSString, errorDescription: NSString
        )
    }

    // ─── Convenience helpers ─────────────────────────────────────

    /// Allocate + frame + title a push button. Defaults to the
    /// rounded `push` bezel — the standard Mac-style push button.
    pub fn makeButton(
        title: string,
        x: f64, y: f64, w: f64, h: f64
    ): NSButton {
        let btn = NSButton.alloc().init(makeRect(x, y, w, h))
        btn.setTitle(nsstr(title))
        btn.setBezelStyle(NSBezelStyle.push)
        btn
    }

    /// Plain non-editable label backed by an `NSTextField`. Flips
    /// off `editable` / `bordered` / `bezeled` / `drawsBackground`
    /// so it reads as static text rather than an input.
    ///
    /// `selectable` toggles `NSTextField.setSelectable`. AppKit
    /// hangs an I-beam cursor rect off any selectable field, so
    /// passing `false` keeps the arrow cursor over labels at the
    /// cost of disabling text selection / copy.
    pub fn makeLabel(
        text: string,
        x: f64, y: f64, w: f64, h: f64,
        selectable: bool
    ): NSTextField {
        let f = NSTextField.alloc().init(makeRect(x, y, w, h))
        f.stringValue = nsstr(text)
        f.setEditable(false)
        f.setBordered(false)
        f.setBezeled(false)
        f.setDrawsBackground(false)
        f.setSelectable(selectable)
        f
    }

    /// Editable single-line text input pre-seeded with `text`.
    pub fn makeTextField(
        text: string,
        x: f64, y: f64, w: f64, h: f64
    ): NSTextField {
        let f = NSTextField.alloc().init(makeRect(x, y, w, h))
        f.stringValue = nsstr(text)
        f.setEditable(true)
        f.setBezeled(true)
        f
    }

    /// Horizontal slider over `[min, max]` initialised to `value`.
    pub fn makeSlider(
        min: f64, max: f64, value: f64,
        x: f64, y: f64, w: f64, h: f64
    ): NSSlider {
        let s = NSSlider.alloc().init(makeRect(x, y, w, h))
        s.setMinValue(min)
        s.setMaxValue(max)
        s.doubleValue = value
        s
    }

    /// `NSPopUpButton` pre-populated with `items`. The first
    /// entry is selected by default.
    pub fn makePopUp(items: NSArray, x: f64, y: f64, w: f64, h: f64): NSPopUpButton {
        let p = NSPopUpButton.alloc().init(makeRect(x, y, w, h), false)
        p.addItemsWithTitles(items)
        p
    }

    /// Determinate horizontal progress bar over `[min, max]`.
    pub fn makeProgressBar(
        min: f64, max: f64, value: f64,
        x: f64, y: f64, w: f64, h: f64
    ): NSProgressIndicator {
        let p = NSProgressIndicator.alloc().init(makeRect(x, y, w, h))
        p.setStyle(NSProgressIndicatorStyle.bar)
        p.setIndeterminate(false)
        p.setMinValue(min)
        p.setMaxValue(max)
        p.doubleValue = value
        p
    }
}
