/// Collapsible section — header with a disclosure triangle on
/// top and a child-bearing content area underneath. Cocoa: a
/// composite of an `NSButton` (disclosure bezel) + an `NSView`
/// content container; Linux: `GtkExpander`; Win32: custom-
/// drawn `IlangGuiAccordion` with a triangle + label in the
/// header strip.
///
///     let g = new gui.Accordion("Advanced")
///     g.x = 20.0; g.y = 20.0
///     g.width = 300.0
///     g.contentHeight = 120.0
///     window.addChild(g)
///     let cb = new gui.Checkbox()
///     cb.text = "Show internals"
///     cb.x = 8.0; cb.y = 8.0
///     cb.width = 200.0; cb.height = 22.0
///     g.addChild(cb)
///     g.onToggle.add(fn(e: gui.ToggleEvent) {
///         console.log("expanded =", e.checked)
///     })
///
/// `contentHeight` is the visible height of the content area
/// when expanded. `headerHeight` defaults to 28 px. The overall
/// `height` getter returns `headerHeight` when collapsed and
/// `headerHeight + contentHeight` when expanded — handy for
/// parent layout containers (VBox / Grid) that need to know how
/// tall the accordion currently is.

use std.events { Signal }
use events { ToggleEvent }
use window { Window, Control, Container }
use gui_impl {
    createNativeAccordion, accordionContentHandle,
    setAccordionTitle, setAccordionExpanded, readAccordionExpanded,
    setAccordionFrame, setAccordionContentHeight, setAccordionHeaderHeight,
    setAccordionVisible, setAccordionTooltip,
    setOnAccordionToggle,
}

pub class Accordion : Container {
    pub handle: i64
    pub _contentHandle: i64
    _title:         string
    _expanded:      bool
    _contentHeight: f64
    _headerHeight:  f64
    _isVisible:     bool
    _tooltip:       string
    pub onToggle: Signal<ToggleEvent>

    pub init(title: string) {
        super()
        this.handle         = -1
        this._contentHandle = -1
        this._title         = title
        this._expanded      = false
        this._contentHeight = 120.0
        this._headerHeight  = 28.0
        this._w             = 240.0
        this._h             = 28.0   // height while collapsed
        this._isVisible     = true
        this._tooltip       = ""
        this.onToggle       = new Signal<ToggleEvent>()
    }

    pub get title(): string { this._title }
    pub set title(v: string) {
        this._title = v
        if this.handle >= 0 { setAccordionTitle(this.handle, v) }
    }

    pub get expanded(): bool {
        if this.handle >= 0 { this._expanded = readAccordionExpanded(this.handle) }
        this._expanded
    }
    pub set expanded(v: bool) {
        this._expanded = v
        this._h = if v { this._headerHeight + this._contentHeight } else { this._headerHeight }
        if this.handle >= 0 { setAccordionExpanded(this.handle, v) }
    }

    pub get contentHeight(): f64 { this._contentHeight }
    pub set contentHeight(v: f64) {
        this._contentHeight = v
        if this._expanded { this._h = this._headerHeight + v }
        if this.handle >= 0 { setAccordionContentHeight(this.handle, v) }
    }

    pub get headerHeight(): f64 { this._headerHeight }
    pub set headerHeight(v: f64) {
        this._headerHeight = v
        this._h = if this._expanded { v + this._contentHeight } else { v }
        if this.handle >= 0 { setAccordionHeaderHeight(this.handle, v) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setAccordionFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setAccordionFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub get width(): f64 { this._w }
    pub set width(v: f64) {
        this._w = v
        if this.handle >= 0 {
            setAccordionFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub get height(): f64 { this._h }

    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setAccordionVisible(this.handle, v) }
    }

    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setAccordionTooltip(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeAccordion(
            parent.nativeChildHandle(),
            this._title,
            this._x, this._y, this._w,
            this._contentHeight, this._headerHeight,
            this._expanded, this._isVisible
        )
        if this.handle >= 0 {
            this._contentHandle = accordionContentHandle(this.handle)
        }
        setOnAccordionToggle(this.handle, fn(expanded: bool) {
            this._expanded = expanded
            this._h = if expanded {
                this._headerHeight + this._contentHeight
            } else {
                this._headerHeight
            }
            this.onToggle.emit(new ToggleEvent(this, expanded))
        })
        super.attachTo(parent)
        if this._tooltip != "" { setAccordionTooltip(this.handle, this._tooltip) }
    }

    /// Native container handle children parent under — the
    /// accordion's content area, NOT the header strip.
    pub override nativeChildHandle(): i64 { this._contentHandle }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._w = w
        // `h` from the layout engine is ignored — the accordion's
        // own expanded / collapsed state drives the height.
        if this.handle >= 0 {
            setAccordionFrame(this.handle, this._x, this._y, this._w, this._h)
        }
    }
    pub override frameWidth():  f64 { this._w }
    pub override frameHeight(): f64 { this._h }
}
