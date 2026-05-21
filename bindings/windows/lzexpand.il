/// lz32.dll — legacy LZ (Lempel-Ziv) decompression.
///
/// Used by InfoTip / .HLP / .CAB-style files compressed with the
/// 1980s-vintage Microsoft LZ scheme. Most modern code reaches for
/// `cabinet.dll`'s `Compress*` (XPRESS / LZMS) instead — this file
/// covers older `.CPL` / `.HLP` / `.OL$` files still shipped by
/// some installers.

@extern(C, "lz32") {
    /// `LZOpenFileA(name, lpReOpenBuf, fMode)`. `fMode`:
    ///   OF_READ    = 0
    ///   OF_WRITE   = 1
    ///   OF_READWRITE = 2
    ///   OF_SHARE_COMPAT = 0
    ///   OF_SHARE_EXCLUSIVE = 0x10
    /// Returns the new HFILE (i32) or LZERROR_* (negative).
    @lib pub fn LZOpenFileA(
        lpFileName:  *const char,
        lpReOpenBuf: *void,         // *OFSTRUCT (136 bytes); pass a zeroed blob
        wStyle:      u16
    ): i32

    @lib pub fn LZOpenFileW(
        lpFileName:  *const u16,
        lpReOpenBuf: *void,
        wStyle:      u16
    ): i32

    @lib pub fn LZClose(hFile: i32)

    @lib pub fn LZRead(
        hFile:        i32,
        lpBuffer:     *char,
        cbRead:       i32
    ): i32

    /// Seek inside the *uncompressed* stream. `iOrigin`:
    ///   FILE_BEGIN 0, FILE_CURRENT 1, FILE_END 2.
    @lib pub fn LZSeek(
        hFile:    i32,
        lOffset:  i32,
        iOrigin:  i32
    ): i32

    /// Copy + decompress an entire file from `hfSource` to `hfDest`.
    /// Returns the number of bytes written, or LZERROR_*.
    @lib pub fn LZCopy(hfSource: i32, hfDest: i32): i32

    /// Initialise a 4096-byte decompression buffer. Most callers
    /// don't need to invoke this directly — `LZRead` handles it.
    @lib pub fn LZInit(hfSource: i32): i32

    /// `GetExpandedNameA(szSource, szBuffer)` — strip the `_` from
    /// a compressed file name. (`MYFILE.EX_` → `MYFILE.EXE`.)
    @lib pub fn GetExpandedNameA(
        lpszSource:  *const char,
        lpszBuffer:  *char
    ): i32

    @lib pub fn GetExpandedNameW(
        lpszSource:  *const u16,
        lpszBuffer:  *u16
    ): i32
}
