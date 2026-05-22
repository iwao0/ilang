/// Cross-platform GUI library — minimal slice (window + lifecycle).
///
/// ilang's `[deps] @target` filter swaps `gui_impl` between
/// `gui-cocoa` (macOS) and `gui-win32` (Windows). User code only
/// imports `gui`; the OS-specific package never appears at the
/// call site.
///
///   use gui
///   let w = new gui.Window("hello", 480.0, 320.0)
///   w.onClose.add(fn(e: gui.CloseEvent) { console.log("bye") })
///   gui.run()

use events { Signal }
use gui_impl {
    createNativeWindow,
    setOnClose,
    setOnResize,
    setOnKeyDown,
    setOnKeyUp,
    setOnMouseMove,
    setOnMouseDown,
    setOnMouseUp,
    createNativeButton,
    setOnButtonClick,
    setButtonText,
    setButtonFrame,
    setButtonEnabled,
    setButtonVisible,
    createNativeLabel,
    setLabelText,
    setLabelFrame,
    setLabelVisible,
    createNativeEdit,
    readEditText,
    setOnEditChange,
    setEditText,
    setEditFrame,
    setEditEnabled,
    setEditVisible,
    createNativeCheckbox,
    setOnCheckboxToggle,
    setCheckboxText,
    setCheckboxChecked,
    readCheckboxChecked,
    setCheckboxFrame,
    setCheckboxEnabled,
    setCheckboxVisible,
    createNativeSlider,
    setOnSliderChange,
    setSliderRange,
    setSliderValue,
    readSliderValue,
    setSliderFrame,
    setSliderEnabled,
    setSliderVisible,
    createNativeProgress,
    setProgressRange,
    setProgressValue,
    setProgressFrame,
    setProgressVisible,
    createNativeCombo,
    setOnComboSelect,
    addComboItem,
    clearComboItems,
    setComboSelectedIndex,
    readComboSelectedIndex,
    setComboFrame,
    setComboEnabled,
    setComboVisible,
    createNativeRadio,
    setOnRadioToggle,
    setRadioText,
    setRadioChecked,
    readRadioChecked,
    setRadioFrame,
    setRadioEnabled,
    setRadioVisible,
    createNativeList,
    setOnListSelect,
    addListItem,
    clearListItems,
    setListSelectedIndex,
    readListSelectedIndex,
    setListFrame,
    setListEnabled,
    setListVisible,
    createNativeGroupBox,
    setGroupBoxText,
    setGroupBoxFrame,
    setGroupBoxVisible,
    runEventLoop,
}

/// Marker base class for every object that can be the `sender`
/// of a GUI `Event`. Empty by design — concrete event-emitters
/// (today: `Window`) inherit it so their instances flow into
/// `Event.sender` without per-event `as` casts on the library
/// side. Users typically downcast (`e.sender as Window`) at the
/// call site when they need the concrete type.
pub class EventTarget {
    pub init() {}
}

/// Base class for every GUI event. The `sender` field is set by
/// the library when the event fires and points at the
/// `EventTarget` that produced it (today always the originating
/// `Window`).
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

/// Fired when a `Checkbox` flips. `checked` is the new state.
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

