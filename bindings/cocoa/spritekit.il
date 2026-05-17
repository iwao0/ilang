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

use foundation { NSObject, NSString, NSArray, NSData, NSPoint, NSSize, NSRect, NSURL, nsstr }
use appkit { NSView, NSColor }

// CGVector — `(dx, dy)` pair Foundation uses for velocities,
// gravity, impulses, etc. Same layout as `NSSize` but the field
// names differ; using a dedicated type keeps call sites readable.
pub struct NSVector { dx: f64; dy: f64 }

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

        // Transform
        @objc("setPosition:")
        pub setPosition(p: NSPoint)
        @objc("position")
        pub position(): NSPoint
        @objc("setZPosition:")
        pub setZPosition(z: f64)
        @objc("zPosition")
        pub zPosition(): f64
        @objc("setZRotation:")
        pub setZRotation(r: f64)
        @objc("zRotation")
        pub zRotation(): f64
        @objc("setXScale:")
        pub setXScale(s: f64)
        @objc("setYScale:")
        pub setYScale(s: f64)
        @objc("xScale")
        pub xScale(): f64
        @objc("yScale")
        pub yScale(): f64
        @objc("setAlpha:")
        pub setAlpha(a: f64)
        @objc("setHidden:")
        pub setHidden(flag: bool)

        // Identity / state
        @objc("setName:")
        pub setName(n: NSString)
        @objc("name")
        pub name(): NSString
        @objc("setPaused:")
        pub setPaused(flag: bool)
        @objc("isPaused")
        pub isPaused(): bool
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

        // Block — fires once. ObjC signature is
        // `+ (SKAction *)runBlock:(dispatch_block_t)block` which
        // is a `void(^)(void)`; we wire it through
        // `cocoa.makeBlock`.
        @objc("runBlock:")
        pub static runBlock(block: i64): SKAction

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

        @objc("setDynamic:")
        pub setDynamic(flag: bool)
        @objc("isDynamic")
        pub isDynamic(): bool
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
    /// delegate object (an ObjC class implementing
    /// `SKPhysicsContactDelegate`).
    @objc pub class SKPhysicsWorld : NSObject {
        @objc("setGravity:")
        pub setGravity(v: NSVector)
        @objc("gravity")
        pub gravity(): NSVector
        @objc("setSpeed:")
        pub setSpeed(s: f64)
        @objc("setContactDelegate:")
        pub setContactDelegate(d: NSObject)
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

    /// Build an `NSVector` (= `CGVector` on macOS). Used for
    /// gravity / velocity / impulse / force.
    pub fn makeVector(dx: f64, dy: f64): NSVector {
        let v = new NSVector()
        v.dx = dx
        v.dy = dy
        v
    }
}
