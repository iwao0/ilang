// winhttp.dll — modern HTTP/HTTPS client.
//
// Unlike WinINet, WinHTTP is service-friendly (no user-interaction,
// no per-user state), supports modern TLS, and is the recommended
// API for server-side and unattended HTTP clients.
//
// Usage:
//   let h = WinHttpOpen(L"agent", AccessType.defaultProxy as u32, NULL, NULL, 0)
//   let c = WinHttpConnect(h, L"host.example", 443, 0)
//   let r = WinHttpOpenRequest(c, L"GET", L"/path", NULL, NULL, NULL,
//                              OpenRequestFlags.secure as u32)
//   WinHttpSendRequest(r, NULL, 0, NULL, 0, 0, 0)
//   WinHttpReceiveResponse(r, NULL)
//   WinHttpReadData(r, buf, len, &read)
//   WinHttpCloseHandle(r); WinHttpCloseHandle(c); WinHttpCloseHandle(h)
//
// All strings are UTF-16 (`*const u16`) — WinHTTP has no ANSI variants.

// ─── WinHttpOpen access type (WINHTTP_ACCESS_TYPE_*) ─────────────
pub enum WinHttpAccessType: u32 {
    defaultProxy = 0    // honour the system default (recommended)
    noProxy      = 1
    namedProxy   = 3
    automaticProxy = 4   // Windows 8.1+
}

// ─── WinHttpOpenRequest flags ────────────────────────────────────
@flags
pub enum WinHttpOpenRequestFlags: u32 {
    secure                = 0x00800000   // HTTPS
    bypassProxyCache      = 0x00000100
    refresh               = 0x00100000
    escapePercent         = 0x00000004
    escapeDisable         = 0x00000040
    escapeDisableQuery    = 0x00000080
    nullCodepage          = 0x00000008
}

// ─── WinHttpQueryHeaders info levels (WINHTTP_QUERY_*) ───────────
pub enum WinHttpQuery: u32 {
    mimeVersion           = 0
    contentType           = 1
    contentLength         = 5
    contentEncoding       = 29
    date                  = 9
    expires               = 10
    lastModified          = 11
    pragma                = 17
    version               = 18
    statusCode            = 19
    statusText            = 20
    rawHeaders            = 21
    rawHeadersCrlf        = 22
    connection            = 23
    accept                = 24
    acceptEncoding        = 26
    acceptLanguage        = 27
    authorization         = 28
    location              = 33
    referer               = 35
    server                = 37
    userAgent             = 39
    wwwAuthenticate       = 40
    proxyAuthenticate     = 41
    acceptRanges          = 42
    setCookie             = 43
    cookie                = 44
    requestMethod         = 45
    /// Combine to receive a u32 instead of a string.
    flagNumber            = 0x20000000
    flagSystemtime        = 0x40000000
    /// Custom header — lpvBuffer holds the name on input.
    flagCustom            = 0x10000000
}

// ─── WinHttpSetOption / QueryOption identifiers ──────────────────
pub enum WinHttpOption: u32 {
    callback              = 1
    resolveTimeout        = 2
    connectTimeout        = 3
    connectRetries        = 4
    sendTimeout           = 5
    receiveTimeout        = 6
    receiveResponseTimeout = 7
    handleType            = 9
    readBufferSize        = 12
    writeBufferSize       = 13
    parentHandle          = 21
    extendedError         = 24
    securityFlags         = 31
    proxy                 = 38
    userAgent             = 41
    httpVersion           = 59
    redirectPolicy        = 88
    secureProtocols       = 84
    enableHttp2           = 133
    enableHttp3           = 162
}

// ─── WinHttpSetSecureProtocols flags ─────────────────────────────
@flags
pub enum WinHttpSecureProtocols: u32 {
    ssl2  = 0x00000008
    ssl3  = 0x00000020
    tls1  = 0x00000080
    tls11 = 0x00000200
    tls12 = 0x00000800
    tls13 = 0x00002000
}

