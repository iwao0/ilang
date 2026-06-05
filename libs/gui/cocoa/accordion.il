/// Accordion. A container NSView holds a header NSButton
/// (disclosure bezel + title) on top and a child-bearing
/// `content` NSView underneath. Clicking the disclosure toggles
/// the content's `setHidden` and reshapes the container so the
/// parent layout sees the right height.

use cocoa {
    NSObject, NSView, NSButton, NSBezelStyle, NSTextField,
    NSString, NSRect,
    nsstr, nil, makeRect, selectorPtr,
}
use core { resolveParentView }

/// Empty NSView subclasses — bindings/cocoa doesn't expose
/// `NSView.alloc()` directly, but a subclass declaration emits
/// the `register()` + `alloc()` boilerplate. `isFlipped` is
/// overridden so child widgets parented under the content view
/// use the same top-left coordinate system the rest of gui-core
/// already assumes.
class AccordionContainerView : NSView {
    pub isFlipped(): bool { true }
}
class AccordionContentView : NSView {
    pub isFlipped(): bool { true }
}

class AccordionSlot {
    pub container: AccordionContainerView
    pub header:    NSButton
    pub label:     NSButton
    pub content:   AccordionContentView
    pub width:        f64
    pub headerH:      f64
    pub contentH:     f64
    pub expanded:     bool
    pub onToggle:     fn(bool)
    pub handler:      NSObject
    pub init(container: AccordionContainerView, header: NSButton, label: NSButton, content: AccordionContentView, handler: NSObject) {
        this.container = container
        this.header    = header
        this.label     = label
        this.content   = content
        this.width    = 0.0
        this.headerH  = 28.0
        this.contentH = 120.0
        this.expanded = false
        this.onToggle = accordionNoOp
        this.handler  = handler
    }
}

fn accordionNoOp(expanded: bool) {}

let ACCORDIONS = new Map<i64, AccordionSlot>()

fn applyFrames(slot: AccordionSlot, x: f64, y: f64) {
    let totalH = if slot.expanded { slot.headerH + slot.contentH } else { slot.headerH }
    (slot.container as NSView).setFrame(makeRect(x, y, slot.width, totalH))
    // Header strip: a small NSButton(disclosure bezel) on the
    // left plus a label sharing the button's y / h so the
    // triangle and the title baseline line up. NSTextField with
    // setBordered(false) renders text on its top baseline; matching
    // the button's frame height (18 px) keeps both centred inside
    // the 28 px header strip.
    let triW: f64 = 22.0
    let triH: f64 = 18.0
    let triY: f64 = (slot.headerH - triH) * 0.5
    slot.header.setFrame(makeRect(6.0, triY, triW, triH))
    (slot.label as NSView).setFrame(
        makeRect(6.0 + triW + 4.0, triY, slot.width - (6.0 + triW + 4.0), triH)
    )
    (slot.content as NSView).setFrame(makeRect(0.0, slot.headerH, slot.width, slot.contentH))
    (slot.content as NSView).setHidden(!slot.expanded)
}

class AccordionHandler : NSObject {
    pub toggled(sender: NSObject) {
        // sender is either the disclosure NSButton on the left or
        // the borderless title NSButton next to it; both are wired
        // to the same action so clicking anywhere on the header
        // strip toggles the section.
        for slot in ACCORDIONS.values() {
            let isHeader = slot.header.handle == sender.handle
            let isLabel  = slot.label.handle == sender.handle
            if isHeader || isLabel {
                // The disclosure button toggles its own state on
                // click (NSButtonTypeOnOff). When the label button
                // is clicked the disclosure didn't change yet, so
                // mirror the new state manually.
                if isLabel {
                    let newState = if slot.expanded { 0 as i64 } else { 1 as i64 }
                    slot.header.state = newState
                }
                slot.expanded = slot.header.state == 1 as i64
                let rc = (slot.container as NSView).frame
                applyFrames(slot, rc.origin.x, rc.origin.y)
                let cb = slot.onToggle
                cb(slot.expanded)
                return
            }
        }
    }
}

