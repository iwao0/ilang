// SpriteKit demo — opens a window hosting an `SKView`, drops a
// red square at the centre, and animates it back and forth with
// a `sequence` of `moveByX:y:` actions wrapped in
// `repeatActionForever:`.
//
// Run with:
//   cargo run --release -p ilang -- run bindings/cocoa/example_spritekit.il

use cocoa
use spritekit { SKView, SKScene, SKSpriteNode, SKLabelNode, SKAction }

let app = cocoa.sharedApplication()
app.setActivationPolicy(cocoa.ActivationPolicy.regular)

let mask = cocoa.StyleMask.titled
    | cocoa.StyleMask.closable
    | cocoa.StyleMask.miniaturizable
    | cocoa.StyleMask.resizable
let win = cocoa.makeWindow("ilang × SpriteKit", 0.0, 0.0, 480.0, 320.0, mask)
win.center()

// SKView covers the window's content area.
let view = SKView.alloc().init(cocoa.makeRect(0.0, 0.0, 480.0, 320.0))
view.setShowsFPS(true)
view.setShowsNodeCount(true)

// Scene: dark grey background, same size as the view.
let scene = SKScene.sceneWithSize(cocoa.makeSize(480.0, 320.0))
scene.setBackgroundColor(cocoa.NSColor.gray)

// Red square centred in the scene.
let sprite = SKSpriteNode.spriteNodeWithColor(
    cocoa.NSColor.red,
    cocoa.makeSize(40.0, 40.0)
)
sprite.position = cocoa.makePoint(240.0, 160.0)
scene.addChild(sprite)

// Title text at the top.
let label = SKLabelNode.labelNodeWithText(cocoa.nsstr("Hello, SpriteKit"))
label.setFontSize(20.0)
label.setFontColor(cocoa.NSColor.white)
label.position = cocoa.makePoint(240.0, 280.0)
scene.addChild(label)

// Bounce: move right 150 over 1s, then back. `reversedAction`
// gives us the mirror; `sequence` chains them; `repeatActionForever`
// loops.
let right = SKAction.moveByXY(150.0, 0.0, 1.0)
let left  = right.reversedAction()
let arr   = cocoa.NSMutableArray.array()
arr.addObject(right)
arr.addObject(left)
let bounce = SKAction.sequence(arr)
sprite.runAction(SKAction.repeatActionForever(bounce))

view.presentScene(scene)
win.contentView.addSubview(view)

win.makeKeyAndOrderFront(cocoa.nil)
app.activate()
app.run()