/// Fired when a `ComboBox` selection changes. `index` is the new
/// 0-based slot; `value` is the matching item's display string.
pub class SelectEvent : Event {
    pub index: i32
    pub value: string
    pub init(sender: EventTarget, index: i32, value: string) {
        super(sender)
        this.index = index
        this.value = value
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

/// Forward-referenced by `Button.attachTo`. The actual class
/// declaration follows below; ilang requires the parent to be
/// declared before a subclass, so `Button : EventTarget` /
/// `Window : EventTarget` can interleave so long as the parent
/// (`EventTarget`) is already in scope by then.
///
/// A top-level OS window. Construct with `new Window(title, w, h)`;
/// the window appears immediately but no events fire until
/// `gui.run()` is called.
///
/// Each `on*` field is a `Signal<T>` users register on with
/// `w.onClose.add(fn(e) { ... })`. Listeners fire on the main
/// thread synchronously from inside `gui.run()`.
pub class Window : EventTarget {
    pub handle: i64
    pub onClose:     Signal<CloseEvent>
    pub onResize:    Signal<SizeEvent>
    pub onKeyDown:   Signal<KeyEvent>
    pub onKeyUp:     Signal<KeyEvent>
    pub onMouseMove: Signal<MouseEvent>
    pub onMouseDown: Signal<MouseEvent>
    pub onMouseUp:   Signal<MouseEvent>

    pub init(title: string, width: f64, height: f64) {
        super()
        this.handle      = createNativeWindow(title, width, height)
        this.onClose     = new Signal<CloseEvent>()
        this.onResize    = new Signal<SizeEvent>()
        this.onKeyDown   = new Signal<KeyEvent>()
        this.onKeyUp     = new Signal<KeyEvent>()
        this.onMouseMove = new Signal<MouseEvent>()
        this.onMouseDown = new Signal<MouseEvent>()
        this.onMouseUp   = new Signal<MouseEvent>()
        // Forward native callbacks to each Signal. The closures
        // capture `this` so the same Window flows through as
        // `event.sender`.
        setOnClose(this.handle, fn() {
            this.onClose.emit(new CloseEvent(this))
        })
        setOnResize(this.handle, fn(w: f64, h: f64) {
            this.onResize.emit(new SizeEvent(this, w, h))
        })
        setOnKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnMouseMove(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseMove.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
    }

    /// Attach a `Button` child to this window. The native OS
    /// control is realized on this call; property setters invoked
    /// before `addChild` apply on realization, those invoked after
    /// propagate immediately.
    pub addChild(child: Button) {
        child.attachTo(this)
    }
    /// Attach a `Label` child to this window. Same realization
    /// semantics as the `Button` overload above.
    pub addChild(child: Label) {
        child.attachTo(this)
    }
    /// Attach a `TextField` child to this window. Same realization
    /// semantics as the `Button` overload above.
    pub addChild(child: TextField) {
        child.attachTo(this)
    }
    pub addChild(child: Checkbox)    { child.attachTo(this) }
    pub addChild(child: Slider)      { child.attachTo(this) }
    pub addChild(child: ProgressBar) { child.attachTo(this) }
    pub addChild(child: ComboBox)    { child.attachTo(this) }
    pub addChild(child: RadioButton) { child.attachTo(this) }
    pub addChild(child: ListBox)     { child.attachTo(this) }
    pub addChild(child: GroupBox)    { child.attachTo(this) }
}

/// A push-button widget. Construct unparented, configure with
/// property setters, then attach with `window.addChild(button)`.
/// Properties set before `addChild` are cached on the Button and
/// applied to the native control once it's realized; properties
/// set after `addChild` propagate immediately.
///
///   let b = new gui.Button()
///   b.text = "OK"
///   b.x = 100.0
///   b.y = 50.0
///   b.width  = 80.0
///   b.height = 30.0
///   b.onClick.add(fn(e: gui.MouseEvent) { console.log("clicked") })
///   window.addChild(b)
pub class Button : EventTarget {
    // Cached state. `handle` is -1 until `addChild` realizes the
    // native control; before then, setters only update the cache.
    handle: i64
    _text:      string
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    pub onClick: Signal<MouseEvent>

    pub init() {
        super()
        this.handle      = -1
        this._text       = ""
        this._x          = 0.0
        this._y          = 0.0
        this._width      = 80.0
        this._height     = 30.0
        this._isEnabled  = true
        this._isVisible  = true
        this.onClick     = new Signal<MouseEvent>()
    }

    pub get text(): string { this._text }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setButtonText(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setButtonFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setButtonEnabled(this.handle, v) }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setButtonVisible(this.handle, v) }
    }

    /// Realize the native control under `window`. Called by
    /// `Window.addChild(this)`; not part of the public surface
    /// the user invokes directly.
    pub attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeButton(
            window.handle,
            this._text, this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        // Forward native click → onClick. Native pos is (centre, 0)
        // on macOS and (0, 0) on win32; we surface it as a MouseEvent
        // with `sender = this` for downcast.
        setOnButtonClick(this.handle, fn(x: f64, y: f64, b: i32) {
            this.onClick.emit(new MouseEvent(this, x, y, b, false, false, false, false))
        })
    }
}

/// A read-only text label. Backed by `STATIC` on win32 and a
/// configured `NSTextField` on macOS. No event surface — labels
/// just display.
///
///   let l = new gui.Label()
///   l.text   = "Name:"
///   l.x      = 20.0
///   l.y      = 24.0
///   l.width  = 80.0
///   l.height = 20.0
///   window.addChild(l)
pub class Label : EventTarget {
    handle: i64
    _text:      string
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isVisible: bool

