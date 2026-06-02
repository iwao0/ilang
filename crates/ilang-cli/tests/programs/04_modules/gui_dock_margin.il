// expect: ok
//
// Pure-logic check for the libs/gui layout layer: `Margin`'s four
// overloaded constructors and the dock placement algorithm.
//
// Cannot `use gui` here — that pulls in `gui_impl`'s native peer
// (Cocoa / Win32 / X11) and the test harness is headless. So we
// duplicate the `Dock` enum, `Margin` class, and `relayout`
// algorithm inline. If those drift from `libs/gui/window.il`'s
// implementation, this fixture will pass but the real Window
// behaviour will differ — keep the two in sync.

use std.test as test

enum Dock { none, fill, top, bottom, left, right }

class Margin {
    pub top:    f64
    pub right:  f64
    pub bottom: f64
    pub left:   f64
    pub init() {
        this.top    = 0.0; this.right = 0.0
        this.bottom = 0.0; this.left  = 0.0
    }
    pub init(all: f64) {
        this.top    = all; this.right = all
        this.bottom = all; this.left  = all
    }
    pub init(vertical: f64, horizontal: f64) {
        this.top    = vertical; this.bottom = vertical
        this.left   = horizontal; this.right  = horizontal
    }
    pub init(top: f64, right: f64, bottom: f64, left: f64) {
        this.top    = top;    this.right = right
        this.bottom = bottom; this.left  = left
    }
}

// Stub control. Tracks its own dock / margin and the rectangle
// the layout pass wrote into it.
class StubControl {
    pub dock:   Dock
    pub margin: Margin
    pub _x: f64
    pub _y: f64
    pub _w: f64
    pub _h: f64
    pub init(dock: Dock, margin: Margin, w: f64, h: f64) {
        this.dock   = dock
        this.margin = margin
        this._x = 0.0; this._y = 0.0
        this._w = w;   this._h = h
    }
    pub setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._w = w; this._h = h
    }
    pub frameWidth():  f64 { this._w }
    pub frameHeight(): f64 { this._h }
}

fn relayout(children: StubControl[], winW: f64, winH: f64) {
    let availX: f64 = 0.0
    let availY: f64 = 0.0
    let availW: f64 = winW
    let availH: f64 = winH
    let n = children.length
    let i = 0
    while i < n {
        let c = children[i]
        let mt = c.margin.top
        let mr = c.margin.right
        let mb = c.margin.bottom
        let ml = c.margin.left
        match c.dock {
            none {}
            top {
                let h = c.frameHeight()
                c.setFrame(availX + ml, availY + mt, availW - ml - mr, h)
                let consumed = h + mt + mb
                availY = availY + consumed
                availH = availH - consumed
            }
            bottom {
                let h = c.frameHeight()
                c.setFrame(availX + ml, availY + availH - h - mb, availW - ml - mr, h)
                availH = availH - (h + mt + mb)
            }
            left {
                let w = c.frameWidth()
                c.setFrame(availX + ml, availY + mt, w, availH - mt - mb)
                let consumed = w + ml + mr
                availX = availX + consumed
                availW = availW - consumed
            }
            right {
                let w = c.frameWidth()
                c.setFrame(availX + availW - w - mr, availY + mt, w, availH - mt - mb)
                availW = availW - (w + ml + mr)
            }
            fill {
                c.setFrame(availX + ml, availY + mt, availW - ml - mr, availH - mt - mb)
            }
        }
        i = i + 1
    }
}

// ─── Margin constructor checks ──────────────────────────────────

let m0 = new Margin()
test.expectF64(m0.top, 0.0)
test.expectF64(m0.right, 0.0)
test.expectF64(m0.bottom, 0.0)
test.expectF64(m0.left, 0.0)

let m1 = new Margin(4.0)
test.expectF64(m1.top, 4.0)
test.expectF64(m1.right, 4.0)
test.expectF64(m1.bottom, 4.0)
test.expectF64(m1.left, 4.0)

let m2 = new Margin(8.0, 16.0)   // (vertical, horizontal)
test.expectF64(m2.top, 8.0)
test.expectF64(m2.bottom, 8.0)
test.expectF64(m2.left, 16.0)
test.expectF64(m2.right, 16.0)

let m4 = new Margin(1.0, 2.0, 3.0, 4.0)
test.expectF64(m4.top, 1.0)
test.expectF64(m4.right, 2.0)
test.expectF64(m4.bottom, 3.0)
test.expectF64(m4.left, 4.0)

