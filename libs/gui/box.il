/// Linear layout containers — `VBox` (vertical stack) and `HBox`
/// (horizontal stack). Children are laid out along the container's
/// main axis (top → bottom for `VBox`, left → right for `HBox`)
/// using each child's `flex` weight:
///
///   * `flex == 0.0` (default) — the child keeps its preferred
///     main-axis extent (`frameHeight()` for VBox / `frameWidth()`
///     for HBox); the cross axis is stretched to the container's
///     full inner width / height.
///   * `flex > 0.0` — the child stretches and absorbs a share of
///     the container's remaining main-axis space proportional to
///     its `flex` value (`flex = 1.0` and `flex = 2.0` would split
///     remaining space 1:2).
///
/// Per-child `margin` is honoured. Between adjacent siblings the
/// effective gap is `max(prev.margin.bottom, curr.margin.top)`
/// (CSS-style margin collapse) so a uniform `Margin(4.0)` on each
/// child produces a uniform 4px gap rather than 8px doubled.
///
///   use gui
///   let v = new gui.VBox()
///   v.dock = gui.Dock.fill
///   window.addChild(v)
///
///   let header = new gui.Label()
///   header.height = 24.0          // fixed-size, flex == 0
///   v.addChild(header)
///
///   let body = new gui.HBox()
///   body.flex = 1.0               // stretches to take remaining
///   v.addChild(body)

use window { Window, Container, Control }

/// Vertical linear container — stacks children top → bottom.
pub class VBox : Container {
    pub init() { super() }

    /// Preferred extent for the dock layout of an outer `Window`
    /// (or `HBox` / `VBox`) when this box is sized with `flex = 0`.
    /// Writing the property re-runs `relayout()` so any children
    /// already added re-flow against the new bounds.
    pub get width():  f64 { this._w }
    pub set width(v: f64) {
        this._w = v
        this.relayout()
    }
    pub get height(): f64 { this._h }
    pub set height(v: f64) {
        this._h = v
        this.relayout()
    }

    pub override relayout() {
        let n = this._children.length
        if n == 0 { return }

        // Pass 1: total fixed main-axis extent + flex weight + the
        // collapsed vertical margin total. Margins collapse the
        // CSS way: top of first child, bottom of last child, and
        // max(prev.bottom, next.top) between each pair.
        let totalFixed: f64 = 0.0
        let totalFlex:  f64 = 0.0
        let i = 0
        while i < n {
            let c = this._children[i]
            if c.flex > 0.0 {
                totalFlex = totalFlex + c.flex
            } else {
                totalFixed = totalFixed + c.frameHeight()
            }
            i = i + 1
        }
        let marginTotal: f64 = this._children[0].margin.top
        i = 1
        while i < n {
            let prev = this._children[i - 1]
            let curr = this._children[i]
            let gap = if prev.margin.bottom > curr.margin.top {
                prev.margin.bottom
            } else {
                curr.margin.top
            }
            marginTotal = marginTotal + gap
            i = i + 1
        }
        marginTotal = marginTotal + this._children[n - 1].margin.bottom

        let remaining: f64 = this._h - totalFixed - marginTotal
        if remaining < 0.0 { remaining = 0.0 }

        // Pass 2: walk top → bottom, setFrame each child.
        let curY: f64 = this._y + this._children[0].margin.top
        i = 0
        while i < n {
            let c = this._children[i]
            if i > 0 {
                let prev = this._children[i - 1]
                let gap = if prev.margin.bottom > c.margin.top {
                    prev.margin.bottom
                } else {
                    c.margin.top
                }
                curY = curY + gap
            }
            let h: f64 = if c.flex > 0.0 {
                remaining * c.flex / totalFlex
            } else {
                c.frameHeight()
            }
            c.setFrame(
                this._x + c.margin.left,
                curY,
                this._w - c.margin.left - c.margin.right,
                h
            )
            curY = curY + h
            i = i + 1
        }
    }
}

/// Horizontal linear container — stacks children left → right.
pub class HBox : Container {
    pub init() { super() }

    pub get width():  f64 { this._w }
    pub set width(v: f64) {
        this._w = v
        this.relayout()
    }
    pub get height(): f64 { this._h }
    pub set height(v: f64) {
        this._h = v
        this.relayout()
    }

    pub override relayout() {
        let n = this._children.length
        if n == 0 { return }

        // Pass 1: total fixed main-axis extent + flex weight + the
        // collapsed horizontal margin total. Same collapse rule as
        // VBox, but on the left/right edges.
        let totalFixed: f64 = 0.0
        let totalFlex:  f64 = 0.0
        let i = 0
        while i < n {
            let c = this._children[i]
            if c.flex > 0.0 {
                totalFlex = totalFlex + c.flex
            } else {
                totalFixed = totalFixed + c.frameWidth()
            }
            i = i + 1
        }
        let marginTotal: f64 = this._children[0].margin.left
        i = 1
        while i < n {
            let prev = this._children[i - 1]
            let curr = this._children[i]
            let gap = if prev.margin.right > curr.margin.left {
                prev.margin.right
            } else {
                curr.margin.left
            }
            marginTotal = marginTotal + gap
            i = i + 1
        }
        marginTotal = marginTotal + this._children[n - 1].margin.right

        let remaining: f64 = this._w - totalFixed - marginTotal
        if remaining < 0.0 { remaining = 0.0 }

        // Pass 2: walk left → right, setFrame each child.
        let curX: f64 = this._x + this._children[0].margin.left
        i = 0
        while i < n {
            let c = this._children[i]
            if i > 0 {
                let prev = this._children[i - 1]
                let gap = if prev.margin.right > c.margin.left {
                    prev.margin.right
                } else {
                    c.margin.left
                }
                curX = curX + gap
            }
            let w: f64 = if c.flex > 0.0 {
                remaining * c.flex / totalFlex
            } else {
                c.frameWidth()
            }
            c.setFrame(
                curX,
                this._y + c.margin.top,
                w,
                this._h - c.margin.top - c.margin.bottom
            )
            curX = curX + w
            i = i + 1
        }
    }
}
