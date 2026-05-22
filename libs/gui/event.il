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

/// Keyboard event. `code` is the W3C UI Events
/// `KeyboardEvent.code` string (e.g. `"KeyA"`, `"Enter"`,
/// `"ArrowLeft"`, `"ShiftLeft"`) — platform-independent and
/// independent of the active layout. The four `*Key` flags
/// mirror the DOM `KeyboardEvent.ctrlKey` / `shiftKey` / `altKey`
/// / `metaKey` booleans.
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

/// Mouse event. `x` / `y` are in the window's client area, with
/// the origin at the top-left (W3C convention). `button` matches
/// `MouseEvent.button`: `0 = left`, `1 = middle`, `2 = right`,
/// `3 = back`, `4 = forward`. Modifier flags as in `KeyEvent`.
pub class MouseEvent : Event {
    pub x: f64
    pub y: f64
    pub button: i32
    pub ctrlKey: bool
    pub shiftKey: bool
    pub altKey: bool
    pub metaKey: bool
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
