/// mswsock.dll — Winsock 2 high-performance extensions.
///
/// The headline extensions:
///   * `AcceptEx`          — accept + initial recv in one syscall;
///                            pairs well with IOCP
///   * `ConnectEx`         — non-blocking TCP connect that can be
///                            polled via WSAGetOverlappedResult
///   * `TransmitFile`      — kernel-space file → socket transfer
///   * `DisconnectEx`      — graceful close + socket reuse
///   * `WSARecvMsg`        — recv with control messages
///
/// `AcceptEx`, `ConnectEx`, `DisconnectEx` are typically obtained
/// indirectly via `WSAIoctl(SIO_GET_EXTENSION_FUNCTION_POINTER, ...)`
/// in production code (the static linkage is allowed but
/// discouraged); the exports below let you skip that hop in
/// prototypes.

use kernel32 { HANDLE }

@extern(C, "mswsock") {
    /// Per-call data for `TransmitFile`. Either of the head /
    /// tail buffers can be NULL.
    pub struct TRANSMIT_FILE_BUFFERS {
        Head:           *void
        HeadLength:     u32
        _pad:           u32
        Tail:           *void
        TailLength:     u32
        _pad2:          u32
    }

    /// `AcceptEx` — accept a connection plus an initial recv into
    /// `lpOutputBuffer`. `dwReceiveDataLength` may be 0 to skip the
    /// data prefetch. `lpOverlapped` is required and used for IOCP.
    /// Returns non-zero on synchronous success.
    @lib pub fn AcceptEx(
        sListenSocket:   u64,          // SOCKET
        sAcceptSocket:   u64,
        lpOutputBuffer:  *void,
        dwReceiveDataLength:    u32,
        dwLocalAddressLength:   u32,
        dwRemoteAddressLength:  u32,
        lpdwBytesReceived: *u32,
        lpOverlapped:    *void         // *WSAOVERLAPPED
    ): i32

    /// `ConnectEx` — outbound connect for an unbound SOCKET. The
    /// socket must be bound to INADDR_ANY first (`bind` with port 0).
    @lib pub fn ConnectEx(
        s:           u64,
        name:        *const void,      // *sockaddr
        namelen:     i32,
        lpSendBuffer: *void,
        dwSendDataLength: u32,
        lpdwBytesSent: *u32,
        lpOverlapped: *void
    ): i32

    /// `DisconnectEx` — graceful close that allows socket reuse
    /// when `dwFlags == TF_REUSE_SOCKET (2)`.
    @lib pub fn DisconnectEx(
        s:           u64,
        lpOverlapped: *void,
        dwFlags:     u32,
        dwReserved:  u32
    ): i32

    /// `TransmitFile` — kernel sends `hFile` over `hSocket` with
    /// optional pre / post buffers. `nNumberOfBytesToWrite == 0` =
    /// send the entire file. `dwFlags`: TF_DISCONNECT (1),
    /// TF_REUSE_SOCKET (2), TF_WRITE_BEHIND (4).
    @lib pub fn TransmitFile(
        hSocket:    u64,
        hFile:      HANDLE,
        nNumberOfBytesToWrite: u32,
        nNumberOfBytesPerSend: u32,
        lpOverlapped: *void,
        lpTransmitBuffers: *const TRANSMIT_FILE_BUFFERS,
        dwFlags:    u32
    ): i32

    /// `WSARecvMsg` — recv with control messages (CMSG_*). Used
    /// for IPv6 PKT info etc.
    @lib pub fn WSARecvMsg(
        s:           u64,
        lpMsg:       *void,             // *WSAMSG
        lpdwNumberOfBytesRecvd: *u32,
        lpOverlapped: *void,
        lpCompletionRoutine: *void
    ): i32

    /// Sleep helpers used by IOCP scenarios.
    @lib pub fn SleepEx(dwMilliseconds: u32, bAlertable: i32): u32

    /// Mark `sSocket` so subsequent `closesocket` releases without
    /// the half-close grace period.
    @lib pub fn WSARecvEx(
        s:        u64,
        buf:      *char,
        len:      i32,
        flags:    *i32
    ): i32

    /// Parse a getXXXbyYYY result: `GetAddressByNameA` and
    /// `GetTypeByNameA` are obsolete; prefer `getaddrinfo` from
    /// ws2_32. Kept here for completeness.
    @lib pub fn GetAddressByNameA(
        dwNameSpace:   u32,
        lpServiceType: *const void,
        lpServiceName: *char,
        lpiProtocols:  *i32,
        dwResolution:  u32,
        lpServiceAsyncInfo: *void,
        lpCsaddrBuffer: *void,
        lpdwBufferLength: *u32,
        lpAliasBuffer: *char,
        lpdwAliasBufferLength: *u32
    ): i32
}
