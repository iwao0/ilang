// Effect / particle / camera / light / audio nodes — scene-
// graph nodes that aren't sprites but still hang under SKScene.

use foundation { NSObject, NSString, NSArray, NSData, NSPoint,
                 NSSize, NSURL, NSVector }
use appkit { NSColor }
use node { SKNode }
use sprites { SKBlendMode }
use textures { SKTexture }

/// SKInterpolationMode — how an `SKKeyframeSequence` blends
/// between successive keyframes.
pub enum SKInterpolationMode: i64 {
    linear = 1
    spline = 2
    step   = 3
}

/// SKRepeatMode — what happens past the last keyframe.
pub enum SKRepeatMode: i64 {
    clamp = 1
    loop  = 2
}

@extern(ObjC, "/System/Library/Frameworks/SpriteKit.framework/SpriteKit") {
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
        pub setBlendMode(m: SKBlendMode)
        @objc("setShader:")
        pub setShader(s: SKShader)
        @objc("shader")
        pub shader(): SKShader

        /// Warp geometry — same shape as `SKSpriteNode`'s; the
        /// EffectNode warps its rasterised subtree as a whole.
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
        pub setParticleBlendMode(m: SKBlendMode)
        @objc("resetSimulation")
        pub resetSimulation()

        /// Rendering target — particles are emitted into
        /// `targetNode`'s coordinate space if set. Useful when
        /// the emitter is mounted on a moving parent but the
        /// particles should stay in world space.
        @objc("setTargetNode:")
        pub setTargetNode(n: SKNode)
        @objc("targetNode")
        pub targetNode(): SKNode

        /// Constant per-frame acceleration applied to each
        /// particle (world units / s²). Combine with the
        /// particle's initial `Speed` / `EmissionAngle` for
        /// gravity-like fall-off, smoke drift, etc.
        @objc("setXAcceleration:")
        pub setXAcceleration(a: f64)
        @objc("setYAcceleration:")
        pub setYAcceleration(a: f64)

        /// Spread the spawn point over a rectangular range
        /// (`dx`, `dy` half-widths). Default `(0, 0)` emits
        /// every particle from the node's exact position.
        @objc("setParticlePositionRange:")
        pub setParticlePositionRange(v: NSVector)

        /// Each emitted particle runs `action` for its lifetime
        /// — handy for per-particle rotation / scale tweens
        /// beyond the built-in `setParticleScaleSpeed` etc.
        @objc("setParticleAction:")
        pub setParticleAction(a: SKAction)
        @objc("particleAction")
        pub particleAction(): SKAction

        /// Keyframe-sequenced animation of per-particle colour
        /// / colour-blend-factor. The sequence is sampled
        /// against each particle's normalised lifetime
        /// (0.0 = birth, 1.0 = death).
        @objc("setParticleColorSequence:")
        pub setParticleColorSequence(s: SKKeyframeSequence)
        @objc("setParticleColorBlendFactor:")
        pub setParticleColorBlendFactor(f: f64)
        @objc("setParticleColorBlendFactorSequence:")
        pub setParticleColorBlendFactorSequence(s: SKKeyframeSequence)

        /// Per-channel drift speeds — combine with the
        /// particle's starting `particleColor` for fade /
        /// tint-shift effects.
        @objc("setParticleColorRedSpeed:")
        pub setParticleColorRedSpeed(s: f64)
        @objc("setParticleColorGreenSpeed:")
        pub setParticleColorGreenSpeed(s: f64)
        @objc("setParticleColorBlueSpeed:")
        pub setParticleColorBlueSpeed(s: f64)
        @objc("setParticleColorAlphaSpeed:")
        pub setParticleColorAlphaSpeed(s: f64)

        /// Per-channel jitter ranges (added randomly at spawn).
        @objc("setParticleColorRedRange:")
        pub setParticleColorRedRange(r: f64)
        @objc("setParticleColorGreenRange:")
        pub setParticleColorGreenRange(r: f64)
        @objc("setParticleColorBlueRange:")
        pub setParticleColorBlueRange(r: f64)
        @objc("setParticleColorAlphaRange:")
        pub setParticleColorAlphaRange(r: f64)

        /// Particle scale rate of change (units / s) — combine
        /// with `setParticleScale` / `setParticleScaleRange`.
        @objc("setParticleScaleSpeed:")
        pub setParticleScaleSpeed(s: f64)

        /// Z-rotation animation parameters for the particle's
        /// own coordinate frame (radians + radians/s).
        @objc("setParticleRotation:")
        pub setParticleRotation(r: f64)
        @objc("setParticleRotationRange:")
        pub setParticleRotationRange(r: f64)
        @objc("setParticleRotationSpeed:")
        pub setParticleRotationSpeed(r: f64)

        /// Custom shader applied to each particle quad.
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
        pub setInterpolationMode(m: SKInterpolationMode)
        @objc("setRepeatMode:")
        pub setRepeatMode(m: SKRepeatMode)
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

}
