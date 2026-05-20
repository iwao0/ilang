/// SKRenderer — render an SKScene into a Metal command-buffer
/// for off-screen / video / advanced compositor use. Lets
/// non-SKView hosts (games using a Metal renderer of their own)
/// drop a SpriteKit scene into a render pass.

use foundation { NSObject, NSRect, NSSize }
use node { SKScene }
use metal { MTLCommandBuffer, MTLRenderPassDescriptor }

@extern(ObjC, "/System/Library/Frameworks/SpriteKit.framework/SpriteKit") {
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
