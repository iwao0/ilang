/// wldap32.dll — LDAP client (queries against Active Directory or
/// any RFC-2251 LDAP server).
///
/// Common pattern:
///   let ld = ldap_init("dc.example.com", 389)
///   ldap_set_option(ld, LDAP_OPT_VERSION, &v3)
///   ldap_bind_s(ld, NULL, &cred, LDAP_AUTH_SIMPLE)
///   ldap_search_s(ld, "DC=example,DC=com", LDAP_SCOPE_SUBTREE,
///                 "(objectClass=user)", NULL, 0, &result)
///   for entry in ldap_first_entry / ldap_next_entry {
///       for name in ldap_first_attribute / ldap_next_attribute {
///           let vals = ldap_get_values(ld, entry, name)
///           ...
///           ldap_value_free(vals)
///       }
///   }
///   ldap_msgfree(result)
///   ldap_unbind(ld)

// ─── LDAP scope codes ────────────────────────────────────────────
pub enum LdapScope: i32 {
    base     = 0
    oneLevel = 1
    subtree  = 2
    children = 3
}

// ─── LDAP_AUTH_* — bind methods ──────────────────────────────────
pub enum LdapAuth: u32 {
    simple          = 0x80
    sasl            = 0x83
    other           = 0x86
    external        = 0xA0
    sicily          = 0x200
    digestMd5       = 0x4080
    msn             = 0x800
    ntlm            = 0x1086
    dpa             = 0x2086
    negotiate       = 0x486
    sspi            = 0x486
}

// ─── LDAP_OPT_* — option ids for ldap_set_option ─────────────────
pub enum LdapOpt: i32 {
    desc            = 0x01
    deref           = 0x02
    sizelimit       = 0x03
    timelimit       = 0x04
    refHopLimit     = 0x05
    protocolVersion = 0x11
    serverControls  = 0x12
    apiInfo         = 0x00
    referrals       = 0x08
    restart         = 0x09
    ssl             = 0x0A
    sendTimeout     = 0x42
    receiveTimeout  = 0x43
    autoReconnect   = 0x91
    timeout         = 0x5002
}

// ─── Error codes ─────────────────────────────────────────────────
pub enum LdapError: u32 {
    success                  = 0
    operationsError          = 1
    protocolError            = 2
    timelimitExceeded        = 3
    sizelimitExceeded        = 4
    noSuchAttribute          = 16
    invalidCredentials       = 49
    insufficientAccessRights = 50
    busy                     = 51
    unavailable              = 52
    unwillingToPerform       = 53
    serverDown               = 0x51
    localError               = 0x52
    encodingError            = 0x53
    decodingError            = 0x54
    timeout                  = 0x55
    authUnknown              = 0x56
    filterError              = 0x57
    userCancelled            = 0x58
    paramError               = 0x59
    noMemory                 = 0x5A
    connectError             = 0x5B
}

