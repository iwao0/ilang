/// t2embed.dll — TrueType font subsetting + embedding helpers.
///
/// Used by PDF / XPS writers to subset only the glyphs actually
/// used in a document. The subset is a self-contained .ttf blob
/// you can ship without licensing the full font.

pub enum TtEmbedResult: u32 {
    success                    = 0
    embedCmapTabError          = 0x1001
    embedNotEmbedFlag          = 0x1002
    embedNoXform               = 0x1003
    embedFailure               = 0x1004
    embedTtfTagNotFound        = 0x1005
    embedMoreThanOneOffset     = 0x1006
    embedInvalidParam          = 0x1007
    embedReadMissing           = 0x1008
}

@flags
pub enum TtEmbedFlags: u32 {
    embedRaw            = 0x00000000
    embedSubset         = 0x00000001
    embedCompressed     = 0x00000004
    embedFailIfVariation = 0x00000010
    embedNoSubsetting   = 0x00000020
    embedAllowSubsetting = 0x00000040
    embedReleaseSpecial = 0x00000080
}

@extern(C, "t2embed") {
    /// `TTEmbedFont(hDC, flags, charSet, writeStatus, ulPrivStatus,
    ///              writeFn, refConst, usSubsetChars, count,
    ///              langCount, pBuffer, &pulBufferSize, pUsage)`.
    ///
    /// Streams the subsetted font through `writeFn`; the function
    /// pointer is called repeatedly until the entire blob is
    /// consumed.
    @lib pub fn TTEmbedFont(
        hDC:            i64,                   // HDC
        ulFlags:        u32,
        ulCharSet:      u32,
        pulPrivStatus:  *u32,
        pulStatus:      *u32,
        lpfnWriteToStream: fn(*const void, u32, u32): u32,
        lpvWriteStream: *void,
        pusCharCodeSet: *const u16,
        usCharCodeCount: u16,
        usLanguage:     u16,
        pTTEmbedInfo:   *void                  // *TTEMBEDINFO (optional)
    ): u32

    /// Variant that writes the subset to a file by handle / path.
    @lib pub fn TTEmbedFontFromFileA(
        hDC:            i64,
        szFontFileName: *const char,
        usTTCIndex:     u16,
        ulFlags:        u32,
        ulCharSet:      u32,
        pulPrivStatus:  *u32,
        pulStatus:      *u32,
        lpfnWriteToStream: fn(*const void, u32, u32): u32,
        lpvWriteStream: *void,
        pusCharCodeSet: *const u16,
        usCharCodeCount: u16,
        usLanguage:     u16,
        pTTEmbedInfo:   *void
    ): u32

    /// Load an embedded font into the current process. Pair with
    /// `TTDeleteEmbeddedFont`.
    @lib pub fn TTLoadEmbeddedFont(
        phFontReference: *i64,
        ulFlags:         u32,
        pulPrivStatus:   *u32,
        ulPrivs:         u32,
        pulStatus:       *u32,
        lpfnReadFromStream: fn(*void, u32, u32): u32,
        lpvReadStream:   *void,
        szWinFamilyName: *const u16,
        szMacFamilyName: *const char,
        pTTLoadInfo:     *void
    ): u32

    @lib pub fn TTDeleteEmbeddedFont(
        hFontReference:  i64,
        ulFlags:         u32,
        pulStatus:       *u32
    ): u32

    /// Subset-only — given an HDC selecting a font, drop everything
    /// not in `pusCharCodeSet`.
    @lib pub fn CreateFontPackage(
        puchSrcBuffer:   *const u8,
        ulSrcBufferSize: u32,
        ppuchFontPackageBuffer: **u8,
        pulFontPackageBufferSize: *u32,
        pulBytesWritten: *u32,
        usFlag:          u16,
        usTTCIndex:      u16,
        usSubsetFormat:  u16,
        usSubsetLanguage: u16,
        usSubsetPlatform: u16,
        usSubsetEncoding: u16,
        pusSubsetKeepList: *const u16,
        usSubsetListCount: u16,
        lpfnAllocate:    fn(u64): *void,
        lpfnReAllocate:  fn(*void, u64): *void,
        lpfnFree:        fn(*void),
        lpvReserved:     *void
    ): u32

    @lib pub fn MergeFontPackage(
        puchMergeFontBuffer: *const u8,
        ulMergeFontBufferSize: u32,
        puchFontPackageBuffer: *const u8,
        ulFontPackageBufferSize: u32,
        ppuchDestBuffer: **u8,
        pulDestBufferSize: *u32,
        pulBytesWritten: *u32,
        usMode:          u16,
        lpfnAllocate:    fn(u64): *void,
        lpfnReAllocate:  fn(*void, u64): *void,
        lpfnFree:        fn(*void),
        lpvReserved:     *void
    ): u32
}
