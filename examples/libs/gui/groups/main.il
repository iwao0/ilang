// 📻 libs/gui — RadioButton, ListBox, GroupBox.
//
// Two grouped panels:
// - "Size" group with three mutually-exclusive radio buttons.
// - "Topping" group with a scrolling ListBox.
//
// Selecting in either group logs to the console.
//
// Run with:
//   ilang run examples/libs/gui/groups/main.il

use gui

let w = new gui.Window()
w.title  = "ilang × gui — groups"
w.width  = 480.0
w.height = 280.0

// Group #1 — size radios.
let sizeBox = new gui.GroupBox()
sizeBox.text   = "Size"
sizeBox.x      = 20.0
sizeBox.y      = 16.0
sizeBox.width  = 200.0
sizeBox.height = 130.0
w.addChild(sizeBox)

let sizes = ["Small", "Medium", "Large"]
let i = 0
let y0 = 44.0
while i < sizes.length {
    let r = new gui.RadioButton()
    r.group  = "size"
    r.text   = sizes[i]
    r.x      = 36.0
    r.y      = y0 + (i as f64) * 26.0
    r.width  = 160.0
    r.height = 22.0
    if i == 1 { r.checked = true }
    r.onToggle.add(fn(e: gui.ToggleEvent) {
        if e.checked { console.log("size:", r.text) }
    })
    w.addChild(r)
    i = i + 1
}

// Group #2 — topping list.
let topBox = new gui.GroupBox()
topBox.text   = "Topping"
topBox.x      = 240.0
topBox.y      = 16.0
topBox.width  = 220.0
topBox.height = 220.0
w.addChild(topBox)

let list = new gui.ListBox()
list.x      = 256.0
list.y      = 44.0
list.width  = 188.0
list.height = 180.0
list.addItem("Cheese")
list.addItem("Pepperoni")
list.addItem("Mushroom")
list.addItem("Olive")
list.addItem("Pineapple")
list.onSelect.add(fn(e: gui.SelectEvent) {
    console.log("topping:", e.value, "@", e.index)
})
w.addChild(list)

w.show()
gui.run()
