// SpriteKit bindings — Apple's 2D game framework. Builds on
// AppKit (SKView is an NSView subclass) and Foundation (every
// scene-graph node inherits NSObject). On macOS, CGFloat is f64
// and CGPoint / CGSize / CGRect share the layout of the
// NSPoint / NSSize / NSRect we already bind in `foundation`;
// CGVector is exposed here as `NSVector`.
//
// Covered:
//   * Scene graph: SKNode, SKScene, SKSpriteNode, SKShapeNode,
//     SKLabelNode, SKCameraNode, SKCropNode, SKEffectNode,
//     SKEmitterNode, SKLightNode, SKAudioNode
//   * Textures: SKTexture, SKTextureAtlas
//   * Physics: SKPhysicsBody, SKPhysicsWorld, SKFieldNode
//   * Animation: SKAction (move / scale / rotate / fade — both
//     relative and absolute, plus sequence / group / repeat /
//     custom block / texture animation)
//   * Host view: SKView
//   * Helpers: makePoint, makeSize, makeVector
//
// What's intentionally NOT bound yet (in case you reach for it):
//   * Texture / shape paths backed by CGPath / CIFilter — needs
//     CoreGraphics / CoreImage bindings.
//   * Two-arg block actions (customActionWithDuration:actionBlock:)
//     — the runtime's block helpers only ship void / single-id
//     trampolines today.

use foundation { NSObject, NSString, NSArray, NSData, NSPoint, NSSize, NSRect, NSVector, NSURL, nsstr }
use appkit { NSView, NSColor }
use metal { MTLCommandBuffer, MTLRenderPassDescriptor }

// SKBlendMode — composition mode for SKSpriteNode / SKShapeNode
// / SKEmitterNode / SKEffectNode. `alpha` is the default
// (standard "over" blending); `replace` skips blending entirely.
pub enum BlendMode: i64 {
    alpha      = 0
    add        = 1
    subtract   = 2
    multiply   = 3
    multiplyX2 = 4
    screen     = 5
    replace    = 6
}

// SKTextureFilteringMode — sampling mode when a texture is
// drawn at a non-1:1 scale. `nearest` for pixel-art look.
pub enum TextureFilteringMode: i64 {
    nearest = 0
    linear  = 1
}

// SKActionTimingMode — easing curve applied to an SKAction's
// progress. `linear` is the default.
pub enum ActionTimingMode: i64 {
    linear        = 0
    easeIn        = 1
    easeOut       = 2
    easeInEaseOut = 3
}

// SKLabelHorizontalAlignmentMode — where the label's anchor
// sits horizontally relative to the rendered glyphs.
pub enum LabelHorizontalAlignmentMode: i64 {
    center = 0
    left   = 1
    right  = 2
}

// SKLabelVerticalAlignmentMode — vertical anchor mode.
pub enum LabelVerticalAlignmentMode: i64 {
    baseline = 0
    center   = 1
    top      = 2
    bottom   = 3
}

// SKSceneScaleMode — how the scene's size is mapped to the view's bounds.
pub enum SceneScaleMode: i64 {
    fill        = 0
    aspectFill  = 1
    aspectFit   = 2
    resizeFill  = 3
}

// SKTransitionDirection — direction for `SKTransition.push…` /
// `reveal…` / `moveIn…` factories.
pub enum TransitionDirection: i64 {
    up    = 0
    down  = 1
    right = 2
    left  = 3
}

// SKAttributeType — value type for a custom `SKAttribute` slot
// exposed to a fragment shader.
pub enum AttributeType: i64 {
    none             = 0
    float            = 1
    vectorFloat2     = 2
    vectorFloat3     = 3
    vectorFloat4     = 4
    halfFloat        = 5
    vectorHalfFloat2 = 6
    vectorHalfFloat3 = 7
    vectorHalfFloat4 = 8
}

// SKInterpolationMode — how an `SKKeyframeSequence` blends
// between successive keyframes.
pub enum InterpolationMode: i64 {
    linear = 1
    spline = 2
    step   = 3
}

// SKRepeatMode — what happens past the last keyframe.
pub enum RepeatMode: i64 {
    clamp = 1
    loop  = 2
}