    pub init() {
        super()
        this.handle     = -1
        this._text      = ""
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 80.0
        this._height    = 20.0
        this._isVisible = true
    }

    pub get text(): string { this._text }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setLabelText(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setLabelFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setLabelVisible(this.handle, v) }
    }

    pub attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeLabel(
            window.handle,
            this._text, this._x, this._y, this._width, this._height,
            this._isVisible
        )
    }
}

/// A single-line editable text field. Backed by the `EDIT`
/// control class on win32 and a bezeled `NSTextField` on macOS.
/// `onChange` fires on every keystroke (DOM `oninput` semantics),
/// not just on commit, so live previews work without polling.
///
///   let t = new gui.TextField()
///   t.text   = "hello"
///   t.x      = 120.0
///   t.y      = 20.0
///   t.width  = 200.0
///   t.height = 24.0
///   t.onChange.add(fn(e: gui.ChangeEvent) { console.log(e.value) })
///   window.addChild(t)
pub class TextField : EventTarget {
    handle: i64
    _text:      string
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    pub onChange: Signal<ChangeEvent>

    pub init() {
        super()
        this.handle     = -1
        this._text      = ""
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 160.0
        this._height    = 24.0
        this._isEnabled = true
        this._isVisible = true
        this.onChange   = new Signal<ChangeEvent>()
    }

    pub get text(): string {
        // Once attached, the native control is the source of truth
        // — the user may have typed since the last setter call.
        if this.handle >= 0 {
            this._text = readEditText(this.handle)
        }
        this._text
    }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setEditText(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setEditFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setEditEnabled(this.handle, v) }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setEditVisible(this.handle, v) }
    }

    pub attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeEdit(
            window.handle,
            this._text, this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnEditChange(this.handle, fn(v: string) {
            this._text = v
            this.onChange.emit(new ChangeEvent(this, v))
        })
    }
}

/// A boolean toggle with an inline title. Win32: `BUTTON` class
/// with `BS_AUTOCHECKBOX`; Cocoa: `NSButton` with
/// `setButtonType(NSButtonTypeSwitch)`. `onToggle` fires whenever
/// the checked state flips.
pub class Checkbox : EventTarget {
    handle: i64
    _text:      string
    _checked:   bool
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    pub onToggle: Signal<ToggleEvent>

    pub init() {
        super()
        this.handle     = -1
        this._text      = ""
        this._checked   = false
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 160.0
        this._height    = 22.0
        this._isEnabled = true
        this._isVisible = true
        this.onToggle   = new Signal<ToggleEvent>()
    }

