/// Layout containers — NSStackView / NSScrollView / NSSplitView /
/// NSGridView. The enums (`NSUserInterfaceLayoutOrientation` /
/// `NSBorderType` / `NSBoxType`) live here so callers only have to
/// import the layout module.

use foundation { NSObject, NSArray, NSSize, NSRect, makeRect }
use core { NSView }

/// NSUserInterfaceLayoutOrientation — direction NSStackView
/// stacks its arranged subviews. Top-down (`vertical`) is the
/// most common; `horizontal` reads left-to-right.
pub enum NSUserInterfaceLayoutOrientation: i64 {
    horizontal = 0
    vertical   = 1
}

/// NSBorderType — `NSScrollView.setBorderType` / `NSBox.setBorderType`
/// border decoration.
pub enum NSBorderType: u64 {
    noBorder    = 0
    lineBorder  = 1
    bezelBorder = 2
    grooveBorder = 3
}

/// NSBoxType — visual style of an NSBox. `primaryBox` is the
/// default; `separator` draws a single divider line; `custom`
/// disables the box chrome so a content view can be drawn at the
/// box's bounds.
pub enum NSBoxType: u64 {
    primaryBox = 0
    secondaryBox = 1
    separator = 2
    custom = 4
}

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    /// NSStackView — stacks its arranged subviews along
    /// `orientation` (use
    /// `cocoa.NSUserInterfaceLayoutOrientation.horizontal` /
    /// `cocoa.NSUserInterfaceLayoutOrientation.vertical`).
    /// `addArrangedSubview:` appends a view; the stack handles
    /// sizing and positioning.
    @objc pub class NSStackView : NSView {
        @objc("alloc")
        pub static alloc(): NSStackView
        @objc("init")
        pub init(): NSStackView

        @objc("setOrientation:")
        pub setOrientation(o: NSUserInterfaceLayoutOrientation)
        @objc("setSpacing:")
        pub setSpacing(s: f64)
        @objc("addArrangedSubview:")
        pub addArrangedSubview(v: NSView)
    }

    /// `NSScrollView` — wraps a document view (typically an
    /// `NSTextView` or `NSTableView`) and adds scroll bars / a
    /// border. Build via the standard `alloc + initWithFrame:`,
    /// then `setDocumentView:` to hand it the content view.
    @objc pub class NSScrollView : NSView {
        @objc("alloc")
        pub static alloc(): NSScrollView
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSScrollView

        @objc("setDocumentView:")
        pub setDocumentView(v: NSView)
        @objc("setHasVerticalScroller:")
        pub setHasVerticalScroller(flag: bool)
        @objc("setHasHorizontalScroller:")
        pub setHasHorizontalScroller(flag: bool)
        @objc("setAutohidesScrollers:")
        pub setAutohidesScrollers(flag: bool)
        @objc("setBorderType:")
        pub setBorderType(b: NSBorderType)
        @objc("contentSize")
        pub get contentSize(): NSSize
    }

    /// `NSSplitView` — divider-separated stack of subviews the
    /// user can re-proportion. `setVertical:` toggles between a
    /// row of columns (`true`) and a column of rows (`false`).
    @objc pub class NSSplitView : NSView {
        @objc("alloc")
        pub static alloc(): NSSplitView
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSSplitView

        @objc("isVertical")
        pub get isVertical(): bool
        @objc("setVertical:")
        pub set isVertical(flag: bool)
        @objc("setDividerStyle:")
        pub setDividerStyle(style: i64)
        @objc("adjustSubviews")
        pub adjustSubviews()
        /// Drag the divider at `dividerIndex` to `position`
        /// pixels along the splitter's main axis. `0` is the
        /// divider between the first two subviews.
        @objc("setPosition:ofDividerAtIndex:")
        pub setPosition(position: f64, ofDividerAtIndex: i64)
    }

    /// `NSGridView` — 2-D grid layout. Build top-down with
    /// `addRow:` / `addColumn:` (each takes an `NSArray` of
    /// views); the grid auto-aligns cells. Spacing is per-axis.
    @objc pub class NSGridView : NSView {
        @objc("alloc")
        pub static alloc(): NSGridView
        @objc("initWithFrame:")
        pub init(rect: NSRect): NSGridView
        @objc("gridViewWithViews:")
        pub static gridViewWithViews(rows: NSArray): NSGridView

        @objc("numberOfRows")
        pub get numberOfRows(): i64
        @objc("numberOfColumns")
        pub get numberOfColumns(): i64
        @objc("addRowWithViews:")
        pub addRowWithViews(views: NSArray): NSObject
        @objc("addColumnWithViews:")
        pub addColumnWithViews(views: NSArray): NSObject
        @objc("setRowSpacing:")
        pub setRowSpacing(s: f64)
        @objc("setColumnSpacing:")
        pub setColumnSpacing(s: f64)
    }

    // ─── Convenience helpers ─────────────────────────────────────

    /// `NSStackView` initialised with the given orientation and
    /// spacing. Position it with `setFrame` and append children
    /// via `addArrangedSubview`.
    pub fn makeStackView(
        orientation: NSUserInterfaceLayoutOrientation,
        spacing: f64
    ): NSStackView {
        let s = NSStackView.alloc().init()
        s.setOrientation(orientation)
        s.setSpacing(spacing)
        s
    }

    /// Scroll view wrapping `doc` as its document view, with a
    /// vertical scroller and a bezel border. Mirrors the
    /// `NSScrollView` Interface-Builder default.
    pub fn makeScrollView(doc: NSView, x: f64, y: f64, w: f64, h: f64): NSScrollView {
        let s = NSScrollView.alloc().init(makeRect(x, y, w, h))
        s.setHasVerticalScroller(true)
        s.setBorderType(NSBorderType.bezelBorder)
        s.setDocumentView(doc)
        s
    }
}
