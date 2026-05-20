// Pipeline-state descriptors and their compiled forms:
//   * Render / compute pipeline (PSO build inputs + states)
//   * Vertex layout (`MTLVertexDescriptor` + attribute / buffer-layout family)
//   * Sampler / depth-stencil descriptors + their compiled states
// All consumed by `MTLDevice.makeRenderPipelineState…` /
// `makeComputePipelineState…` and the encoders.

use foundation { NSObject, NSString, NSError, NSRange, withObjcError, objcRetain }
use types.*

@extern(ObjC, "/System/Library/Frameworks/Metal.framework/Metal") {
    /// `MTLRenderPipelineDescriptor` — describes the pipeline the
    /// caller wants compiled. At minimum supply `vertexFunction`,
    /// `fragmentFunction`, and the colour attachment 0 pixel
    /// format (must match the render-pass colour attachment's
    /// texture). `setVertexDescriptor:` is optional — leave it
    /// unset if the shader fetches vertex data through its own
    /// `[[buffer(...)]]` arguments instead of vertex attributes.
    @objc pub class MTLRenderPipelineDescriptor : NSObject {
        @objc("alloc")
        pub static alloc(): MTLRenderPipelineDescriptor
        @objc("init")
        pub init(): MTLRenderPipelineDescriptor

        @objc("setVertexFunction:")
        pub setVertexFunction(f: MTLFunction)
        @objc("setFragmentFunction:")
        pub setFragmentFunction(f: MTLFunction)
        @objc("setVertexDescriptor:")
        pub setVertexDescriptor(v: MTLVertexDescriptor)
        @objc("colorAttachments")
        pub colorAttachments(): MTLRenderPipelineColorAttachmentDescriptorArray

        /// Depth / stencil pixel formats for the pipeline. Must
        /// match the matching attachment's texture format on the
        /// active render pass, otherwise pipeline creation fails.
        /// Use `.invalid` (the default) to declare no
        /// depth / stencil buffer.
        @objc("setDepthAttachmentPixelFormat:")
        pub setDepthAttachmentPixelFormat(f: MTLPixelFormat)
        @objc("setStencilAttachmentPixelFormat:")
        pub setStencilAttachmentPixelFormat(f: MTLPixelFormat)
    }

    /// Subscriptable collection of colour-attachment slots on an
    /// `MTLRenderPipelineDescriptor`. Apple's ObjC API exposes it
    /// as a real class so the `desc.colorAttachments[0]` form
    /// (`[arr objectAtIndexedSubscript:0]`) can act as a property
    /// reference.
    @objc pub class MTLRenderPipelineColorAttachmentDescriptorArray : NSObject {
        @objc("objectAtIndexedSubscript:")
        pub objectAtIndexedSubscript(idx: u64): MTLRenderPipelineColorAttachmentDescriptor
    }

    /// One slot in `MTLRenderPipelineDescriptor.colorAttachments`.
    /// At minimum set `pixelFormat` to match the render-pass
    /// colour attachment's texture; blending setters are listed
    /// here for the common cases (enable + simple source-over
    /// alpha blending). Mask / write setters / clamp setters
    /// aren't bound yet — add per-need.
    @objc pub class MTLRenderPipelineColorAttachmentDescriptor : NSObject {
        @objc("setPixelFormat:")
        pub setPixelFormat(f: MTLPixelFormat)
        @objc("setBlendingEnabled:")
        pub setBlendingEnabled(flag: bool)
        @objc("setRgbBlendOperation:")
        pub setRgbBlendOperation(op: u64)
        @objc("setAlphaBlendOperation:")
        pub setAlphaBlendOperation(op: u64)
        @objc("setSourceRGBBlendFactor:")
        pub setSourceRGBBlendFactor(f: u64)
        @objc("setSourceAlphaBlendFactor:")
        pub setSourceAlphaBlendFactor(f: u64)
        @objc("setDestinationRGBBlendFactor:")
        pub setDestinationRGBBlendFactor(f: u64)
        @objc("setDestinationAlphaBlendFactor:")
        pub setDestinationAlphaBlendFactor(f: u64)
    }

    /// `id<MTLRenderPipelineState>` — compiled pipeline ready to
    /// be bound on a render encoder. Built via
    /// `device.newRenderPipelineStateWithDescriptor(desc, error)`;
    /// expensive to create (~ms), so cache across frames.
    @objc pub class MTLRenderPipelineState : NSObject {}

    /// `MTLVertexDescriptor` — declares how the vertex fetch unit
    /// reads attribute data out of bound `MTLBuffer`s. Each
    /// element of `attributes` describes one attribute (format
    /// + offset + which buffer slot it lives in); each element
    /// of `layouts` describes a buffer slot (stride + step
    /// function + step rate). Most simple samples can skip this
    /// and use `[[buffer(...)]]` indexing inside the shader.
    @objc pub class MTLVertexDescriptor : NSObject {
        @objc("vertexDescriptor")
        pub static vertexDescriptor(): MTLVertexDescriptor

        @objc("attributes")
        pub attributes(): MTLVertexAttributeDescriptorArray
        @objc("layouts")
        pub layouts(): MTLVertexBufferLayoutDescriptorArray
    }

    /// Subscriptable collection of `MTLVertexAttributeDescriptor`s.
    @objc pub class MTLVertexAttributeDescriptorArray : NSObject {
        @objc("objectAtIndexedSubscript:")
        pub objectAtIndexedSubscript(idx: u64): MTLVertexAttributeDescriptor
    }

    /// One attribute slot. `bufferIndex` selects which buffer the
    /// fetch unit reads from; `offset` is the byte offset into a
    /// single element of that buffer; `format` describes the
    /// width / channel count.
    @objc pub class MTLVertexAttributeDescriptor : NSObject {
        @objc("setFormat:")
        pub setFormat(f: MTLVertexFormat)
        @objc("setOffset:")
        pub setOffset(o: u64)
        @objc("setBufferIndex:")
        pub setBufferIndex(i: u64)
    }

    /// Subscriptable collection of `MTLVertexBufferLayoutDescriptor`s.
    @objc pub class MTLVertexBufferLayoutDescriptorArray : NSObject {
        @objc("objectAtIndexedSubscript:")
        pub objectAtIndexedSubscript(idx: u64): MTLVertexBufferLayoutDescriptor
    }

    /// One buffer-layout slot. `stride` is the byte distance
    /// between vertex elements; `stepFunction` is usually
    /// `.perVertex`; `stepRate` is 1 except for non-standard
    /// instancing.
    @objc pub class MTLVertexBufferLayoutDescriptor : NSObject {
        @objc("setStride:")
        pub setStride(s: u64)
        @objc("setStepFunction:")
        pub setStepFunction(f: MTLVertexStepFunction)
        @objc("setStepRate:")
        pub setStepRate(r: u64)
    }

    /// `id<MTLComputePipelineState>` — compiled compute kernel
    /// ready to bind on a compute encoder. Built via
    /// `device.newComputePipelineStateWithFunction(kernelFn, err)`
    /// from an `MTLFunction` whose source was declared
    /// `kernel ...` in the shader. Three read-only properties
    /// expose the limits the runtime computed for the kernel:
    /// `threadExecutionWidth` (preferred multiple for
    /// threadgroup width), `maxTotalThreadsPerThreadgroup`
    /// (upper bound), and `staticThreadgroupMemoryLength`.
    @objc pub class MTLComputePipelineState : NSObject {
        @objc("threadExecutionWidth")
        pub threadExecutionWidth(): u64
        @objc("maxTotalThreadsPerThreadgroup")
        pub maxTotalThreadsPerThreadgroup(): u64
        @objc("staticThreadgroupMemoryLength")
        pub staticThreadgroupMemoryLength(): u64
    }

    /// `MTLSamplerDescriptor` — describes how a texture is
    /// filtered / addressed when sampled in the shader. Build via
    /// `MTLSamplerDescriptor.alloc().init()` (no class-level
    /// factory exists), tune the filter / address / clamp / lod
    /// values, then bake into a state with
    /// `device.newSamplerStateWithDescriptor(desc)`. The descriptor
    /// is mutable and one-shot — the resulting `MTLSamplerState`
    /// captures the values; you can free the descriptor afterwards.
    @objc pub class MTLSamplerDescriptor : NSObject {
        @objc("alloc")
        pub static alloc(): MTLSamplerDescriptor
        @objc("init")
        pub init(): MTLSamplerDescriptor

        @objc("setMinFilter:")
        pub setMinFilter(f: MTLSamplerMinMagFilter)
        @objc("setMagFilter:")
        pub setMagFilter(f: MTLSamplerMinMagFilter)
        @objc("setMipFilter:")
        pub setMipFilter(f: MTLSamplerMipFilter)
        @objc("setSAddressMode:")
        pub setSAddressMode(m: MTLSamplerAddressMode)
        @objc("setTAddressMode:")
        pub setTAddressMode(m: MTLSamplerAddressMode)
        @objc("setRAddressMode:")
        pub setRAddressMode(m: MTLSamplerAddressMode)
        @objc("setBorderColor:")
        pub setBorderColor(c: MTLSamplerBorderColor)
        @objc("setNormalizedCoordinates:")
        pub setNormalizedCoordinates(flag: bool)
        @objc("setMaxAnisotropy:")
        pub setMaxAnisotropy(n: u64)
        @objc("setLodMinClamp:")
        pub setLodMinClamp(v: f32)
        @objc("setLodMaxClamp:")
        pub setLodMaxClamp(v: f32)
        @objc("setCompareFunction:")
        pub setCompareFunction(f: MTLCompareFunction)
        @objc("setLabel:")
        pub setLabel(s: NSString)
    }

    /// `id<MTLSamplerState>` — compiled sampler ready to bind on
    /// an encoder via the various `setVertex/FragmentSamplerState:`
    /// / `setSamplerState:` selectors. Cheap to keep across
    /// frames; build once per material.
    @objc pub class MTLSamplerState : NSObject {}

    /// `MTLStencilDescriptor` — one face's stencil-test
    /// configuration. Embedded inside an `MTLDepthStencilDescriptor`
    /// twice (front face / back face); pass the same descriptor
    /// to both setters when both faces should use identical
    /// rules. `setStencilCompareFunction:` selects the predicate;
    /// the three operation setters pick what to write to the
    /// stencil buffer on stencil-fail / depth-fail / pass.
    @objc pub class MTLStencilDescriptor : NSObject {
        @objc("alloc")
        pub static alloc(): MTLStencilDescriptor
        @objc("init")
        pub init(): MTLStencilDescriptor

        @objc("setStencilCompareFunction:")
        pub setStencilCompareFunction(f: MTLCompareFunction)
        @objc("setStencilFailureOperation:")
        pub setStencilFailureOperation(op: MTLStencilOperation)
        @objc("setDepthFailureOperation:")
        pub setDepthFailureOperation(op: MTLStencilOperation)
        @objc("setDepthStencilPassOperation:")
        pub setDepthStencilPassOperation(op: MTLStencilOperation)
        @objc("setReadMask:")
        pub setReadMask(m: u32)
        @objc("setWriteMask:")
        pub setWriteMask(m: u32)
    }

    /// `MTLDepthStencilDescriptor` — combined depth + stencil
    /// configuration. Build via
    /// `MTLDepthStencilDescriptor.alloc().init()`, then bake with
    /// `device.newDepthStencilStateWithDescriptor(desc)`. The
    /// depth side has just a compare function (`.always` disables
    /// the test) + write flag; the stencil side wires two
    /// `MTLStencilDescriptor`s in.
    @objc pub class MTLDepthStencilDescriptor : NSObject {
        @objc("alloc")
        pub static alloc(): MTLDepthStencilDescriptor
        @objc("init")
        pub init(): MTLDepthStencilDescriptor

        @objc("setDepthCompareFunction:")
        pub setDepthCompareFunction(f: MTLCompareFunction)
        @objc("setDepthWriteEnabled:")
        pub setDepthWriteEnabled(flag: bool)
        @objc("setFrontFaceStencil:")
        pub setFrontFaceStencil(s: MTLStencilDescriptor)
        @objc("setBackFaceStencil:")
        pub setBackFaceStencil(s: MTLStencilDescriptor)
        @objc("setLabel:")
        pub setLabel(s: NSString)
    }

    /// `id<MTLDepthStencilState>` — compiled depth/stencil state
    /// bound onto a render encoder via `setDepthStencilState:`.
    /// Build once per render mode and reuse.
    @objc pub class MTLDepthStencilState : NSObject {}
}
