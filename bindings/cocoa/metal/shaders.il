// MTLLibrary + MTLFunction — shader source / compiled metallib
// containers. Pipelines pull vertex / fragment / compute
// functions out of these.

use foundation { NSObject, NSString, NSError, NSRange, withObjcError, objcRetain }
use types.*

@extern(ObjC, "/System/Library/Frameworks/Metal.framework/Metal") {
    /// `id<MTLLibrary>` — compiled shader collection. Build via
    /// `device.newLibraryWithSource(src, options, error)` from
    /// Metal Shading Language source, or load the bundle's
    /// default library through `device.newDefaultLibrary()`.
    /// Look up named entry points with `newFunctionWithName`.
    @objc pub class MTLLibrary : NSObject {
        @objc("newFunctionWithName:")
        pub newFunctionWithName(name: NSString): MTLFunction
    }

    /// `id<MTLFunction>` — one compiled shader entry point.
    /// Plug into `MTLRenderPipelineDescriptor.setVertexFunction:`
    /// / `setFragmentFunction:` to wire it into a render
    /// pipeline. Functions are reusable across pipelines.
    @objc pub class MTLFunction : NSObject {
        @objc("name")
        pub functionName(): NSString
        /// Returns an argument encoder whose layout matches the
        /// function's `[[buffer(bufferIndex)]]` argument (which
        /// must be a struct of textures / samplers / buffers — the
        /// Metal argument-buffer feature). Write into an
        /// `MTLBuffer` of `encoder.encodedLength()` bytes using
        /// the encoder's `setBuffer/Texture/SamplerState:atIndex:`
        /// selectors.
        @objc("newArgumentEncoderWithBufferIndex:")
        pub newArgumentEncoderWithBufferIndex(idx: u64): MTLArgumentEncoder
    }
}
