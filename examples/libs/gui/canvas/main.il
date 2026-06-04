// 🎨 libs/gui — Canvas widget. Opens a 400×360 window with a
// Canvas filling the content area and exercises the public
// drawing surface (`fillRect`, `strokeRect`, path / `arc`,
// `fill` / `stroke`, `fillText` / `strokeText` / `measureText`).
//
// Run:
//   ilang run examples/libs/gui/canvas/main.il
//
// Implemented on all three backends: macOS (NSImage), Windows
// (GDI memory-DC bitmap), and Linux (cairo image surface behind a
// GtkDrawingArea).

use gui

let w = new gui.Window()
w.title  = "ilang × gui — canvas"
w.width  = 400.0
w.height = 360.0

let c = new gui.Canvas()
c.x      = 0.0
c.y      = 0.0
c.width  = 400.0
c.height = 360.0
w.addChild(c)

// White background.
c.clear(0xFFFFFFFF as u32)

// Filled red rectangle, top-left.
c.fillColor = 0xFFFF3344 as u32
c.fillRect(20.0, 20.0, 120.0, 80.0)

// Outlined blue rectangle, top-right.
c.strokeColor = 0xFF3366FF as u32
c.lineWidth   = 4.0
c.strokeRect(220.0, 20.0, 140.0, 80.0)

// Diagonal line, full canvas.
c.strokeColor = 0xFF222222 as u32
c.lineWidth   = 2.0
c.beginPath()
c.moveTo(0.0, 0.0)
c.lineTo(400.0, 300.0)
c.stroke()

// Filled green circle, bottom-right. `arc` takes radians (0 …
// 2π = full circle), swept clockwise from 3 o'clock.
c.fillColor = 0xFF22AA55 as u32
c.beginPath()
c.arc(320.0, 220.0, 50.0, 0.0, 6.283185307179586)
c.fill()

// Text — `(x, y)` is the baseline.
c.fontFamily = "sans-serif"
c.fontSize   = 22.0
c.fontWeight = 700 as i32
c.fillColor  = 0xFF222222 as u32
c.fillText("Hello, ilang!", 20.0, 320.0)

// Outlined italic — strokeText uses strokeColor / lineWidth.
c.fontWeight  = 400 as i32
c.italic      = true
c.strokeColor = 0xFF3366FF as u32
c.lineWidth   = 1.0
c.strokeText("italic outline", 180.0, 320.0)

// measureText returns the typographic extents; draw a guide box
// around the next string at exactly its measured width / height.
c.italic     = false
c.fontWeight = 400 as i32
c.fontSize   = 14.0
let label = "measured ✓"
let m = c.measureText(label)
let lx = 20.0
let ly = 350.0
c.strokeColor = 0xFFAAAAAA as u32
c.lineWidth   = 1.0
c.strokeRect(lx, ly - m.ascent, m.width, m.ascent + m.descent)
c.fillColor = 0xFF222222 as u32
c.fillText(label, lx, ly)

w.show()
gui.run()
