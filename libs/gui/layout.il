/// Layout primitives used by `Control` / `Window` to track docking
/// and edge margins. Phase A: `Dock` enum + `Margin` value class
/// only; future container widgets (VBox / HBox / Grid) will reuse
/// the same `Margin` type.
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
