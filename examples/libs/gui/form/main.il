// 📝 libs/gui — Label + TextField walkthrough.
//
// Lays out a tiny form: a Label, a TextField that echoes its
// contents through `onChange`, and a Button that toggles whether
// the field is enabled. Run with:
//
//   ilang run examples/libs/gui/form/main.il

use gui

let w = new gui.Window("ilang × gui — form", 420.0, 200.0)

let nameLabel = new gui.Label()
nameLabel.text   = "Name:"
nameLabel.x      = 24.0
nameLabel.y      = 32.0
nameLabel.width  = 60.0
nameLabel.height = 22.0
w.addChild(nameLabel)

let nameField = new gui.TextField()
nameField.text   = "ilang"
nameField.x      = 100.0
nameField.y      = 28.0
nameField.width  = 280.0
nameField.height = 26.0
nameField.onChange.add(fn(e: gui.ChangeEvent) {
    console.log("name =", e.value)
})
w.addChild(nameField)

let echo = new gui.Label()
echo.text   = "hello, ilang"
echo.x      = 24.0
echo.y      = 76.0
echo.width  = 360.0
echo.height = 22.0
w.addChild(echo)

// Refresh the echo label every time the field's contents change.
nameField.onChange.add(fn(e: gui.ChangeEvent) {
    echo.text = "hello, " + e.value
})

let toggle = new gui.Button()
toggle.text   = "Disable field"
toggle.x      = 24.0
toggle.y      = 120.0
toggle.width  = 140.0
toggle.height = 32.0
toggle.onClick.add(fn(e: gui.MouseEvent) {
    let _ = e
    nameField.isEnabled = !nameField.isEnabled
    toggle.text = if nameField.isEnabled { "Disable field" } else { "Enable field" }
    console.log("toggle, enabled =", nameField.isEnabled)
})
w.addChild(toggle)

gui.run()
