/// GPU resources — MTLTexture / MTLBuffer / MTLHeap / MTLFence.
/// These are the data containers the encoders read from and write
/// to (samplers / depth-stencil state live in `pipeline.il`).

use foundation { NSObject, NSString, NSError, NSRange, withObjcError, objcRetain }
use types.*

@extern(ObjC, "/System/Library/Frameworks/Metal.framework/Metal") {
    /// `id<MTLTexture>` — opaque GPU image. Build via
    /// `device.newTextureWithDescriptor(desc)` from a configured
    /// `MTLTextureDescriptor`, then plug into a render-pass
    /// colour attachment so SKRenderer draws into it, or move
    /// raw bytes in / out via `replaceRegion(...)` /
    /// `getBytes(...)`. The buffer the caller provides for those
    /// is a raw `i64` pointer — see the
    /// `cocoa.{writeU8, readU8, ...}` helpers (or `malloc` /
    /// `MTLBuffer.contents` if you've got a staging buffer
    /// already in hand).
    @objc pub class MTLTexture : NSObject {
        @objc("width")
        pub width(): u64
        @objc("height")
        pub height(): u64
        @objc("pixelFormat")
        pub pixelFormat(): MTLPixelFormat

        // ─── CPU ↔ GPU byte transfer ────────────────────────────
        //
        /// `bytes` is a raw `i64` address (compute via `&local`
        /// inside an @extern(C) block, or take it from
        /// `mtlbuf.contents()`). `bytesPerRow` is the row stride
        /// in bytes — for tight packing pass `region.size.width *
        /// bytes_per_pixel` (e.g. 4 for BGRA8Unorm). The storage
        /// mode must allow CPU access; private-storage textures
        /// need a blit encoder + staging buffer instead.
        @objc("replaceRegion:mipmapLevel:withBytes:bytesPerRow:")
        pub replaceRegion(
            r: MTLRegion,
            mipmapLevel: u64,
            withBytes: i64,
            bytesPerRow: u64
        )
        @objc("replaceRegion:mipmapLevel:slice:withBytes:bytesPerRow:bytesPerImage:")
        pub replaceRegionSlice(
            r: MTLRegion,
            mipmapLevel: u64,
            slice: u64,
            withBytes: i64,
            bytesPerRow: u64,
            bytesPerImage: u64
        )
        @objc("getBytes:bytesPerRow:fromRegion:mipmapLevel:")
        pub getBytes(
            bytes: i64,
            bytesPerRow: u64,
            fromRegion: MTLRegion,
            mipmapLevel: u64
        )
        @objc("getBytes:bytesPerRow:bytesPerImage:fromRegion:mipmapLevel:slice:")
        pub getBytesSlice(
            bytes: i64,
            bytesPerRow: u64,
            bytesPerImage: u64,
            fromRegion: MTLRegion,
            mipmapLevel: u64,
            slice: u64
        )
    }

    /// `MTLTextureDescriptor` — describes a texture to create.
    /// Use the `texture2DDescriptorWithPixelFormat:width:height:mipmapped:`
    /// factory for the common 2D-target case; the setters below
    /// override the defaults (the factory leaves `usage` as
    /// `.shaderRead` only; an SKRenderer target needs the
    /// `.renderTarget` bit too).
    @objc pub class MTLTextureDescriptor : NSObject {
        @objc("texture2DDescriptorWithPixelFormat:width:height:mipmapped:")
        pub static texture2DDescriptor(
            pixelFormat: MTLPixelFormat,
            width: u64,
            height: u64,
            mipmapped: bool
        ): MTLTextureDescriptor

        @objc("setPixelFormat:")
        pub setPixelFormat(f: MTLPixelFormat)
        @objc("setWidth:")
        pub setWidth(w: u64)
        @objc("setHeight:")
        pub setHeight(h: u64)
        @objc("setUsage:")
        pub setUsage(u: MTLTextureUsage)
        @objc("setStorageMode:")
        pub setStorageMode(m: MTLStorageMode)
    }

    /// `id<MTLBuffer>` — GPU-accessible byte buffer. Built via
    /// `device.newBufferWithBytes` (copy host bytes in) or
    /// `device.newBufferWithLength` (allocate and fill via
    /// `contents()`). `contents()` returns the CPU mapping as a
    /// raw `i64` pointer; `length()` is the buffer size in bytes.
    @objc pub class MTLBuffer : NSObject {
        @objc("contents")
        pub contents(): i64
        @objc("length")
        pub bufferLength(): u64
    }

    /// `MTLHeapDescriptor` — pre-creation knobs for an `MTLHeap`.
    /// Set at minimum `size` and (usually) `storageMode`; leave
    /// `type` at the `.automatic` default unless you have a
    /// reason to manage offsets yourself. `setResourceOptions:`
    /// composes storage / cache / hazard-tracking bits in one
    /// call instead of three.
    @objc pub class MTLHeapDescriptor : NSObject {
        @objc("alloc")
        pub static alloc(): MTLHeapDescriptor
        @objc("init")
        pub init(): MTLHeapDescriptor

        @objc("setSize:")
        pub setSize(s: u64)
        @objc("setStorageMode:")
        pub setStorageMode(m: MTLStorageMode)
        @objc("setHazardTrackingMode:")
        pub setHazardTrackingMode(m: u64)
        @objc("setResourceOptions:")
        pub setResourceOptions(o: MTLResourceOptions)
        @objc("setType:")
        pub setType(t: MTLHeapType)
    }

    /// `id<MTLHeap>` — pooled GPU memory. Build via
    /// `device.newHeapWithDescriptor(desc)`, then carve out
    /// buffers / textures with `heap.newBuffer/Texture(...)`. The
    /// pool's lifetime owns every suballocation; releasing the
    /// heap frees everything it holds.
    ///
    /// `usedSize` is the running total of bytes allocated out of
    /// the heap; `currentAllocatedSize` is what the driver has
    /// physically reserved (≥ usedSize, may grow on demand for
    /// `.automatic` heaps). `maxAvailableSizeWithAlignment:`
    /// queries the largest contiguous block of free space.
    @objc pub class MTLHeap : NSObject {
        @objc("newBufferWithLength:options:")
        pub newBufferWithLength(
            length: u64, options: MTLResourceOptions
        ): MTLBuffer
        @objc("newTextureWithDescriptor:")
        pub newTextureWithDescriptor(d: MTLTextureDescriptor): MTLTexture

        @objc("size")
        pub size(): u64
        @objc("usedSize")
        pub usedSize(): u64
        @objc("currentAllocatedSize")
        pub currentAllocatedSize(): u64
        @objc("maxAvailableSizeWithAlignment:")
        pub maxAvailableSizeWithAlignment(align: u64): u64

        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("label")
        pub label(): NSString
    }

    /// `id<MTLFence>` — single-queue synchronisation token. One
    /// encoder calls `updateFence:` to "set" it after its work;
    /// a later encoder on the same queue calls `waitForFence:`
    /// to gate its work until the fence is set. Use this when an
    /// untracked-storage heap or a `useResource:` declared
    /// resource needs explicit ordering — automatic hazard
    /// tracking handles tracked resources for you. For
    /// cross-queue or CPU-side waits, reach for `MTLEvent` /
    /// `MTLSharedEvent` instead.
    @objc pub class MTLFence : NSObject {
        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("label")
        pub label(): NSString
    }
}
