// 🎛 libs/gui — Checkbox, Slider, ProgressBar, ComboBox, Switch.
//
// A small "control panel" window:
// - Checkbox toggles whether the slider's value drives the
//   progress bar.
// - Slider (0..100) feeds the progress bar when the checkbox is
//   checked.
// - ComboBox cycles the slider colour-label which prints to the
//   console.
// - Switch enables / disables the slider.
//
// Run with:
//   ilang run examples/libs/gui/controls/main.il

use gui

let w = new gui.Window()
w.title  = "ilang × gui — controls"
w.width  = 460.0
w.height = 260.0

let mirror = new gui.Checkbox()
mirror.text   = "Drive progress from slider"
mirror.x      = 20.0
mirror.y      = 20.0
mirror.width  = 260.0
mirror.height = 22.0
mirror.checked = true
mirror.onToggle.add(fn(e: gui.ToggleEvent) {
    console.log("mirror =", e.checked)
})
w.addChild(mirror)

let slider = new gui.Slider()
slider.min   = 0.0
slider.max   = 100.0
slider.value = 40.0
slider.x      = 20.0
slider.y      = 60.0
slider.width  = 300.0
slider.height = 28.0
w.addChild(slider)

let prog = new gui.ProgressBar()
prog.min   = 0.0
prog.max   = 100.0
prog.value = 40.0
prog.x      = 20.0
prog.y      = 100.0
prog.width  = 300.0
prog.height = 18.0
w.addChild(prog)

slider.onChange.add(fn(e: gui.SliderEvent) {
    if mirror.checked { prog.value = e.value }
})

let combo = new gui.ComboBox()
combo.x      = 20.0
combo.y      = 140.0
combo.width  = 160.0
combo.height = 26.0
combo.addItem("Red")
combo.addItem("Green")
combo.addItem("Blue")
combo.selectedIndex = 0
combo.onSelect.add(fn(e: gui.SelectEvent) {
    console.log("colour:", e.value, "@", e.index)
})
w.addChild(combo)

let sw = new gui.Switch()
sw.x      = 200.0
sw.y      = 140.0
sw.checked = true
sw.onToggle.add(fn(e: gui.ToggleEvent) {
    slider.isEnabled = e.checked
    console.log("slider enabled:", e.checked)
})
w.addChild(sw)

w.show()
gui.run()
