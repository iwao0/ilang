/// cabinet.dll — Microsoft Compression / Decompression API
/// (the modern `Compress*` family, plus the legacy CAB extraction
/// entry points).
///
/// `Compress*` (Win8+) handles MSZIP / XPRESS / XPRESS_HUFF /
/// LZMS algorithms. Setup pattern:
///
///   let h: i64 = 0
///   CreateCompressor(CompressAlgo.xpressHuff as u32, NULL, &h)
///   Compress(h, src, srcSize, dst, dstSize, &actualSize)
///   CloseCompressor(h)

// ─── COMPRESS_ALGORITHM ──────────────────────────────────────────
pub enum CompressAlgo: u32 {
    invalid    = 0
    null_      = 1    // bypass; copies as-is
    mszip      = 2
    xpress     = 3
    xpressHuff = 4
    lzms       = 5
}

@extern(C, "Cabinet") {
    /// Open a compression handle. Returns non-zero on success;
    /// `*compressorHandle` receives the opaque handle.
    /// `allocationRoutines` is `*COMPRESS_ALLOCATION_ROUTINES`
    /// or NULL for the default heap.
    @lib pub fn CreateCompressor(
        algorithm:           u32,
        allocationRoutines:  *void,
        compressorHandle:    *i64
    ): i32

    /// Open a decompression handle.
    @lib pub fn CreateDecompressor(
        algorithm:           u32,
        allocationRoutines:  *void,
        decompressorHandle:  *i64
    ): i32

    /// `Compress(h, src, srcSize, dst, dstSize, &compressedSize)`.
    /// Returns non-zero on success. `compressedSize` is in/out:
    /// in = capacity of `dst`, out = bytes written.
    @lib pub fn Compress(
        compressorHandle: i64,
        uncompressedData: *const void,
        uncompressedDataSize: u64,
        compressedBuffer: *void,
        compressedBufferSize: u64,
        compressedDataSize: *u64
    ): i32

    /// Inverse — fills `uncompressedBuffer` from the compressed
    /// `compressedData`.
    @lib pub fn Decompress(
        decompressorHandle: i64,
        compressedData:     *const void,
        compressedDataSize: u64,
        uncompressedBuffer: *void,
        uncompressedBufferSize: u64,
        uncompressedDataSize: *u64
    ): i32

    /// Reset between independent payloads. Cheaper than
    /// destroying + recreating the handle.
    @lib pub fn ResetCompressor(compressorHandle: i64): i32
    @lib pub fn ResetDecompressor(decompressorHandle: i64): i32

    @lib pub fn CloseCompressor(compressorHandle: i64): i32
    @lib pub fn CloseDecompressor(decompressorHandle: i64): i32

    /// Query algorithm-specific information. `information`:
    ///   0 COMPRESS_INFORMATION_CLASS_BLOCK_SIZE
    @lib pub fn QueryCompressorInformation(
        compressorHandle: i64,
        information:      u32,
        informationData:  *void,
        informationSize:  u64
    ): i32

    @lib pub fn QueryDecompressorInformation(
        decompressorHandle: i64,
        information:        u32,
        informationData:    *void,
        informationSize:    u64
    ): i32

    /// Set algorithm-specific tunables (block size etc.).
    @lib pub fn SetCompressorInformation(
        compressorHandle: i64,
        information:      u32,
        informationData:  *const void,
        informationSize:  u64
    ): i32

    // ── Legacy CAB extraction (SETUP API) ───────────────────────

    /// `Extract(pSes, pszSource)` — extract a .cab. `pSes` points
    /// at a SESSION struct describing the destination layout.
    /// Returns 0 on success.
    @lib pub fn Extract(pSes: *void, pszSource: *const char): u32
}
