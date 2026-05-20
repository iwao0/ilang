// Sprite / shape / label / video / reference nodes — the
// leaf renderable types that live under SKScene.

use foundation { NSObject, NSString, NSArray, NSPoint, NSSize,
                 NSRect, NSURL, nsstr }
use appkit { NSColor }
use node { SKNode }
use textures { SKTexture }

/// SKBlendMode — composition mode for SKSpriteNode / SKShapeNode
/// / SKEmitterNode / SKEffectNode. `alpha` is the default
/// (standard "over" blending); `replace` skips blending entirely.
pub enum SKBlendMode: i64 {
    alpha      = 0
    add        = 1
    subtract   = 2
    multiply   = 3
    multiplyX2 = 4
    screen     = 5
    replace    = 6
}

/// SKLabelHorizontalAlignmentMode — where the label's anchor
/// sits horizontally relative to the rendered glyphs.
pub enum SKLabelHorizontalAlignmentMode: i64 {
    center = 0
    left   = 1
    right  = 2
}

/// SKLabelVerticalAlignmentMode — vertical anchor mode.
pub enum SKLabelVerticalAlignmentMode: i64 {
    baseline = 0
    center   = 1
    top      = 2
    bottom   = 3
}

@extern(ObjC, "/System/Library/Frameworks/SpriteKit.framework/SpriteKit") {
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
        pub setBlendMode(m: SKBlendMode)
        @objc("setShader:")
        pub setShader(s: SKShader)
        @objc("shader")
        pub shader(): SKShader

        /// Warp geometry (SKWarpable) — drives a per-frame
        /// vertex remap of the sprite quad. Pair with
        /// `SKAction.warpTo` to animate the transition.
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
        pub setBlendMode(m: SKBlendMode)
    }

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
        pub setHorizontalAlignmentMode(m: SKLabelHorizontalAlignmentMode)
        @objc("setVerticalAlignmentMode:")
        pub setVerticalAlignmentMode(m: SKLabelVerticalAlignmentMode)
        @objc("setNumberOfLines:")
        pub setNumberOfLines(n: i64)
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

}
