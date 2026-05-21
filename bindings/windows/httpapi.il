/// httpapi.dll — kernel-mode HTTP server (http.sys) user-mode shim.
///
/// Lets you stand up an HTTP listener without IIS. Used by WCF /
/// Windows Service hosts / OWIN self-host / dotnet's Kestrel
/// fallback.
///
/// Bootstrap:
///   HttpInitialize(HTTPAPI_VERSION_2, HttpInitializeServer, NULL)
///   HttpCreateRequestQueue(HTTPAPI_VERSION_2, NULL, NULL, 0, &q)
///   HttpCreateServerSession(HTTPAPI_VERSION_2, &sessionId, 0)
///   HttpCreateUrlGroup(sessionId, &urlGroupId, 0)
///   HttpAddUrlToUrlGroup(urlGroupId, L"http://+:8080/",
///                       (HTTP_URL_CONTEXT)0, 0)
///   ... receive / respond loop ...
///   HttpCloseRequestQueue(q)
///   HttpCloseServerSession(sessionId)
///   HttpTerminate(HttpInitializeServer, NULL)
///
/// `HTTPAPI_VERSION_2` = 0x00020000 (low word 0, high word 2).

@flags
pub enum HttpInitFlags: u32 {
    server = 0x1
    config = 0x2
}

@flags
pub enum HttpReceiveFlags: u32 {
    copyBody    = 0x00000001
}

@flags
pub enum HttpResponseFlags: u32 {
    disconnect    = 0x00000001
    moreData      = 0x00000002
    bufferData    = 0x00000004
    enableTrailers = 0x00000008
}

@extern(C, "httpapi") {
    /// 8-byte version. HTTPAPI_VERSION_2 = {0, 2}.
    pub struct HTTPAPI_VERSION {
        HttpApiMajorVersion: u16
        HttpApiMinorVersion: u16
        _pad:                u32
    }

    /// `HttpInitialize(Version, Flags, pReserved)`. `Flags`:
    /// HttpInitFlags.server (typical) | .config (lets the caller
    /// set ACLs etc.). Returns NO_ERROR (0) on success.
    @lib pub fn HttpInitialize(
        Version:   HTTPAPI_VERSION,
        Flags:     u32,
        pReserved: *void
    ): u32

    @lib pub fn HttpTerminate(Flags: u32, pReserved: *void): u32

    // ── Session / URL group ──────────────────────────────────────

    @lib pub fn HttpCreateServerSession(
        Version:        HTTPAPI_VERSION,
        ServerSessionId: *u64,
        Reserved:       u32
    ): u32

    @lib pub fn HttpCloseServerSession(ServerSessionId: u64): u32

    @lib pub fn HttpCreateUrlGroup(
        ServerSessionId: u64,
        pUrlGroupId:     *u64,
        Reserved:        u32
    ): u32

    @lib pub fn HttpCloseUrlGroup(UrlGroupId: u64): u32

    @lib pub fn HttpAddUrlToUrlGroup(
        UrlGroupId:  u64,
        pFullyQualifiedUrl: *const u16,
        UrlContext:  u64,
        Reserved:    u32
    ): u32

    @lib pub fn HttpRemoveUrlFromUrlGroup(
        UrlGroupId:  u64,
        pFullyQualifiedUrl: *const u16,
        Flags:       u32
    ): u32

    // ── Request queue ────────────────────────────────────────────

    /// `HttpCreateRequestQueue(Version, Name, sd, Flags, &queueHandle)`.
    /// `Name == NULL` creates an anonymous queue.
    @lib pub fn HttpCreateRequestQueue(
        Version:    HTTPAPI_VERSION,
        Name:       *const u16,
        SecurityAttributes: *void,
        Flags:      u32,
        pReqQueueHandle: *i64
    ): u32

    @lib pub fn HttpCloseRequestQueue(ReqQueueHandle: i64): u32

    /// Bind the URL group's traffic into a request queue (the
    /// queue must already exist).
    @lib pub fn HttpSetUrlGroupProperty(
        UrlGroupId:  u64,
        Property:    i32,        // HTTP_SERVER_PROPERTY enum
        pPropertyInformation: *const void,
        PropertyInformationLength: u32
    ): u32

    // ── I/O ──────────────────────────────────────────────────────

    /// Pull the next request out of the queue. `pRequestBuffer`
    /// receives an `HTTP_REQUEST` (V2) prefix plus inline strings.
    @lib pub fn HttpReceiveHttpRequest(
        ReqQueueHandle: i64,
        RequestId:      u64,         // HTTP_NULL_ID = 0 → any
        Flags:          u32,         // HttpReceiveFlags
        pRequestBuffer: *void,
        RequestBufferLength: u32,
        pBytesReturned: *u32,
        pOverlapped:    *void
    ): u32

    /// Read the entity body bytes for an existing request.
    @lib pub fn HttpReceiveRequestEntityBody(
        ReqQueueHandle: i64,
        RequestId:      u64,
        Flags:          u32,
        pBuffer:        *void,
        EntityBufferLength: u32,
        pBytesReturned: *u32,
        pOverlapped:    *void
    ): u32

    /// Send the response — usually drives `HTTP_RESPONSE_V2` with
    /// status + reason + header array.
    @lib pub fn HttpSendHttpResponse(
        ReqQueueHandle: i64,
        RequestId:      u64,
        Flags:          u32,         // HttpResponseFlags
        pHttpResponse:  *const void, // *HTTP_RESPONSE_V2
        pCachePolicy:   *const void,
        pBytesSent:     *u32,
        pReserved1:     *void,
        Reserved2:      u32,
        pOverlapped:    *void,
        pLogData:       *const void
    ): u32

    /// Send additional body chunks after a primary response.
    @lib pub fn HttpSendResponseEntityBody(
        ReqQueueHandle:  i64,
        RequestId:       u64,
        Flags:           u32,
        EntityChunkCount: u16,
        pEntityChunks:   *const void, // *HTTP_DATA_CHUNK[]
        pBytesSent:      *u32,
        pReserved1:      *void,
        Reserved2:       u32,
        pOverlapped:     *void,
        pLogData:        *const void
    ): u32

    /// Cancel a pending request.
    @lib pub fn HttpCancelHttpRequest(
        ReqQueueHandle: i64,
        RequestId:      u64,
        pOverlapped:    *void
    ): u32
}
