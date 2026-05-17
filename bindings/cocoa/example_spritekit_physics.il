// SpriteKit physics demo — three circles drop under gravity and
// bounce inside an edge-loop box. Shows:
//   * `SKPhysicsBody.bodyWithEdgeLoopFromRect` for static walls
//     attached to the scene itself (no SKNode needed).
//   * `bodyWithCircleOfRadius` for dynamic balls; setRestitution
//     keeps them bouncy.
//   * `SKPhysicsWorld.setGravity` set on the scene's world.
//
// Run with:
//   cargo run --release -p ilang -- run bindings/cocoa/example_spritekit_physics.il

use cocoa
use spritekit { SKView, SKScene, SKShapeNode, SKLabelNode, SKPhysicsBody }
use foundation { makeSize, makePoint, makeVector }

let app = cocoa.sharedApplication()
app.setActivationPolicy(cocoa.ActivationPolicy.regular)

let mask = cocoa.StyleMask.titled
    | cocoa.StyleMask.closable
    | cocoa.StyleMask.miniaturizable
    | cocoa.StyleMask.resizable
let win = cocoa.makeWindow("SpriteKit physics", 0.0, 0.0, 480.0, 360.0, mask)
win.center()

let view = SKView.alloc().init(cocoa.makeRect(0.0, 0.0, 480.0, 360.0))
view.setShowsFPS(true)
view.setShowsPhysics(true)

let scene = SKScene.sceneWithSize(makeSize(480.0, 360.0))
scene.setBackgroundColor(cocoa.NSColor.black())

// Tune the gravity (default is 0, -9.8). Stronger for a snappier
// feel inside the small play area.
let world = scene.physicsWorld()
world.setGravity(makeVector(0.0, -16.0))

// Box edge-loop covering most of the scene. Bodies set directly
// on SKScene attach to the scene itself, no SKNode needed.
let bounds = cocoa.makeRect(10.0, 10.0, 460.0, 340.0)
let wallBody = SKPhysicsBody.bodyWithEdgeLoopFromRect(bounds)
scene.setPhysicsBody(wallBody)

// Three balls of different colours and sizes.
fn makeBall(radius: f64, x: f64, y: f64, colour: cocoa.NSColor): SKShapeNode {
    let n = SKShapeNode.shapeNodeWithCircleOfRadius(radius)
    n.setFillColor(colour)
    n.setStrokeColor(cocoa.NSColor.white())
    n.setLineWidth(1.5)
    n.setPosition(makePoint(x, y))
    let body = SKPhysicsBody.bodyWithCircleOfRadius(radius)
    body.setRestitution(0.85)
    body.setFriction(0.1)
    n.setPhysicsBody(body)
    n
}

scene.addChild(makeBall(20.0, 120.0, 320.0, cocoa.NSColor.red()))
scene.addChild(makeBall(28.0, 240.0, 300.0, cocoa.NSColor.green()))
scene.addChild(makeBall(14.0, 360.0, 340.0, cocoa.NSColor.blue()))

// Title
let label = SKLabelNode.labelNodeWithText(cocoa.nsstr("Drop & bounce"))
label.setFontSize(16.0)
label.setFontColor(cocoa.NSColor.white())
label.setPosition(makePoint(240.0, 340.0))
scene.addChild(label)

view.presentScene(scene)
win.contentView().addSubview(view)

win.makeKeyAndOrderFront(cocoa.nil)
app.activate()
app.run()
