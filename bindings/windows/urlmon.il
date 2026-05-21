/// urlmon.dll — URL monikers, URL → file download, MIME detection.
///
/// Two practical entry points carry most cases:
///   * `URLDownloadToFileA` — one-shot file download (HTTP / HTTPS /
///     FTP / file:// supported).
///   * `FindMimeFromData` — sniff the MIME type of an in-memory blob.

@extern(C, "urlmon") {
    /// `URLDownloadToFileA(pCaller, szURL, szFileName, dwReserved, lpfnCB)`.
    /// Returns S_OK (0) on success; an HRESULT on failure. The
    /// progress callback (an `IBindStatusCallback`) is usually 0.
    @lib pub fn URLDownloadToFileA(
        pCaller:    *void,           // *IUnknown — typically 0
        szURL:      *const char,
        szFileName: *const char,
        dwReserved: u32,
        lpfnCB:     *void            // *IBindStatusCallback — typically 0
    ): i32

    @lib pub fn URLDownloadToFileW(
        pCaller:    *void,
        szURL:      *const u16,
        szFileName: *const u16,
        dwReserved: u32,
        lpfnCB:     *void
    ): i32

    /// Same but writes into a heap-allocated `*BYTE` buffer published
    /// via `STGMEDIUM`. Rarely used in modern code — prefer winhttp.
    @lib pub fn URLOpenBlockingStreamA(
        pCaller:    *void,
        szURL:      *const char,
        ppStream:   **void,          // *IStream
        dwReserved: u32,
        lpfnCB:     *void
    ): i32

    /// MIME sniffing. `pBC` may be 0; `pwzUrl` is optional context;
    /// `pBuffer` (`cbSize` bytes) is the sniffed prefix. Returns
    /// the negotiated MIME type as a wide string in `*ppwzMimeOut`
    /// (caller frees with `CoTaskMemFree`).
    @lib pub fn FindMimeFromData(
        pBC:           *void,
        pwzUrl:        *const u16,
        pBuffer:       *const void,
        cbSize:        u32,
        pwzMimeProposed: *const u16,
        dwMimeFlags:   u32,
        ppwzMimeOut:   **u16,
        dwReserved:    u32
    ): i32

    /// Canonicalise a URL string. `cbSize` is in-out: cap on input,
    /// actual on output.
    @lib pub fn CoInternetParseUrl(
        pwzUrl:        *const u16,
        ParseAction:   u32,
        dwFlags:       u32,
        pszResult:     *u16,
        cchResult:     u32,
        pcchResult:    *u32,
        dwReserved:    u32
    ): i32

    /// Combine a base + relative URL.
    @lib pub fn CoInternetCombineUrl(
        pwzBaseUrl:    *const u16,
        pwzRelativeUrl: *const u16,
        dwCombineFlags: u32,
        pszResult:     *u16,
        cchResult:     u32,
        pcchResult:    *u32,
        dwReserved:    u32
    ): i32

    /// HRESULT description ("the resource cannot be found", …).
    /// `dwReserved` is unused.
    @lib pub fn ObtainUserAgentString(
        dwOption:        u32,
        pszUAOut:        *char,
        cbSize:          *u32
    ): i32
}
