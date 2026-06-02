/// Layout primitives used by `Control` / `Window`. Holds the
/// `Dock` enum, the `Margin` value class, and the geometry value
/// classes (`Point` / `Size` / `Rect`) `Window`'s position /
/// bounds / min-max-size accessors return.
///
///   use gui
///   let btn = new gui.Button()
///   btn.dock   = gui.Dock.top
///   btn.margin = new gui.Margin(8.0)   // 8px on all sides
///   window.addChild(btn)
///
/// Layout is recomputed automatically on `Window.onResize` and once
/// when each child is added.

/// Where a `Control` snaps inside its parent's remaining area.
/// `none` keeps the existing absolute-position behaviour (the
/// control's own `x` / `y` / `width` / `height` are honoured as-is).
pub enum Dock {
    none
    fill
    top
    bottom
    left
    right
}

/// Edge margins in logical pixels. Four overloaded constructors —
/// the no-arg form yields a zero margin, the single-arg form
/// uniform, the two-arg form (vertical, horizontal), and the
/// four-arg form (top, right, bottom, left) — same convention as
/// CSS shorthand.
pub class Margin {
    pub top:    f64
    pub right:  f64
    pub bottom: f64
    pub left:   f64

    pub init() {
        this.top    = 0.0
        this.right  = 0.0
        this.bottom = 0.0
        this.left   = 0.0
    }
    pub init(all: f64) {
        this.top    = all
        this.right  = all
        this.bottom = all
        this.left   = all
    }
    pub init(vertical: f64, horizontal: f64) {
        this.top    = vertical
        this.bottom = vertical
        this.left   = horizontal
        this.right  = horizontal
    }
    pub init(top: f64, right: f64, bottom: f64, left: f64) {
        this.top    = top
        this.right  = right
        this.bottom = bottom
        this.left   = left
    }
}

/// 2D point in logical pixels — the unit `Window`'s
/// `getPosition()` and the move-event handlers use.
pub class Point {
    pub x: f64
    pub y: f64
    pub init(x: f64, y: f64) {
        this.x = x
        this.y = y
    }
}

/// 2D size in logical pixels. Returned from `getMinimumSize` /
/// `getMaximumSize`; carried by the geometry events.
pub class Size {
    pub width:  f64
    pub height: f64
    pub init(width: f64, height: f64) {
        this.width  = width
        this.height = height
    }
}

/// Axis-aligned rectangle in logical pixels. `getBounds()`
/// returns one; `setBounds` takes the four components inline so
/// callers don't have to allocate a `Rect` to set them.
pub class Rect {
    pub x:      f64
    pub y:      f64
    pub width:  f64
    pub height: f64
    pub init(x: f64, y: f64, width: f64, height: f64) {
        this.x      = x
        this.y      = y
        this.width  = width
        this.height = height
    }
}
