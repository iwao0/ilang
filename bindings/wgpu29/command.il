/// Command encoder + the two pass encoders (render / compute) +
/// every `RenderPassEncoder*` / `ComputePassEncoder*` op.
/// Friendly `RenderPass` wrapper lives here too.

use core { WGPUStringView, WGPUColor, WGPUExtent3D, WGPUDevice }
use buffer { WGPUBuffer }
use texture { WGPUTextureView, WGPUTexelCopyTextureInfo, WGPUTexelCopyBufferInfo }
use pipeline { WGPURenderPipeline, WGPUComputePipeline, Pipeline }
use bind_group { WGPUBindGroup }

@extern(C, "wgpu_native") {
    @handle pub struct WGPUCommandEncoder {}
    @handle pub struct WGPUCommandBuffer {}
    @handle pub struct WGPURenderPassEncoder {}
    @handle pub struct WGPUComputePassEncoder {}

    // ─── render pass descriptors ──────────────────────────────
    // WGPULoadOp_Clear = 2, WGPUStoreOp_Store = 1.
    pub struct WGPURenderPassColorAttachment {
        nextInChain:   *void
        view:          WGPUTextureView
        depthSlice:    u32        // WGPU_DEPTH_SLICE_UNDEFINED = 0xFFFFFFFF
        resolveTarget: WGPUTextureView   // nullable
        loadOp:        u32
        storeOp:       u32
        clearValue:    WGPUColor
    }
    pub struct WGPURenderPassDescriptor {
        nextInChain:            *void
        label:                  WGPUStringView
        colorAttachmentCount:   u64
        colorAttachments:       *WGPURenderPassColorAttachment
        depthStencilAttachment: *void
        occlusionQuerySet:      *void
        timestampWrites:        *void
    }

    // ─── compute pass descriptor ──────────────────────────────
    pub struct WGPUComputePassDescriptor {
        nextInChain:     *void
        label:           WGPUStringView
        timestampWrites: *void        // WGPUPassTimestampWrites const*
    }

    // ─── command encoder lifecycle ────────────────────────────
    @lib pub fn wgpuDeviceCreateCommandEncoder(
        device: WGPUDevice, descriptor: *void
    ): WGPUCommandEncoder
    @lib pub fn wgpuCommandEncoderBeginRenderPass(
        commandEncoder: WGPUCommandEncoder,
        descriptor: *WGPURenderPassDescriptor
    ): WGPURenderPassEncoder
    @lib pub fn wgpuCommandEncoderBeginComputePass(
        encoder: WGPUCommandEncoder, descriptor: *WGPUComputePassDescriptor
    ): WGPUComputePassEncoder
    @lib pub fn wgpuCommandEncoderFinish(
        commandEncoder: WGPUCommandEncoder, descriptor: *void
    ): WGPUCommandBuffer

    // ─── command encoder copy ops ─────────────────────────────
    @lib pub fn wgpuCommandEncoderCopyBufferToBuffer(
        encoder: WGPUCommandEncoder,
        source:      WGPUBuffer, sourceOffset:      u64,
        destination: WGPUBuffer, destinationOffset: u64,
        size: u64
    )
    @lib pub fn wgpuCommandEncoderCopyBufferToTexture(
        encoder: WGPUCommandEncoder,
        source:      *WGPUTexelCopyBufferInfo,
        destination: *WGPUTexelCopyTextureInfo,
        copySize:    *WGPUExtent3D
    )
    @lib pub fn wgpuCommandEncoderCopyTextureToBuffer(
        encoder:     WGPUCommandEncoder,
        source:      *WGPUTexelCopyTextureInfo,
        destination: *WGPUTexelCopyBufferInfo,
        copySize:    *WGPUExtent3D
    )
    @lib pub fn wgpuCommandEncoderCopyTextureToTexture(
        encoder: WGPUCommandEncoder,
        source:      *WGPUTexelCopyTextureInfo,
        destination: *WGPUTexelCopyTextureInfo,
        copySize:    *WGPUExtent3D
    )
    // Zero-fill a buffer range — useful before append-style use
    // of CopyDst buffers when wgpu's debug validation chokes on
    // uninitialised reads.
    @lib pub fn wgpuCommandEncoderClearBuffer(
        encoder: WGPUCommandEncoder,
        buffer:  WGPUBuffer,
        offset:  u64,
        size:    u64
    )

    // ─── debug groups on command / pass encoders ──────────────
    //
    // Push / Pop nest a label; Insert drops a single marker. All
    // show up as named regions in PIX / RenderDoc / Xcode-frame-
    // debugger captures. `markerLabel` / `groupLabel` use the
    // standard `WGPUStringView` shape (data + length).
    @lib pub fn wgpuCommandEncoderInsertDebugMarker(
        encoder: WGPUCommandEncoder, markerLabel: WGPUStringView
    )
    @lib pub fn wgpuCommandEncoderPushDebugGroup(
        encoder: WGPUCommandEncoder, groupLabel: WGPUStringView
    )
    @lib pub fn wgpuCommandEncoderPopDebugGroup(encoder: WGPUCommandEncoder)

    @lib pub fn wgpuRenderPassEncoderInsertDebugMarker(
        encoder: WGPURenderPassEncoder, markerLabel: WGPUStringView
    )
    @lib pub fn wgpuRenderPassEncoderPushDebugGroup(
        encoder: WGPURenderPassEncoder, groupLabel: WGPUStringView
    )
    @lib pub fn wgpuRenderPassEncoderPopDebugGroup(encoder: WGPURenderPassEncoder)

    @lib pub fn wgpuComputePassEncoderInsertDebugMarker(
        encoder: WGPUComputePassEncoder, markerLabel: WGPUStringView
    )
    @lib pub fn wgpuComputePassEncoderPushDebugGroup(
        encoder: WGPUComputePassEncoder, groupLabel: WGPUStringView
    )
    @lib pub fn wgpuComputePassEncoderPopDebugGroup(encoder: WGPUComputePassEncoder)

    // ─── render-pass encoder ops ──────────────────────────────
    @lib pub fn wgpuRenderPassEncoderSetPipeline(
        encoder: WGPURenderPassEncoder, pipeline: WGPURenderPipeline
    )
    @lib pub fn wgpuRenderPassEncoderDraw(
        encoder: WGPURenderPassEncoder,
        vertexCount: u32, instanceCount: u32,
        firstVertex: u32, firstInstance: u32
    )
    // `slot` = vertex-buffer binding index in the pipeline's
    // `vertex.buffers[]`. `buffer` may be null to unbind. `size`
    // = WGPU_WHOLE_SIZE (= u64::MAX) means "use the rest of the
    // buffer from `offset`".
    @lib pub fn wgpuRenderPassEncoderSetVertexBuffer(
        encoder: WGPURenderPassEncoder, slot: u32,
        buffer:  WGPUBuffer, offset: u64, size: u64
    )
    // WGPUIndexFormat: Undefined=0, Uint16=1, Uint32=2.
    @lib pub fn wgpuRenderPassEncoderSetIndexBuffer(
        encoder: WGPURenderPassEncoder, buffer: WGPUBuffer,
        format: u32, offset: u64, size: u64
    )
    // `baseVertex` is signed — lets you reuse an index buffer
    // across geometry sub-ranges by shifting the per-vertex
    // lookup.
    @lib pub fn wgpuRenderPassEncoderDrawIndexed(
        encoder:       WGPURenderPassEncoder,
        indexCount:    u32, instanceCount: u32,
        firstIndex:    u32, baseVertex:    i32,
        firstInstance: u32
    )
    @lib pub fn wgpuRenderPassEncoderSetViewport(
        encoder: WGPURenderPassEncoder,
        x: f32, y: f32, width: f32, height: f32,
        minDepth: f32, maxDepth: f32
    )
    @lib pub fn wgpuRenderPassEncoderSetScissorRect(
        encoder: WGPURenderPassEncoder,
        x: u32, y: u32, width: u32, height: u32
    )
    // `group` may be null to unbind. `dynamicOffsetCount` / -s
    // are zero / null unless any binding in `group`'s layout was
    // declared `hasDynamicOffset = true`.
    @lib pub fn wgpuRenderPassEncoderSetBindGroup(
        encoder:             WGPURenderPassEncoder,
        groupIndex:          u32,
        group:               WGPUBindGroup,
        dynamicOffsetCount:  u64,
        dynamicOffsets:      *const u32
    )
    @lib pub fn wgpuRenderPassEncoderEnd(encoder: WGPURenderPassEncoder)

    // ─── compute-pass encoder ops ─────────────────────────────
    @lib pub fn wgpuComputePassEncoderSetPipeline(
        encoder: WGPUComputePassEncoder, pipeline: WGPUComputePipeline
    )
    @lib pub fn wgpuComputePassEncoderSetBindGroup(
        encoder:            WGPUComputePassEncoder,
        groupIndex:         u32,
        group:              WGPUBindGroup,
        dynamicOffsetCount: u64,
        dynamicOffsets:     *const u32
    )
    @lib pub fn wgpuComputePassEncoderDispatchWorkgroups(
        encoder: WGPUComputePassEncoder,
        workgroupCountX: u32, workgroupCountY: u32, workgroupCountZ: u32
    )
    // Indirect dispatch reads `[x, y, z]` u32 triples out of
    // `indirectBuffer` at `indirectOffset` — useful when the
    // workgroup count is itself computed on the GPU.
    @lib pub fn wgpuComputePassEncoderDispatchWorkgroupsIndirect(
        encoder: WGPUComputePassEncoder,
        indirectBuffer: WGPUBuffer, indirectOffset: u64
    )
    @lib pub fn wgpuComputePassEncoderEnd(encoder: WGPUComputePassEncoder)

    // ─── releases ─────────────────────────────────────────────
    @lib pub fn wgpuRenderPassEncoderRelease(encoder: WGPURenderPassEncoder)
    @lib pub fn wgpuComputePassEncoderRelease(encoder: WGPUComputePassEncoder)
    @lib pub fn wgpuCommandEncoderRelease(encoder: WGPUCommandEncoder)
    @lib pub fn wgpuCommandBufferRelease(commandBuffer: WGPUCommandBuffer)

    // ─── friendly wrapper ─────────────────────────────────────

    /// Wraps `WGPURenderPassEncoder` for the gui-side `onRender`
    /// callback. The underlying encoder's lifecycle (begin / end
    /// / submit / present) is owned by gui-core; this wrapper
    /// exposes only the recording verbs.
    pub class RenderPass {
        pub handle: i64

        pub init(handle: WGPURenderPassEncoder) {
            this.handle = handle as i64
        }

        pub setPipeline(pipeline: Pipeline) {
            wgpuRenderPassEncoderSetPipeline(
                this.handle as WGPURenderPassEncoder,
                pipeline.handle as WGPURenderPipeline
            )
        }

        /// Record a draw of `vertexCount` vertices into
        /// `instanceCount` instances, starting at
        /// `firstVertex` / `firstInstance`. For the hello-triangle
        /// case use `draw(3, 1, 0, 0)`.
        ///
        /// Counts are i64 here so call sites don't need `as u32`
        /// casts on integer literals; the wgpu C ABI gets the
        /// expected u32 inside.
        pub draw(
            vertexCount: i64, instanceCount: i64,
            firstVertex: i64, firstInstance: i64
        ) {
            wgpuRenderPassEncoderDraw(
                this.handle as WGPURenderPassEncoder,
                vertexCount as u32, instanceCount as u32,
                firstVertex as u32, firstInstance as u32
            )
        }
    }
}
