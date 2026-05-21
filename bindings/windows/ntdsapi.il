/// ntdsapi.dll — Active Directory: DNS / SPN / Domain controller
/// discovery, name translation, CRACK helpers.
///
/// Higher-level AD operations live in `wldap32` (LDAP) and
/// `activeds` (the ADSI COM API). This file binds the lower-level
/// flat utility surface.

pub enum DsNameFormat: i32 {
    unknown            = 0
    fullyQualifiedDN   = 1
    samCompatible      = 2
    display            = 3
    uniqueId           = 6
    canonical          = 7
    userPrincipal      = 8
    canonicalEx        = 9
    serviceName        = 10
    sidOrSidHistory    = 11
    dnsDomain          = 12
}

pub enum DsNameError: i32 {
    noError             = 0
    errResolving        = 1
    errNotFound         = 2
    errNotUnique        = 3
    errNoMapping        = 4
    errDomainOnly       = 5
    errNoSyntacticalMapping = 6
    errTrustReferral    = 7
}

@flags
pub enum DcGetFlags: u32 {
    forceRediscovery       = 0x00000001
    directoryServiceRequired = 0x00000010
    directoryServicePreferred = 0x00000020
    gcServerRequired       = 0x00000040
    pdcRequired            = 0x00000080
    isFlatName             = 0x00010000
    isDnsName              = 0x00020000
    returnDnsName          = 0x40000000
    returnFlatName         = 0x80000000
}

@extern(C, "ntdsapi") {
    /// `DOMAIN_CONTROLLER_INFOA` — 80 bytes. Returned by
    /// `DsGetDcNameA`; free with `NetApiBufferFree` from netapi32.
    pub struct DOMAIN_CONTROLLER_INFOA {
        DomainControllerName:        *const char
        DomainControllerAddress:     *const char
        DomainControllerAddressType: u32
        _pad:                        u32
        DomainGuid:                  u8[16]
        DomainName:                  *const char
        DnsForestName:               *const char
        Flags:                       u32
        _pad2:                       u32
        DcSiteName:                  *const char
        ClientSiteName:              *const char
    }

    /// Locate a domain controller for `DomainName`. `SiteName`
    /// may be NULL (use the calling computer's site). Returns
    /// ERROR_SUCCESS (0) on success.
    @lib pub fn DsGetDcNameA(
        ComputerName:    *const char,
        DomainName:      *const char,
        DomainGuid:      *const void,
        SiteName:        *const char,
        Flags:           u32,
        DomainControllerInfo: **DOMAIN_CONTROLLER_INFOA
    ): u32

    @lib pub fn DsGetDcNameW(
        ComputerName:    *const u16,
        DomainName:      *const u16,
        DomainGuid:      *const void,
        SiteName:        *const u16,
        Flags:           u32,
        DomainControllerInfo: **void   // *DOMAIN_CONTROLLER_INFOW
    ): u32

    // ── DS name translation ─────────────────────────────────────

    /// Open a handle to a DS connection for batch name lookups.
    @lib pub fn DsBindA(
        DomainControllerName: *const char,
        DnsDomainName:        *const char,
        phDS:                 *i64
    ): u32

    @lib pub fn DsUnBindA(phDS: *i64): u32

    /// `DsCrackNamesA(hDS, flags, formatOffered, formatDesired,
    ///                cNames, rpNames, &result)`. `result` is freed
    /// with `DsFreeNameResultA`.
    @lib pub fn DsCrackNamesA(
        hDS:             i64,
        flags:           u32,
        formatOffered:   i32,           // DsNameFormat
        formatDesired:   i32,
        cNames:          u32,
        rpNames:         **const char,
        ppResult:        **void         // *DS_NAME_RESULT_A
    ): u32

    @lib pub fn DsFreeNameResultA(pResult: *void)

    // ── SPN helpers ─────────────────────────────────────────────

    /// Compose / decompose a Service Principal Name. Returns 0 on
    /// success. Free `*ppszSpn` with `NetApiBufferFree`.
    @lib pub fn DsMakeSpnA(
        ServiceClass:  *const char,
        ServiceName:   *const char,
        InstanceName:  *const char,
        InstancePort:  u16,
        Referrer:      *const char,
        pcSpnLength:   *u32,
        pszSpn:        *char
    ): u32

    @lib pub fn DsCrackSpnA(
        pszSpn:          *const char,
        pcServiceClass:  *u32,
        ServiceClass:    *char,
        pcServiceName:   *u32,
        ServiceName:     *char,
        pcInstanceName:  *u32,
        InstanceName:    *char,
        pInstancePort:   *u16
    ): u32

    @lib pub fn DsGetSpnA(
        ServiceType:   i32,
        ServiceClass:  *const char,
        ServiceName:   *const char,
        InstancePort:  u16,
        cInstanceNames: u16,
        pInstanceNames: **const char,
        pInstancePorts: *const u16,
        pcSpn:          *u32,
        prpszSpn:       ***char
    ): u32

    @lib pub fn DsWriteAccountSpnA(
        hDS:           i64,
        Operation:     i32,             // 1=add, 2=replace, 3=delete
        pszAccount:    *const char,
        cSpn:          u32,
        rpszSpn:       **const char
    ): u32

    // ── Forest topology ─────────────────────────────────────────

    /// Enumerate the sites in the forest.
    @lib pub fn DsListSitesA(
        hDs:             i64,
        ppSites:         **void
    ): u32

    @lib pub fn DsListDomainsInSiteA(
        hDs:             i64,
        site:            *const char,
        ppDomains:       **void
    ): u32

    @lib pub fn DsListServersForDomainInSiteA(
        hDs:             i64,
        domain:          *const char,
        site:            *const char,
        ppServers:       **void
    ): u32

}