    pub get text(): string { this._text }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setCheckboxText(this.handle, v) }
    }
    pub get checked(): bool {
        if this.handle >= 0 { this._checked = readCheckboxChecked(this.handle) }
        this._checked
    }
    pub set checked(v: bool) {
        this._checked = v
        if this.handle >= 0 { setCheckboxChecked(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setCheckboxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setCheckboxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setCheckboxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setCheckboxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setCheckboxEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setCheckboxVisible(this.handle, v) }
    }

    pub attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeCheckbox(
            window.handle,
            this._text, this._checked,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnCheckboxToggle(this.handle, fn(checked: bool) {
            this._checked = checked
            this.onToggle.emit(new ToggleEvent(this, checked))
        })
    }
}

/// Continuous numeric slider over `[min, max]`. Win32:
/// `msctls_trackbar32`; Cocoa: `NSSlider` (continuous). `onChange`
/// fires live while the thumb is being dragged.
pub class Slider : EventTarget {
    handle: i64
    _min:       f64
    _max:       f64
    _value:     f64
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    pub onChange: Signal<SliderEvent>

    pub init() {
        super()
        this.handle     = -1
        this._min       = 0.0
        this._max       = 100.0
        this._value     = 0.0
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 200.0
        this._height    = 26.0
        this._isEnabled = true
        this._isVisible = true
        this.onChange   = new Signal<SliderEvent>()
    }

    pub get min(): f64 { this._min }
    pub set min(v: f64) {
        this._min = v
        if this.handle >= 0 { setSliderRange(this.handle, this._min, this._max) }
    }
    pub get max(): f64 { this._max }
    pub set max(v: f64) {
        this._max = v
        if this.handle >= 0 { setSliderRange(this.handle, this._min, this._max) }
    }
    pub get value(): f64 {
        if this.handle >= 0 { this._value = readSliderValue(this.handle) }
        this._value
    }
    pub set value(v: f64) {
        this._value = v
        if this.handle >= 0 { setSliderValue(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setSliderFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setSliderFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setSliderFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setSliderFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setSliderEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setSliderVisible(this.handle, v) }
    }

    pub attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeSlider(
            window.handle,
            this._min, this._max, this._value,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        setOnSliderChange(this.handle, fn(v: f64) {
            this._value = v
            this.onChange.emit(new SliderEvent(this, v))
        })
    }
}

/// Read-only progress bar. Win32: `msctls_progress32`; Cocoa:
/// `NSProgressIndicator` (determinate bar). Set `value` within
/// `[min, max]` to advance.
pub class ProgressBar : EventTarget {
    handle: i64
    _min:       f64
    _max:       f64
    _value:     f64
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isVisible: bool

    pub init() {
        super()
        this.handle     = -1
        this._min       = 0.0
        this._max       = 100.0
        this._value     = 0.0
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 200.0
        this._height    = 18.0
        this._isVisible = true
    }

    pub get min(): f64 { this._min }
    pub set min(v: f64) {
        this._min = v
        if this.handle >= 0 { setProgressRange(this.handle, this._min, this._max) }
    }
    pub get max(): f64 { this._max }
    pub set max(v: f64) {
        this._max = v
        if this.handle >= 0 { setProgressRange(this.handle, this._min, this._max) }
    }
    pub get value(): f64 { this._value }
    pub set value(v: f64) {
        this._value = v
        if this.handle >= 0 { setProgressValue(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setProgressFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setProgressFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setProgressFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setProgressFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setProgressVisible(this.handle, v) }
    }

    pub attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeProgress(
            window.handle,
            this._min, this._max, this._value,
            this._x, this._y, this._width, this._height,
            this._isVisible
        )
    }
}

/// Drop-down list of pre-populated string items. Win32: `COMBOBOX`
/// with `CBS_DROPDOWNLIST` (read-only); Cocoa: `NSPopUpButton`.
/// `onSelect` fires on each selection change.
///
/// Items added with `addItem` before `addChild` are pushed to the
/// native control on realization; calls after `addChild` propagate
/// immediately.
pub class ComboBox : EventTarget {
    handle: i64
    _items:     string[]
    _selected:  i32
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    pub onSelect: Signal<SelectEvent>

