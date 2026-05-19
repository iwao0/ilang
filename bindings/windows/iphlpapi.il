// iphlpapi.dll — IP Helper API: query and configure network
// interfaces, routing, TCP/UDP tables, and ICMP echo (ping).
//
// All "Get*Table*" functions follow the same pattern:
//   1. call with a small (or zero) buffer; out-param fills the required size
//   2. allocate a buffer of that size and call again
// This is documented per function.

// ─── Interface type (IF_TYPE_*) ──────────────────────────────────
pub enum IfType: u32 {
    other          = 1
    ethernetCsmacd = 6
    iso88025Tokenring = 9
    ppp            = 23
    softwareLoopback = 24
    atm            = 37
    ieee80211      = 71    // Wi-Fi
    tunnel         = 131
    ieee1394       = 144
}

// ─── Interface operational status (IF_OPER_STATUS_*) ─────────────
pub enum IfOperStatus: u32 {
    up               = 1
    down             = 2
    testing          = 3
    unknown          = 4
    dormant          = 5
    notPresent       = 6
    lowerLayerDown   = 7
}

// ─── MIB_TCP_STATE values ────────────────────────────────────────
pub enum TcpState: u32 {
    closed       = 1
    listening    = 2
    synSent      = 3
    synRcvd      = 4
    established  = 5
    finWait1     = 6
    finWait2     = 7
    closeWait    = 8
    closing      = 9
    lastAck      = 10
    timeWait     = 11
    deleteTcb    = 12
}

// ─── GetIpAddrTable / SetIfEntry — return codes ──────────────────
pub enum NetError: u32 {
    success           = 0       // NO_ERROR
    accessDenied      = 5
    invalidParameter  = 87
    insufficientBuffer = 122
    notFound          = 1168
    notSupported      = 50
}

