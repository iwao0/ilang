// ws2_32.dll — Windows Sockets 2.
//
// Before any other Winsock call: WSAStartup(0x0202, &wsaData).
// Always call WSACleanup() when done.
// INVALID_SOCKET = `0xFFFFFFFFFFFFFFFF as u64`
// SOCKET_ERROR   = -1 (returned as i32 by most functions)

// ─── Address families (AF_*) ─────────────────────────────────────
pub enum AddressFamily: i32 {
    unspec  = 0
    unix    = 1
    inet    = 2    // IPv4
    implink = 3
    pup     = 4
    chaos   = 5
    ns      = 6
    ipx     = 6
    iso     = 7
    ecma    = 8
    datakit = 9
    ccitt   = 10
    sna     = 11
    decnet  = 12
    dli     = 13
    lat     = 14
    hylink  = 15
    appletalk = 16
    netbios = 17
    inet6   = 23   // IPv6
    irda    = 26
    bluetooth = 32
}

// ─── Socket types (SOCK_*) ───────────────────────────────────────
pub enum SocketType: i32 {
    stream    = 1   // TCP
    dgram     = 2   // UDP
    raw       = 3
    rdm       = 4   // reliably delivered message
    seqpacket = 5
}

// ─── Protocols (IPPROTO_*) ───────────────────────────────────────
pub enum Protocol: i32 {
    ip      = 0
    icmp    = 1
    igmp    = 2
    ggp     = 3
    ipv4    = 4
    tcp     = 6
    pup     = 12
    udp     = 17
    idp     = 22
    ipv6    = 41
    routing = 43
    fragment = 44
    esp     = 50
    ah      = 51
    icmpv6  = 58
    none_   = 59
    dstopts = 60
    raw_    = 255
    max     = 256
}

// ─── Socket option levels / names ────────────────────────────────
pub enum SockOptLevel: i32 {
    socket  = 0xFFFF   // SOL_SOCKET
    ip      = 0        // IPPROTO_IP
    ipv6    = 41       // IPPROTO_IPV6
    tcp     = 6        // IPPROTO_TCP
}

@flags
pub enum SockOptName: i32 {
    debug            = 0x0001
    acceptConn       = 0x0002
    reuseAddr        = 0x0004
    keepAlive        = 0x0008
    dontRoute        = 0x0010
    broadcast        = 0x0020
    useLoopback      = 0x0040
    linger           = 0x0080
    oobinline        = 0x0100
    dontLinger       = -129          // ~SO_LINGER
    exclusiveAddrUse = -5            // ~SO_REUSEADDR
    sndBuf           = 0x1001
    rcvBuf           = 0x1002
    sndLowat         = 0x1003
    rcvLowat         = 0x1004
    sndTimeo         = 0x1005
    rcvTimeo         = 0x1006
    error_           = 0x1007
    type_            = 0x1008
    bspState         = 0x1009
    group_id         = 0x2001
    group_priority   = 0x2002
    max_msg_size     = 0x2003
    conditional_accept = 0x3002
    pause_accept     = 0x3003
    compartment_id   = 0x3004
    randomize_port   = 0x3005
    port_scalability = 0x3006
}

pub enum TcpOptName: i32 {
    noDelay    = 1
    expedited  = 2
    keepAlive  = 3
    maxSeg     = 4
    maxRt      = 5
    stdurg     = 6
    noPush     = 4
    noOpts     = 8
}

// ─── send/recv flags (MSG_*) ─────────────────────────────────────
@flags
pub enum MsgFlags: i32 {
    oob       = 0x1
    peek      = 0x2
    dontRoute = 0x4
    waitAll   = 0x8
    interrupt = 0x10
    push      = 0x20
    partial   = 0x8000
}

// ─── shutdown() how argument (SD_*) ──────────────────────────────
pub enum ShutdownHow: i32 {
    receive = 0   // SD_RECEIVE
    send    = 1   // SD_SEND
    both    = 2   // SD_BOTH
}

