/// mscms.dll — Microsoft Color Management System.
///
/// Manage ICC color profiles and transform colors between them.
/// Used by printing, photo apps, and high-gamut display tooling.
///
/// Profile lifecycle:
///   let prof = OpenColorProfileA(&PROFILE, ProfileRead, FileMode.share, OpenExisting)
///   GetColorProfileHeader(prof, &header)
///   CloseColorProfile(prof)

// ─── PROFILE_TYPE — used inside PROFILE.dwType ───────────────────
pub enum ProfileType: u32 {
    fileName   = 1
    memBuf     = 2
}

// ─── PROFILE access ─────────────────────────────────────────────
@flags
pub enum ProfileAccess: u32 {
    read       = 0x1
    readWrite  = 0x2
}

// ─── BMFORMAT — bitmap formats fed to TranslateBitmapBits ────────
pub enum BmFormat: i32 {
    rgbTriplets    = 0x0002
    bgrTriplets    = 0x0004
    xRgbQuads      = 0x0006
    bgrxQuads      = 0x0008
    cmykQuads      = 0x000A
    rgbPacked555   = 0x0010
    rgbPacked565   = 0x0014
    rgbaPacked     = 0x0024
}

// ─── COLORTYPE — ICC color space ─────────────────────────────────
pub enum ColorType: u32 {
    gray   = 1
    rgb    = 2
    xyz    = 3
    yxy    = 4
    lab    = 5
    threeChannels = 6
    cmyk   = 7
    fiveChannels = 8
    sixChannels  = 9
    sevenChannels = 10
    eightChannels = 11
    named  = 12
}

@extern(C, "mscms") {
    /// File-based profile descriptor — set `dwType = fileName` and
    /// `pProfileData = path`, `cbDataSize = byte length of path`.
    pub struct PROFILE {
        dwType:      u32
        _pad:        u32
        pProfileData: *void       // path or memory
        cbDataSize:  u32
        _pad2:       u32
    }

    /// `OpenColorProfileA(profile, access, shareMode, creationMode)`.
    /// `access` is `ProfileAccess.*`; `shareMode` is FILE_SHARE_*;
    /// `creationMode` is OPEN_EXISTING (3) typically. Returns
    /// HPROFILE (cast to i64), or 0 on failure.
    @lib pub fn OpenColorProfileA(
        pProfile:     *const PROFILE,
        dwDesiredAccess: u32,
        dwShareMode:  u32,
        dwCreationMode: u32
    ): i64

    @lib pub fn OpenColorProfileW(
        pProfile:     *const PROFILE,
        dwDesiredAccess: u32,
        dwShareMode:  u32,
        dwCreationMode: u32
    ): i64

    @lib pub fn CloseColorProfile(hProfile: i64): i32

    /// Read the 128-byte ICC profile header.
    @lib pub fn GetColorProfileHeader(
        hProfile: i64,
        pHeader:  *void          // *PROFILEHEADER (128 bytes)
    ): i32

    @lib pub fn SetColorProfileHeader(
        hProfile: i64,
        pHeader:  *const void
    ): i32

    /// Get the system's default sRGB display profile path.
    @lib pub fn GetStandardColorSpaceProfileA(
        pMachineName:  *const char,
        dwSCS:         u32,            // 1=sRGB, 2=AdobeRGB(98), …
        pBuffer:       *char,
        pdwSize:       *u32
    ): i32

    /// Default profile for `dwType`:
    ///   1=monitor, 2=printer, 3=scanner.
    @lib pub fn InstallColorProfileA(
        pMachineName:  *const char,
        pProfileName:  *const char
    ): i32

    @lib pub fn UninstallColorProfileA(
        pMachineName:  *const char,
        pProfileName:  *const char,
        bDelete:       i32
    ): i32

    /// Per-device default profile.
    @lib pub fn AssociateColorProfileWithDeviceA(
        pMachineName:  *const char,
        pProfileName:  *const char,
        pDeviceName:   *const char
    ): i32

    @lib pub fn DisassociateColorProfileFromDeviceA(
        pMachineName:  *const char,
        pProfileName:  *const char,
        pDeviceName:   *const char
    ): i32

    // ── Transforms ──────────────────────────────────────────────

    /// `CreateMultiProfileTransform(profiles, n, intents, n,
    ///                              flags, indexPreferredCMM)` —
    /// builds an HTRANSFORM for the chain.
    /// Intent codes:
    ///   0 INTENT_PERCEPTUAL
    ///   1 INTENT_RELATIVE_COLORIMETRIC
    ///   2 INTENT_SATURATION
    ///   3 INTENT_ABSOLUTE_COLORIMETRIC
    @lib pub fn CreateMultiProfileTransform(
        pahProfiles:   *i64,
        nProfiles:     u32,
        padwIntent:    *u32,
        nIntents:      u32,
        dwFlags:       u32,
        indexPreferredCMM: u32
    ): i64

    @lib pub fn DeleteColorTransform(hxform: i64): i32

    /// Apply the transform to a 2-D bitmap of `cPixels` per row.
    @lib pub fn TranslateBitmapBits(
        hColorTransform: i64,
        pSrcBits:        *const void,
        bmInput:         i32,           // BmFormat
        dwWidth:         u32,
        dwHeight:        u32,
        dwInputStride:   u32,
        pDestBits:       *void,
        bmOutput:        i32,
        dwOutputStride:  u32,
        pfnCallBack:     *void,         // optional progress callback
        ulCallbackData:  u64
    ): i32

    /// Apply the transform to a flat array of colour values.
    @lib pub fn TranslateColors(
        hColorTransform: i64,
        paInputColors:   *const void,
        nColors:         u32,
        ctInput:         u32,           // ColorType
        paOutputColors:  *void,
        ctOutput:        u32
    ): i32
}