// ─── Redirect policy ─────────────────────────────────────────────
pub enum WinHttpRedirectPolicy: u32 {
    never            = 0
    disallowHttpsToHttp = 1
    always           = 2
}

@extern(C, "winhttp") {
    /// WinHTTP handle (HINTERNET, same opaque type as in wininet).
    pub struct HWINHTTP {}

    // ── Session lifecycle ─────────────────────────────────────────

    /// Opens a session. `pszAgentW`: user-agent UTF-16 string.
    /// `dwAccessType`: WinHttpAccessType.*.
    /// `pszProxyW` / `pszProxyBypassW`: only with namedProxy.
    /// `dwFlags`: 0 for synchronous; 0x10000000 (WINHTTP_FLAG_ASYNC) for async.
    @lib pub fn WinHttpOpen(
        pszAgentW:          *const u16,
        dwAccessType:       u32,
        pszProxyW:          *const u16,
        pszProxyBypassW:    *const u16,
        dwFlags:            u32
    ): *HWINHTTP

    /// Connects to a server. `nServerPort`: 80, 443, or 0 (default).
    /// `dwReserved`: must be 0.
    @lib pub fn WinHttpConnect(
        hSession:        *HWINHTTP,
        pswzServerName:  *const u16,
        nServerPort:     u16,
        dwReserved:      u32
    ): *HWINHTTP

    /// Closes any WinHTTP handle (session, connection, or request).
    @lib pub fn WinHttpCloseHandle(hInternet: *HWINHTTP): i32

    // ── Request lifecycle ─────────────────────────────────────────

    /// Builds (but does not send) an HTTP request.
    /// `pwszVerb`: L"GET", L"POST", etc. NULL = "GET".
    /// `pwszVersion`: NULL = "HTTP/1.1".
    /// `pwszReferrer`: optional Referer header.
    /// `ppwszAcceptTypes`: NULL-terminated array of accept-type strings, or NULL.
    /// `dwFlags`: WinHttpOpenRequestFlags.*.
    @lib pub fn WinHttpOpenRequest(
        hConnect:           *HWINHTTP,
        pwszVerb:           *const u16,
        pwszObjectName:     *const u16,
        pwszVersion:        *const u16,
        pwszReferrer:       *const u16,
        ppwszAcceptTypes:   **const u16,
        dwFlags:            u32
    ): *HWINHTTP

    /// Sends the request. `pwszHeaders` / `dwHeadersLength`: extra
    /// headers. `lpOptional` / `dwOptionalLength`: body data
    /// (e.g. POST payload). `dwTotalLength`: total body length;
    /// 0 = use `dwOptionalLength`. `dwContext`: app-defined context for callbacks.
    @lib pub fn WinHttpSendRequest(
        hRequest:        *HWINHTTP,
        lpszHeaders:     *const u16,
        dwHeadersLength: u32,
        lpOptional:      *void,
        dwOptionalLength: u32,
        dwTotalLength:   u32,
        dwContext:       u64
    ): i32

    /// Waits for the response headers to arrive.  Must be called
    /// before `WinHttpReadData` or header queries.
    @lib pub fn WinHttpReceiveResponse(hRequest: *HWINHTTP, lpReserved: *void): i32

    /// Writes additional request body bytes after WinHttpSendRequest
    /// was called with `dwOptionalLength = 0`.
    @lib pub fn WinHttpWriteData(
        hRequest:        *HWINHTTP,
        lpBuffer:        *const void,
        dwNumberOfBytesToWrite: u32,
        lpdwNumberOfBytesWritten: *u32
    ): i32

    /// Reads response body bytes. Returns 0 = end-of-stream when
    /// `*lpdwNumberOfBytesRead` is 0.
    @lib pub fn WinHttpReadData(
        hRequest:        *HWINHTTP,
        lpBuffer:        *void,
        dwNumberOfBytesToRead: u32,
        lpdwNumberOfBytesRead: *u32
    ): i32

    /// Returns the number of bytes currently available without blocking.
    @lib pub fn WinHttpQueryDataAvailable(
        hRequest:                  *HWINHTTP,
        lpdwNumberOfBytesAvailable: *u32
    ): i32

    // ── Headers ──────────────────────────────────────────────────

    /// Adds, replaces, or removes request headers before sending.
    /// `dwModifiers`: WINHTTP_ADDREQ_FLAG_ADD = 0x20000000,
    /// _REPLACE = 0x80000000, _ADD_IF_NEW = 0x10000000.
    @lib pub fn WinHttpAddRequestHeaders(
        hRequest:        *HWINHTTP,
        lpszHeaders:     *const u16,
        dwHeadersLength: u32,
        dwModifiers:     u32
    ): i32

    /// Reads response headers. `dwInfoLevel`: WinHttpQuery.* (optionally
    /// combined with flagNumber / flagSystemtime / flagCustom).
    @lib pub fn WinHttpQueryHeaders(
        hRequest:        *HWINHTTP,
        dwInfoLevel:     u32,
        pwszName:        *const u16,
        lpBuffer:        *void,
        lpdwBufferLength: *u32,
        lpdwIndex:       *u32
    ): i32

    // ── Options ──────────────────────────────────────────────────

    @lib pub fn WinHttpSetOption(
        hInternet:       *HWINHTTP,
        dwOption:        u32,
        lpBuffer:        *const void,
        dwBufferLength:  u32
    ): i32

    @lib pub fn WinHttpQueryOption(
        hInternet:       *HWINHTTP,
        dwOption:        u32,
        lpBuffer:        *void,
        lpdwBufferLength: *u32
    ): i32

    // ── Timeouts ──────────────────────────────────────────────────

    /// Sets all four timeouts at once.  Pass -1 to leave a value unchanged.
    @lib pub fn WinHttpSetTimeouts(
        hSession:        *HWINHTTP,
        nResolveTimeout: i32,
        nConnectTimeout: i32,
        nSendTimeout:    i32,
        nReceiveTimeout: i32
    ): i32

    // ── Authentication / credentials ──────────────────────────────

    /// Provides credentials for a 401/407 challenge.
    /// `AuthTargets`: 0=server, 1=proxy.
    /// `AuthScheme`: 1=basic, 2=ntlm, 4=passport, 8=digest, 16=negotiate.
    @lib pub fn WinHttpSetCredentials(
        hRequest:    *HWINHTTP,
        AuthTargets: u32,
        AuthScheme:  u32,
        pwszUserName: *const u16,
        pwszPassword: *const u16,
        pAuthParams: *void
    ): i32

    @lib pub fn WinHttpQueryAuthSchemes(
        hRequest:           *HWINHTTP,
        lpdwSupportedSchemes: *u32,
        lpdwFirstScheme:    *u32,
        pdwAuthTarget:      *u32
    ): i32

    // ── URL parsing / cracking ────────────────────────────────────

    /// Parses a URL into its components.
    /// `lpUrlComponents`: pre-filled with field offsets/lengths.
    @lib pub fn WinHttpCrackUrl(
        pwszUrl:          *const u16,
        dwUrlLength:      u32,
        dwFlags:          u32,
        lpUrlComponents:  *void           // *URL_COMPONENTS
    ): i32

    /// Builds a URL from its components.
    @lib pub fn WinHttpCreateUrl(
        lpUrlComponents:  *const void,
        dwFlags:          u32,
        pwszUrl:          *u16,
        pdwUrlLength:     *u32
    ): i32

    // ── Async status callback ─────────────────────────────────────

    @lib pub fn WinHttpSetStatusCallback(
        hInternet:        *HWINHTTP,
        lpfnInternetCallback: *void,
        dwNotificationFlags: u32,
        dwReserved:       u64
    ): *void
}
