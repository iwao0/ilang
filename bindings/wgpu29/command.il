/// Command encoder + the two pass encoders (render / compute) +
/// every `RenderPassEncoder*` / `ComputePassEncoder*` op.
/// Friendly `RenderPass` wrapper lives here too.

use core { WGPUStringView, WGPUColor, WGPUExtent3D, WGPUDevice }
use buffer { WGPUBuffer, Buffer }
use texture { WGPUTextureView, TextureView, WGPUTexelCopyTextureInfo, WGPUTexelCopyBufferInfo }
use pipeline { WGPURenderPipeline, WGPUComputePipeline, Pipeline, ComputePipeline }
use bind_group { WGPUBindGroup, BindGroup }
use std.ffi { cstrFromString }

pub enum LoadOp: u32 {
    undefined = 0x00
    load      = 0x01
    clear     = 0x02
}

pub enum StoreOp: u32 {
    undefined = 0x00
    store     = 0x01
    discard   = 0x02
}

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
        loadOp:        LoadOp
        storeOp:       StoreOp
        clearValue:    WGPUColor
    }
    pub struct WGPURenderPassDepthStencilAttachment {
        view:              WGPUTextureView
        depthLoadOp:       LoadOp
        depthStoreOp:      StoreOp
        depthClearValue:   f32
        depthReadOnly:     u32      // WGPUBool
        stencilLoadOp:     LoadOp
        stencilStoreOp:    StoreOp
        stencilClearValue: u32
        stencilReadOnly:   u32      // WGPUBool
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
    // Indirect draws read the (vertexCount, instanceCount,
    // firstVertex, firstInstance) tuple — or for indexed,
    // (indexCount, instanceCount, firstIndex, baseVertex,
    // firstInstance) — out of `indirectBuffer` at
    // `indirectOffset`. Useful when the draw parameters are
    // computed on the GPU and you don't want to round-trip.
    @lib pub fn wgpuRenderPassEncoderDrawIndirect(
        encoder: WGPURenderPassEncoder,
        indirectBuffer: WGPUBuffer, indirectOffset: u64
    )
    @lib pub fn wgpuRenderPassEncoderDrawIndexedIndirect(
        encoder: WGPURenderPassEncoder,
        indirectBuffer: WGPUBuffer, indirectOffset: u64
    )
    // Pair these around any draw calls whose visibility you want
    // to sample into a QuerySet of type `Occlusion`. Slot index
    // matches an entry in `WGPURenderPassDescriptor.occlusionQuerySet`.
    @lib pub fn wgpuRenderPassEncoderBeginOcclusionQuery(
        encoder: WGPURenderPassEncoder, queryIndex: u32
    )
    @lib pub fn wgpuRenderPassEncoderEndOcclusionQuery(encoder: WGPURenderPassEncoder)
    // Updates the constant-blend colour used by pipelines whose
    // blend state references `ConstantColor` / `OneMinusConstantColor`.
    @lib pub fn wgpuRenderPassEncoderSetBlendConstant(
        encoder: WGPURenderPassEncoder, color: *WGPUColor
    )
    // Updates the stencil-test reference value (the integer
    // `ref` slot in the depth-stencil state).
    @lib pub fn wgpuRenderPassEncoderSetStencilReference(
        encoder: WGPURenderPassEncoder, reference: u32
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

    // ─── wgpu-native multi-draw extension ─────────────────────
    //
    // Records `count` indirect draws back-to-back from
    // `buffer[offset .. offset + count*stride]`. The Count
    // variants instead read the actual draw count from
    // `count_buffer[count_buffer_offset]` (capped to
    // `max_count`) — useful when the GPU computes the draw count.
    @lib pub fn wgpuRenderPassEncoderMultiDrawIndirect(
        encoder: WGPURenderPassEncoder, buffer: WGPUBuffer,
        offset: u64, count: u32
    )
    @lib pub fn wgpuRenderPassEncoderMultiDrawIndexedIndirect(
        encoder: WGPURenderPassEncoder, buffer: WGPUBuffer,
        offset: u64, count: u32
    )
    @lib pub fn wgpuRenderPassEncoderMultiDrawIndirectCount(
        encoder: WGPURenderPassEncoder, buffer: WGPUBuffer, offset: u64,
        countBuffer: WGPUBuffer, countBufferOffset: u64, maxCount: u32
    )
    @lib pub fn wgpuRenderPassEncoderMultiDrawIndexedIndirectCount(
        encoder: WGPURenderPassEncoder, buffer: WGPUBuffer, offset: u64,
        countBuffer: WGPUBuffer, countBufferOffset: u64, maxCount: u32
    )

    // ─── wgpu-native immediate (push-constant) data ───────────
    //
    // Direct CPU → shader constant write inside a pass; no
    // backing buffer needed. `offset` / `sizeBytes` index into
    // the pipeline's immediate-data range (declared via
    // `var<push_constant>` in WGSL when the
    // PushConstants/Immediates feature is enabled).
    @lib pub fn wgpuRenderPassEncoderSetImmediates(
        encoder: WGPURenderPassEncoder,
        offset: u32, sizeBytes: u32, data: *const void
    )
    @lib pub fn wgpuComputePassEncoderSetImmediates(
        encoder: WGPUComputePassEncoder,
        offset: u32, sizeBytes: u32, data: *const void
    )

    // ─── labels + ref counts ──────────────────────────────────
    @lib pub fn wgpuRenderPassEncoderSetLabel(encoder: WGPURenderPassEncoder, label: WGPUStringView)
    @lib pub fn wgpuRenderPassEncoderAddRef(encoder: WGPURenderPassEncoder)
    @lib pub fn wgpuRenderPassEncoderRelease(encoder: WGPURenderPassEncoder)

    @lib pub fn wgpuComputePassEncoderSetLabel(encoder: WGPUComputePassEncoder, label: WGPUStringView)
    @lib pub fn wgpuComputePassEncoderAddRef(encoder: WGPUComputePassEncoder)
    @lib pub fn wgpuComputePassEncoderRelease(encoder: WGPUComputePassEncoder)

    @lib pub fn wgpuCommandEncoderSetLabel(encoder: WGPUCommandEncoder, label: WGPUStringView)
    @lib pub fn wgpuCommandEncoderAddRef(encoder: WGPUCommandEncoder)
    @lib pub fn wgpuCommandEncoderRelease(encoder: WGPUCommandEncoder)

    @lib pub fn wgpuCommandBufferSetLabel(commandBuffer: WGPUCommandBuffer, label: WGPUStringView)
    @lib pub fn wgpuCommandBufferAddRef(commandBuffer: WGPUCommandBuffer)
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
        pub drawIndexed(
            indexCount:    i64, instanceCount: i64,
            firstIndex:    i64, baseVertex:    i64,
            firstInstance: i64
        ) {
            wgpuRenderPassEncoderDrawIndexed(
                this.handle as WGPURenderPassEncoder,
                indexCount as u32, instanceCount as u32,
                firstIndex as u32, baseVertex as i32,
                firstInstance as u32
            )
        }
        /// Indirect draw — params read from `indirectBuffer` at
        /// `indirectOffset` (4 × u32 for `draw`, 5 × u32 for
        /// `drawIndexed`).
        pub drawIndirect(buffer: Buffer, offset: u64) {
            wgpuRenderPassEncoderDrawIndirect(
                this.handle as WGPURenderPassEncoder,
                buffer.handle as WGPUBuffer, offset
            )
        }
        pub drawIndexedIndirect(buffer: Buffer, offset: u64) {
            wgpuRenderPassEncoderDrawIndexedIndirect(
                this.handle as WGPURenderPassEncoder,
                buffer.handle as WGPUBuffer, offset
            )
        }

        /// Bind a vertex buffer to `slot` (matches the index in
        /// the pipeline's `vertex.buffers[]`). `size` =
        /// `0xFFFFFFFFFFFFFFFF` means "to end of buffer".
        pub setVertexBuffer(slot: i64, buffer: Buffer, offset: u64, size: u64) {
            wgpuRenderPassEncoderSetVertexBuffer(
                this.handle as WGPURenderPassEncoder, slot as u32,
                buffer.handle as WGPUBuffer, offset, size
            )
        }
        pub setIndexBuffer(
            buffer: Buffer, format: IndexFormat,
            offset: u64, size: u64
        ) {
            wgpuRenderPassEncoderSetIndexBuffer(
                this.handle as WGPURenderPassEncoder,
                buffer.handle as WGPUBuffer,
                format, offset, size
            )
        }
        // The dynamic-offset overload of `setBindGroup` would
        // need a `*const u32` parameter that the friendly gate
        // rejects. Call the raw
        // `wgpuRenderPassEncoderSetBindGroup(pass.handle as
        // WGPURenderPassEncoder, group, …, &offsets)` for that
        // case.

        /// Bind a `BindGroup` to `groupIndex` (matches
        /// `@group(N)` in WGSL). No dynamic offsets — the
        /// common case.
        pub setBindGroup(groupIndex: i64, group: BindGroup) {
            wgpuRenderPassEncoderSetBindGroup(
                this.handle as WGPURenderPassEncoder,
                groupIndex as u32,
                group.handle as WGPUBindGroup,
                0 as u64, 0 as *const u32
            )
        }


        pub setViewport(
            x: f32, y: f32, width: f32, height: f32,
            minDepth: f32, maxDepth: f32
        ) {
            wgpuRenderPassEncoderSetViewport(
                this.handle as WGPURenderPassEncoder,
                x, y, width, height, minDepth, maxDepth
            )
        }
        pub setScissorRect(x: i64, y: i64, width: i64, height: i64) {
            wgpuRenderPassEncoderSetScissorRect(
                this.handle as WGPURenderPassEncoder,
                x as u32, y as u32, width as u32, height as u32
            )
        }
        /// Updates the constant-blend colour used by pipelines
        /// whose blend state references `ConstantColor` /
        /// `OneMinusConstantColor`.
        pub setBlendConstant(color: WGPUColor) {
            wgpuRenderPassEncoderSetBlendConstant(
                this.handle as WGPURenderPassEncoder, &color
            )
        }
        /// Updates the stencil-test reference value (the integer
        /// `ref` slot in the depth-stencil state).
        pub setStencilReference(reference: i64) {
            wgpuRenderPassEncoderSetStencilReference(
                this.handle as WGPURenderPassEncoder, reference as u32
            )
        }

        // `executeBundles` lives in render_bundle.il (which
        // imports `command.WGPURenderPassEncoder`); for the
        // friendly method we'd need a back-edge that would cycle
        // the module graph. Callers use the raw
        // `wgpuRenderPassEncoderExecuteBundles(this.handle as
        // WGPURenderPassEncoder, count, bundles)`.

        /// Ends the pass — every recording verb after this is an
        /// error. The encoder lifecycle (begin → end → submit →
        /// present) typically runs inside gui-core; call `end()`
        /// directly only when you're driving an off-screen pass
        /// yourself.
        pub end() {
            wgpuRenderPassEncoderEnd(this.handle as WGPURenderPassEncoder)
        }
    }

    /// Wraps `WGPUCommandBuffer` — the immutable output of a
    /// `CommandEncoder.finish()` ready for `Queue.submit`.
    pub class CommandBuffer {
        pub handle: i64

        pub init(handle: WGPUCommandBuffer) {
            this.handle = handle as i64
        }
        pub release() {
            if this.handle != 0 {
                wgpuCommandBufferRelease(this.handle as WGPUCommandBuffer)
                this.handle = 0
            }
        }
        pub setLabel(label: string) {
            wgpuCommandBufferSetLabel(this.handle as WGPUCommandBuffer, WGPUStringView {
                data:   cstrFromString(label) as *void,
                length: 0xFFFFFFFFFFFFFFFF as u64,
            })
        }
    }

    /// Wraps `WGPUComputePassEncoder`. Built via
    /// `CommandEncoder.beginComputePass`.
    pub class ComputePass {
        pub handle: i64

        pub init(handle: WGPUComputePassEncoder) {
            this.handle = handle as i64
        }
        pub release() {
            if this.handle != 0 {
                wgpuComputePassEncoderRelease(this.handle as WGPUComputePassEncoder)
                this.handle = 0
            }
        }
        pub setLabel(label: string) {
            wgpuComputePassEncoderSetLabel(this.handle as WGPUComputePassEncoder, WGPUStringView {
                data:   cstrFromString(label) as *void,
                length: 0xFFFFFFFFFFFFFFFF as u64,
            })
        }
        pub setPipeline(pipeline: ComputePipeline) {
            wgpuComputePassEncoderSetPipeline(
                this.handle as WGPUComputePassEncoder,
                pipeline.handle as WGPUComputePipeline
            )
        }
        pub setBindGroup(groupIndex: i64, group: BindGroup) {
            wgpuComputePassEncoderSetBindGroup(
                this.handle as WGPUComputePassEncoder,
                groupIndex as u32,
                group.handle as WGPUBindGroup,
                0 as u64, 0 as *const u32
            )
        }
        pub dispatchWorkgroups(x: i64, y: i64, z: i64) {
            wgpuComputePassEncoderDispatchWorkgroups(
                this.handle as WGPUComputePassEncoder,
                x as u32, y as u32, z as u32
            )
        }
        pub dispatchWorkgroupsIndirect(buffer: Buffer, offset: u64) {
            wgpuComputePassEncoderDispatchWorkgroupsIndirect(
                this.handle as WGPUComputePassEncoder,
                buffer.handle as WGPUBuffer, offset
            )
        }
        pub end() {
            wgpuComputePassEncoderEnd(this.handle as WGPUComputePassEncoder)
        }
    }

    /// Wraps `WGPUCommandEncoder`. Built via
    /// `Device.createCommandEncoder`; record copy / debug / pass
    /// verbs, then `finish()` to obtain the submittable buffer.
    pub class CommandEncoder {
        pub handle: i64

        pub init(handle: WGPUCommandEncoder) {
            this.handle = handle as i64
        }
        pub release() {
            if this.handle != 0 {
                wgpuCommandEncoderRelease(this.handle as WGPUCommandEncoder)
                this.handle = 0
            }
        }
        pub setLabel(label: string) {
            wgpuCommandEncoderSetLabel(this.handle as WGPUCommandEncoder, WGPUStringView {
                data:   cstrFromString(label) as *void,
                length: 0xFFFFFFFFFFFFFFFF as u64,
            })
        }

        /// Begin a render pass from the supplied options.
        /// Supports multiple colour attachments + optional
        /// depth-stencil. For occlusion / timestamp slots drive
        /// the raw `wgpuCommandEncoderBeginRenderPass(enc, &desc)`
        /// directly.
        pub beginRenderPass(opts: RenderPassOptions): RenderPass {
            let attachments: WGPURenderPassColorAttachment[] = []
            let i: i64 = 0
            while i < opts.colors.length {
                let c = opts.colors[i]
                attachments.push(WGPURenderPassColorAttachment {
                    nextInChain:   0 as *void,
                    view:          c.view.handle as WGPUTextureView,
                    depthSlice:    c.depthSlice as u32,
                    resolveTarget: c.resolveTarget.handle as WGPUTextureView,
                    loadOp:        c.loadOp,
                    storeOp:       c.storeOp,
                    clearValue:    c.clearValue,
                })
                i = i + 1
            }
            let ds = WGPURenderPassDepthStencilAttachment {
                view:              0 as WGPUTextureView,
                depthLoadOp:       LoadOp.undefined,
                depthStoreOp:      StoreOp.undefined,
                depthClearValue:   0.0,
                depthReadOnly:     0 as u32,
                stencilLoadOp:     LoadOp.undefined,
                stencilStoreOp:    StoreOp.undefined,
                stencilClearValue: 0 as u32,
                stencilReadOnly:   0 as u32,
            }
            let dsPtr: *void = 0 as *void
            if opts.hasDepth {
                let d = opts.depthStencil
                let dRO: u32 = 0 as u32
                let sRO: u32 = 0 as u32
                if d.depthReadOnly   { dRO = 1 as u32 }
                if d.stencilReadOnly { sRO = 1 as u32 }
                ds = WGPURenderPassDepthStencilAttachment {
                    view:              d.view.handle as WGPUTextureView,
                    depthLoadOp:       d.depthLoadOp,
                    depthStoreOp:      d.depthStoreOp,
                    depthClearValue:   d.depthClearValue,
                    depthReadOnly:     dRO,
                    stencilLoadOp:     d.stencilLoadOp,
                    stencilStoreOp:    d.stencilStoreOp,
                    stencilClearValue: d.stencilClearValue as u32,
                    stencilReadOnly:   sRO,
                }
                dsPtr = (&ds) as *void
            }
            let desc = WGPURenderPassDescriptor {
                nextInChain:            0 as *void,
                label:                  WGPUStringView {
                    data:   cstrFromString(opts.label) as *void,
                    length: 0xFFFFFFFFFFFFFFFF as u64,
                },
                colorAttachmentCount:   attachments.length as u64,
                colorAttachments:       attachments,
                depthStencilAttachment: dsPtr,
                occlusionQuerySet:      0 as *void,
                timestampWrites:        0 as *void,
            }
            new RenderPass(wgpuCommandEncoderBeginRenderPass(
                this.handle as WGPUCommandEncoder, &desc
            ))
        }
        /// Begin a compute pass without timestamp writes — pass
        /// `0 as *void` for the descriptor.
        pub beginComputePass(): ComputePass {
            new ComputePass(wgpuCommandEncoderBeginComputePass(
                this.handle as WGPUCommandEncoder, 0 as *WGPUComputePassDescriptor
            ))
        }
        pub copyBufferToBuffer(
            src: Buffer, srcOffset: u64,
            dst: Buffer, dstOffset: u64, size: u64
        ) {
            wgpuCommandEncoderCopyBufferToBuffer(
                this.handle as WGPUCommandEncoder,
                src.handle as WGPUBuffer, srcOffset,
                dst.handle as WGPUBuffer, dstOffset, size
            )
        }
        pub clearBuffer(buf: Buffer, offset: u64, size: u64) {
            wgpuCommandEncoderClearBuffer(
                this.handle as WGPUCommandEncoder,
                buf.handle as WGPUBuffer, offset, size
            )
        }
        pub insertDebugMarker(label: string) {
            wgpuCommandEncoderInsertDebugMarker(
                this.handle as WGPUCommandEncoder,
                WGPUStringView {
                    data:   cstrFromString(label) as *void,
                    length: 0xFFFFFFFFFFFFFFFF as u64,
                }
            )
        }
        pub pushDebugGroup(label: string) {
            wgpuCommandEncoderPushDebugGroup(
                this.handle as WGPUCommandEncoder,
                WGPUStringView {
                    data:   cstrFromString(label) as *void,
                    length: 0xFFFFFFFFFFFFFFFF as u64,
                }
            )
        }
        pub popDebugGroup() {
            wgpuCommandEncoderPopDebugGroup(this.handle as WGPUCommandEncoder)
        }
        /// Finish recording and produce the submittable
        /// `CommandBuffer`. The encoder is consumed — using it
        /// after `finish` is an error.
        pub finish(): CommandBuffer {
            new CommandBuffer(wgpuCommandEncoderFinish(
                this.handle as WGPUCommandEncoder, 0 as *void
            ))
        }
    }

    /// One colour attachment for `RenderPassOptions`.
    /// Mirrors `WGPURenderPassColorAttachment`. Construct with
    /// `(view, loadOp, storeOp, clearValue)`; `resolveTarget`
    /// defaults to null (no MSAA resolve) and `depthSlice` to
    /// `WGPU_DEPTH_SLICE_UNDEFINED` (0xFFFFFFFF).
    pub class ColorAttachmentOption {
        pub view:          TextureView
        pub resolveTarget: TextureView   // nullable; TextureView(0) = none
        pub loadOp:        LoadOp
        pub storeOp:       StoreOp
        pub clearValue:    WGPUColor
        pub depthSlice:    u32

        pub init(view: TextureView, loadOp: LoadOp, storeOp: StoreOp, clearValue: WGPUColor) {
            this.view          = view
            this.resolveTarget = new TextureView(0 as WGPUTextureView)
            this.loadOp        = loadOp
            this.storeOp       = storeOp
            this.clearValue    = clearValue
            this.depthSlice    = 0xFFFFFFFF as u32
        }
    }

    /// Optional depth-stencil attachment for `RenderPassOptions`.
    /// Mirrors `WGPURenderPassDepthStencilAttachment`. Construct
    /// with `(view)` then assign the relevant load/store/clear
    /// fields before calling
    /// `RenderPassOptions.setDepthStencil(...)`.
    pub class DepthStencilAttachmentOption {
        pub view:              TextureView
        pub depthLoadOp:       LoadOp
        pub depthStoreOp:      StoreOp
        pub depthClearValue:   f32
        pub depthReadOnly:     bool
        pub stencilLoadOp:     LoadOp
        pub stencilStoreOp:    StoreOp
        pub stencilClearValue: i64
        pub stencilReadOnly:   bool

        pub init(view: TextureView) {
            this.view              = view
            this.depthLoadOp       = LoadOp.undefined
            this.depthStoreOp      = StoreOp.undefined
            this.depthClearValue   = 1.0
            this.depthReadOnly     = false
            this.stencilLoadOp     = LoadOp.undefined
            this.stencilStoreOp    = StoreOp.undefined
            this.stencilClearValue = 0
            this.stencilReadOnly   = false
        }
    }

    /// Configuration for `CommandEncoder.beginRenderPass`.
    /// Holds an array of colour attachments and an optional
    /// depth-stencil attachment (selected by `hasDepth`).
    /// For occlusion / timestamp passes drive
    /// `wgpuCommandEncoderBeginRenderPass` directly.
    pub class RenderPassOptions {
        pub label:        string
        pub colors:       ColorAttachmentOption[]
        pub depthStencil: DepthStencilAttachmentOption
        pub hasDepth:     bool

        pub init(colors: ColorAttachmentOption[]) {
            this.label        = ""
            this.colors       = colors
            this.hasDepth     = false
            this.depthStencil = new DepthStencilAttachmentOption(
                new TextureView(0 as WGPUTextureView)
            )
        }

        /// Attach a depth-stencil view to this pass.
        pub setDepthStencil(d: DepthStencilAttachmentOption) {
            this.depthStencil = d
            this.hasDepth     = true
        }
    }
}