pub fn createNativeAccordion(
    windowHandle: i64,
    title: string,
    x: f64, y: f64, width: f64,
    contentH: f64, headerH: f64,
    expanded: bool, visible: bool
): i64 {
    let parent = match resolveParentView(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    AccordionContainerView.register()
    AccordionContentView.register()

    let totalH = if expanded { headerH + contentH } else { headerH }
    let container = AccordionContainerView.alloc().init()
    (container as NSView).setFrame(makeRect(x, y, width, totalH))
    (container as NSView).setHidden(!visible)

    // Borderless square NSButton + manual ▶ / ▼ title so the
    // arrow always reflects `expanded`. The system disclosure
    // bezel only updates its glyph on macOS 11+ themes and even
    // then it lags behind the model state — driving the glyph
    // ourselves removes the ambiguity.
    // Disclosure triangle — keep the small native bezel so the
    // glyph follows the system look. NSButtonTypeOnOff makes the
    // bezel auto-flip the glyph as `state` is set.
    let header = NSButton.alloc().init(makeRect(6.0, 5.0, 22.0, 18.0))
    header.setBezelStyle(NSBezelStyle.disclosure)
    header.setButtonType(6 as i64)   // NSButtonTypeOnOff
    header.setTitle(nsstr(""))
    header.state = if expanded { 1 as i64 } else { 0 as i64 }
    // Borderless NSButton hosts the title so the whole header
    // strip — not just the small triangle — is clickable while
    // looking like a flat text label.
    let label = NSButton.alloc().init(makeRect(32.0, 5.0, width - 36.0, 18.0))
    label.setBordered(false)
    label.setButtonType(0 as i64)    // NSButtonTypeMomentaryLight
    label.setTitle(nsstr(title))

    AccordionHandler.register()
    let h = AccordionHandler.alloc().init()
    header.setTarget(h as NSObject)
    header.setAction(selectorPtr("toggled:"))
    label.setTarget(h as NSObject)
    label.setAction(selectorPtr("toggled:"))

    let content = AccordionContentView.alloc().init()
    (content as NSView).setFrame(makeRect(0.0, headerH, width, contentH))
    (content as NSView).setHidden(!expanded)

    (container as NSView).addSubview(header as NSView)
    (container as NSView).addSubview(label as NSView)
    (container as NSView).addSubview(content as NSView)

    parent.addSubview(container as NSView)

    let handle = (container as NSView).handle
    let slot = new AccordionSlot(container, header, label, content, h as NSObject)
    slot.width    = width
    slot.headerH  = headerH
    slot.contentH = contentH
    slot.expanded = expanded
    ACCORDIONS[handle] = slot
    // First-pass layout — header, label, content frames the
    // creation-time constants above used were approximations.
    // applyFrames normalises them against the canonical formula
    // the toggle handler also uses, so the initial paint matches
    // post-toggle paints.
    applyFrames(slot, x, y)
    handle
}

pub fn accordionContentHandle(handle: i64): i64 {
    match ACCORDIONS.get(handle) {
        some(slot) { (slot.content as NSView).handle }
        none       { -1 as i64 }
    }
}

pub fn setAccordionTitle(handle: i64, title: string) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.label.setTitle(nsstr(title))
    }
}

pub fn setAccordionExpanded(handle: i64, expanded: bool) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.expanded = expanded
        slot.header.state = if expanded { 1 as i64 } else { 0 as i64 }
        let rc = (slot.container as NSView).frame
        applyFrames(slot, rc.origin.x, rc.origin.y)
    }
}

pub fn readAccordionExpanded(handle: i64): bool {
    match ACCORDIONS.get(handle) {
        some(slot) { slot.expanded }
        none       { false }
    }
}

pub fn setAccordionFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.width = width
        applyFrames(slot, x, y)
    }
}

pub fn setAccordionContentHeight(handle: i64, h: f64) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.contentH = h
        let rc = (slot.container as NSView).frame
        applyFrames(slot, rc.origin.x, rc.origin.y)
    }
}

pub fn setAccordionHeaderHeight(handle: i64, h: f64) {
    if let some(slot) = ACCORDIONS.get(handle) {
        slot.headerH = h
        let rc = (slot.container as NSView).frame
        applyFrames(slot, rc.origin.x, rc.origin.y)
    }
}

pub fn setAccordionVisible(handle: i64, visible: bool) {
    if let some(slot) = ACCORDIONS.get(handle) {
        (slot.container as NSView).setHidden(!visible)
    }
}

pub fn setAccordionTooltip(handle: i64, text: string) {
    if let some(slot) = ACCORDIONS.get(handle) {
        if text == "" {
            slot.header.setToolTip(nil)
        } else {
            slot.header.setToolTip(nsstr(text) as NSObject)
        }
    }
}

pub fn setOnAccordionToggle(handle: i64, cb: fn(bool)) {
    if let some(slot) = ACCORDIONS.get(handle) { slot.onToggle = cb }
}
