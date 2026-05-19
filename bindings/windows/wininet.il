// wininet.dll — WinINet: classic HTTP/HTTPS/FTP/Gopher client API.
//
// For new code prefer WinHTTP (winhttp.il) — WinINet integrates with
// IE settings and is more suitable for user-mode interactive apps.
//
// Usage:
//   let h  = InternetOpenA("agent", AccessType.preconfig as u32, NULL, NULL, 0)
//   let c  = InternetConnectA(h, "host", 80, NULL, NULL, Service.http as u32, 0, 0)
//   let r  = HttpOpenRequestA(c, "GET", "/path", NULL, NULL, NULL, OpenFlags.secure as u32, 0)
//   HttpSendRequestA(r, NULL, 0, NULL, 0)
//   InternetReadFile(r, buf, len, &read)
//   InternetCloseHandle(r); InternetCloseHandle(c); InternetCloseHandle(h)

// ─── InternetOpen access type (INTERNET_OPEN_TYPE_*) ─────────────
pub enum AccessType: u32 {
    preconfig         = 0   // use IE proxy settings (most common)
    direct            = 1   // bypass proxy
    proxy             = 3   // use the proxy specified in lpszProxy
    preconfigWithNoAutoproxy = 4
}

// ─── InternetConnect service type (INTERNET_SERVICE_*) ───────────
pub enum Service: u32 {
    ftp     = 1
    gopher  = 2
    http    = 3
}

// ─── HttpOpenRequest / InternetOpenUrl flags (INTERNET_FLAG_*) ───
@flags
pub enum OpenFlags: u32 {
    reload              = 0x80000000   // get from server, not cache
    rawData             = 0x40000000
    existingConnect     = 0x20000000
    async               = 0x10000000
    passive             = 0x08000000   // FTP: passive mode
    noCacheWrite        = 0x04000000
    makePersistent      = 0x02000000
    fromCache           = 0x01000000
    secure              = 0x00800000   // HTTPS
    keepConnection      = 0x00400000
    noAutoRedirect      = 0x00200000   // don't follow 3xx
    readPrefetch        = 0x00100000
    noCookies           = 0x00080000
    noAuth              = 0x00040000
    restrictedZone      = 0x00020000
    cacheIfNetFail      = 0x00010000
    ignoreRedirectToHttp  = 0x00008000
    ignoreRedirectToHttps = 0x00004000
    ignoreCertDateInvalid = 0x00002000
    ignoreCertCnInvalid   = 0x00001000
    resynchronize       = 0x00000800
    hyperlink           = 0x00000400
    needFile            = 0x00000200
    transferAscii       = 0x00000001
    transferBinary      = 0x00000002
}

// ─── HttpQueryInfo info levels (HTTP_QUERY_*) ────────────────────
pub enum HttpQuery: u32 {
    mimeVersion       = 0
    contentType       = 1
    contentTransferEncoding = 2
    contentId         = 3
    contentDescription = 4
    contentLength     = 5
    contentLanguage   = 6
    allow             = 7
    public            = 8
    date              = 9
    expires           = 10
    lastModified      = 11
    messageId         = 12
    uri               = 13
    derivedFrom       = 14
    cost              = 15
    link              = 16
    pragma            = 17
    version           = 18
    statusCode        = 19
    statusText        = 20
    rawHeaders        = 21
    rawHeadersCrlf    = 22
    connection        = 23
    accept            = 24
    acceptCharset     = 25
    acceptEncoding    = 26
    acceptLanguage    = 27
    authorization     = 28
    contentEncoding   = 29
    forwarded         = 30
    from              = 31
    ifModifiedSince   = 32
    location          = 33
    orig_uri          = 34
    referer           = 35
    retryAfter        = 36
    server            = 37
    title             = 38
    userAgent         = 39
    wwwAuthenticate   = 40
    proxyAuthenticate = 41
    acceptRanges      = 42
    setCookie         = 43
    cookie            = 44
    requestMethod     = 45
    /// Flag: combine to receive a u32 status code instead of a string.
    flagNumber        = 0x20000000
    /// Flag: combine to receive a SYSTEMTIME (16 bytes).
    flagSystemtime    = 0x40000000
    /// Flag: request a custom header by name; lpvBuffer holds the name on input.
    flagCustom        = 0x10000000
}