// ─── Single-dock cases on a 400×300 window ──────────────────────

// fill with zero margin
let cFill = new StubControl(Dock.fill, new Margin(), 0.0, 0.0)
relayout([cFill], 400.0, 300.0)
test.expectF64(cFill._x, 0.0)
test.expectF64(cFill._y, 0.0)
test.expectF64(cFill._w, 400.0)
test.expectF64(cFill._h, 300.0)

// fill with uniform 10 margin
let cFillM = new StubControl(Dock.fill, new Margin(10.0), 0.0, 0.0)
relayout([cFillM], 400.0, 300.0)
test.expectF64(cFillM._x, 10.0)
test.expectF64(cFillM._y, 10.0)
test.expectF64(cFillM._w, 380.0)
test.expectF64(cFillM._h, 280.0)

// top — height comes from the control's own height
let cTop = new StubControl(Dock.top, new Margin(), 0.0, 40.0)
relayout([cTop], 400.0, 300.0)
test.expectF64(cTop._x, 0.0)
test.expectF64(cTop._y, 0.0)
test.expectF64(cTop._w, 400.0)
test.expectF64(cTop._h, 40.0)

// bottom
let cBot = new StubControl(Dock.bottom, new Margin(), 0.0, 24.0)
relayout([cBot], 400.0, 300.0)
test.expectF64(cBot._x, 0.0)
test.expectF64(cBot._y, 276.0)   // 300 - 24
test.expectF64(cBot._w, 400.0)
test.expectF64(cBot._h, 24.0)

// left
let cLeft = new StubControl(Dock.left, new Margin(), 80.0, 0.0)
relayout([cLeft], 400.0, 300.0)
test.expectF64(cLeft._x, 0.0)
test.expectF64(cLeft._y, 0.0)
test.expectF64(cLeft._w, 80.0)
test.expectF64(cLeft._h, 300.0)

// right
let cRight = new StubControl(Dock.right, new Margin(), 100.0, 0.0)
relayout([cRight], 400.0, 300.0)
test.expectF64(cRight._x, 300.0)   // 400 - 100
test.expectF64(cRight._y, 0.0)
test.expectF64(cRight._w, 100.0)
test.expectF64(cRight._h, 300.0)

// none — coordinates stay put
let cNone = new StubControl(Dock.none, new Margin(), 50.0, 30.0)
cNone._x = 12.0; cNone._y = 13.0
relayout([cNone], 400.0, 300.0)
test.expectF64(cNone._x, 12.0)
test.expectF64(cNone._y, 13.0)
test.expectF64(cNone._w, 50.0)
test.expectF64(cNone._h, 30.0)

// ─── Composed: top bar + bottom bar + filling centre ────────────
//
// Mirrors the dock_demo example. 400×300 window, 40-px top bar,
// 24-px bottom bar, fill takes the middle (300 - 40 - 24 = 236).

let top  = new StubControl(Dock.top,    new Margin(), 0.0, 40.0)
let bot  = new StubControl(Dock.bottom, new Margin(), 0.0, 24.0)
let mid  = new StubControl(Dock.fill,   new Margin(), 0.0, 0.0)
relayout([top, bot, mid], 400.0, 300.0)

test.expectF64(top._y, 0.0)
test.expectF64(top._h, 40.0)
test.expectF64(bot._y, 276.0)
test.expectF64(bot._h, 24.0)
test.expectF64(mid._x, 0.0)
test.expectF64(mid._y, 40.0)
test.expectF64(mid._w, 400.0)
test.expectF64(mid._h, 236.0)

// ─── Composed with margins: top with 6-px ring, fill centre ─────

let topM = new StubControl(Dock.top,  new Margin(6.0), 0.0, 30.0)
let midM = new StubControl(Dock.fill, new Margin(),    0.0, 0.0)
relayout([topM, midM], 400.0, 300.0)
test.expectF64(topM._x, 6.0)         // ml
test.expectF64(topM._y, 6.0)         // mt
test.expectF64(topM._w, 388.0)       // 400 - 6 - 6
test.expectF64(topM._h, 30.0)
// top consumed h + mt + mb = 30 + 6 + 6 = 42
test.expectF64(midM._x, 0.0)
test.expectF64(midM._y, 42.0)
test.expectF64(midM._w, 400.0)
test.expectF64(midM._h, 258.0)       // 300 - 42

console.log("ok")
