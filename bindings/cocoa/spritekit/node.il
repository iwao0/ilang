/// Scene graph base — SKNode + SKScene + SKView. Every other
/// SpriteKit class derives from SKNode, so every category file
/// in this folder will `use node { SKNode, ... }`.

use foundation { NSObject, NSString, NSArray, NSPoint, NSSize,
                 NSRect, NSVector, NSURL, nsstr }
use appkit { NSView, NSColor }
use metal { MTLCommandBuffer, MTLRenderPassDescriptor }

/// SKSceneScaleMode — how the scene's size is mapped to the
/// view's bounds.
pub enum SKSceneScaleMode: i64 {
    fill        = 0
    aspectFill  = 1
    aspectFit   = 2
    resizeFill  = 3
}

@extern(ObjC, "/System/Library/Frameworks/SpriteKit.framework/SpriteKit") {
    /// Animation primitive — built via the class factories and
    /// run on an `SKNode` via `runAction:`. Composable via
    /// `sequence:` / `group:` / `repeatActionForever:`.
    /// Relative variants take a delta (`moveByX:y:`); absolute
    /// variants take the destination (`moveTo:`).
    @objc pub class SKNode : NSObject {
        @objc("alloc")
        pub static alloc(): SKNode
        @objc("init")
        pub init(): SKNode

        /// Hierarchy
        @objc("addChild:")
        pub addChild(n: SKNode)
        @objc("insertChild:atIndex:")
        pub insertChild(n: SKNode, atIndex: i64)
        @objc("removeFromParent")
        pub removeFromParent()
        @objc("removeAllChildren")
        pub removeAllChildren()
        @objc("removeAllActions")
        pub removeAllActions()
        @objc("parent")
        pub parent(): SKNode
        @objc("children")
        pub children(): NSArray
        @objc("childNodeWithName:")
        pub childNodeWithName(name: NSString): SKNode

        /// Transform — exposed as properties so user code reads
        /// `node.position` / writes `node.position = p` rather
        /// than `node.position()` / `node.setPosition(p)`.
        @objc("position")
        pub get position(): NSPoint
        @objc("setPosition:")
        pub set position(p: NSPoint)
        @objc("zPosition")
        pub get zPosition(): f64
        @objc("setZPosition:")
        pub set zPosition(z: f64)
        @objc("zRotation")
        pub get zRotation(): f64
        @objc("setZRotation:")
        pub set zRotation(r: f64)
        @objc("xScale")
        pub get xScale(): f64
        @objc("setXScale:")
        pub set xScale(s: f64)
        @objc("yScale")
        pub get yScale(): f64
        @objc("setYScale:")
        pub set yScale(s: f64)
        @objc("alpha")
        pub get alpha(): f64
        @objc("setAlpha:")
        pub set alpha(a: f64)
        @objc("isHidden")
        pub get isHidden(): bool
        @objc("setHidden:")
        pub set isHidden(flag: bool)

        /// Identity / state
        @objc("setName:")
        pub setName(n: NSString)
        @objc("name")
        pub name(): NSString
        @objc("isPaused")
        pub get isPaused(): bool
        @objc("setPaused:")
        pub set isPaused(flag: bool)
        @objc("setSpeed:")
        pub setSpeed(s: f64)
        @objc("setUserInteractionEnabled:")
        pub setUserInteractionEnabled(flag: bool)

        /// Geometry tests
        @objc("containsPoint:")
        pub containsPoint(p: NSPoint): bool
        @objc("frame")
        pub frame(): NSRect

        /// Actions
        @objc("runAction:")
        pub runAction(a: SKAction)
        @objc("runAction:withKey:")
        pub runActionWithKey(a: SKAction, key: NSString)
        @objc("removeActionForKey:")
        pub removeActionForKey(key: NSString)
        @objc("hasActions")
        pub hasActions(): bool

        /// Physics
        @objc("setPhysicsBody:")
        pub setPhysicsBody(b: SKPhysicsBody)
        @objc("physicsBody")
        pub physicsBody(): SKPhysicsBody

        /// Constraints — apply an array of `SKConstraint`s
        /// after every frame's transform update.
        @objc("setConstraints:")
        pub setConstraints(arr: NSArray)
        @objc("constraints")
        pub constraints(): NSArray

        /// Scene-graph search — `nodeAtPoint:` finds the
        /// top-most node whose accumulated frame contains
        /// `point` (in this node's coordinate system).
        /// `nodesAtPoint:` returns every match.
        @objc("nodeAtPoint:")
        pub nodeAtPoint(p: NSPoint): SKNode
        @objc("nodesAtPoint:")
        pub nodesAtPoint(p: NSPoint): NSArray
        @objc("intersectsNode:")
        pub intersectsNode(n: SKNode): bool

        /// Coordinate conversion between two nodes in the same
        /// scene graph (or between scene-graph and view space —
        /// bridge via `SKScene.convertPointFromView:` first).
        @objc("convertPoint:toNode:")
        pub convertPointToNode(p: NSPoint, toNode: SKNode): NSPoint
        @objc("convertPoint:fromNode:")
        pub convertPointFromNode(p: NSPoint, fromNode: SKNode): NSPoint

        /// Inverse-kinematics joint limits — consumed by
        /// `SKAction.reachTo:rootNode:duration:` and the like.
        @objc("setReachConstraints:")
        pub setReachConstraints(c: SKReachConstraints)
        @objc("reachConstraints")
        pub reachConstraints(): SKReachConstraints

        /// Per-node shader-attribute values — the fragment shader
        /// reads each `a_<name>` uniform from the value table set
        /// here. Pair with `SKAttribute.attributeWithName:type:`
        /// declared on the shader so the runtime knows the layout.
        @objc("setValue:forAttributeNamed:")
        pub setValueForAttributeNamed(value: SKAttributeValue, forAttributeNamed: NSString)
        @objc("valueForAttributeNamed:")
        pub valueForAttributeNamed(key: NSString): SKAttributeValue
    }

    /// `NSView` subclass that hosts an `SKScene`. Embed in an
    /// `NSWindow.contentView` (or any AppKit view tree) and call
    /// `presentScene:` once the scene is ready.
    @objc pub class SKScene : SKNode {
        @objc("sceneWithSize:")
        pub static sceneWithSize(s: NSSize): SKScene
        @objc("setBackgroundColor:")
        pub setBackgroundColor(c: NSColor)
        @objc("setSize:")
        pub setSize(s: NSSize)
        @objc("size")
        pub size(): NSSize
        @objc("setAnchorPoint:")
        pub setAnchorPoint(p: NSPoint)
        @objc("setScaleMode:")
        pub setScaleMode(m: SKSceneScaleMode)

        @objc("setCamera:")
        pub setCamera(c: SKCameraNode)
        @objc("camera")
        pub camera(): SKCameraNode

        @objc("physicsWorld")
        pub physicsWorld(): SKPhysicsWorld

        /// View ↔ scene point conversion. Used inside
        /// `mouseDown:` overrides to turn an
        /// `event.locationInWindow()` into scene-graph
        /// coordinates the hit-testing helpers understand.
        @objc("convertPointFromView:")
        pub convertPointFromView(p: NSPoint): NSPoint
        @objc("convertPointToView:")
        pub convertPointToView(p: NSPoint): NSPoint

        /// The view currently hosting the scene (`nil`-handle
        /// until `presentScene:` runs).
        @objc("view")
        pub view(): SKView

        /// Per-frame callback hook. The delegate is typed
        /// `NSObject` so any @objc subclass that implements the
        /// `SKSceneDelegate` selectors (see the class docstring)
        /// can be plugged in.
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
        @objc("delegate")
        pub delegate(): NSObject
    }

    /// The top-level scene rendered by an `SKView`. Size is in
    /// SpriteKit points; the view's `presentScene:` swaps it in.
    ///
    /// Per-frame update / physics / constraint hooks are
    /// delivered through an `SKSceneDelegate` installed via
    /// `setDelegate:`. The delegate is any ilang `@objc` class
    /// implementing one or more of the `SKSceneDelegate`
    /// selectors:
    ///
    /// * `@objc("update:forScene:")` — `(t: f64, scene: SKScene)`
    /// * `@objc("didEvaluateActionsForScene:")` — `(scene: SKScene)`
    /// * `@objc("didSimulatePhysicsForScene:")` — `(scene: SKScene)`
    /// * `@objc("didApplyConstraintsForScene:")` — `(scene: SKScene)`
    /// * `@objc("didFinishUpdateForScene:")` — `(scene: SKScene)`
    /// All are optional. Methods left unimplemented stay silent.
    @objc pub class SKView : NSView {
        @objc("alloc")
        pub static alloc(): SKView
        @objc("initWithFrame:")
        pub init(rect: NSRect): SKView

        @objc("presentScene:")
        pub presentScene(s: SKScene)
        @objc("presentScene:transition:")
        pub presentSceneTransition(s: SKScene, transition: SKTransition)
        @objc("setShowsFPS:")
        pub setShowsFPS(flag: bool)
        @objc("setShowsNodeCount:")
        pub setShowsNodeCount(flag: bool)
        @objc("setShowsDrawCount:")
        pub setShowsDrawCount(flag: bool)
        @objc("setShowsPhysics:")
        pub setShowsPhysics(flag: bool)
        @objc("setShowsFields:")
        pub setShowsFields(flag: bool)
        @objc("scene")
        pub scene(): SKScene
    }

}
