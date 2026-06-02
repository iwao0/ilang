// libs/gui — VBox / HBox demo. Vertical stack of:
//   * a fixed-height header label,
//   * a horizontal row of two flex-stretched buttons (1 : 2),
//   * a fixed-height footer label.
// Resize the window: the header / footer stay at their requested
// height, the button row absorbs the rest, and within the row
// button "B" gets twice as wide as button "A".
//
// Run on macOS:
//   cargo run -p ilang -- run examples/libs/gui/box_demo/main.il

use gui

fn run(): i64 {
    let w = new gui.Window()
    w.title  = "ilang gui × VBox/HBox"
    w.width  = 480.0
    w.height = 320.0

    // Outer VBox fills the window; everything else nests inside it.
    let v = new gui.VBox()
    v.dock = gui.Dock.fill
    w.addChild(v)

    let header = new gui.Label()
    header.text   = "Header (fixed 24px)"
    header.height = 24.0
    header.margin = new gui.Margin(4.0)
    v.addChild(header)

    let row = new gui.HBox()
    row.flex   = 1.0                       // absorbs the rest of the column
    row.margin = new gui.Margin(4.0)
    v.addChild(row)

    let a = new gui.Button()
    a.text   = "A (flex 1)"
    a.flex   = 1.0
    a.margin = new gui.Margin(4.0)
    row.addChild(a)

    let b = new gui.Button()
    b.text   = "B (flex 2)"
    b.flex   = 2.0
    b.margin = new gui.Margin(4.0)
    row.addChild(b)

    let footer = new gui.Label()
    footer.text   = "Footer (fixed 24px)"
    footer.height = 24.0
    footer.margin = new gui.Margin(4.0)
    v.addChild(footer)

    w.show()
    gui.run()
    0
}

run()
