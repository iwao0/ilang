/// Cross-platform GUI event types. Pure data — no FFI here; the
/// per-control modules construct these inside their `setOnXxx`
/// closures and dispatch them through `events.Signal<T>`.
///
/// `EventTarget` is the marker base every emitter (today: `Window`
/// + every concrete `Control`) inherits. Concrete event classes
/// carry the payload the listener receives.

/// Marker base class for every object that can be the `sender` of
/// a GUI `Event`. Empty by design — concrete event-emitters
/// inherit it so their instances flow into `Event.sender` without
/// per-event `as` casts on the library side. Users typically
/// downcast (`e.sender as Window`) at the call site when they need
/// the concrete type.
pub class EventTarget {
    pub init() {}
}

/// Base class for every GUI event. The `sender` field is set by
/// the library when the event fires and points at the
/// `EventTarget` that produced it.
pub class Event {
    pub sender: EventTarget
    pub init(sender: EventTarget) {
        this.sender = sender
    }
}

/// Fired when the user closes the window (clicks the close box,
/// uses ⌘W / Alt+F4, etc.). Carries no extra payload beyond the
/// inherited `sender`.
pub class CloseEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}

/// Window state-transition events. Each fires once per
/// transition (entering, not staying); no extra payload beyond
/// the inherited `sender` (the source `Window`).
///
///   * `MinimizeEvent`        — collapsed to the OS task-bar / Dock
///   * `RestoreEvent`         — un-minimised back to the normal frame
///   * `MaximizeEvent`        — zoomed / maximised to the screen
///   * `UnmaximizeEvent`      — un-zoomed back to the user frame
///   * `EnterFullScreenEvent` — entered native full-screen mode
///   * `LeaveFullScreenEvent` — exited native full-screen mode
pub class MinimizeEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}
pub class RestoreEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}
pub class MaximizeEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}
pub class UnmaximizeEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}
pub class EnterFullScreenEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}
pub class LeaveFullScreenEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}

/// Fired when the window gains / loses keyboard focus (becomes
/// the key window on macOS, gains foreground on Win32, becomes
/// active on GTK). 0-payload — the sender carries the source.
pub class FocusEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}
pub class BlurEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}

/// Fired when `Window.show()` / `Window.hide()` flips the
/// window's visibility (programmatic only — there is no OS hook
/// for "user revealed the window", so external means like the
/// Dock icon don't fire these). 0-payload.
pub class ShowEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}
pub class HideEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}

/// Fired when the window is moved on screen. `x` / `y` are the
/// new top-left position in screen coordinates (top-left origin
/// on Win32 and on macOS after AppKit-to-W3C y-inversion). GTK4
/// has no portable position API and never fires this event on
/// Linux.
pub class MoveEvent : Event {
    pub x: f64
    pub y: f64
    pub init(sender: EventTarget, x: f64, y: f64) {
        super(sender)
        this.x = x
        this.y = y
    }
}

/// Fired when the window's drawable content area changes size.
/// `width` / `height` are the new client-area dimensions in
/// device-independent points.
pub class SizeEvent : Event {
    pub width: f64
    pub height: f64
    pub init(sender: EventTarget, width: f64, height: f64) {
        super(sender)
        this.width = width
        this.height = height
    }
}

/// W3C UI Events `KeyboardEvent.code` values. The discriminant
/// string matches the standard (e.g. `KeyCode.keyA as string ==
/// "KeyA"`) — same identifier the DOM exposes — so user code can
/// compare `e.code` against the enum variants or read the string
/// form for logging:
///
///     if e.code == gui.KeyCode.enter { ... }
///     console.log("code =", e.code as string)
///
/// `unknown` (= `""`) is the fallback for keys the backend
/// doesn't map yet; user code should usually treat it like a
/// "did-not-fire" no-op.
pub enum KeyCode: string {
    unknown    = ""
    keyA       = "KeyA"
    keyB       = "KeyB"
    keyC       = "KeyC"
    keyD       = "KeyD"
    keyE       = "KeyE"
    keyF       = "KeyF"
    keyG       = "KeyG"
    keyH       = "KeyH"
    keyI       = "KeyI"
    keyJ       = "KeyJ"
    keyK       = "KeyK"
    keyL       = "KeyL"
    keyM       = "KeyM"
    keyN       = "KeyN"
    keyO       = "KeyO"
    keyP       = "KeyP"
    keyQ       = "KeyQ"
    keyR       = "KeyR"
    keyS       = "KeyS"
    keyT       = "KeyT"
    keyU       = "KeyU"
    keyV       = "KeyV"
    keyW       = "KeyW"
    keyX       = "KeyX"
    keyY       = "KeyY"
    keyZ       = "KeyZ"
    digit0     = "Digit0"
    digit1     = "Digit1"
    digit2     = "Digit2"
    digit3     = "Digit3"
    digit4     = "Digit4"
    digit5     = "Digit5"
    digit6     = "Digit6"
    digit7     = "Digit7"
    digit8     = "Digit8"
    digit9     = "Digit9"
    enter      = "Enter"
    escape     = "Escape"
    backspace  = "Backspace"
    tab        = "Tab"
    space      = "Space"
    arrowLeft  = "ArrowLeft"
    arrowRight = "ArrowRight"
    arrowUp    = "ArrowUp"
    arrowDown  = "ArrowDown"
}

