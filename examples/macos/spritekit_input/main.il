// 🖱️⌨️ ilang × SpriteKit — input + scene subclass. Clicks spawn
// small dots at the cursor; arrow keys move a player sprite.
// Exercises:
//
//   * `@objc class MyScene : SKScene` with overridden
//     `mouseDown:` / `keyDown:` / `update:` selectors. The parser
//     desugar registers the IMPs with libobjc so SpriteKit's
//     responder chain dispatches into ilang code.
//   * NSEvent unpacking: `locationInWindow()` (window coords) →
//     `view.convertPointFromView` (view coords, passing
//     `cocoa.nil` to mean "from window") → `scene.convertPointFromView`
//     (scene coords).
//   * Module-level `let` bindings (the player sprite + velocity)
//     captured by the overridden methods.
//
// Run (the cocoa bindings dep is wired through this directory's
// `ilang.toml`):
//   ilang run examples/macos/spritekit_input/main.il

use cocoa
use spritekit { SKView, SKScene, SKShapeNode, SKLabelNode }
use appkit { NSEvent }
use foundation { NSSize }

// Module-level state the overrides mutate / read.
let player = SKShapeNode.shapeNodeWithCircleOfRadius(14.0)
let _v = cocoa.NSMutableArray.array()   // [vx, vy] as NSNumber

@extern(ObjC) {
    @objc pub class InputScene : SKScene {
        // Factory — `SKScene.sceneWithSize:` returns
        // `[instanceOfReceiver]`; declaring it on the subclass
        // gives us a typed `InputScene` handle back from
        // `InputScene.sceneWithSize(...)`.
        @objc("sceneWithSize:")
        pub static sceneWithSize(s: NSSize): InputScene

        @objc("mouseDown:")
        mouseDown(event: NSEvent) {
            let inWindow = event.locationInWindow
            let view = this.view()
            let inView = view.convertPointFromView(inWindow, cocoa.nil)
            let scenePt = this.convertPointFromView(inView)

            let dot = SKShapeNode.shapeNodeWithCircleOfRadius(4.0)
            dot.setFillColor(cocoa.NSColor.yellow)
            dot.setStrokeColor(cocoa.NSColor.clear)
            dot.position = scenePt
            this.addChild(dot)
        }

        @objc("keyDown:")
        keyDown(event: NSEvent) {
            // Arrow keys: 123=left 124=right 125=down 126=up
            let code = event.keyCode as i64
            let dx = 0.0
            let dy = 0.0
            if      code == 123 { dx = -20.0 }
            elif code == 124 { dx =  20.0 }
            elif code == 125 { dy = -20.0 }
            elif code == 126 { dy =  20.0 }
            if dx != 0.0 || dy != 0.0 {
                let p = player.position
                player.position = cocoa.makePoint(p.x + dx, p.y + dy)
            }
        }

        // Frame hook — SpriteKit calls this every frame with
        // the current time (seconds, monotonic). Use it for
        // game-loop logic (input polling, AI, etc.).
        @objc("update:")
        update(currentTime: f64) {
            let _ = currentTime
        }
    }
}

let app = cocoa.sharedApplication()
app.setActivationPolicy(cocoa.ActivationPolicy.regular)

let mask = cocoa.StyleMask.titled
    | cocoa.StyleMask.closable
    | cocoa.StyleMask.miniaturizable
    | cocoa.StyleMask.resizable
let win = cocoa.makeWindow("SpriteKit input", 0.0, 0.0, 480.0, 320.0, mask)
win.center()

let view = SKView.alloc().init(cocoa.makeRect(0.0, 0.0, 480.0, 320.0))
view.setShowsFPS(true)

InputScene.register()
let scene = InputScene.sceneWithSize(cocoa.makeSize(480.0, 320.0))
scene.setBackgroundColor(cocoa.NSColor.gray)

// Title + player.
let title = SKLabelNode.labelNodeWithText(
    cocoa.nsstr("Click to drop dots — arrow keys move the player")
)
title.setFontSize(14.0)
title.setFontColor(cocoa.NSColor.white)
title.position = cocoa.makePoint(240.0, 290.0)
scene.addChild(title)

player.setFillColor(cocoa.NSColor.red)
player.setStrokeColor(cocoa.NSColor.white)
player.setLineWidth(1.5)
player.position = cocoa.makePoint(240.0, 160.0)
scene.addChild(player)

view.presentScene(scene)
win.contentView.addSubview(view)
win.makeKeyAndOrderFront(cocoa.nil)
app.activate()
app.run()