@extern(C, "iphlpapi") {
    // ── IP_ADDR_STRING — 64 bytes ────────────────────────────────

    /// Linked-list node of dotted-decimal IPv4 strings.
    pub struct IP_ADDR_STRING {
        Next:      *IP_ADDR_STRING
        IpAddress: u64    // first 8 of IP_ADDRESS_STRING[16]
        IpAddress2: u64   // second 8 of IP_ADDRESS_STRING[16]
        IpMask:    u64    // first 8 of IP_MASK_STRING[16]
        IpMask2:   u64    // second 8 of IP_MASK_STRING[16]
        Context:   u32
        _pad:      u32
    }

    // ── FIXED_INFO ────────────────────────────────────────────────
    // Layout (approx. 920 bytes; treat as opaque buffer):
    //   char  HostName[132]         offset 0
    //   char  DomainName[132]       offset 132
    //   *IP_ADDR_STRING CurrentDnsServer  offset 264 (8-aligned, +4 pad)
    //   IP_ADDR_STRING  DnsServerList     offset 272 (64 bytes)
    //   u32   NodeType              offset 336
    //   char  ScopeId[260]          offset 340
    //   u32   EnableRouting / EnableProxy / EnableDns
    // Allocate with `GetNetworkParams(NULL, &len)` to learn the size.

    // ── MIB_TCPROW — 20 bytes ────────────────────────────────────

    /// One TCP connection.  Addresses and ports are in network byte order.
    pub struct MIB_TCPROW {
        dwState:      u32   // TcpState.*
        dwLocalAddr:  u32
        dwLocalPort:  u32
        dwRemoteAddr: u32
        dwRemotePort: u32
    }

    // ── MIB_TCPTABLE header — 4 bytes followed by dwNumEntries rows ─

    /// Layout: { u32 dwNumEntries; MIB_TCPROW table[dwNumEntries]; }
    /// Pass `pOutBufLen` = 0 to query the required size.
    pub struct MIB_TCPTABLE {
        dwNumEntries: u32
        // followed by table[dwNumEntries] of MIB_TCPROW (20 bytes each)
    }

    // ── MIB_UDPROW — 8 bytes ─────────────────────────────────────

    pub struct MIB_UDPROW {
        dwLocalAddr: u32
        dwLocalPort: u32
    }

    // ── ICMP_ECHO_REPLY — 28 bytes ───────────────────────────────

    /// Result of an `IcmpSendEcho` call. Followed by `Data` bytes
    /// in the reply buffer.
    pub struct ICMP_ECHO_REPLY {
        Address:       u32       // IPAddr (network byte order)
        Status:        u32       // 0 = IP_SUCCESS
        RoundTripTime: u32       // milliseconds
        DataSize:      u16
        Reserved:      u16
        Data:          *void     // pointer into ReplyBuffer
        // IP_OPTION_INFORMATION Options (8 bytes)
        OptionsTtl:    u8
        OptionsTos:    u8
        OptionsFlags:  u8
        OptionsSize:   u8
        OptionsData:   *u8
    }

    // ── Configuration / state ─────────────────────────────────────

    /// Fills `pFixedInfo` with TCP/IP configuration.
    /// First call: pOutBufLen=0 → returns ERROR_BUFFER_OVERFLOW (111)
    /// with required size in *pOutBufLen. Allocate and retry.
    @lib pub fn GetNetworkParams(
        pFixedInfo: *void,
        pOutBufLen: *u32
    ): u32

    /// Returns the adapter information as a linked list of
    /// `IP_ADAPTER_INFO` (≈ 640 bytes per entry).
    /// Same buffer-sizing pattern as `GetNetworkParams`.
    @lib pub fn GetAdaptersInfo(
        pAdapterInfo: *void,
        pOutBufLen:   *u32
    ): u32

    /// Modern adapter enumeration with IPv6 support. Returns linked
    /// list of `IP_ADAPTER_ADDRESSES` (large; treat as opaque).
    /// `Family`: 0=both, 2=IPv4-only, 23=IPv6-only.
    /// `Flags`: 0 (default).
    @lib pub fn GetAdaptersAddresses(
        Family:      u32,
        Flags:       u32,
        Reserved:    *void,
        AdapterAddresses: *void,
        SizePointer: *u32
    ): u32

    /// Returns the index of the best interface for routing to `dwDestAddr`
    /// (host order, despite the type).
    @lib pub fn GetBestInterface(
        dwDestAddr: u32,
        pdwBestIfIndex: *u32
    ): u32

    /// Sends an ARP request for `DestIP` on `SrcIP` and writes the MAC
    /// address into `pMacAddr` (length in `*PhyAddrLen`, usually 6).
    @lib pub fn SendARP(
        DestIP:     u32,
        SrcIP:      u32,
        pMacAddr:   *void,
        PhyAddrLen: *u32
    ): u32

    // ── TCP/UDP tables ───────────────────────────────────────────

    /// Lists all TCP connections.
    /// `pTcpTable`: caller-allocated buffer of `*pdwSize` bytes.
    /// `bOrder`: 0 = unordered, non-zero = sort by local IP/port.
    @lib pub fn GetTcpTable(
        pTcpTable: *MIB_TCPTABLE,
        pdwSize:   *u32,
        bOrder:    i32
    ): u32

    @lib pub fn GetUdpTable(
        pUdpTable: *void,
        pdwSize:   *u32,
        bOrder:    i32
    ): u32

    /// Returns the IPv4 address table (interface index, address, mask).
    @lib pub fn GetIpAddrTable(
        pIpAddrTable: *void,
        pdwSize:      *u32,
        bOrder:       i32
    ): u32

    /// Returns the system interface table.
    @lib pub fn GetIfTable(
        pIfTable: *void,
        pdwSize:  *u32,
        bOrder:   i32
    ): u32

    /// Sets the operational status of an interface (admin up/down).
    @lib pub fn SetIfEntry(pIfRow: *const void): u32

    // ── ICMP echo (ping) ─────────────────────────────────────────

    /// Opens a handle for sending ICMP echo requests.
    @lib pub fn IcmpCreateFile(): *void

    @lib pub fn IcmpCloseHandle(IcmpHandle: *void): i32

    /// Sends an ICMP echo and waits for the reply.
    /// `RequestData`/`RequestSize`: payload to include in the echo.
    /// `ReplyBuffer`/`ReplySize`: large enough for one ICMP_ECHO_REPLY
    ///   plus the request payload (recommended: 1024 bytes minimum).
    /// `Timeout`: milliseconds.
    /// Returns the number of replies received (typically 0 or 1).
    @lib pub fn IcmpSendEcho(
        IcmpHandle:    *void,
        DestinationAddress: u32,    // network byte order
        RequestData:   *const void,
        RequestSize:   u16,
        RequestOptions: *const void,
        ReplyBuffer:   *void,
        ReplySize:     u32,
        Timeout:       u32
    ): u32

    // ── Notification ──────────────────────────────────────────────

    /// Receives a notification when the routing table changes.
    /// `hEvent` is signalled on change; combine with `WaitForSingleObject`.
    @lib pub fn NotifyRouteChange(Handle: *void, overlapped: *void): u32

    /// Same for the IP-address table.
    @lib pub fn NotifyAddrChange(Handle: *void, overlapped: *void): u32
}