    pub init() {
        super()
        this.handle     = -1
        this._items     = []
        this._selected  = -1
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 160.0
        this._height    = 26.0
        this._isEnabled = true
        this._isVisible = true
        this.onSelect   = new Signal<SelectEvent>()
    }

    /// Append `item` to the dropdown.
    pub addItem(item: string) {
        this._items.push(item)
        if this.handle >= 0 { addComboItem(this.handle, item) }
    }
    /// Drop every item.
    pub clear() {
        this._items = []
        this._selected = -1
        if this.handle >= 0 { clearComboItems(this.handle) }
    }
    /// 0-based index of the currently selected item, or `-1` when
    /// nothing is selected.
    pub get selectedIndex(): i32 {
        if this.handle >= 0 { this._selected = readComboSelectedIndex(this.handle) }
        this._selected
    }
    pub set selectedIndex(v: i32) {
        this._selected = v
        if this.handle >= 0 { setComboSelectedIndex(this.handle, v) }
    }
    pub get selectedValue(): string {
        let i = this.selectedIndex
        if i < 0 || (i as i64) >= this._items.length { return "" }
        this._items[i as i64]
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setComboFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setComboFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setComboFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setComboFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setComboEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setComboVisible(this.handle, v) }
    }

    pub attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeCombo(
            window.handle,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        // Push the cached items / selection.
        let n = this._items.length
        let i: i64 = 0
        while i < n {
            addComboItem(this.handle, this._items[i])
            i = i + 1
        }
        if this._selected >= 0 {
            setComboSelectedIndex(this.handle, this._selected)
        }
        setOnComboSelect(this.handle, fn(idx: i32) {
            this._selected = idx
            let v = if idx >= 0 && (idx as i64) < this._items.length {
                this._items[idx as i64]
            } else {
                ""
            }
            this.onSelect.emit(new SelectEvent(this, idx, v))
        })
    }
}

// Process-wide registry of every `RadioButton` ever constructed.
// `RadioButton.attachTo` appends to this list so the toggle
// handler can walk siblings sharing the same `group` and turn
// them off — mutual exclusion handled entirely in user space so
// neither backend has to round-trip through native group bits.
let RADIOS: RadioButton[] = []

/// A radio button — one member of a mutually-exclusive group.
/// All `RadioButton`s sharing the same `group` string flip off
/// when any one of them is checked.
///
///   let a = new gui.RadioButton(); a.group = "size"; a.text = "S"
///   let b = new gui.RadioButton(); b.group = "size"; b.text = "M"
///   let c = new gui.RadioButton(); c.group = "size"; c.text = "L"
///   a.checked = true
///   window.addChild(a); window.addChild(b); window.addChild(c)
pub class RadioButton : EventTarget {
    handle: i64
    pub group: string
    _text:      string
    _checked:   bool
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    pub onToggle: Signal<ToggleEvent>

    pub init() {
        super()
        this.handle     = -1
        this.group      = ""
        this._text      = ""
        this._checked   = false
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 160.0
        this._height    = 22.0
        this._isEnabled = true
        this._isVisible = true
        this.onToggle   = new Signal<ToggleEvent>()
    }

    pub get text(): string { this._text }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setRadioText(this.handle, v) }
    }
    pub get checked(): bool {
        if this.handle >= 0 { this._checked = readRadioChecked(this.handle) }
        this._checked
    }
    pub set checked(v: bool) {
        this._checked = v
        if this.handle >= 0 { setRadioChecked(this.handle, v) }
        // Programmatic check should also drive sibling mutex.
        if v { this._unselectSiblings() }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setRadioFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setRadioFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setRadioFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setRadioFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setRadioEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setRadioVisible(this.handle, v) }
    }

    _unselectSiblings() {
        let n = RADIOS.length
        let i: i64 = 0
        while i < n {
            let r = RADIOS[i]
            if r.group == this.group && r.handle != this.handle {
                if r.handle >= 0 { setRadioChecked(r.handle, false) }
                r._checked = false
            }
            i = i + 1
        }
    }

    pub attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeRadio(
            window.handle,
            this._text, this._checked,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        RADIOS.push(this)
        setOnRadioToggle(this.handle, fn(checked: bool) {
            this._checked = checked
            if checked { this._unselectSiblings() }
            this.onToggle.emit(new ToggleEvent(this, checked))
        })
    }
}