// SKTileAdjacencyMask — bitmask describing which neighbouring
// tiles a `SKTileGroupRule` is supposed to match. Bound as raw
// `u32` flags — combine via bitwise OR. Mirrors
// `<SpriteKit/SKTileSet.h>` `SKTileAdjacency*`.
pub const TileAdjacencyUp:        u32 = 0x01
pub const TileAdjacencyUpperRight: u32 = 0x02
pub const TileAdjacencyRight:     u32 = 0x04
pub const TileAdjacencyLowerRight: u32 = 0x08
pub const TileAdjacencyDown:      u32 = 0x10
pub const TileAdjacencyLowerLeft: u32 = 0x20
pub const TileAdjacencyLeft:      u32 = 0x40
pub const TileAdjacencyUpperLeft: u32 = 0x80

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

        // Hierarchy
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

        // Transform — exposed as properties so user code reads
        // `node.position` / writes `node.position = p` rather
        // than `node.position()` / `node.setPosition(p)`.
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

        // Identity / state
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

        // Geometry tests
        @objc("containsPoint:")
        pub containsPoint(p: NSPoint): bool
        @objc("frame")
        pub frame(): NSRect

        // Actions
        @objc("runAction:")
        pub runAction(a: SKAction)
        @objc("runAction:withKey:")
        pub runActionWithKey(a: SKAction, key: NSString)
        @objc("removeActionForKey:")
        pub removeActionForKey(key: NSString)
        @objc("hasActions")
        pub hasActions(): bool

        // Physics
        @objc("setPhysicsBody:")
        pub setPhysicsBody(b: SKPhysicsBody)
        @objc("physicsBody")
        pub physicsBody(): SKPhysicsBody

        // Constraints — apply an array of `SKConstraint`s
        // after every frame's transform update.
        @objc("setConstraints:")
        pub setConstraints(arr: NSArray)
        @objc("constraints")
        pub constraints(): NSArray

        // Scene-graph search — `nodeAtPoint:` finds the
        // top-most node whose accumulated frame contains
        // `point` (in this node's coordinate system).
        // `nodesAtPoint:` returns every match.
        @objc("nodeAtPoint:")
        pub nodeAtPoint(p: NSPoint): SKNode
        @objc("nodesAtPoint:")
        pub nodesAtPoint(p: NSPoint): NSArray
        @objc("intersectsNode:")
        pub intersectsNode(n: SKNode): bool

        // Coordinate conversion between two nodes in the same
        // scene graph (or between scene-graph and view space —
        // bridge via `SKScene.convertPointFromView:` first).
        @objc("convertPoint:toNode:")
        pub convertPointToNode(p: NSPoint, toNode: SKNode): NSPoint
        @objc("convertPoint:fromNode:")
        pub convertPointFromNode(p: NSPoint, fromNode: SKNode): NSPoint

        // Inverse-kinematics joint limits — consumed by
        // `SKAction.reachTo:rootNode:duration:` and the like.
        @objc("setReachConstraints:")
        pub setReachConstraints(c: SKReachConstraints)
        @objc("reachConstraints")
        pub reachConstraints(): SKReachConstraints

        // Per-node shader-attribute values — the fragment shader
        // reads each `a_<name>` uniform from the value table set
        // here. Pair with `SKAttribute.attributeWithName:type:`
        // declared on the shader so the runtime knows the layout.
        @objc("setValue:forAttributeNamed:")
        pub setValueForAttributeNamed(value: SKAttributeValue, forAttributeNamed: NSString)
        @objc("valueForAttributeNamed:")
        pub valueForAttributeNamed(key: NSString): SKAttributeValue
    }

    /// The top-level scene rendered by an `SKView`. Size is in
    /// SpriteKit points; the view's `presentScene:` swaps it in.
    ///
    /// Per-frame update / physics / constraint hooks are
    /// delivered through an `SKSceneDelegate` installed via
    /// `setDelegate:`. The delegate is any ilang `@objc` class
    /// implementing one or more of the `SKSceneDelegate`
    /// selectors:
    ///   * `@objc("update:forScene:")` — `(t: f64, scene: SKScene)`
    ///   * `@objc("didEvaluateActionsForScene:")` — `(scene: SKScene)`
    ///   * `@objc("didSimulatePhysicsForScene:")` — `(scene: SKScene)`
    ///   * `@objc("didApplyConstraintsForScene:")` — `(scene: SKScene)`
    ///   * `@objc("didFinishUpdateForScene:")` — `(scene: SKScene)`
    /// All are optional. Methods left unimplemented stay silent.
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
        pub setScaleMode(m: SceneScaleMode)

        @objc("setCamera:")
        pub setCamera(c: SKCameraNode)
        @objc("camera")
        pub camera(): SKCameraNode

        @objc("physicsWorld")
        pub physicsWorld(): SKPhysicsWorld

        // View ↔ scene point conversion. Used inside
        // `mouseDown:` overrides to turn an
        // `event.locationInWindow()` into scene-graph
        // coordinates the hit-testing helpers understand.
        @objc("convertPointFromView:")
        pub convertPointFromView(p: NSPoint): NSPoint
        @objc("convertPointToView:")
        pub convertPointToView(p: NSPoint): NSPoint

        // The view currently hosting the scene (`nil`-handle
        // until `presentScene:` runs).
        @objc("view")
        pub view(): SKView

        // Per-frame callback hook. The delegate is typed
        // `NSObject` so any @objc subclass that implements the
        // `SKSceneDelegate` selectors (see the class docstring)
        // can be plugged in.
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
        @objc("delegate")
        pub delegate(): NSObject
    }

    /// Coloured / textured rectangular sprite. The
    /// `spriteNodeWithColor:size:` factory is the easiest way to
    /// get something visible without preparing image assets;
    /// `spriteNodeWithTexture:` and friends pull from an
    /// `SKTexture`.
    @objc pub class SKSpriteNode : SKNode {
        @objc("spriteNodeWithColor:size:")
        pub static spriteNodeWithColor(c: NSColor, size: NSSize): SKSpriteNode
        @objc("spriteNodeWithImageNamed:")
        pub static spriteNodeWithImageNamed(name: NSString): SKSpriteNode
        @objc("spriteNodeWithTexture:")
        pub static spriteNodeWithTexture(t: SKTexture): SKSpriteNode
        @objc("spriteNodeWithTexture:size:")
        pub static spriteNodeWithTextureAndSize(t: SKTexture, size: NSSize): SKSpriteNode

        @objc("setColor:")
        pub setColor(c: NSColor)
        @objc("setSize:")
        pub setSize(s: NSSize)
        @objc("size")
        pub size(): NSSize
        @objc("setTexture:")
        pub setTexture(t: SKTexture)
        @objc("texture")
        pub texture(): SKTexture
        @objc("setAnchorPoint:")
        pub setAnchorPoint(p: NSPoint)
        @objc("setColorBlendFactor:")
        pub setColorBlendFactor(f: f64)
        @objc("setBlendMode:")
        pub setBlendMode(m: BlendMode)
        @objc("setShader:")
        pub setShader(s: SKShader)
        @objc("shader")
        pub shader(): SKShader

        // Warp geometry (SKWarpable) — drives a per-frame
        // vertex remap of the sprite quad. Pair with
        // `SKAction.warpTo` to animate the transition.
        @objc("setWarpGeometry:")
        pub setWarpGeometry(g: SKWarpGeometry)
        @objc("warpGeometry")
        pub warpGeometry(): SKWarpGeometry
    }

    /// Vector shape node — circles / rects / arbitrary paths.
    /// Fill / stroke are set separately so you can do
    /// outline-only or filled-only.
    @objc pub class SKShapeNode : SKNode {
        @objc("shapeNodeWithRect:")
        pub static shapeNodeWithRect(r: NSRect): SKShapeNode
        @objc("shapeNodeWithRectOfSize:")
        pub static shapeNodeWithRectOfSize(s: NSSize): SKShapeNode
        @objc("shapeNodeWithRect:cornerRadius:")
        pub static shapeNodeWithRoundedRect(r: NSRect, cornerRadius: f64): SKShapeNode
        @objc("shapeNodeWithCircleOfRadius:")
        pub static shapeNodeWithCircleOfRadius(r: f64): SKShapeNode
        @objc("shapeNodeWithEllipseOfSize:")
        pub static shapeNodeWithEllipseOfSize(s: NSSize): SKShapeNode
        @objc("shapeNodeWithEllipseInRect:")
        pub static shapeNodeWithEllipseInRect(r: NSRect): SKShapeNode

        @objc("setFillColor:")
        pub setFillColor(c: NSColor)
        @objc("setStrokeColor:")
        pub setStrokeColor(c: NSColor)
        @objc("setLineWidth:")
        pub setLineWidth(w: f64)
        @objc("setGlowWidth:")
        pub setGlowWidth(w: f64)
        @objc("setAntialiased:")
        pub setAntialiased(flag: bool)
        @objc("setBlendMode:")
        pub setBlendMode(m: BlendMode)
    }

    /// Text node — single-line label rendered with the named
    /// font (defaults to Helvetica). For multi-line, build a
    /// container `SKNode` of stacked labels.
    @objc pub class SKLabelNode : SKNode {
        @objc("labelNodeWithText:")
        pub static labelNodeWithText(t: NSString): SKLabelNode
        @objc("labelNodeWithFontNamed:")
        pub static labelNodeWithFontNamed(name: NSString): SKLabelNode

        @objc("setText:")
        pub setText(t: NSString)
        @objc("text")
        pub text(): NSString
        @objc("setFontName:")
        pub setFontName(n: NSString)
        @objc("setFontSize:")
        pub setFontSize(s: f64)
        @objc("setFontColor:")
        pub setFontColor(c: NSColor)
        @objc("setHorizontalAlignmentMode:")
        pub setHorizontalAlignmentMode(m: LabelHorizontalAlignmentMode)
        @objc("setVerticalAlignmentMode:")
        pub setVerticalAlignmentMode(m: LabelVerticalAlignmentMode)
        @objc("setNumberOfLines:")
        pub setNumberOfLines(n: i64)
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

    /// `SKTexture` — an immutable 2-D image source consumed by
    /// `SKSpriteNode` / `SKEmitterNode` / `SKShapeNode`. Build
    /// from an image-asset name, raw bytes, or by slicing a
    /// region out of an existing texture (sprite-sheet
    /// pattern).
    @objc pub class SKTexture : NSObject {
        @objc("textureWithImageNamed:")
        pub static textureWithImageNamed(name: NSString): SKTexture
        @objc("textureWithImageNamed:normalMap:")
        pub static textureWithImageNamedAndNormalMap(name: NSString, normalMap: bool): SKTexture
        @objc("textureWithRect:inTexture:")
        pub static textureWithRect(r: NSRect, inTexture: SKTexture): SKTexture
        @objc("textureWithData:size:")
        pub static textureWithData(data: NSData, size: NSSize): SKTexture

        @objc("size")
        pub size(): NSSize
        @objc("setFilteringMode:")
        pub setFilteringMode(m: TextureFilteringMode)
        @objc("filteringMode")
        pub filteringMode(): TextureFilteringMode

        // Normal-map generation. The first form computes a
        // default-tuned normal map; the second lets you dial
        // `smoothness` (0..1) and `contrast` (≥ 1).
        @objc("textureByGeneratingNormalMap")
        pub textureByGeneratingNormalMap(): SKTexture
        @objc("textureByGeneratingNormalMapWithSmoothness:contrast:")
        pub textureByGeneratingNormalMapWithParams(smoothness: f64, contrast: f64): SKTexture

        // Async preload — the block is invoked on the main
        // queue once the texture's pixels are uploaded. Wrap an
        // ilang `fn()` with `new ObjCBlock(...)` at the call
        // site; the type checker requires the matching signature.
        @objc("preloadWithCompletionHandler:")
        pub preloadWithCompletionHandler(block: ObjCBlock<fn()>)
        @objc("preloadTextures:withCompletionHandler:")
        pub static preloadTexturesWithCompletionHandler(
            textures: NSArray, withCompletionHandler: ObjCBlock<fn()>
        )
    }

    /// `SKMutableTexture` — texture whose pixel data can be
    /// updated post-creation. Useful for procedurally-generated
    /// textures or per-frame readback. Pixel mutation goes
    /// through `modifyPixelDataWithBlock:` — pass a block built
    /// with `new ObjCBlock(fn(ptr: i64, len: i64) { ... })` and
    /// write into the buffer via the `writeU8` / `writeU32`
    /// runtime helpers. The buffer is the texture's backing
    /// store, so mutations are persisted into the texture.
    @objc pub class SKMutableTexture : SKTexture {
        @objc("mutableTextureWithSize:")
        pub static mutableTextureWithSize(s: NSSize): SKMutableTexture
        @objc("mutableTextureWithSize:pixelFormat:")
        pub static mutableTextureWithSizeAndPixelFormat(s: NSSize, pixelFormat: i32): SKMutableTexture

        @objc("modifyPixelDataWithBlock:")
        pub modifyPixelDataWithBlock(block: ObjCBlock<fn(i64, i64)>)
    }

    /// `SKTextureAtlas` — collection of textures loaded as one
    /// asset (`Foo.atlas` directory or `.spriteatlas` bundle).
    /// `textureNames` enumerates everything in declaration
    /// order; `textureNamed:` pulls one out by name.
    @objc pub class SKTextureAtlas : NSObject {
        @objc("atlasNamed:")
        pub static atlasNamed(name: NSString): SKTextureAtlas

        @objc("textureNamed:")
        pub textureNamed(name: NSString): SKTexture
        @objc("textureNames")
        pub textureNames(): NSArray
    }

    /// Animation primitive — built via the class factories and
    /// run on an `SKNode` via `runAction:`. Composable via
    /// `sequence:` / `group:` / `repeatActionForever:`.
    /// Relative variants take a delta (`moveByX:y:`); absolute
    /// variants take the destination (`moveTo:`).
    @objc pub class SKAction : NSObject {
        // Movement — relative
        @objc("moveByX:y:duration:")
        pub static moveByXY(x: f64, y: f64, duration: f64): SKAction
        @objc("moveBy:duration:")
        pub static moveBy(delta: NSVector, duration: f64): SKAction
        // Movement — absolute
        @objc("moveTo:duration:")
        pub static moveTo(p: NSPoint, duration: f64): SKAction
        @objc("moveToX:duration:")
        pub static moveToX(x: f64, duration: f64): SKAction
        @objc("moveToY:duration:")
        pub static moveToY(y: f64, duration: f64): SKAction

        // Rotation
        @objc("rotateByAngle:duration:")
        pub static rotateByAngle(radians: f64, duration: f64): SKAction
        @objc("rotateToAngle:duration:")
        pub static rotateToAngle(radians: f64, duration: f64): SKAction

        // Scale
        @objc("scaleBy:duration:")
        pub static scaleBy(scale: f64, duration: f64): SKAction
        @objc("scaleTo:duration:")
        pub static scaleTo(scale: f64, duration: f64): SKAction
        @objc("scaleXTo:y:duration:")
        pub static scaleXYTo(xs: f64, ys: f64, duration: f64): SKAction
        @objc("scaleXTo:duration:")
        pub static scaleXTo(xs: f64, duration: f64): SKAction
        @objc("scaleYTo:duration:")
        pub static scaleYTo(ys: f64, duration: f64): SKAction

        // Fade
        @objc("fadeAlphaBy:duration:")
        pub static fadeAlphaBy(alpha: f64, duration: f64): SKAction
        @objc("fadeAlphaTo:duration:")
        pub static fadeAlphaTo(alpha: f64, duration: f64): SKAction
        @objc("fadeInWithDuration:")
        pub static fadeIn(duration: f64): SKAction
        @objc("fadeOutWithDuration:")
        pub static fadeOut(duration: f64): SKAction

        // Texture animation (for SKSpriteNode)
        @objc("setTexture:")
        pub static setTexture(t: SKTexture): SKAction
        @objc("animateWithTextures:timePerFrame:")
        pub static animateWithTextures(textures: NSArray, timePerFrame: f64): SKAction

        // Tree manipulation
        @objc("removeFromParent")
        pub static removeFromParent(): SKAction
        @objc("waitForDuration:")
        pub static waitForDuration(d: f64): SKAction
        @objc("waitForDuration:withRange:")
        pub static waitForDurationRange(d: f64, withRange: f64): SKAction

        // Sprite colourise (only meaningful on SKSpriteNode receivers)
        @objc("colorizeWithColor:colorBlendFactor:duration:")
        pub static colorizeWithColor(c: NSColor, colorBlendFactor: f64, duration: f64): SKAction
        @objc("colorizeWithColorBlendFactor:duration:")
        pub static colorizeWithColorBlendFactor(blend: f64, duration: f64): SKAction

        // Audio — file is looked up in the main bundle
        @objc("playSoundFileNamed:waitForCompletion:")
        pub static playSoundFileNamed(name: NSString, waitForCompletion: bool): SKAction

        // Dispatch to a named descendant — the action runs on the
        // first node found by `childNodeWithName:` at the target.
        @objc("runAction:onChildWithName:")
        pub static runActionOnChild(action: SKAction, onChildWithName: NSString): SKAction

        // ObjC selector — `[target performSelector]` fires
        // synchronously when the action runs. Build the SEL via
        // `cocoa.selectorPtr("name")`.
        @objc("performSelector:onTarget:")
        pub static performSelectorOnTarget(sel: i64, onTarget: NSObject): SKAction

        // Block — fires once. ObjC signature is
        // `+ (SKAction *)runBlock:(dispatch_block_t)block` which
        // is a `void(^)(void)`; build the argument with
        // `new ObjCBlock(fn() { ... })`.
        @objc("runBlock:")
        pub static runBlock(block: ObjCBlock<fn()>): SKAction

        // Instance — reverse / timing
        @objc("reversedAction")
        pub reversedAction(): SKAction
        @objc("setTimingMode:")
        pub setTimingMode(m: ActionTimingMode)
        @objc("setSpeed:")
        pub setActionSpeed(s: f64)
        @objc("setDuration:")
        pub setDuration(d: f64)

        // Composition
        @objc("sequence:")
        pub static sequence(actions: NSArray): SKAction
        @objc("group:")
        pub static group(actions: NSArray): SKAction
        @objc("repeatAction:count:")
        pub static repeatAction(action: SKAction, count: i64): SKAction
        @objc("repeatActionForever:")
        pub static repeatActionForever(action: SKAction): SKAction

        // Warp geometry — animate a sprite's per-frame vertex
        // remap from its current `SKWarpGeometry` to `warp` over
        // `duration` seconds. `numberOfColumns` / `numberOfRows`
        // must match for a smooth interpolation; mismatched
        // dimensions snap to the new mesh instantly.
        @objc("warpTo:duration:")
        pub static warpTo(warp: SKWarpGeometry, duration: f64): SKAction
    }

    // ─── Physics ─────────────────────────────────────────────────

    /// `SKPhysicsBody` — rigid body attached to an `SKNode` via
    /// `setPhysicsBody:`. The factory class methods build the
    /// shape (rect / circle / edge); the setters configure mass
    /// / damping / categoryBitMask / etc. Set
    /// `setDynamic(false)` for static colliders (walls / floor).
    @objc pub class SKPhysicsBody : NSObject {
        @objc("bodyWithRectangleOfSize:")
        pub static bodyWithRectangleOfSize(s: NSSize): SKPhysicsBody
        @objc("bodyWithCircleOfRadius:")
        pub static bodyWithCircleOfRadius(r: f64): SKPhysicsBody
        @objc("bodyWithEdgeFromPoint:toPoint:")
        pub static bodyWithEdgeFromTo(a: NSPoint, toPoint: NSPoint): SKPhysicsBody
        @objc("bodyWithEdgeLoopFromRect:")
        pub static bodyWithEdgeLoopFromRect(r: NSRect): SKPhysicsBody

        @objc("isDynamic")
        pub get isDynamic(): bool
        @objc("setDynamic:")
        pub set isDynamic(flag: bool)
        @objc("setAffectedByGravity:")
        pub setAffectedByGravity(flag: bool)
        @objc("setAllowsRotation:")
        pub setAllowsRotation(flag: bool)
        @objc("setUsesPreciseCollisionDetection:")
        pub setUsesPreciseCollisionDetection(flag: bool)

        @objc("setMass:")
        pub setMass(m: f64)
        @objc("mass")
        pub mass(): f64
        @objc("setDensity:")
        pub setDensity(d: f64)
        @objc("setFriction:")
        pub setFriction(f: f64)
        @objc("setRestitution:")
        pub setRestitution(r: f64)
        @objc("setLinearDamping:")
        pub setLinearDamping(v: f64)
        @objc("setAngularDamping:")
        pub setAngularDamping(v: f64)

        @objc("setVelocity:")
        pub setVelocity(v: NSVector)
        @objc("velocity")
        pub velocity(): NSVector
        @objc("setAngularVelocity:")
        pub setAngularVelocity(v: f64)

        @objc("setCategoryBitMask:")
        pub setCategoryBitMask(m: u32)
        @objc("setCollisionBitMask:")
        pub setCollisionBitMask(m: u32)
        @objc("setContactTestBitMask:")
        pub setContactTestBitMask(m: u32)
        @objc("setFieldBitMask:")
        pub setFieldBitMask(m: u32)

        @objc("applyForce:")
        pub applyForce(f: NSVector)
        @objc("applyForce:atPoint:")
        pub applyForceAtPoint(f: NSVector, atPoint: NSPoint)
        @objc("applyImpulse:")
        pub applyImpulse(i: NSVector)
        @objc("applyImpulse:atPoint:")
        pub applyImpulseAtPoint(i: NSVector, atPoint: NSPoint)
        @objc("applyTorque:")
        pub applyTorque(t: f64)
        @objc("applyAngularImpulse:")
        pub applyAngularImpulse(i: f64)
    }

    /// `SKPhysicsWorld` — owned by each `SKScene`. Sets the
    /// simulation's gravity / speed; routes contact events to a
    /// delegate.
    ///
    /// The delegate is any ilang `@objc` class implementing the
    /// `SKPhysicsContactDelegate` selectors:
    ///   * `@objc("didBeginContact:")` — `(contact: SKPhysicsContact)`
    ///   * `@objc("didEndContact:")` — `(contact: SKPhysicsContact)`
    /// Install via `world.setContactDelegate(delegate)`. Read
    /// the contact off `contact.bodyA()` / `bodyB()` /
    /// `contactPoint()` / `contactNormal()` / `collisionImpulse()`.
    @objc pub class SKPhysicsWorld : NSObject {
        @objc("setGravity:")
        pub setGravity(v: NSVector)
        @objc("gravity")
        pub gravity(): NSVector
        @objc("setSpeed:")
        pub setSpeed(s: f64)
        @objc("setContactDelegate:")
        pub setContactDelegate(d: NSObject)

        @objc("addJoint:")
        pub addJoint(j: SKPhysicsJoint)
        @objc("removeJoint:")
        pub removeJoint(j: SKPhysicsJoint)
        @objc("removeAllJoints")
        pub removeAllJoints()
    }

    /// `SKFieldNode` — physics field that perturbs nearby
    /// dynamic bodies. The class methods cover the most common
    /// kinds; `strength` scales their effect.
    @objc pub class SKFieldNode : SKNode {
        @objc("radialGravityField")
        pub static radialGravityField(): SKFieldNode
        @objc("linearGravityFieldWithVector:")
        pub static linearGravityFieldWithVector(v: NSVector): SKFieldNode
        @objc("vortexField")
        pub static vortexField(): SKFieldNode
        @objc("dragField")
        pub static dragField(): SKFieldNode
        @objc("springField")
        pub static springField(): SKFieldNode
        @objc("electricField")
        pub static electricField(): SKFieldNode
        @objc("magneticField")
        pub static magneticField(): SKFieldNode

        @objc("setStrength:")
        pub setStrength(s: f64)
        @objc("setFalloff:")
        pub setFalloff(f: f64)
        @objc("setEnabled:")
        pub setFieldEnabled(flag: bool)
        @objc("setCategoryBitMask:")
        pub setCategoryBitMask(m: u32)
        @objc("setRegion:")
        pub setRegion(r: SKRegion)
        @objc("region")
        pub region(): SKRegion
    }

    // ─── Effect nodes ────────────────────────────────────────────

    /// `SKCameraNode` — when attached to a scene via
    /// `scene.setCamera(...)`, the scene's content is rendered
    /// relative to the camera's position / rotation / scale.
    /// Camera itself is a plain `SKNode` — no extra properties.
    @objc pub class SKCameraNode : SKNode {
        @objc("alloc")
        pub static alloc(): SKCameraNode
        @objc("init")
        pub init(): SKCameraNode
    }

    /// `SKCropNode` — masks its child subtree by the alpha of
    /// `maskNode`. Use a shape / sprite as the mask.
    @objc pub class SKCropNode : SKNode {
        @objc("alloc")
        pub static alloc(): SKCropNode
        @objc("init")
        pub init(): SKCropNode

        @objc("setMaskNode:")
        pub setMaskNode(n: SKNode)
        @objc("maskNode")
        pub maskNode(): SKNode
    }

    /// `SKEffectNode` — flattens its subtree into a texture
    /// before drawing, optionally applying a Core Image filter.
    /// (Filter setting needs a CIFilter binding — not exposed
    /// here yet.)
    @objc pub class SKEffectNode : SKNode {
        @objc("alloc")
        pub static alloc(): SKEffectNode
        @objc("init")
        pub init(): SKEffectNode

        @objc("setShouldRasterize:")
        pub setShouldRasterize(flag: bool)
        @objc("setShouldEnableEffects:")
        pub setShouldEnableEffects(flag: bool)
        @objc("setBlendMode:")
        pub setBlendMode(m: BlendMode)
        @objc("setShader:")
        pub setShader(s: SKShader)
        @objc("shader")
        pub shader(): SKShader

        // Warp geometry — same shape as `SKSpriteNode`'s; the
        // EffectNode warps its rasterised subtree as a whole.
        @objc("setWarpGeometry:")
        pub setWarpGeometry(g: SKWarpGeometry)
        @objc("warpGeometry")
        pub warpGeometry(): SKWarpGeometry
    }

    /// `SKEmitterNode` — particle source. The setters here
    /// cover the basics (rate / lifetime / colour / size /
    /// speed / angle); load a designed `.sks` file via
    /// `nodeWithFileNamed:` (on `SKNode`) for a fully tuned
    /// system.
    @objc pub class SKEmitterNode : SKNode {
        @objc("alloc")
        pub static alloc(): SKEmitterNode
        @objc("init")
        pub init(): SKEmitterNode

        @objc("setParticleTexture:")
        pub setParticleTexture(t: SKTexture)
        @objc("setParticleBirthRate:")
        pub setParticleBirthRate(r: f64)
        @objc("setNumParticlesToEmit:")
        pub setNumParticlesToEmit(n: i64)
        @objc("setParticleLifetime:")
        pub setParticleLifetime(l: f64)
        @objc("setParticleLifetimeRange:")
        pub setParticleLifetimeRange(l: f64)
        @objc("setParticleColor:")
        pub setParticleColor(c: NSColor)
        @objc("setParticleSize:")
        pub setParticleSize(s: NSSize)
        @objc("setParticleSpeed:")
        pub setParticleSpeed(s: f64)
        @objc("setParticleSpeedRange:")
        pub setParticleSpeedRange(s: f64)
        @objc("setEmissionAngle:")
        pub setEmissionAngle(a: f64)
        @objc("setEmissionAngleRange:")
        pub setEmissionAngleRange(a: f64)
        @objc("setParticleAlpha:")
        pub setParticleAlpha(a: f64)
        @objc("setParticleAlphaRange:")
        pub setParticleAlphaRange(a: f64)
        @objc("setParticleScale:")
        pub setParticleScale(s: f64)
        @objc("setParticleScaleRange:")
        pub setParticleScaleRange(s: f64)
        @objc("setParticleBlendMode:")
        pub setParticleBlendMode(m: BlendMode)
        @objc("resetSimulation")
        pub resetSimulation()

        // Rendering target — particles are emitted into
        // `targetNode`'s coordinate space if set. Useful when
        // the emitter is mounted on a moving parent but the
        // particles should stay in world space.
        @objc("setTargetNode:")
        pub setTargetNode(n: SKNode)
        @objc("targetNode")
        pub targetNode(): SKNode

        // Constant per-frame acceleration applied to each
        // particle (world units / s²). Combine with the
        // particle's initial `Speed` / `EmissionAngle` for
        // gravity-like fall-off, smoke drift, etc.
        @objc("setXAcceleration:")
        pub setXAcceleration(a: f64)
        @objc("setYAcceleration:")
        pub setYAcceleration(a: f64)

        // Spread the spawn point over a rectangular range
        // (`dx`, `dy` half-widths). Default `(0, 0)` emits
        // every particle from the node's exact position.
        @objc("setParticlePositionRange:")
        pub setParticlePositionRange(v: NSVector)

        // Each emitted particle runs `action` for its lifetime
        // — handy for per-particle rotation / scale tweens
        // beyond the built-in `setParticleScaleSpeed` etc.
        @objc("setParticleAction:")
        pub setParticleAction(a: SKAction)
        @objc("particleAction")
        pub particleAction(): SKAction

        // Keyframe-sequenced animation of per-particle colour
        // / colour-blend-factor. The sequence is sampled
        // against each particle's normalised lifetime
        // (0.0 = birth, 1.0 = death).
        @objc("setParticleColorSequence:")
        pub setParticleColorSequence(s: SKKeyframeSequence)
        @objc("setParticleColorBlendFactor:")
        pub setParticleColorBlendFactor(f: f64)
        @objc("setParticleColorBlendFactorSequence:")
        pub setParticleColorBlendFactorSequence(s: SKKeyframeSequence)

        // Per-channel drift speeds — combine with the
        // particle's starting `particleColor` for fade /
        // tint-shift effects.
        @objc("setParticleColorRedSpeed:")
        pub setParticleColorRedSpeed(s: f64)
        @objc("setParticleColorGreenSpeed:")
        pub setParticleColorGreenSpeed(s: f64)
        @objc("setParticleColorBlueSpeed:")
        pub setParticleColorBlueSpeed(s: f64)
        @objc("setParticleColorAlphaSpeed:")
        pub setParticleColorAlphaSpeed(s: f64)

        // Per-channel jitter ranges (added randomly at spawn).
        @objc("setParticleColorRedRange:")
        pub setParticleColorRedRange(r: f64)
        @objc("setParticleColorGreenRange:")
        pub setParticleColorGreenRange(r: f64)
        @objc("setParticleColorBlueRange:")
        pub setParticleColorBlueRange(r: f64)
        @objc("setParticleColorAlphaRange:")
        pub setParticleColorAlphaRange(r: f64)

        // Particle scale rate of change (units / s) — combine
        // with `setParticleScale` / `setParticleScaleRange`.
        @objc("setParticleScaleSpeed:")
        pub setParticleScaleSpeed(s: f64)

        // Z-rotation animation parameters for the particle's
        // own coordinate frame (radians + radians/s).
        @objc("setParticleRotation:")
        pub setParticleRotation(r: f64)
        @objc("setParticleRotationRange:")
        pub setParticleRotationRange(r: f64)
        @objc("setParticleRotationSpeed:")
        pub setParticleRotationSpeed(r: f64)

        // Custom shader applied to each particle quad.
        @objc("setShader:")
        pub setShader(s: SKShader)
    }

    /// `SKKeyframeSequence` — list of `(value, time)` pairs
    /// sampled by `SKEmitterNode.setParticleColorSequence:` and
    /// friends. Time values are in seconds; the interpolation
    /// mode controls how successive frames blend
    /// (linear / spline / step).
    @objc pub class SKKeyframeSequence : NSObject {
        @objc("keyframeSequenceWithKeyframeValues:times:")
        pub static keyframeSequenceWithValuesAndTimes(
            values: NSArray, times: NSArray
        ): SKKeyframeSequence

        @objc("addKeyframeValue:time:")
        pub addKeyframeValue(value: NSObject, time: f64)
        @objc("removeKeyframeAtIndex:")
        pub removeKeyframeAtIndex(i: i64)
        @objc("removeLastKeyframe")
        pub removeLastKeyframe()
        @objc("count")
        pub count(): i64

        @objc("setInterpolationMode:")
        pub setInterpolationMode(m: InterpolationMode)
        @objc("setRepeatMode:")
        pub setRepeatMode(m: RepeatMode)
        @objc("sampleAtTime:")
        pub sampleAtTime(t: f64): NSObject
    }

    /// `SKLightNode` — point light. Sprites whose
    /// `lightingBitMask` matches `categoryBitMask` are lit.
    @objc pub class SKLightNode : SKNode {
        @objc("alloc")
        pub static alloc(): SKLightNode
        @objc("init")
        pub init(): SKLightNode

        @objc("setEnabled:")
        pub setLightEnabled(flag: bool)
        @objc("setLightColor:")
        pub setLightColor(c: NSColor)
        @objc("setAmbientColor:")
        pub setAmbientColor(c: NSColor)
        @objc("setShadowColor:")
        pub setShadowColor(c: NSColor)
        @objc("setFalloff:")
        pub setLightFalloff(f: f64)
        @objc("setCategoryBitMask:")
        pub setCategoryBitMask(m: u32)
    }

    /// `SKAudioNode` — positional / non-positional audio source.
    /// Construct from a file in the main bundle or an
    /// `NSURL`; with `setAutoplayLooped(true)` (the default)
    /// playback starts as soon as the node is added to the
    /// scene.
    @objc pub class SKAudioNode : SKNode {
        @objc("alloc")
        pub static alloc(): SKAudioNode
        @objc("initWithFileNamed:")
        pub initWithFileNamed(name: NSString): SKAudioNode
        @objc("initWithURL:")
        pub initWithURL(url: NSURL): SKAudioNode

        @objc("setAutoplayLooped:")
        pub setAutoplayLooped(flag: bool)
        @objc("setPositional:")
        pub setPositional(flag: bool)
    }

    // ─── Tile maps ───────────────────────────────────────────────

    /// `SKTileDefinition` — one paintable tile, backed by an
    /// `SKTexture`. Carries the tile's pixel size and optional
    /// debug name.
    @objc pub class SKTileDefinition : NSObject {
        @objc("tileDefinitionWithTexture:")
        pub static tileDefinitionWithTexture(t: SKTexture): SKTileDefinition
        @objc("tileDefinitionWithTexture:size:")
        pub static tileDefinitionWithTextureAndSize(t: SKTexture, size: NSSize): SKTileDefinition

        @objc("setSize:")
        pub setSize(s: NSSize)
        @objc("size")
        pub size(): NSSize
        @objc("setName:")
        pub setName(n: NSString)
        @objc("name")
        pub name(): NSString
    }

    /// `SKTileGroup` — paintable cluster of tile definitions
    /// (typically one definition for the simple case, multiple
    /// rules for auto-tiling). Painted into an `SKTileMapNode`
    /// at a (column, row) cell.
    @objc pub class SKTileGroup : NSObject {
        @objc("tileGroupWithTileDefinition:")
        pub static tileGroupWithTileDefinition(def: SKTileDefinition): SKTileGroup
        @objc("tileGroupWithRules:")
        pub static tileGroupWithRules(rules: NSArray): SKTileGroup
        @objc("emptyTileGroup")
        pub static emptyTileGroup(): SKTileGroup

        @objc("setName:")
        pub setGroupName(n: NSString)
        @objc("rules")
        pub rules(): NSArray
    }

    /// `SKTileSet` — collection of `SKTileGroup`s loaded from
    /// the asset catalog. Pass into `SKTileMapNode`'s factory to
    /// pick the brush palette.
    @objc pub class SKTileSet : NSObject {
        @objc("tileSetNamed:")
        pub static tileSetNamed(name: NSString): SKTileSet
        @objc("tileSetWithTileGroups:")
        pub static tileSetWithTileGroups(groups: NSArray): SKTileSet

        @objc("tileGroups")
        pub tileGroups(): NSArray
        @objc("setName:")
        pub setSetName(n: NSString)
        @objc("setDefaultTileSize:")
        pub setDefaultTileSize(s: NSSize)
    }

    /// `SKTileMapNode` — grid of tiles drawn from one or more
    /// `SKTileGroup`s. Build via
    /// `tileMapNodeWithTileSet:columns:rows:tileSize:`, then
    /// paint cells with `setTileGroup:forColumn:row:`.
    @objc pub class SKTileMapNode : SKNode {
        @objc("tileMapNodeWithTileSet:columns:rows:tileSize:")
        pub static tileMapNodeWithTileSet(
            tileSet: SKTileSet, columns: i64, rows: i64, tileSize: NSSize
        ): SKTileMapNode

        @objc("numberOfColumns")
        pub numberOfColumns(): i64
        @objc("numberOfRows")
        pub numberOfRows(): i64
        @objc("tileSize")
        pub tileSize(): NSSize
        @objc("mapSize")
        pub mapSize(): NSSize

        @objc("setTileGroup:forColumn:row:")
        pub setTileGroup(g: SKTileGroup, forColumn: i64, row: i64)
        @objc("tileGroupAtColumn:row:")
        pub tileGroupAtColumn(col: i64, row: i64): SKTileGroup
        @objc("fillWithTileGroup:")
        pub fillWithTileGroup(g: SKTileGroup)
        @objc("setEnableAutomapping:")
        pub setEnableAutomapping(flag: bool)
    }

    /// `SKReferenceNode` — load a `.sks` scene file at runtime
    /// and present it as a sub-node. Useful for splitting scene
    /// editor work across files.
    @objc pub class SKReferenceNode : SKNode {
        @objc("referenceNodeWithFileNamed:")
        pub static referenceNodeWithFileNamed(name: NSString): SKReferenceNode
        @objc("referenceNodeWithURL:")
        pub static referenceNodeWithURL(url: NSURL): SKReferenceNode
    }

    /// `SKVideoNode` — plays a movie file inside the scene
    /// graph. Use `play` / `pause` for transport, and embed the
    /// node like any other `SKNode`.
    @objc pub class SKVideoNode : SKNode {
        @objc("videoNodeWithFileNamed:")
        pub static videoNodeWithFileNamed(name: NSString): SKVideoNode
        @objc("videoNodeWithURL:")
        pub static videoNodeWithURL(url: NSURL): SKVideoNode

        @objc("play")
        pub play()
        @objc("pause")
        pub pause()
        @objc("setSize:")
        pub setSize(s: NSSize)
        @objc("size")
        pub size(): NSSize
    }

    // ─── Physics joints ──────────────────────────────────────────

    /// `SKPhysicsJoint` — abstract base for the joint classes
    /// below. Holds the two bodies it connects.
    @objc pub class SKPhysicsJoint : NSObject {
        @objc("bodyA")
        pub bodyA(): SKPhysicsBody
        @objc("bodyB")
        pub bodyB(): SKPhysicsBody
    }

    /// `SKPhysicsJointPin` — hinges two bodies around a shared
    /// anchor; bodies rotate around the anchor but can't
    /// translate apart from it.
    @objc pub class SKPhysicsJointPin : SKPhysicsJoint {
        @objc("jointWithBodyA:bodyB:anchor:")
        pub static jointWithBodies(
            a: SKPhysicsBody, bodyB: SKPhysicsBody, anchor: NSPoint
        ): SKPhysicsJointPin

        @objc("setShouldEnableLimits:")
        pub setShouldEnableLimits(flag: bool)
        @objc("setLowerAngleLimit:")
        pub setLowerAngleLimit(v: f64)
        @objc("setUpperAngleLimit:")
        pub setUpperAngleLimit(v: f64)
        @objc("setFrictionTorque:")
        pub setFrictionTorque(v: f64)
        @objc("setRotationSpeed:")
        pub setRotationSpeed(v: f64)
    }

    /// `SKPhysicsJointSpring` — soft constraint between two
    /// anchor points; configurable frequency and damping.
    @objc pub class SKPhysicsJointSpring : SKPhysicsJoint {
        @objc("jointWithBodyA:bodyB:anchorA:anchorB:")
        pub static jointWithBodies(
            a: SKPhysicsBody, bodyB: SKPhysicsBody, anchorA: NSPoint, anchorB: NSPoint
        ): SKPhysicsJointSpring

        @objc("setFrequency:")
        pub setFrequency(v: f64)
        @objc("setDamping:")
        pub setDamping(v: f64)
    }

    /// `SKPhysicsJointFixed` — rigidly bonds two bodies at a
    /// common anchor — they move together as one rigid body.
    @objc pub class SKPhysicsJointFixed : SKPhysicsJoint {
        @objc("jointWithBodyA:bodyB:anchor:")
        pub static jointWithBodies(
            a: SKPhysicsBody, bodyB: SKPhysicsBody, anchor: NSPoint
        ): SKPhysicsJointFixed
    }

    /// `SKPhysicsJointLimit` — caps the distance between two
    /// anchor points to `maxLength`. Bodies can swing around
    /// the anchor but never get further apart.
    @objc pub class SKPhysicsJointLimit : SKPhysicsJoint {
        @objc("jointWithBodyA:bodyB:anchorA:anchorB:")
        pub static jointWithBodies(
            a: SKPhysicsBody, bodyB: SKPhysicsBody, anchorA: NSPoint, anchorB: NSPoint
        ): SKPhysicsJointLimit

        @objc("setMaxLength:")
        pub setMaxLength(v: f64)
        @objc("maxLength")
        pub maxLength(): f64
    }

    /// `SKPhysicsJointSliding` — slider along an axis;
    /// optionally clamps the distance with lower / upper limits.
    @objc pub class SKPhysicsJointSliding : SKPhysicsJoint {
        @objc("jointWithBodyA:bodyB:anchor:axis:")
        pub static jointWithBodies(
            a: SKPhysicsBody, bodyB: SKPhysicsBody, anchor: NSPoint, axis: NSVector
        ): SKPhysicsJointSliding

        @objc("setShouldEnableLimits:")
        pub setShouldEnableLimits(flag: bool)
        @objc("setLowerDistanceLimit:")
        pub setLowerDistanceLimit(v: f64)
        @objc("setUpperDistanceLimit:")
        pub setUpperDistanceLimit(v: f64)
    }

    /// `SKPhysicsContact` — read-only contact record passed to
    /// `SKPhysicsContactDelegate.didBeginContact:` /
    /// `…didEndContact:`. The fields are derived from the
    /// physics step; don't mutate.
    @objc pub class SKPhysicsContact : NSObject {
        @objc("bodyA")
        pub bodyA(): SKPhysicsBody
        @objc("bodyB")
        pub bodyB(): SKPhysicsBody
        @objc("contactPoint")
        pub contactPoint(): NSPoint
        @objc("contactNormal")
        pub contactNormal(): NSVector
        @objc("collisionImpulse")
        pub collisionImpulse(): f64
    }

    // ─── Constraints ─────────────────────────────────────────────

    /// `SKRange` — open-ended numeric interval. Used by
    /// `SKConstraint` to express the allowed band for a
    /// position / rotation / distance.
    @objc pub class SKRange : NSObject {
        @objc("rangeWithLowerLimit:upperLimit:")
        pub static rangeWithLowerUpper(lower: f64, upperLimit: f64): SKRange
        @objc("rangeWithConstantValue:")
        pub static rangeWithConstantValue(v: f64): SKRange
        @objc("rangeWithLowerLimit:")
        pub static rangeWithLowerLimit(v: f64): SKRange
        @objc("rangeWithUpperLimit:")
        pub static rangeWithUpperLimit(v: f64): SKRange
        @objc("rangeWithNoLimits")
        pub static rangeWithNoLimits(): SKRange

        @objc("setLowerLimit:")
        pub setLowerLimit(v: f64)
        @objc("setUpperLimit:")
        pub setUpperLimit(v: f64)
    }

    /// `SKConstraint` — applied to an `SKNode` to clamp its
    /// position / rotation / orientation / distance to a target.
    /// Multiple constraints run in declaration order.
    @objc pub class SKConstraint : NSObject {
        @objc("positionX:")
        pub static positionX(range: SKRange): SKConstraint
        @objc("positionY:")
        pub static positionY(range: SKRange): SKConstraint
        @objc("positionX:y:")
        pub static positionXY(rx: SKRange, y: SKRange): SKConstraint
        @objc("distance:toNode:")
        pub static distanceToNode(range: SKRange, toNode: SKNode): SKConstraint
        @objc("distance:toPoint:")
        pub static distanceToPoint(range: SKRange, toPoint: NSPoint): SKConstraint
        @objc("orientToNode:offset:")
        pub static orientToNode(node: SKNode, offset: SKRange): SKConstraint
        @objc("orientToPoint:offset:")
        pub static orientToPoint(point: NSPoint, offset: SKRange): SKConstraint
        @objc("zRotation:")
        pub static zRotation(range: SKRange): SKConstraint

        @objc("setEnabled:")
        pub setEnabled(flag: bool)
        @objc("enabled")
        pub enabled(): bool
    }

    // ─── Shader / attribute ──────────────────────────────────────

    /// `SKUniform` — named scalar / texture value handed to a
    /// fragment shader. Construct with the typed factory that
    /// matches the GLSL uniform's declared type.
    @objc pub class SKUniform : NSObject {
        @objc("uniformWithName:")
        pub static uniformWithName(name: NSString): SKUniform
        @objc("uniformWithName:float:")
        pub static uniformWithNameFloat(name: NSString, v: f32): SKUniform
        @objc("uniformWithName:texture:")
        pub static uniformWithNameTexture(name: NSString, texture: SKTexture): SKUniform

        @objc("setFloatValue:")
        pub setFloatValue(v: f32)
        @objc("floatValue")
        pub floatValue(): f32
        @objc("setTextureValue:")
        pub setTextureValue(t: SKTexture)
    }

    /// `SKAttribute` — per-node shader input declaration. The
    /// fragment shader reads it as `a_<name>`; runtime values are
    /// supplied per-node via `SKNode.setValueForAttributeNamed(...)`.
    @objc pub class SKAttribute : NSObject {
        @objc("attributeWithName:type:")
        pub static attributeWithName(name: NSString, type: AttributeType): SKAttribute

        @objc("name")
        pub name(): NSString
        @objc("type")
        pub attrType(): AttributeType
    }

    /// `SKShader` — custom fragment shader (GLSL ES). Attach via
    /// `SKSpriteNode.setShader:` / `SKEffectNode.setShader:`;
    /// per-shader globals are passed as `SKUniform`s,
    /// per-node values as `SKAttribute`s.
    @objc pub class SKShader : NSObject {
        @objc("shaderWithSource:")
        pub static shaderWithSource(source: NSString): SKShader
        @objc("shaderWithFileNamed:")
        pub static shaderWithFileNamed(name: NSString): SKShader

        @objc("setSource:")
        pub setSource(source: NSString)
        @objc("setUniforms:")
        pub setUniforms(arr: NSArray)
        @objc("uniforms")
        pub uniforms(): NSArray
        @objc("addUniform:")
        pub addUniform(u: SKUniform)
        @objc("uniformNamed:")
        pub uniformNamed(name: NSString): SKUniform
        @objc("setAttributes:")
        pub setAttributes(arr: NSArray)
        @objc("attributes")
        pub attributes(): NSArray
    }

    // ─── Scene transitions ───────────────────────────────────────

    /// `SKTransition` — animated swap between scenes. Pair with
    /// `SKView.presentScene:transition:` to fade / push / flip
    /// from the outgoing scene to the new one.
    @objc pub class SKTransition : NSObject {
        @objc("crossFadeWithDuration:")
        pub static crossFade(d: f64): SKTransition
        @objc("fadeWithDuration:")
        pub static fade(d: f64): SKTransition
        @objc("fadeWithColor:duration:")
        pub static fadeWithColor(c: NSColor, duration: f64): SKTransition
        @objc("revealWithDirection:duration:")
        pub static reveal(dir: TransitionDirection, duration: f64): SKTransition
        @objc("pushWithDirection:duration:")
        pub static push(dir: TransitionDirection, duration: f64): SKTransition
        @objc("moveInWithDirection:duration:")
        pub static moveIn(dir: TransitionDirection, duration: f64): SKTransition
        @objc("doorsOpenHorizontalWithDuration:")
        pub static doorsOpenHorizontal(d: f64): SKTransition
        @objc("doorsOpenVerticalWithDuration:")
        pub static doorsOpenVertical(d: f64): SKTransition
        @objc("doorsCloseHorizontalWithDuration:")
        pub static doorsCloseHorizontal(d: f64): SKTransition
        @objc("doorsCloseVerticalWithDuration:")
        pub static doorsCloseVertical(d: f64): SKTransition
        @objc("flipHorizontalWithDuration:")
        pub static flipHorizontal(d: f64): SKTransition
        @objc("flipVerticalWithDuration:")
        pub static flipVertical(d: f64): SKTransition

        @objc("setPausesIncomingScene:")
        pub setPausesIncomingScene(flag: bool)
        @objc("setPausesOutgoingScene:")
        pub setPausesOutgoingScene(flag: bool)
    }

    // ─── Shader / per-node attribute value ───────────────────────

    /// `SKAttributeValue` — per-node value handed to a custom
    /// shader, paired with the matching `SKAttribute` declared
    /// on the shader. Scalar `valueWithFloat:` covers the common
    /// 1-uniform case; the vector setters take SIMD literals
    /// (`[x, y]` etc.) at the call site.
    @objc pub class SKAttributeValue : NSObject {
        @objc("valueWithFloat:")
        pub static valueWithFloat(v: f32): SKAttributeValue
        @objc("valueWithVectorFloat2:")
        pub static valueWithVectorFloat2(v: simd.f32x2): SKAttributeValue
        /// `vector_float3` shares ABI with `vector_float4` —
        /// the 4th element is padding; supply 0.0 there.
        @objc("valueWithVectorFloat3:")
        pub static valueWithVectorFloat3(v: simd.f32x4): SKAttributeValue
        @objc("valueWithVectorFloat4:")
        pub static valueWithVectorFloat4(v: simd.f32x4): SKAttributeValue

        @objc("setFloatValue:")
        pub setFloatValue(v: f32)
        @objc("floatValue")
        pub floatValue(): f32
    }

    // ─── Tile auto-mapping rule ──────────────────────────────────

    /// `SKTileGroupRule` — one entry in an `SKTileGroup`'s
    /// `rules` array driving auto-tiling. Pair the adjacency
    /// bitmask (combine `cocoa.TileAdjacency*` constants) with
    /// the tile definitions to use when neighbouring cells
    /// match.
    @objc pub class SKTileGroupRule : NSObject {
        @objc("tileGroupRuleWithAdjacency:tileDefinitions:")
        pub static tileGroupRuleWithAdjacency(
            adjacency: u32, tileDefinitions: NSArray
        ): SKTileGroupRule

        @objc("setAdjacency:")
        pub setAdjacency(a: u32)
        @objc("adjacency")
        pub adjacency(): u32
        @objc("setTileDefinitions:")
        pub setTileDefinitions(defs: NSArray)
        @objc("tileDefinitions")
        pub tileDefinitions(): NSArray
        @objc("setName:")
        pub setRuleName(n: NSString)
        @objc("name")
        pub ruleName(): NSString
    }

    // ─── Field / emitter region ──────────────────────────────────

    /// `SKRegion` — 2-D area, attached to `SKFieldNode` /
    /// `SKEmitterNode` (and a few hit-test APIs) to clip the
    /// effect's influence to a shape. Build with the size /
    /// radius factories, then compose via the boolean helpers
    /// (`union` / `intersection` / `difference`).
    @objc pub class SKRegion : NSObject {
        @objc("alloc")
        pub static alloc(): SKRegion
        @objc("infiniteRegion")
        pub static infiniteRegion(): SKRegion
        @objc("initWithRadius:")
        pub initWithRadius(r: f32): SKRegion
        @objc("initWithSize:")
        pub initWithSize(s: NSSize): SKRegion

        @objc("inverseRegion")
        pub inverseRegion(): SKRegion
        @objc("regionByUnionWithRegion:")
        pub regionByUnion(other: SKRegion): SKRegion
        @objc("regionByDifferenceFromRegion:")
        pub regionByDifferenceFrom(other: SKRegion): SKRegion
        @objc("regionByIntersectionWithRegion:")
        pub regionByIntersection(other: SKRegion): SKRegion
        @objc("containsPoint:")
        pub containsPoint(p: NSPoint): bool
    }

    // ─── Inverse kinematics ──────────────────────────────────────

    /// `SKReachConstraints` — per-joint angle limits used by
    /// SpriteKit's IK solver
    /// (`SKAction.reachTo:rootNode:duration:`). Both angles are
    /// in radians; pass `±π` for "no limit".
    @objc pub class SKReachConstraints : NSObject {
        @objc("alloc")
        pub static alloc(): SKReachConstraints
        @objc("initWithLowerAngleLimit:upperAngleLimit:")
        pub init(lower: f64, upperAngleLimit: f64): SKReachConstraints

        @objc("setLowerAngleLimit:")
        pub setLowerAngleLimit(v: f64)
        @objc("setUpperAngleLimit:")
        pub setUpperAngleLimit(v: f64)
        @objc("lowerAngleLimit")
        pub lowerAngleLimit(): f64
        @objc("upperAngleLimit")
        pub upperAngleLimit(): f64
    }

    // ─── Warp geometry ───────────────────────────────────────────

    /// `SKWarpGeometry` — abstract base for the warp meshes
    /// applied to `SKSpriteNode` / `SKEffectNode` via
    /// `setWarpGeometry:`. Concrete shapes are subclasses
    /// (today: `SKWarpGeometryGrid`).
    ///
    /// Apple exposes the warp-enabled node set via the
    /// `SKWarpable` protocol — `SKSpriteNode` and `SKEffectNode`
    /// both adopt it. ilang code doesn't need a separate type
    /// for the protocol; just call `setWarpGeometry:` /
    /// `warpGeometry` directly on those nodes, and pair with
    /// `SKAction.warpTo` for animated transitions.
    @objc pub class SKWarpGeometry : NSObject {}

    /// `SKWarpGeometryGrid` — `columns × rows` quad mesh that
    /// remaps source positions (in normalised 0..1 texture
    /// coords) to destination positions (in normalised 0..1
    /// node coords). The position-array factories take
    /// `const vector_float2 *` (= `simd.f32x2[]`); pass row-
    /// major source / dest arrays of length `(cols+1)×(rows+1)`.
    /// `sourcePositionAtIndex:` / `destPositionAtIndex:` return
    /// `vector_float2`, not `CGPoint` — match Apple's header.
    @objc pub class SKWarpGeometryGrid : SKWarpGeometry {
        @objc("gridWithColumns:rows:")
        pub static gridWithColumns(columns: i64, rows: i64): SKWarpGeometryGrid

        @objc("gridWithColumns:rows:sourcePositions:destPositions:")
        pub static gridWithColumnsRowsSourceDest(
            columns: i64,
            rows: i64,
            source: simd.f32x2[],
            dest: simd.f32x2[]
        ): SKWarpGeometryGrid

        @objc("gridByReplacingSourcePositions:")
        pub gridByReplacingSourcePositions(source: simd.f32x2[]): SKWarpGeometryGrid
        @objc("gridByReplacingDestPositions:")
        pub gridByReplacingDestPositions(dest: simd.f32x2[]): SKWarpGeometryGrid

        @objc("numberOfColumns")
        pub numberOfColumns(): i64
        @objc("numberOfRows")
        pub numberOfRows(): i64
        @objc("vertexCount")
        pub vertexCount(): i64
        @objc("sourcePositionAtIndex:")
        pub sourcePositionAtIndex(i: i64): simd.f32x2
        @objc("destPositionAtIndex:")
        pub destPositionAtIndex(i: i64): simd.f32x2
    }

    // ─── Off-screen rendering ────────────────────────────────────

    /// `SKRenderer` — render an SKScene through a caller-owned
    /// Metal command buffer. Construct via `rendererWithDevice:`
    /// passing the result of `MTLCreateSystemDefaultDevice()`
    /// (the device handle flows through the `NSObject` slot —
    /// SpriteKit's signature treats it as opaque `id`), attach
    /// the scene with `setScene:`, advance simulation with
    /// `updateAtTime:`, then issue `renderWithViewport:...` per
    /// frame against a caller-prepared `MTLRenderPassDescriptor`.
    @objc pub class SKRenderer : NSObject {
        @objc("rendererWithDevice:")
        pub static rendererWithDevice(device: NSObject): SKRenderer

        @objc("setScene:")
        pub setScene(s: SKScene)
        @objc("scene")
        pub scene(): SKScene
        @objc("updateAtTime:")
        pub updateAtTime(t: f64)
        @objc("renderWithViewport:commandBuffer:renderPassDescriptor:")
        pub renderWithViewport(
            viewport: NSRect,
            commandBuffer: MTLCommandBuffer,
            renderPassDescriptor: MTLRenderPassDescriptor
        )
        @objc("setShowsFields:")
        pub setShowsFields(flag: bool)
        @objc("setShowsPhysics:")
        pub setShowsPhysics(flag: bool)
        @objc("setShowsQuadCount:")
        pub setShowsQuadCount(flag: bool)
        @objc("setShowsNodeCount:")
        pub setShowsNodeCount(flag: bool)
        @objc("setShowsDrawCount:")
        pub setShowsDrawCount(flag: bool)
    }
}