// ─── WSA error codes (WSAE*) ─────────────────────────────────────
pub enum WsaError: i32 {
    wsaBaseErr           = 10000
    wsaEIntr             = 10004
    wsaEBadf             = 10009
    wsaEAcces            = 10013
    wsaEFault            = 10014
    wsaEInval            = 10022
    wsaEMFile            = 10024
    wsaEWouldBlock       = 10035
    wsaEInProgress       = 10036
    wsaEAlready          = 10037
    wsaENotsock          = 10038
    wsaEDestAddrReq      = 10039
    wsaEMsgSize          = 10040
    wsaEPrototype        = 10041
    wsaENoprotOpt        = 10042
    wsaEProtonosupport   = 10043
    wsaESocktnosupport   = 10044
    wsaEOpnosupport      = 10045
    wsaEPfnosupport      = 10046
    wsaEAfnosupport      = 10047
    wsaEAddrInUse        = 10048
    wsaEAddrNotAvail     = 10049
    wsaENetDown          = 10050
    wsaENetUnreach       = 10051
    wsaENetreset         = 10052
    wsaEConnAborted      = 10053
    wsaEConnReset        = 10054
    wsaENoBufs           = 10055
    wsaEIsConn           = 10056
    wsaENotConn          = 10057
    wsaEShutdown         = 10058
    wsaETooManyRefs      = 10059
    wsaETimedOut         = 10060
    wsaEConnRefused      = 10061
    wsaELoop             = 10062
    wsaENameTooLong      = 10063
    wsaEHostDown         = 10064
    wsaEHostUnreach      = 10065
    wsaENotEmpty         = 10066
    wsaEProClim          = 10067
    wsaEUsers            = 10068
    wsaEDquot            = 10069
    wsaEStale            = 10070
    wsaERemote           = 10071
    wsaSysNotReady       = 10091
    wsaVerNotSupported   = 10092
    wsaNotInitialised    = 10093
    wsaEDiscon           = 10101
    wsaENoMore           = 10102
    wsaECancelled        = 10103
    wsaEInvalidProcTable = 10104
    wsaEInvalidProvider  = 10105
    wsaEProviderFailedInit = 10106
    wsaSyscallfailure    = 10107
    wsaServiceNotFound   = 10108
    wsaTypeNotFound      = 10109
    wsaENoMore2          = 10110
    wsaECancelled2       = 10111
    wsaERefused          = 10112
    wsaHostNotFound      = 11001
    wsaTryAgain          = 11002
    wsaNoRecovery        = 11003
    wsaNoData            = 11004
}

// ─── getaddrinfo hint flags (AI_*) ───────────────────────────────
@flags
pub enum AddrInfoFlags: i32 {
    passive     = 0x01   // AI_PASSIVE — suitable for bind()
    canonName   = 0x02   // AI_CANONNAME — fill ai_canonname
    numericHost = 0x04   // AI_NUMERICHOST — host is numeric
    numericServ = 0x08   // AI_NUMERICSERV — service is numeric
    all         = 0x0100
    addrConfig  = 0x0400
    v4Mapped    = 0x0800
    nonBlocking = 0x1000
    extendedAttributes = 0x2000
    secureWithFallback = 0x40000
}