@extern(C, "wldap32") {
    /// Opaque LDAP connection handle.
    pub struct LDAP {}

    /// `LDAPMessage` opaque chain — `ldap_first_entry` advances
    /// through search results.
    pub struct LDAPMessage {}

    /// `BERVAL` — length-prefixed byte string used by SASL-bound
    /// credentials + binary attribute values.
    pub struct berval {
        bv_len:  u32
        _pad:    u32
        bv_val:  *char
    }

    // ── Connect / bind / unbind ──────────────────────────────────

    /// Connect to `HostName:PortNumber`. Returns `*LDAP` or NULL.
    @lib pub fn ldap_initA(HostName: *const char, PortNumber: u32): *LDAP
    @lib pub fn ldap_initW(HostName: *const u16, PortNumber: u32): *LDAP

    /// Like `ldap_init` but adds SSL when `secure != 0`.
    @lib pub fn ldap_sslinitA(HostName: *const char, PortNumber: u32, secure: i32): *LDAP
    @lib pub fn ldap_sslinitW(HostName: *const u16, PortNumber: u32, secure: i32): *LDAP

    /// `ldap_bind_sA(ld, dn, cred, method)`. Synchronous. `cred`
    /// is the password (Simple) or a `SEC_WINNT_AUTH_IDENTITY*`
    /// (Sicily / NTLM / Negotiate).
    @lib pub fn ldap_bind_sA(
        ld:     *LDAP,
        dn:     *const char,
        cred:   *const char,
        method: u32
    ): u32

    @lib pub fn ldap_bind_sW(
        ld:     *LDAP,
        dn:     *const u16,
        cred:   *const u16,
        method: u32
    ): u32

    /// Disconnect + free.
    @lib pub fn ldap_unbind(ld: *LDAP): u32
    @lib pub fn ldap_unbind_s(ld: *LDAP): u32

    // ── Options ──────────────────────────────────────────────────

    @lib pub fn ldap_set_option(ld: *LDAP, option: i32, invalue: *const void): u32
    @lib pub fn ldap_get_option(ld: *LDAP, option: i32, outvalue: *void): u32

    // ── Search ───────────────────────────────────────────────────

    /// Synchronous search. `attrs` is a NULL-terminated list of
    /// attribute names (or NULL for all). `attrsonly`: 1 = names
    /// only (no values).
    @lib pub fn ldap_search_sA(
        ld:        *LDAP,
        base:      *const char,
        scope:     i32,
        filter:    *const char,
        attrs:     **const char,
        attrsonly: i32,
        res:       **LDAPMessage
    ): u32

    @lib pub fn ldap_search_sW(
        ld:        *LDAP,
        base:      *const u16,
        scope:     i32,
        filter:    *const u16,
        attrs:     **const u16,
        attrsonly: i32,
        res:       **LDAPMessage
    ): u32

    @lib pub fn ldap_search_ext_sA(
        ld:           *LDAP,
        base:         *const char,
        scope:        i32,
        filter:       *const char,
        attrs:        **const char,
        attrsonly:    i32,
        ServerControls: *const void,
        ClientControls: *const void,
        timeout:      *const void,           // *LDAP_TIMEVAL
        SizeLimit:    u32,
        res:          **LDAPMessage
    ): u32
    @lib pub fn ldap_search_ext_sW(
        ld:           *LDAP,
        base:         *const u16,
        scope:        i32,
        filter:       *const u16,
        attrs:        **const u16,
        attrsonly:    i32,
        ServerControls: *const void,
        ClientControls: *const void,
        timeout:      *const void,           // *LDAP_TIMEVAL
        SizeLimit:    u32,
        res:          **LDAPMessage
    ): u32

    @lib pub fn ldap_msgfree(res: *LDAPMessage): u32

    // ── Result iteration ────────────────────────────────────────

    @lib pub fn ldap_first_entry(ld: *LDAP, res: *LDAPMessage): *LDAPMessage
    @lib pub fn ldap_next_entry(ld: *LDAP, entry: *LDAPMessage): *LDAPMessage
    @lib pub fn ldap_count_entries(ld: *LDAP, res: *LDAPMessage): i32

    /// `ldap_get_dn` returns a malloc-allocated DN; free with
    /// `ldap_memfree`.
    @lib pub fn ldap_get_dnA(ld: *LDAP, entry: *LDAPMessage): *char
    @lib pub fn ldap_get_dnW(ld: *LDAP, entry: *LDAPMessage): *u16
    @lib pub fn ldap_memfree(block: *void)

    // ── Attribute iteration ─────────────────────────────────────

    @lib pub fn ldap_first_attributeA(
        ld:    *LDAP,
        entry: *LDAPMessage,
        ptr:   **void                       // *BerElement (state)
    ): *char
    @lib pub fn ldap_first_attributeW(
        ld:    *LDAP,
        entry: *LDAPMessage,
        ptr:   **void                       // *BerElement (state)
    ): *u16

    @lib pub fn ldap_next_attributeA(
        ld:    *LDAP,
        entry: *LDAPMessage,
        ptr:   *void
    ): *char
    @lib pub fn ldap_next_attributeW(
        ld:    *LDAP,
        entry: *LDAPMessage,
        ptr:   *void
    ): *u16

    @lib pub fn ber_free(pBerElement: *void, fbuf: i32)

    /// Get attribute string values. Returns a NULL-terminated
    /// array. Free with `ldap_value_free`.
    @lib pub fn ldap_get_valuesA(
        ld:    *LDAP,
        entry: *LDAPMessage,
        attr:  *const char
    ): **char
    @lib pub fn ldap_get_valuesW(
        ld:    *LDAP,
        entry: *LDAPMessage,
        attr:  *const u16
    ): **u16

    @lib pub fn ldap_value_free(vals: **char): u32

    /// Binary attribute values (everything in AD that isn't a
    /// printable string).
    @lib pub fn ldap_get_values_lenA(
        ld:    *LDAP,
        entry: *LDAPMessage,
        attr:  *const char
    ): **berval
    @lib pub fn ldap_get_values_lenW(
        ld:    *LDAP,
        entry: *LDAPMessage,
        attr:  *const u16
    ): **berval

    @lib pub fn ldap_value_free_len(vals: **berval): u32

    // ── Modify ──────────────────────────────────────────────────

    @lib pub fn ldap_add_sA(
        ld:    *LDAP,
        dn:    *const char,
        mods:  **const void                 // *LDAPMod*[]
    ): u32
    @lib pub fn ldap_add_sW(
        ld:    *LDAP,
        dn:    *const u16,
        mods:  **const void                 // *LDAPMod*[]
    ): u32

    @lib pub fn ldap_modify_sA(
        ld:    *LDAP,
        dn:    *const char,
        mods:  **const void
    ): u32
    @lib pub fn ldap_modify_sW(
        ld:    *LDAP,
        dn:    *const u16,
        mods:  **const void
    ): u32

    @lib pub fn ldap_delete_sA(
        ld:    *LDAP,
        dn:    *const char
    ): u32
    @lib pub fn ldap_delete_sW(
        ld:    *LDAP,
        dn:    *const u16
    ): u32

    @lib pub fn ldap_rename_extA(
        ld:           *LDAP,
        dn:           *const char,
        NewRDN:       *const char,
        NewParent:    *const char,
        DeleteOldRdn: i32,
        ServerControls: *const void,
        ClientControls: *const void,
        MessageNumber: *i32
    ): u32
    @lib pub fn ldap_rename_extW(
        ld:           *LDAP,
        dn:           *const u16,
        NewRDN:       *const u16,
        NewParent:    *const u16,
        DeleteOldRdn: i32,
        ServerControls: *const void,
        ClientControls: *const void,
        MessageNumber: *i32
    ): u32

    /// Stringified error.
    @lib pub fn ldap_err2stringA(err: u32): *const char
    @lib pub fn ldap_err2stringW(err: u32): *const u16
}