/// Scrollable single-column list of string items. Win32:
/// `LISTBOX`; Cocoa: `NSTableView` (single column) inside an
/// `NSScrollView`. `onSelect` fires whenever the highlighted row
/// changes.
pub class ListBox : EventTarget {
    handle: i64
    _items:     string[]
    _selected:  i32
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    pub onSelect: Signal<SelectEvent>

    pub init() {
        super()
        this.handle     = -1
        this._items     = []
        this._selected  = -1
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 200.0
        this._height    = 120.0
        this._isEnabled = true
        this._isVisible = true
        this.onSelect   = new Signal<SelectEvent>()
    }

    pub addItem(item: string) {
        this._items.push(item)
        if this.handle >= 0 { addListItem(this.handle, item) }
    }
    pub clear() {
        this._items = []
        this._selected = -1
        if this.handle >= 0 { clearListItems(this.handle) }
    }
    pub get selectedIndex(): i32 {
        if this.handle >= 0 { this._selected = readListSelectedIndex(this.handle) }
        this._selected
    }
    pub set selectedIndex(v: i32) {
        this._selected = v
        if this.handle >= 0 { setListSelectedIndex(this.handle, v) }
    }
    pub get selectedValue(): string {
        let i = this.selectedIndex
        if i < 0 || (i as i64) >= this._items.length { return "" }
        this._items[i as i64]
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setListFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setListFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setListFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setListFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setListEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setListVisible(this.handle, v) }
    }

    pub attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeList(
            window.handle,
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        let n = this._items.length
        let i: i64 = 0
        while i < n {
            addListItem(this.handle, this._items[i])
            i = i + 1
        }
        if this._selected >= 0 {
            setListSelectedIndex(this.handle, this._selected)
        }
        setOnListSelect(this.handle, fn(idx: i32) {
            this._selected = idx
            let v = if idx >= 0 && (idx as i64) < this._items.length {
                this._items[idx as i64]
            } else {
                ""
            }
            this.onSelect.emit(new SelectEvent(this, idx, v))
        })
    }
}

/// Titled frame for visually grouping related controls. Win32:
/// `BUTTON` class with `BS_GROUPBOX`; Cocoa: `NSBox` with a
/// title. Children are NOT reparented — lay them out manually
/// over the frame's bounds.
pub class GroupBox : EventTarget {
    handle: i64
    _text:      string
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isVisible: bool

    pub init() {
        super()
        this.handle     = -1
        this._text      = ""
        this._x         = 0.0
        this._y         = 0.0
        this._width     = 200.0
        this._height    = 100.0
        this._isVisible = true
    }

    pub get text(): string { this._text }
    pub set text(v: string) {
        this._text = v
        if this.handle >= 0 { setGroupBoxText(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setGroupBoxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setGroupBoxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setGroupBoxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setGroupBoxFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setGroupBoxVisible(this.handle, v) }
    }

    pub attachTo(window: Window) {
        if this.handle >= 0 { return }
        this.handle = createNativeGroupBox(
            window.handle,
            this._text,
            this._x, this._y, this._width, this._height,
            this._isVisible
        )
    }
}

/// Run the platform event loop until every window has closed
/// (macOS: `NSApp.run()` until `terminate:`; Windows:
/// `GetMessage` loop until `WM_QUIT`). Blocks the calling thread.
pub fn run() {
    runEventLoop()
}
