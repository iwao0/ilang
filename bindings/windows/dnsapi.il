/// dnsapi.dll — DNS name resolution + cache helpers.
///
/// `DnsQuery_A(name, type, options, extra, ppRecord, pReserved)`
/// is the workhorse. Returns 0 on success and writes a linked
/// list of records to `*ppRecord`; free with `DnsRecordListFree`.

// ─── DNS record types (DNS_TYPE_*) ───────────────────────────────
pub enum DnsType: u16 {
    a       = 0x0001    // IPv4 address
    ns      = 0x0002    // name server
    cname   = 0x0005    // canonical name
    soa     = 0x0006    // start of authority
    ptr     = 0x000C    // pointer
    mx      = 0x000F    // mail exchanger
    txt     = 0x0010    // text strings
    aaaa    = 0x001C    // IPv6 address
    srv     = 0x0021    // service location
    naptr   = 0x0023
    any     = 0x00FF
}

// ─── DnsQuery options (DNS_QUERY_*) ──────────────────────────────
@flags
pub enum DnsQueryOptions: u32 {
    standard           = 0
    acceptTruncatedResponse = 0x00000001
    useTcpOnly         = 0x00000002
    noRecursion        = 0x00000004
    bypassCache        = 0x00000008
    noWireQuery        = 0x00000010
    noLocalName        = 0x00000020
    noHostsFile        = 0x00000040
    noNetbt            = 0x00000080
    wireOnly           = 0x00000100
    returnMessage      = 0x00000200
    multicastOnly      = 0x00000400
    noMulticast        = 0x00000800
    treatAsFqdn        = 0x00001000
}

@extern(C, "Dnsapi") {
    /// DNS record header — the union fields below depend on
    /// `wType` and live in the trailing 16 bytes. For type-A
    /// records the IPv4 address is at offset 24 from the start
    /// of the record (`IpAddress: u32`); for type-AAAA the
    /// IPv6 16-byte array follows the same offset.
    pub struct DNS_RECORD {
        pNext:       *DNS_RECORD
        pName:       *const char
        wType:       u16
        wDataLength: u16
        Flags:       u32
        dwTtl:       u32
        dwReserved:  u32
        /// Type-dependent payload — caller reads the right
        /// interpretation based on `wType`. 16 bytes covers
        /// every type up to AAAA.
        Data:        u8[16]
    }

    /// `DnsQuery_A(pszName, wType, options, pExtra, ppQueryResult, pReserved)`.
    /// Returns 0 (ERROR_SUCCESS) on success, a Win32 error on failure
    /// (DNS_ERROR_RCODE_NAME_ERROR = 9003 for NXDOMAIN, etc.).
    @lib pub fn DnsQuery_A(
        pszName:       *const char,
        wType:         u16,
        Options:       u32,
        pExtra:        *void,        // *PIP4_ARRAY — DNS servers; 0 for default
        ppQueryResult: **DNS_RECORD,
        pReserved:     *void
    ): i32

    /// Wide variant.
    @lib pub fn DnsQuery_W(
        pszName:       *const u16,
        wType:         u16,
        Options:       u32,
        pExtra:        *void,
        ppQueryResult: **DNS_RECORD,
        pReserved:     *void
    ): i32

    /// Free the linked list returned by `DnsQuery_*`.
    /// `FreeType`: DnsFreeFlat 0 / DnsFreeRecordList 1 /
    /// DnsFreeParsedMessageFields 2.
    @lib pub fn DnsRecordListFree(pRecordList: *DNS_RECORD, FreeType: i32)

    /// Clear the resolver cache.
    @lib pub fn DnsFlushResolverCache(): i32

    /// Validate a host name. Returns 0 on success.
    @lib pub fn DnsValidateName_A(pszName: *const char, format: i32): i32
    @lib pub fn DnsValidateName_W(pszName: *const u16, format: i32): i32
}
