/// mfplat.dll — Media Foundation platform layer.
///
/// Setup:
///   MFStartup(MF_VERSION, MFSTARTUP_FULL)   // exactly once
///   ... use the rest of the MF surface ...
///   MFShutdown()
///
/// Companion DLLs commonly bound alongside:
///   mfreadwrite — IMFSourceReader / IMFSinkWriter (separate file)
///   mf          — IMFMediaSession / IMFTopology (legacy session API)
///
/// `MF_VERSION` = 0x00020070 (== MF_SDK_VERSION 2, MF_API_VERSION 0x70).
///
/// Heavy COM surface is deferred. This file binds platform-level
/// entry points + key constants.

pub enum MfStartupFlags: u32 {
    full = 0
    lite = 1
}

@extern(C, "mfplat") {
    /// One-time platform init. Returns S_OK.
    @lib pub fn MFStartup(Version: u32, dwFlags: u32): i32

    /// Tear down the platform.
    @lib pub fn MFShutdown(): i32

    /// Allocate an `IMFAttributes` collection with initial capacity.
    @lib pub fn MFCreateAttributes(
        ppMFAttributes: **void,
        cInitialSize:   u32
    ): i32

    /// Create a memory buffer (`IMFMediaBuffer`) of `cbMaxLength` bytes.
    @lib pub fn MFCreateMemoryBuffer(
        cbMaxLength: u32,
        ppBuffer:    **void
    ): i32

    /// Wrap an aligned buffer (alignment must be a power of two).
    @lib pub fn MFCreateAlignedMemoryBuffer(
        cbMaxLength:   u32,
        cbAligment:    u32,    // 0 = 16-byte
        ppBuffer:      **void
    ): i32

    /// Create an `IMFSample` (envelopes one or more `IMFMediaBuffer`).
    @lib pub fn MFCreateSample(ppIMFSample: **void): i32

    /// Create a media type (`IMFMediaType`).
    @lib pub fn MFCreateMediaType(ppMFType: **void): i32

    /// Open / create a Media Foundation source from a URL.
    @lib pub fn MFCreateSourceResolver(ppISourceResolver: **void): i32

    /// Decompose a media type into the (major, sub) pair.
    @lib pub fn MFGetAttributeRatio(
        pAttributes: *void,
        guidKey:     *const void,
        pnNumerator: *u32,
        pnDenominator: *u32
    ): i32

    @lib pub fn MFGetAttributeSize(
        pAttributes: *void,
        guidKey:     *const void,
        punWidth:    *u32,
        punHeight:   *u32
    ): i32

    @lib pub fn MFSetAttributeRatio(
        pAttributes: *void,
        guidKey:     *const void,
        unNumerator: u32,
        unDenominator: u32
    ): i32

    @lib pub fn MFSetAttributeSize(
        pAttributes: *void,
        guidKey:     *const void,
        unWidth:     u32,
        unHeight:    u32
    ): i32

    /// Create a system-time clock used for AV sync.
    @lib pub fn MFCreateSystemTimeSource(ppSystemTimeSource: **void): i32

    /// Lock the work queue used by async callbacks.
    @lib pub fn MFLockPlatform(): i32
    @lib pub fn MFUnlockPlatform(): i32
}