/// Keyboard event. `code` is the W3C UI Events
/// `KeyboardEvent.code` string. Compare against `KeyCode` enum
/// variants via their string form (`e.code == KeyCode.keyA as
/// string`) — the discriminant strings are identical to the
/// values backends emit, so no conversion happens at runtime.
/// The four `*Key` flags mirror the DOM `KeyboardEvent.ctrlKey`
/// / `shiftKey` / `altKey` / `metaKey` booleans.
pub class KeyEvent : Event {
    pub code: string
    pub ctrlKey: bool
    pub shiftKey: bool
    pub altKey: bool
    pub metaKey: bool
    pub init(
        sender: EventTarget, code: string,
        ctrlKey: bool, shiftKey: bool, altKey: bool, metaKey: bool
    ) {
        super(sender)
        this.code = code
        this.ctrlKey = ctrlKey
        this.shiftKey = shiftKey
        this.altKey = altKey
        this.metaKey = metaKey
    }
}

/// Mouse event. `x` / `y` are in the source widget's local
/// coordinates (window client area for `Window`; view-local for
/// `GpuView`), with the origin at the top-left (W3C convention).
/// `button` matches `MouseEvent.button`: `0 = left`, `1 = middle`,
/// `2 = right`, `3 = back`, `4 = forward`. Modifier flags as in
/// `KeyEvent`. `wheelDx` / `wheelDy` carry the scroll delta for
/// wheel events (units are "lines" in the W3C sense — backends
/// normalise to roughly ±1 per detent); both are `0.0` for
/// non-wheel events.
pub class MouseEvent : Event {
    pub x: f64
    pub y: f64
    pub button: i32
    pub ctrlKey: bool
    pub shiftKey: bool
    pub altKey: bool
    pub metaKey: bool
    pub wheelDx: f64
    pub wheelDy: f64
    pub init(
        sender: EventTarget, x: f64, y: f64, button: i32,
        ctrlKey: bool, shiftKey: bool, altKey: bool, metaKey: bool
    ) {
        super(sender)
        this.x = x
        this.y = y
        this.button = button
        this.ctrlKey = ctrlKey
        this.shiftKey = shiftKey
        this.altKey = altKey
        this.metaKey = metaKey
        this.wheelDx = 0.0
        this.wheelDy = 0.0
    }
}

/// Fired when an editable control's text changes. `value` is the
/// full new contents (UTF-8). For `TextField` this fires on every
/// keystroke (DOM `oninput` semantics, not `onchange`).
pub class ChangeEvent : Event {
    pub value: string
    pub init(sender: EventTarget, value: string) {
        super(sender)
        this.value = value
    }
}

/// Fired when a `Checkbox` (or `RadioButton`) flips. `checked` is
/// the new state.
pub class ToggleEvent : Event {
    pub checked: bool
    pub init(sender: EventTarget, checked: bool) {
        super(sender)
        this.checked = checked
    }
}

/// Fired continuously while a `Slider` is being dragged. `value`
/// is the current slider position.
pub class SliderEvent : Event {
    pub value: f64
    pub init(sender: EventTarget, value: f64) {
        super(sender)
        this.value = value
    }
}

/// Fired when a `ComboBox` or `ListBox` selection changes.
/// `index` is the new 0-based slot; `value` is the matching item's
/// display string.
pub class SelectEvent : Event {
    pub index: i32
    pub value: string
    pub init(sender: EventTarget, index: i32, value: string) {
        super(sender)
        this.index = index
        this.value = value
    }
}

