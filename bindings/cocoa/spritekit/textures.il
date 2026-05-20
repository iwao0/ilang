// SKTexture / SKMutableTexture / SKTextureAtlas — GPU image
// surfaces used by sprite / shape / particle nodes.

use foundation { NSObject, NSString, NSArray, NSData, NSSize,
                 NSRect, NSURL, nsstr }
use appkit { NSColor }

/// SKTextureFilteringMode — sampling mode when a texture is
/// drawn at a non-1:1 scale. `nearest` for pixel-art look.
pub enum SKTextureFilteringMode: i64 {
    nearest = 0
    linear  = 1
}

@extern(ObjC, "/System/Library/Frameworks/SpriteKit.framework/SpriteKit") {
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
        pub setFilteringMode(m: SKTextureFilteringMode)
        @objc("filteringMode")
        pub filteringMode(): SKTextureFilteringMode

        /// Normal-map generation. The first form computes a
        /// default-tuned normal map; the second lets you dial
        /// `smoothness` (0..1) and `contrast` (≥ 1).
        @objc("textureByGeneratingNormalMap")
        pub textureByGeneratingNormalMap(): SKTexture
        @objc("textureByGeneratingNormalMapWithSmoothness:contrast:")
        pub textureByGeneratingNormalMapWithParams(smoothness: f64, contrast: f64): SKTexture

        /// Async preload — the block is invoked on the main
        /// queue once the texture's pixels are uploaded. Wrap an
        /// ilang `fn()` with `new ObjCBlock(...)` at the call
        /// site; the type checker requires the matching signature.
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

}