// ─── INTERNET_OPTION_* values ────────────────────────────────────
pub enum InternetOption: u32 {
    callback                = 1
    connectTimeout          = 2     // ms
    receiveTimeout          = 6
    sendTimeout             = 5
    controlReceiveTimeout   = 6
    controlSendTimeout      = 5
    dataReceiveTimeout      = 4
    dataSendTimeout         = 3
    handleType              = 9
    listenTimeout           = 11
    readBufferSize          = 12
    writeBufferSize         = 13
    asyncId                 = 15
    asyncPriority           = 16
    parentHandle            = 21
    keepConnection          = 22
    requestFlags            = 23
    extendedError           = 24
    offline                 = 26
    cacheStreamHandle       = 27
    username                = 28
    password                = 29
    asyncTimeout            = 30
    securityFlags           = 31
    securityCertificateStruct = 32
    datafileName            = 33
    url                     = 34
    securityCertificate     = 35
    securityKeyBitness      = 36
    refresh                 = 37
    proxy                   = 38
    settingsChanged         = 39
    version                 = 40
    userAgent               = 41
    httpVersion             = 59
}

@extern(C, "wininet") {
    // ── Opaque handle ─────────────────────────────────────────────

    /// WinINet handle (HINTERNET). Closed with InternetCloseHandle.
    pub struct HINTERNET {}

    // ── INTERNET_BUFFERSA — for HttpSendRequestEx (rarely needed) ──

    pub struct INTERNET_BUFFERSA {
        dwStructSize:        u32
        // 4 bytes implicit padding
        Next:                *INTERNET_BUFFERSA
        lpcszHeader:         *const char
        dwHeadersLength:     u32
        dwHeadersTotal:      u32
        lpvBuffer:           *void
        dwBufferLength:      u32
        dwBufferTotal:       u32
        dwOffsetLow:         u32
        dwOffsetHigh:        u32
    }

    // ── Session lifecycle ─────────────────────────────────────────

    /// Initialises an internet session.
    /// `lpszAgent`: user-agent string sent to servers.
    /// `dwAccessType`: AccessType.*.
    /// `lpszProxy` / `lpszProxyBypass`: only used with AccessType.proxy.
    @lib pub fn InternetOpenA(
        lpszAgent:       *const char,
        dwAccessType:    u32,
        lpszProxy:       *const char,
        lpszProxyBypass: *const char,
        dwFlags:         u32
    ): *HINTERNET

    /// Connects to an HTTP/FTP/Gopher server.
    /// `nServerPort`: 0 = default for service; 80 / 443 / 21 typical.
    /// `dwService`: Service.http etc.
    @lib pub fn InternetConnectA(
        hInternet:    *HINTERNET,
        lpszServerName: *const char,
        nServerPort:    u16,
        lpszUserName:   *const char,
        lpszPassword:   *const char,
        dwService:      u32,
        dwFlags:        u32,
        dwContext:      u64
    ): *HINTERNET

    /// Closes a session, connection, or request handle.
    @lib pub fn InternetCloseHandle(hInternet: *HINTERNET): i32

    // ── Direct URL fetch ──────────────────────────────────────────

    /// One-shot fetch.  `lpszHeaders`: optional request headers
    /// (e.g. "Accept: text/html\r\n"); `dwHeadersLength`: -1 = compute
    /// via lstrlen.  `dwFlags`: OpenFlags.*.
    @lib pub fn InternetOpenUrlA(
        hInternet:     *HINTERNET,
        lpszUrl:       *const char,
        lpszHeaders:   *const char,
        dwHeadersLength: u32,
        dwFlags:       u32,
        dwContext:     u64
    ): *HINTERNET

    /// Reads up to `dwNumberOfBytesToRead` bytes into `lpBuffer`.
    /// `lpdwNumberOfBytesRead` receives the actual count.
    /// Returns 0 = end of file, non-zero = more data available.
    @lib pub fn InternetReadFile(
        hFile:                  *HINTERNET,
        lpBuffer:               *void,
        dwNumberOfBytesToRead:  u32,
        lpdwNumberOfBytesRead:  *u32
    ): i32

    @lib pub fn InternetWriteFile(
        hFile:                  *HINTERNET,
        lpBuffer:               *const void,
        dwNumberOfBytesToWrite: u32,
        lpdwNumberOfBytesWritten: *u32
    ): i32

    @lib pub fn InternetQueryDataAvailable(
        hFile:               *HINTERNET,
        lpdwNumberOfBytesAvailable: *u32,
        dwFlags:             u32,
        dwContext:           u64
    ): i32

    // ── HTTP-specific ─────────────────────────────────────────────

    /// Builds (does not send) an HTTP request handle.
    /// `lpszVerb`: "GET", "POST", "PUT", "DELETE", NULL = GET.
    /// `lpszVersion`: NULL = HTTP/1.1.
    /// `lplpszAcceptTypes`: NULL-terminated array of accept-type strings, or NULL.
    @lib pub fn HttpOpenRequestA(
        hConnect:           *HINTERNET,
        lpszVerb:           *const char,
        lpszObjectName:     *const char,
        lpszVersion:        *const char,
        lpszReferer:        *const char,
        lplpszAcceptTypes:  **const char,
        dwFlags:            u32,
        dwContext:          u64
    ): *HINTERNET

    /// Sends the request. `lpszHeaders` / `dwHeadersLength`:
    /// additional headers. `lpOptional` / `dwOptionalLength`: body data
    /// (e.g. POST data).  Pass `0 as *void` and 0 for no body.
    @lib pub fn HttpSendRequestA(
        hRequest:        *HINTERNET,
        lpszHeaders:     *const char,
        dwHeadersLength: u32,
        lpOptional:      *void,
        dwOptionalLength: u32
    ): i32

    /// Adds, replaces, or removes a request header before the request
    /// is sent. `dwModifiers`: HTTP_ADDREQ_FLAG_ADD = 0x20000000, etc.
    @lib pub fn HttpAddRequestHeadersA(
        hRequest:    *HINTERNET,
        lpszHeaders: *const char,
        dwHeadersLength: u32,
        dwModifiers: u32
    ): i32

    /// Queries response info. `dwInfoLevel`: HttpQuery.*.
    /// `lpvBuffer` / `lpdwBufferLength`: output buffer for the value.
    /// `lpdwIndex`: 0 for first instance; receives index for next call.
    @lib pub fn HttpQueryInfoA(
        hRequest:      *HINTERNET,
        dwInfoLevel:   u32,
        lpvBuffer:     *void,
        lpdwBufferLength: *u32,
        lpdwIndex:     *u32
    ): i32

    // ── FTP ───────────────────────────────────────────────────────

    @lib pub fn FtpGetFileA(
        hConnect:      *HINTERNET,
        lpszRemoteFile: *const char,
        lpszNewFile:    *const char,
        fFailIfExists:  i32,
        dwFlagsAndAttributes: u32,
        dwFlags:        u32,
        dwContext:      u64
    ): i32

    @lib pub fn FtpPutFileA(
        hConnect:      *HINTERNET,
        lpszLocalFile:  *const char,
        lpszNewRemoteFile: *const char,
        dwFlags:        u32,
        dwContext:      u64
    ): i32

    @lib pub fn FtpDeleteFileA(hConnect: *HINTERNET, lpszFileName: *const char): i32
    @lib pub fn FtpRenameFileA(
        hConnect:      *HINTERNET,
        lpszExisting:  *const char,
        lpszNew:       *const char
    ): i32

    @lib pub fn FtpCreateDirectoryA(hConnect: *HINTERNET, lpszDirectory: *const char): i32
    @lib pub fn FtpRemoveDirectoryA(hConnect: *HINTERNET, lpszDirectory: *const char): i32
    @lib pub fn FtpSetCurrentDirectoryA(hConnect: *HINTERNET, lpszDirectory: *const char): i32
    @lib pub fn FtpGetCurrentDirectoryA(
        hConnect:      *HINTERNET,
        lpszCurrentDirectory: *char,
        lpdwCurrentDirectory: *u32
    ): i32

    // ── Cookies ───────────────────────────────────────────────────

    @lib pub fn InternetGetCookieA(
        lpszUrl:        *const char,
        lpszCookieName: *const char,
        lpszCookieData: *char,
        lpdwSize:       *u32
    ): i32

    @lib pub fn InternetSetCookieA(
        lpszUrl:        *const char,
        lpszCookieName: *const char,
        lpszCookieData: *const char
    ): i32

    // ── Options ───────────────────────────────────────────────────

    @lib pub fn InternetSetOptionA(
        hInternet:   *HINTERNET,
        dwOption:    u32,
        lpBuffer:    *const void,
        dwBufferLength: u32
    ): i32

    @lib pub fn InternetQueryOptionA(
        hInternet:   *HINTERNET,
        dwOption:    u32,
        lpBuffer:    *void,
        lpdwBufferLength: *u32
    ): i32

    // ── Cache ─────────────────────────────────────────────────────

    /// Deletes a cached resource by URL.
    @lib pub fn DeleteUrlCacheEntryA(lpszUrlName: *const char): i32

    // ── Error retrieval ───────────────────────────────────────────

    /// Retrieves the last extended error info, with optional buffer
    /// describing the error string.
    @lib pub fn InternetGetLastResponseInfoA(
        lpdwError:     *u32,
        lpszBuffer:    *char,
        lpdwBufferLength: *u32
    ): i32

    // ── Status callback ───────────────────────────────────────────

    /// Registers a callback that is invoked for status notifications.
    @lib pub fn InternetSetStatusCallback(
        hInternet:       *HINTERNET,
        lpfnInternetCallback: *void
    ): *void
}
