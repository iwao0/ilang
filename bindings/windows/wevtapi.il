/// wevtapi.dll — Windows Event Log (Vista+) reader / writer.
///
/// Reading pattern:
///   let hQuery = EvtQuery(0, L"System", L"*[System/Level=2]",
///                         EvtQueryChannelPath | EvtQueryForwardDirection)
///   let evts: i64[10] = ...
///   loop {
///       let n: u32 = 0
///       if EvtNext(hQuery, 10, evts, INFINITE, 0, &n) == 0 { break }
///       for i in 0..n {
///           EvtRender(0, evts[i], EvtRenderEventXml, bufSize,
///                     buf, &written, &propCount)
///           EvtClose(evts[i])
///       }
///   }
///   EvtClose(hQuery)

// ─── EVT_QUERY_FLAGS ─────────────────────────────────────────────
@flags
pub enum EvtQueryFlags: u32 {
    channelPath           = 0x1
    filePath              = 0x2
    forwardDirection      = 0x100
    reverseDirection      = 0x200
    tolerateQueryErrors   = 0x1000
}

// ─── EVT_SUBSCRIBE_FLAGS ─────────────────────────────────────────
@flags
pub enum EvtSubscribeFlags: u32 {
    toFutureEvents              = 1
    startAtOldestRecord         = 2
    startAfterBookmark          = 3
    originMask                  = 3
    tolerateQueryErrors         = 0x1000
    strict                      = 0x10000
}

// ─── EVT_RENDER_FLAGS ────────────────────────────────────────────
pub enum EvtRenderFlags: u32 {
    eventValues        = 0
    eventXml           = 1
    bookmark           = 2
}

// ─── EVT_FORMAT_MESSAGE_FLAGS ────────────────────────────────────
pub enum EvtFormatMessageFlags: u32 {
    event       = 1
    level       = 2
    task        = 3
    opcode      = 4
    keyword     = 5
    channel     = 6
    provider    = 7
    id          = 8
    xml         = 9
}

// ─── EVT_LOG_PROPERTY_ID ─────────────────────────────────────────
pub enum EvtLogPropertyId: u32 {
    creationTime  = 0
    lastAccessTime = 1
    lastWriteTime  = 2
    fileSize       = 3
    attributes     = 4
    numberOfLogRecords = 5
    oldestRecordNumber = 6
    full           = 7
}

@extern(C, "wevtapi") {
    /// Run an XPath query against a channel or `.evtx` file.
    /// `flags`: EvtQueryFlags.* OR-ed. Returns an opaque HANDLE
    /// (cast to i64), or 0 on error.
    @lib pub fn EvtQuery(
        session: i64,                // 0 for local
        path:    *const u16,         // channel name or .evtx path
        query:   *const u16,         // XPath; NULL = match all
        flags:   u32
    ): i64

    /// Pull the next batch of events. `eventArray` is filled with
    /// up to `eventsSize` event handles. Returns non-zero on
    /// success and writes the actual count to `*returned`.
    @lib pub fn EvtNext(
        resultSet:  i64,
        eventsSize: u32,
        events:     *i64,
        timeout:    u32,             // ms; 0xFFFFFFFF = INFINITE
        flags:      u32,             // reserved, pass 0
        returned:   *u32
    ): i32

    /// Format an event into XML or one of the named fields.
    @lib pub fn EvtRender(
        context:       i64,          // 0 = default
        fragment:      i64,
        flags:         u32,          // EvtRenderFlags
        bufferSize:    u32,          // in bytes
        buffer:        *void,
        bufferUsed:    *u32,
        propertyCount: *u32
    ): i32

    /// Resolve the localized message for an event (the human-
    /// readable description shown by Event Viewer).
    @lib pub fn EvtFormatMessage(
        publisherMetadata: i64,
        event:             i64,
        messageId:         u32,
        valueCount:        u32,
        values:            *void,
        flags:             u32,      // EvtFormatMessageFlags
        bufferSize:        u32,
        buffer:            *u16,
        bufferUsed:        *u32
    ): i32

    /// Subscribe to live events. `callback` is invoked once per
    /// event under the EVT_SUBSCRIBE_NOTIFY action.
    @lib pub fn EvtSubscribe(
        session:        i64,
        signalEvent:    i64,
        channelPath:    *const u16,
        query:          *const u16,
        bookmark:       i64,
        context:        *void,
        callback:       fn(u32, *void, i64): u32,
        flags:          u32
    ): i64

    /// Open a publisher metadata handle for resolving message IDs.
    @lib pub fn EvtOpenPublisherMetadata(
        session:      i64,
        publisherId:  *const u16,
        logFilePath:  *const u16,
        locale:       u32,
        flags:        u32
    ): i64

    /// Channel enumeration.
    @lib pub fn EvtOpenChannelEnum(session: i64, flags: u32): i64
    @lib pub fn EvtNextChannelPath(
        channelEnum: i64,
        bufferSize:  u32,
        buffer:      *u16,
        bufferUsed:  *u32
    ): i32

    @lib pub fn EvtClose(object: i64): i32

    /// Publisher enumeration (event sources).
    @lib pub fn EvtOpenPublisherEnum(session: i64, flags: u32): i64
    @lib pub fn EvtNextPublisherId(
        publisherEnum: i64,
        bufferSize:    u32,
        buffer:        *u16,
        bufferUsed:    *u32
    ): i32

    /// Channel / log properties (record count, oldest record, size).
    @lib pub fn EvtGetLogInfo(
        log:           i64,
        propertyId:    u32,
        propertyValueBufferSize: u32,
        propertyValueBuffer: *void,
        propertyValueBufferUsed: *u32
    ): i32

    /// Export selected events to an `.evtx` file.
    @lib pub fn EvtExportLog(
        session:      i64,
        path:         *const u16,
        query:        *const u16,
        targetFilePath: *const u16,
        flags:        u32
    ): i32
}
