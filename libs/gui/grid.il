/// Grid layout — row × column cell layout in the same
/// ilang-only flavour as `VBox` / `HBox`. Cells are equal-sized;
/// per-child `margin` is honoured and shrinks the cell rectangle
/// before assigning the frame.
///
///   use gui
///   let g = new gui.Grid()
///   g.dock = gui.Dock.fill
///   window.addChild(g)
///
///   let a = new gui.Button()
///   a.text = "A"
///   g.addCell(a, 0 as i32, 0 as i32)               // row 0, col 0
///
///   let b = new gui.Button()
///   b.text = "B"
///   g.addCellSpan(b, 0 as i32, 1 as i32, 1 as i32, 2 as i32)
///                                                  // row 0, col 1..3
///
/// Round 1 splits the grid into equally-sized rows / columns —
/// the row / column count is auto-computed from the highest
/// `row + rowSpan` / `col + colSpan` the caller has registered.
/// Heterogeneous row-heights / column-widths land in Round 2.

use window { Window, Container, Control }

/// Per-cell metadata kept alongside `Container._children`. We
/// can't widen `Control` itself with grid coords (Control is the
/// shared base for every widget), so the index in `_children`
/// and the index in `_cells` are kept in lockstep — push to both
/// at the same time, never reorder either.
class GridCell {
    pub row:     i32
    pub col:     i32
    pub rowSpan: i32
    pub colSpan: i32
    pub init(row: i32, col: i32, rowSpan: i32, colSpan: i32) {
        this.row     = row
        this.col     = col
        this.rowSpan = rowSpan
        this.colSpan = colSpan
    }
}

pub class Grid : Container {
    pub _cells: GridCell[]

    pub init() {
        super()
        this._cells = []
    }

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

    /// Place `child` at (row, col) spanning `rowSpan × colSpan`
    /// cells. Unlike `VBox` / `HBox`, the inherited
    /// `Container.addChild(child)` doesn't accept Grid children —
    /// always use `addCell` so the layout records the grid coords.
    pub addCell(child: Control, row: i32, col: i32) {
        this.addCellSpan(child, row, col, 1 as i32, 1 as i32)
    }

    /// `addCell` with explicit row / column spans.
    pub addCellSpan(child: Control, row: i32, col: i32, rowSpan: i32, colSpan: i32) {
        this._children.push(child)
        this._cells.push(new GridCell(row, col, rowSpan, colSpan))
        if let some(w) = this._root {
            child.attachTo(w)
        }
        this.relayout()
    }

    pub override relayout() {
        let n = this._children.length
        if n == 0 { return }

        // Pass 1: discover the implicit row / column counts.
        // `rowCount` = max(cell.row + cell.rowSpan) across all
        // registered cells; same for `colCount`.
        let rowCount: i32 = 0 as i32
        let colCount: i32 = 0 as i32
        let i = 0
        while i < n {
            let c = this._cells[i]
            let r = c.row + c.rowSpan
            let cc = c.col + c.colSpan
            if r > rowCount { rowCount = r }
            if cc > colCount { colCount = cc }
            i = i + 1
        }
        if rowCount <= 0 as i32 || colCount <= 0 as i32 { return }

        let rowH: f64 = this._h / (rowCount as f64)
        let colW: f64 = this._w / (colCount as f64)

        // Pass 2: assign frames.
        i = 0
        while i < n {
            let child = this._children[i]
            let cell = this._cells[i]
            let cellX: f64 = this._x + (cell.col as f64) * colW
            let cellY: f64 = this._y + (cell.row as f64) * rowH
            let cellW: f64 = (cell.colSpan as f64) * colW
            let cellH: f64 = (cell.rowSpan as f64) * rowH
            child.setFrame(
                cellX + child.margin.left,
                cellY + child.margin.top,
                cellW - child.margin.left - child.margin.right,
                cellH - child.margin.top - child.margin.bottom
            )
            i = i + 1
        }
    }
}
