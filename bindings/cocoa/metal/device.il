// MTLDevice + MTLCommandQueue + MTLCommandBuffer — the top of
// every Metal call chain. Apps acquire a device via
// `MTLCreateSystemDefaultDevice()`, build queues off it, and
// enqueue command buffers carrying encoder workloads onto a queue.

use foundation { NSObject, NSString, NSError, NSRange, withObjcError, objcRetain }
use types.*

@extern(ObjC, "/System/Library/Frameworks/Metal.framework/Metal") {
    /// Raw C entrypoint — returns an `id<MTLDevice>` as an `i64`
    /// handle. Users should call the `defaultDevice()` wrapper
    /// below instead so the handle gets boxed into an `MTLDevice`
    /// instance via `__wrap_handle` (matching how the rest of
    /// the @objc surface marshals incoming `id`s).
    @lib pub fn MTLCreateSystemDefaultDevice(): i64

    /// `id<MTLDevice>` — opaque GPU handle. Bridge it to APIs
    /// (e.g. `SKRenderer.rendererWithDevice:`) that take an
    /// arbitrary `NSObject` receiver by passing the instance
    /// through; the @objc dispatch unwraps the underlying id.
    @objc pub class MTLDevice : NSObject {
        /// Wrap the raw `id` returned by
        /// `MTLCreateSystemDefaultDevice()` into a typed
        /// `MTLDevice` instance. The returned device's `handle`
        /// is `0` on machines without Metal support — callers
        /// that want to fail-fast should check `device.handle
        /// != 0` before sending the first message.
        pub static defaultDevice(): MTLDevice {
            MTLDevice.__wrap_handle(MTLCreateSystemDefaultDevice())
        }

        @objc("newCommandQueue")
        pub newCommandQueue(): MTLCommandQueue

        @objc("newTextureWithDescriptor:")
        pub newTextureWithDescriptor(d: MTLTextureDescriptor): MTLTexture

        // Shader / pipeline construction. The `error:` parameter
        // is an `NSError **` out-pointer expressed as a raw `i64`
        // slot address — pass `0` to ignore the error, or feed
        // it the per-thread slot via `cocoa.withObjcError(...)`
        // which is what the `OrError` top-level wrappers below
        // use. Returned object handle is 0 on failure.
        @objc("newLibraryWithSource:options:error:")
        pub newLibraryWithSource(
            source: NSString, options: NSObject, error: i64
        ): MTLLibrary
        @objc("newDefaultLibrary")
        pub newDefaultLibrary(): MTLLibrary
        @objc("newRenderPipelineStateWithDescriptor:error:")
        pub newRenderPipelineStateWithDescriptor(
            descriptor: MTLRenderPipelineDescriptor, error: i64
        ): MTLRenderPipelineState
        @objc("newComputePipelineStateWithFunction:error:")
        pub newComputePipelineStateWithFunction(
            function: MTLFunction, error: i64
        ): MTLComputePipelineState

        // Buffer factories. `newBufferWithBytes` copies `length`
        // bytes from `ptr` into a fresh buffer; `newBufferWithLength`
        // allocates uninitialised storage the caller fills via
        // `buffer.contents()`. `options` selects storage mode etc.
        // (`MTLResourceOptions.storageModeShared` is the casual
        // default on Apple silicon).
        @objc("newBufferWithBytes:length:options:")
        pub newBufferWithBytes(
            bytes: i64, length: u64, options: MTLResourceOptions
        ): MTLBuffer
        @objc("newBufferWithLength:options:")
        pub newBufferWithLength(length: u64, options: MTLResourceOptions): MTLBuffer

        // Heap factories + size queries. `newHeapWithDescriptor:`
        // allocates a backing region the caller suballocates from
        // via `heap.newBuffer/Texture(...)`. The two size queries
        // report the (size, align) pair a resource would consume
        // *if* it were placed on a heap with those options — use
        // them to total up the heap size before creating it.
        @objc("newHeapWithDescriptor:")
        pub newHeapWithDescriptor(d: MTLHeapDescriptor): MTLHeap
        @objc("heapBufferSizeAndAlignWithLength:options:")
        pub heapBufferSizeAndAlign(
            length: u64, options: MTLResourceOptions
        ): MTLSizeAndAlign
        @objc("heapTextureSizeAndAlignWithDescriptor:")
        pub heapTextureSizeAndAlign(d: MTLTextureDescriptor): MTLSizeAndAlign

        // Fence factory — a single-queue synchronisation primitive
        // updated by one encoder's `updateFence:` and waited on
        // by a later encoder's `waitForFence:`. Cheap; reuse one
        // per producer/consumer pair per frame.
        @objc("newFence")
        pub newFence(): MTLFence

        // Event factories. `MTLEvent` is a private GPU-only signal
        // (no CPU-visible value); `MTLSharedEvent` exposes a u64
        // counter the CPU can poll, set, and `notifyListener:`
        // wait on — the bridge for CPU↔GPU rendezvous and
        // cross-process / cross-device GPU sync.
        @objc("newEvent")
        pub newEvent(): MTLEvent
        @objc("newSharedEvent")
        pub newSharedEvent(): MTLSharedEvent

        // Manual-layout argument-encoder factory takes an
        // `NSArray<MTLArgumentDescriptor *>`. `MTLArgumentDescriptor`
        // itself isn't bound yet — for the casual "let the shader
        // describe its own argument buffer" path use
        // `function.newArgumentEncoderWithBufferIndex(idx)` on
        // `MTLFunction` instead.

        // Sampler / depth-stencil state factories. Both are
        // immutable handles built from a mutable descriptor, the
        // same pattern as `MTLRenderPipelineState`; reuse the
        // resulting state across frames (cheap to bind, more
        // expensive to build).
        @objc("newSamplerStateWithDescriptor:")
        pub newSamplerStateWithDescriptor(d: MTLSamplerDescriptor): MTLSamplerState
        @objc("newDepthStencilStateWithDescriptor:")
        pub newDepthStencilStateWithDescriptor(
            d: MTLDepthStencilDescriptor
        ): MTLDepthStencilState

        @objc("name")
        pub deviceName(): NSObject

    }

    /// `id<MTLCommandQueue>` — mint command buffers off this.
    /// One queue per device is sufficient for SKRenderer's
    /// use case; reuse across frames.
    @objc pub class MTLCommandQueue : NSObject {
        @objc("commandBuffer")
        pub commandBuffer(): MTLCommandBuffer
    }

    /// `id<MTLCommandBuffer>` — collects encoded GPU work. The
    /// SKRenderer flow is:
    ///   1. `let buf = queue.commandBuffer()`
    ///   2. `renderer.renderWithViewport(rect, buf, desc)`
    ///   3. `buf.commit()`
    ///   4. (optional) `buf.waitUntilCompleted()`
    @objc pub class MTLCommandBuffer : NSObject {
        @objc("commit")
        pub commit()
        @objc("waitUntilScheduled")
        pub waitUntilScheduled()
        @objc("waitUntilCompleted")
        pub waitUntilCompleted()
        // `presentDrawable:` is here for caller-controlled
        // compositing pipelines that target a CAMetalLayer
        // drawable. SKRenderer-direct usage doesn't need it.
        @objc("presentDrawable:")
        pub presentDrawable(drawable: NSObject)
        // Mint a render encoder bound to a render-pass descriptor.
        // The returned encoder records draw calls into this command
        // buffer until `endEncoding` is called.
        @objc("renderCommandEncoderWithDescriptor:")
        pub renderCommandEncoderWithDescriptor(
            descriptor: MTLRenderPassDescriptor
        ): MTLRenderCommandEncoder
        // Mint a compute encoder. The encoder records dispatch
        // calls into this command buffer until `endEncoding`.
        @objc("computeCommandEncoder")
        pub computeCommandEncoder(): MTLComputeCommandEncoder
        // Mint a blit (memory-transfer) encoder. The encoder
        // records buffer ↔ texture copies, mipmap generation, and
        // managed-storage synchronisation into this command
        // buffer until `endEncoding`.
        @objc("blitCommandEncoder")
        pub blitCommandEncoder(): MTLBlitCommandEncoder

        // Lifecycle inspection. `status` advances through the
        // `MTLCommandBufferStatus` enum as the buffer is queued /
        // scheduled / executed; `commandBufferError` returns the
        // `NSError *` that caused a `.error` status (or a 0 handle
        // when none).
        @objc("status")
        pub status(): MTLCommandBufferStatus
        @objc("error")
        pub commandBufferError(): NSError

        // Completion / schedule callbacks. The raw `@objc` slots
        // take an `ObjCBlock<fn(i64)>` (built via
        // `new ObjCBlock(fn(raw: i64) { ... })`); the typed
        // `addCompletedHandler(fn(i64))` / `addScheduledHandler(fn(i64))`
        // wrappers above box the closure for you. Prefer the
        // `onCompleted(...)` / `onScheduled(...)` top-level helpers
        // below — they wrap the incoming raw `id` back into an
        // `MTLCommandBuffer` for the user-supplied closure.
        //
        // Apple documents these handlers as "may be called on
        // a non-main thread"; if the closure has to touch UI
        // state, hop back to the main queue (e.g. via
        // `NSOperationQueue.mainQueue().addOperationWithBlock`).
        @objc("addCompletedHandler:")
        _addCompletedHandler(block: ObjCBlock<fn(i64)>)
        @objc("addScheduledHandler:")
        _addScheduledHandler(block: ObjCBlock<fn(i64)>)
        /// `addCompletedHandler:` — called after the GPU finishes
        /// executing this command buffer. `body` receives the
        /// command buffer itself as a raw `id` handle (wrap with
        /// `MTLCommandBuffer.__wrap_handle_unowned` inside if you
        /// want a typed view).
        pub addCompletedHandler(body: fn(i64)) {
            this._addCompletedHandler(new ObjCBlock(body))
        }
        /// `addScheduledHandler:` — called when the command buffer
        /// has been scheduled on the GPU (not yet executed). Same
        /// argument shape as `addCompletedHandler`.
        pub addScheduledHandler(body: fn(i64)) {
            this._addScheduledHandler(new ObjCBlock(body))
        }

        // GPU-side event signalling / waiting. Both selectors are
        // queued into the command buffer; the GPU executes them
        // in submit order against the event's u64 counter. Use
        // `MTLSharedEvent` if a CPU thread also wants to observe
        // or set the same counter.
        @objc("encodeSignalEvent:value:")
        pub encodeSignalEvent(event: MTLEvent, value: u64)
        @objc("encodeWaitForEvent:value:")
        pub encodeWaitForEvent(event: MTLEvent, value: u64)
    }
}
