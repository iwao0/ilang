// MTLRenderCommandEncoder / MTLComputeCommandEncoder /
// MTLBlitCommandEncoder — the per-pass workload recorders, plus
// `MTLArgumentEncoder` for filling argument buffers. Obtained
// from a command buffer; closed via `endEncoding()`.

use foundation { NSObject, NSString, NSError, NSRange, withObjcError, objcRetain }
use types.*

@extern(ObjC, "/System/Library/Frameworks/Metal.framework/Metal") {
    /// `id<MTLRenderCommandEncoder>` — records draw calls into the
    /// parent `MTLCommandBuffer`. Bind a pipeline state, set
    /// buffers / textures / sampler states, issue
    /// `drawPrimitives` (or `drawIndexedPrimitives`), then call
    /// `endEncoding` before committing the command buffer.
    @objc pub class MTLRenderCommandEncoder : NSObject {
        @objc("setRenderPipelineState:")
        pub setRenderPipelineState(s: MTLRenderPipelineState)

        /// Rasteriser state. `setViewport:` sets the visible
        /// rectangle (and depth-clip range) the fragments are
        /// written into; `setScissorRect:` discards fragments
        /// outside the supplied rectangle. Cull / winding / fill
        /// configure the triangle pipeline:
        ///   * cull mode    — `.back` is the casual default
        ///   * winding      — `.counterClockwise` (front-facing) is
        ///                    the Metal default
        ///   * fill mode    — `.fill` is solid; `.lines` is wireframe
        /// `setRenderTargetArrayLength:` toggles between rendering
        /// into a single layer (0) and multi-layered rendering
        /// (matching the texture's `arrayLength` or `depth`).
        /// `setBlendColorRed:green:blue:alpha:` feeds the constant
        /// blend factor used by `.sourceConstantColor` /
        /// `.destinationConstantColor` blend factors.
        @objc("setViewport:")
        pub setViewport(v: MTLViewport)
        @objc("setScissorRect:")
        pub setScissorRect(r: MTLScissorRect)
        @objc("setCullMode:")
        pub setCullMode(m: MTLCullMode)
        @objc("setFrontFacingWinding:")
        pub setFrontFacingWinding(w: MTLWinding)
        @objc("setTriangleFillMode:")
        pub setTriangleFillMode(m: MTLTriangleFillMode)
        @objc("setRenderTargetArrayLength:")
        pub setRenderTargetArrayLength(n: u64)
        @objc("setBlendColorRed:green:blue:alpha:")
        pub setBlendColor(r: f32, green: f32, blue: f32, alpha: f32)

        /// Intra-queue fence ordering. `updateFence:afterStages:`
        /// signals the fence after the named pipeline stages
        /// complete; `waitForFence:beforeStages:` blocks the named
        /// stages from starting until the fence is signalled. The
        /// stage bitmask is `MTLRenderStages` — typically
        /// `.vertex | .fragment` for a full-pipeline barrier.
        @objc("updateFence:afterStages:")
        pub updateFence(fence: MTLFence, afterStages: MTLRenderStages)
        @objc("waitForFence:beforeStages:")
        pub waitForFence(fence: MTLFence, beforeStages: MTLRenderStages)

        /// Argument-buffer residency. Hazard tracking can't see
        /// resources reached through an argument buffer, so the
        /// caller must declare them explicitly. `useResource:usage:`
        /// marks one buffer/texture as in-use for the upcoming
        /// draws; `useResources:count:usage:` does N at once;
        /// `useHeap:` covers every resource in a heap.
        @objc("useResource:usage:")
        pub useResource(resource: NSObject, usage: MTLResourceUsage)
        @objc("useResources:count:usage:")
        pub useResources(resources: i64, count: u64, usage: MTLResourceUsage)
        @objc("useHeap:")
        pub useHeap(heap: MTLHeap)

        /// Debug labels — visible in Xcode's GPU frame capture. The
        /// push/pop pair brackets a span of encoder calls; `setLabel`
        /// names the encoder itself.
        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("pushDebugGroup:")
        pub pushDebugGroup(s: NSString)
        @objc("popDebugGroup")
        pub popDebugGroup()

        @objc("setVertexBuffer:offset:atIndex:")
        pub setVertexBuffer(b: MTLBuffer, offset: u64, atIndex: u64)
        @objc("setVertexBytes:length:atIndex:")
        pub setVertexBytes(bytes: i64, length: u64, atIndex: u64)
        @objc("setVertexTexture:atIndex:")
        pub setVertexTexture(t: MTLTexture, atIndex: u64)

        @objc("setFragmentBuffer:offset:atIndex:")
        pub setFragmentBuffer(b: MTLBuffer, offset: u64, atIndex: u64)
        @objc("setFragmentBytes:length:atIndex:")
        pub setFragmentBytes(bytes: i64, length: u64, atIndex: u64)
        @objc("setFragmentTexture:atIndex:")
        pub setFragmentTexture(t: MTLTexture, atIndex: u64)

        /// Sampler-state bindings — one per shader stage, indexed by
        /// the `[[sampler(n)]]` slot in the shader.
        @objc("setVertexSamplerState:atIndex:")
        pub setVertexSamplerState(s: MTLSamplerState, atIndex: u64)
        @objc("setFragmentSamplerState:atIndex:")
        pub setFragmentSamplerState(s: MTLSamplerState, atIndex: u64)

        /// Depth / stencil pipeline. `setDepthStencilState:` binds
        /// a compiled depth-stencil state; `setStencilReferenceValue:`
        /// sets the reference compared against the buffer; the
        /// depth-bias triple feeds the rasteriser's polygon-offset.
        @objc("setDepthStencilState:")
        pub setDepthStencilState(s: MTLDepthStencilState)
        @objc("setStencilReferenceValue:")
        pub setStencilReferenceValue(ref: u32)
        @objc("setDepthBias:slopeScale:clamp:")
        pub setDepthBias(depthBias: f32, slopeScale: f32, clamp: f32)

        @objc("drawPrimitives:vertexStart:vertexCount:")
        pub drawPrimitives(
            primitiveType: MTLPrimitiveType, vertexStart: u64, vertexCount: u64
        )
        @objc("drawPrimitives:vertexStart:vertexCount:instanceCount:")
        pub drawPrimitivesInstanced(
            primitiveType: MTLPrimitiveType,
            vertexStart: u64,
            vertexCount: u64,
            instanceCount: u64
        )
        @objc("drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferOffset:")
        pub drawIndexedPrimitives(
            primitiveType: MTLPrimitiveType,
            indexCount: u64,
            indexType: MTLIndexType,
            indexBuffer: MTLBuffer,
            indexBufferOffset: u64
        )

        @objc("endEncoding")
        pub endEncoding()
    }

    @objc pub class MTLComputeCommandEncoder : NSObject {
        @objc("setComputePipelineState:")
        pub setComputePipelineState(s: MTLComputePipelineState)

        @objc("setBuffer:offset:atIndex:")
        pub setBuffer(b: MTLBuffer, offset: u64, atIndex: u64)
        @objc("setBytes:length:atIndex:")
        pub setBytes(bytes: i64, length: u64, atIndex: u64)
        @objc("setTexture:atIndex:")
        pub setTexture(t: MTLTexture, atIndex: u64)
        @objc("setSamplerState:atIndex:")
        pub setSamplerState(s: MTLSamplerState, atIndex: u64)

        /// Reserve threadgroup-local memory for a specific binding
        /// slot (`threadgroup` qualified storage in the kernel).
        /// `length` is in bytes; the kernel must declare a matching
        /// `[[threadgroup(n)]]` parameter at `atIndex`.
        @objc("setThreadgroupMemoryLength:atIndex:")
        pub setThreadgroupMemoryLength(length: u64, atIndex: u64)

        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("pushDebugGroup:")
        pub pushDebugGroup(s: NSString)
        @objc("popDebugGroup")
        pub popDebugGroup()

        @objc("dispatchThreads:threadsPerThreadgroup:")
        pub dispatchThreads(
            threads: MTLSize, threadsPerThreadgroup: MTLSize
        )
        @objc("dispatchThreadgroups:threadsPerThreadgroup:")
        pub dispatchThreadgroups(
            threadgroups: MTLSize, threadsPerThreadgroup: MTLSize
        )

        /// Intra-queue fences. Compute encoders don't have a
        /// stage mask — the whole dispatch is the "stage".
        @objc("updateFence:")
        pub updateFence(fence: MTLFence)
        @objc("waitForFence:")
        pub waitForFence(fence: MTLFence)

        /// Argument-buffer residency declarations — see the
        /// matching block on `MTLRenderCommandEncoder` for why
        /// these are necessary.
        @objc("useResource:usage:")
        pub useResource(resource: NSObject, usage: MTLResourceUsage)
        @objc("useResources:count:usage:")
        pub useResources(resources: i64, count: u64, usage: MTLResourceUsage)
        @objc("useHeap:")
        pub useHeap(heap: MTLHeap)

        @objc("endEncoding")
        pub endEncoding()
    }

    /// `id<MTLBlitCommandEncoder>` — records memory-transfer work
    /// (buffer ↔ buffer, buffer ↔ texture, texture ↔ texture) and
    /// resource maintenance (mipmap generation, managed-storage
    /// CPU/GPU synchronisation) into the parent command buffer.
    /// Mint via `cmdBuf.blitCommandEncoder()`, queue copies, then
    /// `endEncoding` before committing.
    ///
    /// The copy selectors take raw `MTLOrigin` / `MTLSize` structs
    /// (build with `metal.origin(...)` / `metal.size(...)`), so
    /// `bytesPerRow` and `bytesPerImage` are caller-computed —
    /// `width * bytesPerTexel` for tight 2D packing,
    /// `bytesPerRow * height` for tight 3D packing.
    @objc pub class MTLBlitCommandEncoder : NSObject {
        /// Buffer ↔ buffer byte copy.
        @objc("copyFromBuffer:sourceOffset:toBuffer:destinationOffset:size:")
        pub copyFromBufferToBuffer(
            src: MTLBuffer,
            sourceOffset: u64,
            toBuffer: MTLBuffer,
            destinationOffset: u64,
            length: u64
        )

        /// Buffer → texture upload. `sourceBytesPerImage` should be
        /// 0 for 2D destinations (no slice stride).
        @objc("copyFromBuffer:sourceOffset:sourceBytesPerRow:sourceBytesPerImage:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:")
        pub copyFromBufferToTexture(
            src: MTLBuffer,
            sourceOffset: u64,
            sourceBytesPerRow: u64,
            sourceBytesPerImage: u64,
            sourceSize: MTLSize,
            toTexture: MTLTexture,
            destinationSlice: u64,
            destinationLevel: u64,
            destinationOrigin: MTLOrigin
        )

        /// Texture → buffer readback. Mirrors the upload variant.
        @objc("copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toBuffer:destinationOffset:destinationBytesPerRow:destinationBytesPerImage:")
        pub copyFromTextureToBuffer(
            src: MTLTexture,
            sourceSlice: u64,
            sourceLevel: u64,
            sourceOrigin: MTLOrigin,
            sourceSize: MTLSize,
            toBuffer: MTLBuffer,
            destinationOffset: u64,
            destinationBytesPerRow: u64,
            destinationBytesPerImage: u64
        )

        /// Texture → texture copy of a single region.
        @objc("copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:")
        pub copyFromTextureToTexture(
            src: MTLTexture,
            sourceSlice: u64,
            sourceLevel: u64,
            sourceOrigin: MTLOrigin,
            sourceSize: MTLSize,
            toTexture: MTLTexture,
            destinationSlice: u64,
            destinationLevel: u64,
            destinationOrigin: MTLOrigin
        )

        /// Whole-texture copy — both textures must share the same
        /// descriptor (size, pixelFormat, mip count).
        @objc("copyFromTexture:toTexture:")
        pub copyTextureWhole(src: MTLTexture, toTexture: MTLTexture)

        /// Fill a byte range of a buffer with `value`. `range` is
        /// an `(location, length)` `NSRange`; build with
        /// `foundation.makeRange(...)`.
        @objc("fillBuffer:range:value:")
        pub fillBuffer(buffer: MTLBuffer, range: NSRange, value: u8)

        /// Compute the mip chain for `texture` from its level-0
        /// image. The texture must have been allocated with
        /// `mipmapped = true`.
        @objc("generateMipmapsForTexture:")
        pub generateMipmapsForTexture(texture: MTLTexture)

        /// Managed-storage synchronisation. macOS-only — pushes any
        /// GPU-side writes back to the CPU mapping. No-op (and
        /// wasted work) on Apple silicon shared-storage resources.
        @objc("synchronizeResource:")
        pub synchronizeResource(resource: NSObject)
        @objc("synchronizeTexture:slice:level:")
        pub synchronizeTexture(t: MTLTexture, slice: u64, level: u64)

        /// Hint the driver to reformat the texture's storage for
        /// GPU- or CPU-friendly access. macOS 10.14+ / iOS 12+;
        /// a no-op on shared-storage resources.
        @objc("optimizeContentsForGPUAccess:")
        pub optimizeContentsForGPUAccess(resource: NSObject)
        @objc("optimizeContentsForCPUAccess:")
        pub optimizeContentsForCPUAccess(resource: NSObject)

        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("pushDebugGroup:")
        pub pushDebugGroup(s: NSString)
        @objc("popDebugGroup")
        pub popDebugGroup()

        /// Intra-queue fences (blit has no stage mask either).
        @objc("updateFence:")
        pub updateFence(fence: MTLFence)
        @objc("waitForFence:")
        pub waitForFence(fence: MTLFence)

        @objc("endEncoding")
        pub endEncoding()
    }

    /// `id<MTLArgumentEncoder>` — packs textures / samplers /
    /// buffers / inline constants into a single `MTLBuffer` whose
    /// layout matches a shader's argument-buffer struct. The
    /// shader's `[[buffer(n)]]` argument is then a single pointer
    /// to the packed buffer; this lets a fragment / vertex /
    /// kernel function reach hundreds of resources without
    /// burning bind slots.
    ///
    /// Typical use:
    ///
    /// 1. `enc = fn.newArgumentEncoderWithBufferIndex(0)`
    /// 2. `buf = device.newBufferWithLength(enc.encodedLength(), …)`
    /// 3. `enc.setArgumentBuffer(buf, 0)`
    /// 4. `enc.setTexture(t, 0) ; enc.setSamplerState(s, 1) ; …`
    /// 5. Bind `buf` to the matching `[[buffer(0)]]` slot on the
    ///      encoder with `setVertex/FragmentBuffer:offset:atIndex:`
    ///      (you still need to call `renderEncoder.useResource:usage:`
    ///      for each resource referenced inside, so hazard tracking
    ///      knows).
    @objc pub class MTLArgumentEncoder : NSObject {
        @objc("encodedLength")
        pub encodedLength(): u64
        @objc("alignment")
        pub alignment(): u64

        /// Bind the destination buffer + start offset the encoder
        /// writes into. The array-element variant points at one
        /// slot of an argument-buffer array.
        @objc("setArgumentBuffer:offset:")
        pub setArgumentBuffer(buffer: MTLBuffer, offset: u64)
        @objc("setArgumentBuffer:startOffset:arrayElement:")
        pub setArgumentBufferElement(
            buffer: MTLBuffer, startOffset: u64, arrayElement: u64
        )

        /// Resource setters — write each kind into the slot the
        /// shader's argument-buffer struct named at `atIndex`.
        @objc("setBuffer:offset:atIndex:")
        pub setBuffer(b: MTLBuffer, offset: u64, atIndex: u64)
        @objc("setTexture:atIndex:")
        pub setTexture(t: MTLTexture, atIndex: u64)
        @objc("setSamplerState:atIndex:")
        pub setSamplerState(s: MTLSamplerState, atIndex: u64)
        /// Inline-constant pointer for the slot at `atIndex` — write
        /// raw bytes there via `cocoa.writeU8` / `writeF32` etc.
        @objc("constantDataAtIndex:")
        pub constantDataAtIndex(atIndex: u64): i64
        /// Get an inner argument encoder for a nested argument-buffer
        /// slot (an argument buffer that itself contains another
        /// argument buffer).
        @objc("newArgumentEncoderForBufferAtIndex:")
        pub newArgumentEncoderForBufferAtIndex(idx: u64): MTLArgumentEncoder

        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("label")
        pub label(): NSString
    }
}
