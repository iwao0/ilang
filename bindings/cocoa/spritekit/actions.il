/// SKAction + SKTransition — animation framework. SKActions are
/// recipes for changing node properties over time;
/// `node.runAction(...)` enqueues them. SKTransition is the
/// scene-to-scene crossfade / push / reveal.

use foundation { NSObject, NSString, NSArray, NSPoint, NSSize,
                 NSVector, nsstr }
use appkit { NSColor }
use textures { SKTexture }

/// SKActionTimingMode — easing curve applied to an SKAction's
/// progress. `linear` is the default.
pub enum SKActionTimingMode: i64 {
    linear        = 0
    easeIn        = 1
    easeOut       = 2
    easeInEaseOut = 3
}

/// SKTransitionDirection — direction for `SKTransition.push…` /
/// `reveal…` / `moveIn…` factories.
pub enum SKTransitionDirection: i64 {
    up    = 0
    down  = 1
    right = 2
    left  = 3
}

@extern(ObjC, "/System/Library/Frameworks/SpriteKit.framework/SpriteKit") {
    @objc pub class SKAction : NSObject {
        /// Movement — relative
        @objc("moveByX:y:duration:")
        pub static moveByXY(x: f64, y: f64, duration: f64): SKAction
        @objc("moveBy:duration:")
        pub static moveBy(delta: NSVector, duration: f64): SKAction
        /// Movement — absolute
        @objc("moveTo:duration:")
        pub static moveTo(p: NSPoint, duration: f64): SKAction
        @objc("moveToX:duration:")
        pub static moveToX(x: f64, duration: f64): SKAction
        @objc("moveToY:duration:")
        pub static moveToY(y: f64, duration: f64): SKAction

        /// Rotation
        @objc("rotateByAngle:duration:")
        pub static rotateByAngle(radians: f64, duration: f64): SKAction
        @objc("rotateToAngle:duration:")
        pub static rotateToAngle(radians: f64, duration: f64): SKAction

        /// Scale
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

        /// Fade
        @objc("fadeAlphaBy:duration:")
        pub static fadeAlphaBy(alpha: f64, duration: f64): SKAction
        @objc("fadeAlphaTo:duration:")
        pub static fadeAlphaTo(alpha: f64, duration: f64): SKAction
        @objc("fadeInWithDuration:")
        pub static fadeIn(duration: f64): SKAction
        @objc("fadeOutWithDuration:")
        pub static fadeOut(duration: f64): SKAction

        /// Texture animation (for SKSpriteNode)
        @objc("setTexture:")
        pub static setTexture(t: SKTexture): SKAction
        @objc("animateWithTextures:timePerFrame:")
        pub static animateWithTextures(textures: NSArray, timePerFrame: f64): SKAction

        /// Tree manipulation
        @objc("removeFromParent")
        pub static removeFromParent(): SKAction
        @objc("waitForDuration:")
        pub static waitForDuration(d: f64): SKAction
        @objc("waitForDuration:withRange:")
        pub static waitForDurationRange(d: f64, withRange: f64): SKAction

        /// Sprite colourise (only meaningful on SKSpriteNode receivers)
        @objc("colorizeWithColor:colorBlendFactor:duration:")
        pub static colorizeWithColor(c: NSColor, colorBlendFactor: f64, duration: f64): SKAction
        @objc("colorizeWithColorBlendFactor:duration:")
        pub static colorizeWithColorBlendFactor(blend: f64, duration: f64): SKAction

        /// Audio — file is looked up in the main bundle
        @objc("playSoundFileNamed:waitForCompletion:")
        pub static playSoundFileNamed(name: NSString, waitForCompletion: bool): SKAction

        /// Dispatch to a named descendant — the action runs on the
        /// first node found by `childNodeWithName:` at the target.
        @objc("runAction:onChildWithName:")
        pub static runActionOnChild(action: SKAction, onChildWithName: NSString): SKAction

        /// ObjC selector — `[target performSelector]` fires
        /// synchronously when the action runs. Build the SEL via
        /// `cocoa.selectorPtr("name")`.
        @objc("performSelector:onTarget:")
        pub static performSelectorOnTarget(sel: i64, onTarget: NSObject): SKAction

        /// Block — fires once. ObjC signature is
        /// `+ (SKAction *)runBlock:(dispatch_block_t)block` which
        /// is a `void(^)(void)`; build the argument with
        /// `new ObjCBlock(fn() { ... })`.
        @objc("runBlock:")
        pub static runBlock(block: ObjCBlock<fn()>): SKAction

        /// Instance — reverse / timing
        @objc("reversedAction")
        pub reversedAction(): SKAction
        @objc("setTimingMode:")
        pub setTimingMode(m: SKActionTimingMode)
        @objc("setSpeed:")
        pub setActionSpeed(s: f64)
        @objc("setDuration:")
        pub setDuration(d: f64)

        /// Composition
        @objc("sequence:")
        pub static sequence(actions: NSArray): SKAction
        @objc("group:")
        pub static group(actions: NSArray): SKAction
        @objc("repeatAction:count:")
        pub static repeatAction(action: SKAction, count: i64): SKAction
        @objc("repeatActionForever:")
        pub static repeatActionForever(action: SKAction): SKAction

        /// Warp geometry — animate a sprite's per-frame vertex
        /// remap from its current `SKWarpGeometry` to `warp` over
        /// `duration` seconds. `numberOfColumns` / `numberOfRows`
        /// must match for a smooth interpolation; mismatched
        /// dimensions snap to the new mesh instantly.
        @objc("warpTo:duration:")
        pub static warpTo(warp: SKWarpGeometry, duration: f64): SKAction
    }

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
        pub static reveal(dir: SKTransitionDirection, duration: f64): SKTransition
        @objc("pushWithDirection:duration:")
        pub static push(dir: SKTransitionDirection, duration: f64): SKTransition
        @objc("moveInWithDirection:duration:")
        pub static moveIn(dir: SKTransitionDirection, duration: f64): SKTransition
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

}
