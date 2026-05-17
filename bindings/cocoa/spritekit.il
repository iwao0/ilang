// SpriteKit bindings — Apple's 2D game framework. Builds on top
// of AppKit (SKView is an NSView subclass) and Foundation (the
// scene graph nodes inherit NSObject). On macOS, CGFloat is f64
// and CGPoint / CGSize / CGRect share the layout of the
// NSPoint / NSSize / NSRect we already bind in `foundation`.
//
// Minimal coverage so you can put a sprite on screen and move it:
//   * Scene graph: SKNode, SKScene, SKSpriteNode, SKShapeNode,
//     SKLabelNode
//   * Host view: SKView (an NSView subclass)
//   * Animations: SKAction (move / rotate / sequence / repeat)
//
// Physics / textures / blend modes / shader effects are
// intentionally not bound yet — add them when there's a concrete
// use.

use foundation { NSObject, NSString, NSArray, NSPoint, NSSize, NSRect, nsstr }
use appkit { NSView, NSColor }

@extern(ObjC, "/System/Library/Frameworks/SpriteKit.framework/SpriteKit") {
    /// Root of the SpriteKit scene-graph hierarchy. Holds
    /// position / scale / z-position / actions; every visible
    /// SpriteKit object is an `SKNode` subclass. Don't
    /// instantiate directly — use `SKScene` / `SKSpriteNode`
    /// / `SKShapeNode` / `SKLabelNode` etc.
    @objc pub class SKNode : NSObject {
        @objc("alloc")
        pub static alloc(): SKNode
        @objc("init")
        pub init(): SKNode

        @objc("addChild:")
        pub addChild(n: SKNode)
        @objc("removeFromParent")
        pub removeFromParent()
        @objc("removeAllChildren")
        pub removeAllChildren()
        @objc("removeAllActions")
        pub removeAllActions()

        @objc("setPosition:")
        pub setPosition(p: NSPoint)
        @objc("position")
        pub position(): NSPoint
        @objc("setZPosition:")
        pub setZPosition(z: f64)
        @objc("setZRotation:")
        pub setZRotation(r: f64)
        @objc("setXScale:")
        pub setXScale(s: f64)
        @objc("setYScale:")
        pub setYScale(s: f64)
        @objc("setAlpha:")
        pub setAlpha(a: f64)
        @objc("setHidden:")
        pub setHidden(flag: bool)

        @objc("runAction:")
        pub runAction(a: SKAction)
    }

    /// The top-level scene rendered by an `SKView`. Size is in
    /// SpriteKit points; the view's `presentScene:` swaps it in.
    @objc pub class SKScene : SKNode {
        @objc("sceneWithSize:")
        pub static sceneWithSize(s: NSSize): SKScene
        @objc("setBackgroundColor:")
        pub setBackgroundColor(c: NSColor)
        @objc("setSize:")
        pub setSize(s: NSSize)
        @objc("size")
        pub size(): NSSize
    }

    /// Coloured / textured rectangular sprite. The
    /// `spriteNodeWithColor:size:` factory is the easiest way to
    /// get something visible without preparing image assets.
    @objc pub class SKSpriteNode : SKNode {
        @objc("spriteNodeWithColor:size:")
        pub static spriteNodeWithColor(c: NSColor, size: NSSize): SKSpriteNode
        @objc("spriteNodeWithImageNamed:")
        pub static spriteNodeWithImageNamed(name: NSString): SKSpriteNode

        @objc("setColor:")
        pub setColor(c: NSColor)
        @objc("setSize:")
        pub setSize(s: NSSize)
        @objc("size")
        pub size(): NSSize
    }

    /// Vector shape node — circles / rects / arbitrary paths.
    /// Fill / stroke are set separately so you can do
    /// outline-only or filled-only.
    @objc pub class SKShapeNode : SKNode {
        @objc("shapeNodeWithRect:")
        pub static shapeNodeWithRect(r: NSRect): SKShapeNode
        @objc("shapeNodeWithCircleOfRadius:")
        pub static shapeNodeWithCircleOfRadius(r: f64): SKShapeNode

        @objc("setFillColor:")
        pub setFillColor(c: NSColor)
        @objc("setStrokeColor:")
        pub setStrokeColor(c: NSColor)
        @objc("setLineWidth:")
        pub setLineWidth(w: f64)
    }

    /// Text node — single-line label rendered with the named
    /// font (defaults to Helvetica). For multi-line, build a
    /// container `SKNode` of stacked labels.
    @objc pub class SKLabelNode : SKNode {
        @objc("labelNodeWithText:")
        pub static labelNodeWithText(t: NSString): SKLabelNode
        @objc("setText:")
        pub setText(t: NSString)
        @objc("setFontName:")
        pub setFontName(n: NSString)
        @objc("setFontSize:")
        pub setFontSize(s: f64)
        @objc("setFontColor:")
        pub setFontColor(c: NSColor)
    }

    /// `NSView` subclass that hosts an `SKScene`. Embed in an
    /// `NSWindow.contentView` (or any AppKit view tree) and call
    /// `presentScene:` once the scene is ready.
    @objc pub class SKView : NSView {
        @objc("alloc")
        pub static alloc(): SKView
        @objc("initWithFrame:")
        pub init(rect: NSRect): SKView

        @objc("presentScene:")
        pub presentScene(s: SKScene)
        @objc("setShowsFPS:")
        pub setShowsFPS(flag: bool)
        @objc("setShowsNodeCount:")
        pub setShowsNodeCount(flag: bool)
        @objc("setShowsDrawCount:")
        pub setShowsDrawCount(flag: bool)
        @objc("scene")
        pub scene(): SKScene
    }

    /// Animation primitive — built via the class factories and
    /// run on an `SKNode` via `runAction:`. Composable via
    /// `sequence:` / `group:` / `repeatActionForever:`.
    @objc pub class SKAction : NSObject {
        @objc("moveByX:y:duration:")
        pub static moveByXY(x: f64, y: f64, duration: f64): SKAction
        @objc("moveBy:duration:")
        pub static moveBy(delta: NSSize, duration: f64): SKAction
        @objc("rotateByAngle:duration:")
        pub static rotateByAngle(radians: f64, duration: f64): SKAction
        @objc("scaleBy:duration:")
        pub static scaleBy(scale: f64, duration: f64): SKAction
        @objc("fadeAlphaBy:duration:")
        pub static fadeAlphaBy(alpha: f64, duration: f64): SKAction
        @objc("waitForDuration:")
        pub static waitForDuration(d: f64): SKAction

        @objc("reversedAction")
        pub reversedAction(): SKAction

        @objc("sequence:")
        pub static sequence(actions: NSArray): SKAction
        @objc("group:")
        pub static group(actions: NSArray): SKAction
        @objc("repeatAction:count:")
        pub static repeatAction(action: SKAction, count: i64): SKAction
        @objc("repeatActionForever:")
        pub static repeatActionForever(action: SKAction): SKAction
    }

    // ─── Convenience helpers ─────────────────────────────────────

    /// Build an `NSPoint` (= `CGPoint` on macOS) — handy when
    /// dropping sprites at specific scene-graph coordinates.
    pub fn makePoint(x: f64, y: f64): NSPoint {
        let p = new NSPoint()
        p.x = x
        p.y = y
        p
    }

    /// Build an `NSSize` (= `CGSize` on macOS).
    pub fn makeSize(w: f64, h: f64): NSSize {
        let s = new NSSize()
        s.width = w
        s.height = h
        s
    }
}