@extern(C, "ws2_32") {
    // ── WSADATA — 408 bytes on x64 ───────────────────────────────

    /// Filled by WSAStartup. Only `wVersion` is typically inspected.
    pub struct WSADATA {
        wVersion:     u16
        wHighVersion: u16
        iMaxSockets:  u16
        iMaxUdpDg:    u16
        lpVendorInfo: *char
        // szDescription[257] + szSystemStatus[129] + 6-byte pad = 392 bytes
        _d00: u64
        _d01: u64
        _d02: u64
        _d03: u64
        _d04: u64
        _d05: u64
        _d06: u64
        _d07: u64
        _d08: u64
        _d09: u64
        _d10: u64
        _d11: u64
        _d12: u64
        _d13: u64
        _d14: u64
        _d15: u64
        _d16: u64
        _d17: u64
        _d18: u64
        _d19: u64
        _d20: u64
        _d21: u64
        _d22: u64
        _d23: u64
        _d24: u64
        _d25: u64
        _d26: u64
        _d27: u64
        _d28: u64
        _d29: u64
        _d30: u64
        _d31: u64
        _d32: u64
        _d33: u64
        _d34: u64
        _d35: u64
        _d36: u64
        _d37: u64
        _d38: u64
        _d39: u64
        _d40: u64
        _d41: u64
        _d42: u64
        _d43: u64
        _d44: u64
        _d45: u64
        _d46: u64
        _d47: u64
        _d48: u64
    }

    // ── sockaddr_in — 16 bytes ────────────────────────────────────

    /// IPv4 socket address. `sin_port` must be in network byte order;
    /// use `htons(port)` to convert. `sin_addr` is 4 bytes in
    /// network byte order; use `inet_addr("1.2.3.4")`.
    pub struct SOCKADDR_IN {
        sin_family: i16     // AddressFamily.inet = 2
        sin_port:   u16     // network byte order
        sin_addr:   u32     // network byte order (big-endian IPv4)
        sin_zero:   u64     // must be zero
    }

    // ── sockaddr_in6 — 28 bytes ───────────────────────────────────

    /// IPv6 socket address.
    pub struct SOCKADDR_IN6 {
        sin6_family:   i16
        sin6_port:     u16
        sin6_flowinfo: u32
        sin6_addr_hi:  u64   // first 8 bytes of 128-bit IPv6 address
        sin6_addr_lo:  u64   // last  8 bytes of 128-bit IPv6 address
        sin6_scope_id: u32
    }

    // ── sockaddr — 16 bytes (generic base) ───────────────────────

    /// Generic socket address; cast to SOCKADDR_IN or SOCKADDR_IN6.
    pub struct SOCKADDR {
        sa_family: u16
        sa_data:   u64   // first 8 bytes of sa_data[14]
        sa_data2:  u32   // remaining 4 bytes of sa_data[14]
        _pad:      u16
    }

    // ── timeval — 8 bytes ─────────────────────────────────────────

    /// Timeout for select(). Both fields are in i32 (long on Windows).
    pub struct TIMEVAL {
        tv_sec:  i32
        tv_usec: i32
    }

    // ── addrinfo — 48 bytes on x64 ───────────────────────────────

    /// Linked-list node returned by getaddrinfo.
    /// Walk `ai_next` until it is NULL.
    pub struct ADDRINFO {
        ai_flags:     i32       // AddrInfoFlags.*
        ai_family:    i32       // AddressFamily.*
        ai_socktype:  i32       // SocketType.*
        ai_protocol:  i32       // Protocol.*
        ai_addrlen:   u64       // byte length of ai_addr
        ai_canonname: *char     // canonical name (or NULL)
        ai_addr:      *SOCKADDR // socket address
        ai_next:      *void     // next ADDRINFO node (or NULL)
    }

    // ── hostent — 32 bytes on x64 ────────────────────────────────

    pub struct HOSTENT {
        h_name:      *char    // official host name
        h_aliases:   **char   // NULL-terminated list of aliases
        h_addrtype:  i16
        h_length:    i16
        h_addr_list: **char   // NULL-terminated list of addresses
    }

    // ── Startup / cleanup ─────────────────────────────────────────

    /// Must be called before any other Winsock function.
    /// `wVersionRequested` = 0x0202 for Winsock 2.2.
    @lib pub fn WSAStartup(wVersionRequested: u16, lpWSAData: *WSADATA): i32

    @lib pub fn WSACleanup(): i32

    /// Returns the error code from the last failed Winsock call.
    @lib pub fn WSAGetLastError(): i32

    @lib pub fn WSASetLastError(iError: i32)

    // ── Socket creation ───────────────────────────────────────────

    /// Creates a socket. Returns INVALID_SOCKET (0xFFFFFFFFFFFFFFFF)
    /// on failure.
    @lib pub fn socket(af: i32, type_: i32, protocol: i32): u64

    /// Creates a socket with additional options (overlapped I/O etc.).
    /// `dwFlags`: 1=WSA_FLAG_OVERLAPPED, 0x80=WSA_FLAG_NO_HANDLE_INHERIT.
    @lib pub fn WSASocketA(
        af:         i32,
        type_:      i32,
        protocol:   i32,
        lpProtocolInfo: *void,
        g:          u32,
        dwFlags:    u32
    ): u64

    @lib pub fn closesocket(s: u64): i32

    // ── Server socket setup ───────────────────────────────────────

    @lib pub fn bind(
        s:       u64,
        name:    *const SOCKADDR,
        namelen: i32
    ): i32

    @lib pub fn listen(s: u64, backlog: i32): i32

    /// Accepts a connection. Fills `addr` with the remote address.
    /// Returns a new socket, or INVALID_SOCKET on failure.
    @lib pub fn accept(
        s:       u64,
        addr:    *SOCKADDR,
        addrlen: *i32
    ): u64

    // ── Client socket setup ───────────────────────────────────────

    @lib pub fn connect(
        s:       u64,
        name:    *const SOCKADDR,
        namelen: i32
    ): i32

    // ── Send / receive ────────────────────────────────────────────

    @lib pub fn send(
        s:     u64,
        buf:   *const char,
        len:   i32,
        flags: i32
    ): i32

    @lib pub fn recv(
        s:     u64,
        buf:   *char,
        len:   i32,
        flags: i32
    ): i32

    @lib pub fn sendto(
        s:       u64,
        buf:     *const char,
        len:     i32,
        flags:   i32,
        to:      *const SOCKADDR,
        tolen:   i32
    ): i32

    @lib pub fn recvfrom(
        s:        u64,
        buf:      *char,
        len:      i32,
        flags:    i32,
        from:     *SOCKADDR,
        fromlen:  *i32
    ): i32

    // ── Socket options ────────────────────────────────────────────

    @lib pub fn getsockopt(
        s:       u64,
        level:   i32,
        optname: i32,
        optval:  *char,
        optlen:  *i32
    ): i32

    @lib pub fn setsockopt(
        s:       u64,
        level:   i32,
        optname: i32,
        optval:  *const char,
        optlen:  i32
    ): i32

    /// Controls I/O mode. `cmd`: 0x8004667E=FIONBIO (nonblocking),
    /// 0x4004667F=FIONREAD (bytes available). `argp`: argument.
    @lib pub fn ioctlsocket(s: u64, cmd: i32, argp: *u32): i32

    // ── Shutdown ──────────────────────────────────────────────────

    @lib pub fn shutdown(s: u64, how: i32): i32

    // ── Multiplexing ─────────────────────────────────────────────

    /// Classic select. `nfds` is ignored on Windows; pass 0.
    /// `readfds`, `writefds`, `exceptfds`: pass `0 as *void` to skip.
    /// `timeout`: NULL = block indefinitely.
    /// Returns total ready descriptors, 0 on timeout, SOCKET_ERROR on error.
    @lib pub fn select(
        nfds:      i32,
        readfds:   *void,   // *fd_set (or NULL)
        writefds:  *void,
        exceptfds: *void,
        timeout:   *const TIMEVAL
    ): i32

    // ── Address / name resolution ─────────────────────────────────

    /// Resolves a hostname + service to a list of ADDRINFO nodes.
    /// Free with `freeaddrinfo`.
    @lib pub fn getaddrinfo(
        pNodeName:    *const char,
        pServiceName: *const char,
        pHints:       *const ADDRINFO,
        ppResult:     **ADDRINFO
    ): i32

    @lib pub fn freeaddrinfo(pAddrInfo: *ADDRINFO)

    /// Inverse of getaddrinfo: numeric addresses → human-readable strings.
    @lib pub fn getnameinfo(
        sa:        *const SOCKADDR,
        salen:     i32,
        host:      *char,
        hostlen:   u32,
        serv:      *char,
        servlen:   u32,
        flags:     i32
    ): i32

    /// Legacy lookup: resolves a hostname to a HOSTENT struct.
    /// Prefer `getaddrinfo` for new code.
    @lib pub fn gethostbyname(name: *const char): *HOSTENT

    @lib pub fn gethostname(name: *char, namelen: i32): i32

    // ── Byte-order conversion ─────────────────────────────────────

    /// Host-to-network 16-bit (short). Use for `sin_port`.
    @lib pub fn htons(hostshort: u16): u16

    /// Host-to-network 32-bit (long). Use for `sin_addr` raw values.
    @lib pub fn htonl(hostlong: u32): u32

    @lib pub fn ntohs(netshort: u16): u16
    @lib pub fn ntohl(netlong: u32): u32

    // ── Address string conversion ─────────────────────────────────

    /// Converts an IPv4 dotted-decimal string to network-order u32.
    /// Returns INADDR_NONE (0xFFFFFFFF) on error.
    @lib pub fn inet_addr(cp: *const char): u32

    /// Converts a network-order IPv4 address to dotted-decimal string.
    /// The returned pointer is a static buffer — copy before next call.
    @lib pub fn inet_ntoa(in_: u32): *char

    /// Converts an IPv4/IPv6 address string to binary form.
    /// `af`: AddressFamily.inet or .inet6.
    /// `src`: dotted-decimal or colon-hex string.
    /// `dst`: caller-allocated buffer (4 bytes for IPv4, 16 for IPv6).
    /// Returns 1 on success, 0 if src is invalid, -1 on bad family.
    @lib pub fn inet_pton(af: i32, src: *const char, dst: *void): i32

    /// Converts binary address to string. `dst` size: 16 for IPv4, 46 for IPv6.
    @lib pub fn inet_ntop(af: i32, src: *const void, dst: *char, size: u32): *const char

    // ── Socket address helpers ────────────────────────────────────

    @lib pub fn getsockname(
        s:       u64,
        name:    *SOCKADDR,
        namelen: *i32
    ): i32

    @lib pub fn getpeername(
        s:       u64,
        name:    *SOCKADDR,
        namelen: *i32
    ): i32
}
