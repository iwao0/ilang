/// NSStackView / NSScrollView / NSGridView — construct + child
/// management, without showing them.

use std.test
use appkit { NSStackView, NSScrollView, NSSplitView, NSGridView,
             NSUserInterfaceLayoutOrientation, NSBorderType,
             NSTextField,
             sharedApplication, makeStackView, makeScrollView,
             makeLabel }
use foundation { makeRect, NSMutableArray, nsstr }

let _ = sharedApplication()

// NSStackView with two arranged subviews.
let stack = makeStackView(NSUserInterfaceLayoutOrientation.vertical, 8.0)
test.expectFalse(stack.handle == 0)
stack.addArrangedSubview(makeLabel("first", 0.0, 0.0, 100.0, 20.0))
stack.addArrangedSubview(makeLabel("second", 0.0, 0.0, 100.0, 20.0))

// NSScrollView via convenience helper — wraps a label as its
// document view.
let doc = makeLabel("scrolled content", 0.0, 0.0, 400.0, 200.0)
let sv = makeScrollView(doc, 0.0, 0.0, 200.0, 100.0)
test.expectFalse(sv.handle == 0)
let cs = sv.contentSize
test.expectTrue(cs.width > 0.0)
test.expectTrue(cs.height > 0.0)

// NSSplitView toggle vertical.
let split = NSSplitView.alloc().init(makeRect(0.0, 0.0, 400.0, 200.0))
test.expectFalse(split.handle == 0)
split.isVertical = true
test.expectTrue(split.isVertical)
split.isVertical = false
test.expectFalse(split.isVertical)

// NSGridView — addRow / addColumn change the row / column count.
let grid = NSGridView.alloc().init(makeRect(0.0, 0.0, 200.0, 100.0))
test.expect(grid.numberOfRows, 0)
test.expect(grid.numberOfColumns, 0)
let cells = NSMutableArray.alloc().init()
cells.addObject(makeLabel("a", 0.0, 0.0, 50.0, 20.0))
cells.addObject(makeLabel("b", 0.0, 0.0, 50.0, 20.0))
let _ = grid.addRowWithViews(cells)
test.expect(grid.numberOfRows, 1)
test.expect(grid.numberOfColumns, 2)
grid.setRowSpacing(4.0)
grid.setColumnSpacing(6.0)

0