/// Fired when a `TabView`'s selected tab changes. `index` is the
/// new 0-based selected slot.
pub class TabChangeEvent : Event {
    pub index: i32
    pub init(sender: EventTarget, index: i32) {
        super(sender)
        this.index = index
    }
}

/// Fired when a `TreeView`'s selected node changes. `node` is the
/// new `TreeNode` (or `none` if the selection was cleared).
/// Defined here so user code can `use gui { TreeSelectEvent }`
/// without pulling in `treeview` directly; the `node` payload is
/// typed as `EventTarget` because `TreeNode` lives in `treeview.il`
/// and would create an import cycle — downcast with
/// `e.node as TreeNode` at the call site.
pub class TreeSelectEvent : Event {
    pub node: EventTarget?
    pub init(sender: EventTarget, node: EventTarget?) {
        super(sender)
        this.node = node
    }
}

/// Fired when a `TreeNode` is expanded or collapsed by the user.
/// `node` is the `TreeNode` that changed state — downcast with
/// `e.node as TreeNode` at the call site. Same payload for
/// `onExpand` and `onCollapse`; the signal identifies the
/// direction.
pub class TreeExpandEvent : Event {
    pub node: EventTarget
    pub init(sender: EventTarget, node: EventTarget) {
        super(sender)
        this.node = node
    }
}

/// Fired when a `TreeNode`'s checkbox is toggled (only on nodes
/// with `checkable = true`). `checked` is the new state.
pub class TreeCheckEvent : Event {
    pub node: EventTarget
    pub checked: bool
    pub init(sender: EventTarget, node: EventTarget, checked: bool) {
        super(sender)
        this.node = node
        this.checked = checked
    }
}

/// Fired when a `Table`'s selected row changes. `row` is the
/// new 0-based row index (`-1` if the selection was cleared);
/// `rowData` is the matching row's cell strings (empty array on
/// a cleared selection).
pub class TableSelectEvent : Event {
    pub row: i32
    pub rowData: string[]
    pub init(sender: EventTarget, row: i32, rowData: string[]) {
        super(sender)
        this.row = row
        this.rowData = rowData
    }
}

/// Fired when the user activates a `Table` row — typically a
/// double-click on macOS / Win32, an `activate` signal (Enter /
/// double-click) on GTK. `row` and `rowData` mirror
/// `TableSelectEvent`.
pub class TableActivateEvent : Event {
    pub row: i32
    pub rowData: string[]
    pub init(sender: EventTarget, row: i32, rowData: string[]) {
        super(sender)
        this.row = row
        this.rowData = rowData
    }
}

/// Fired when a `NumericUpDown`'s value changes — either via
/// the up/down stepper buttons or by the user committing a new
/// number into the embedded text field. `value` is the new
/// numeric value already clamped to the `[min, max]` range.
pub class NumericChangeEvent : Event {
    pub value: f64
    pub init(sender: EventTarget, value: f64) {
        super(sender)
        this.value = value
    }
}

/// Fired when a `DatePicker`'s value changes. `year` / `month`
/// / `day` are the new calendar fields the user picked. The
/// `DatePicker` itself stores a full `std.time.DateTime` and
/// reads these back through the event; `year` / `month` /
/// `day` are split out here so listeners that don't care about
/// the wider time-of-day fields don't have to deal with them.
pub class DateChangeEvent : Event {
    pub year:  i32
    pub month: i32
    pub day:   i32
    pub init(sender: EventTarget, year: i32, month: i32, day: i32) {
        super(sender)
        this.year  = year
        this.month = month
        this.day   = day
    }
}

/// Fired when a `TimePicker`'s value changes. `hour` /
/// `minute` / `second` carry the new wall-clock time the user
/// committed. `second` is `0` on pickers built with
/// `showSeconds = false`.
pub class TimeChangeEvent : Event {
    pub hour:   i32
    pub minute: i32
    pub second: i32
    pub init(sender: EventTarget, hour: i32, minute: i32, second: i32) {
        super(sender)
        this.hour   = hour
        this.minute = minute
        this.second = second
    }
}

/// Fired when files are dropped onto a window or widget. `files`
/// holds the absolute paths in the order the OS reported them;
/// `x` / `y` are the drop coordinates in the receiver's local
/// (top-left origin) space.
pub class FilesDroppedEvent : Event {
    pub files: string[]
    pub x:     f64
    pub y:     f64
    pub init(sender: EventTarget, files: string[], x: f64, y: f64) {
        super(sender)
        this.files = files
        this.x = x
        this.y = y
    }
}
