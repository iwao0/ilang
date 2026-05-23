/// eappcfg.dll — EAP (Extensible Authentication Protocol) host
/// configuration. Used for 802.1X / VPN / WiFi enterprise auth.
///
/// Reads / writes EAP method configuration blobs (XML-shaped on
/// disk). Pair with `wlanapi.WlanSetProfile` to install the
/// resulting profile.

use windef { HWND }
@extern(C, "eappcfg") {
    /// 8-byte EAP method id — (vendor_id, type_id) pair plus a 3-byte
    /// reserved field, packed.
    pub struct EAP_METHOD_TYPE {
        eapType:                u32          // packed (id + vendor)
        dwAuthorId:             u32
    }

    /// `EAP_ATTRIBUTE`. `dwLength` is in bytes; `pValue` lives in
    /// caller storage.
    pub struct EAP_ATTRIBUTE {
        eaType:     u32                       // EAP_ATTRIBUTE_TYPE enum
        dwLength:   u32
        pValue:     *u8
    }

    pub struct EAP_ATTRIBUTES {
        dwNumberOfAttributes: u32
        _pad:                 u32
        pAttribs:             *EAP_ATTRIBUTE
    }

    /// `EAPHostPeerGetMethods(&pEapMethodInfoArray, &error)`. Free
    /// the array via `EAPHostPeerFreeMemory`.
    @lib pub fn EapHostPeerGetMethods(
        pEapMethodInfoArray: **void,         // *EAP_METHOD_INFO_ARRAY
        ppEapError:          **void
    ): u32

    /// Install / remove a UI extension method into the EAP host.
    @lib pub fn EapHostPeerConfigXml2Blob(
        dwFlags:        u32,
        pConfigDoc:     *void,                // *IXMLDOMNode
        eapMethodType:  *const EAP_METHOD_TYPE,
        pdwSizeOfConfigOut: *u32,
        ppConfigOut:    **u8,
        ppEapError:     **void
    ): u32

    @lib pub fn EapHostPeerConfigBlob2Xml(
        dwFlags:        u32,
        eapMethodType:  *const EAP_METHOD_TYPE,
        dwSizeofConfigIn: u32,
        pConfigIn:      *const u8,
        ppConfigDoc:    **void,
        ppEapError:     **void
    ): u32

    @lib pub fn EapHostPeerInvokeConfigUI(
        hwndParent:     HWND,
        dwFlags:        u32,
        eapMethodType:  *const EAP_METHOD_TYPE,
        dwSizeOfConfigIn: u32,
        pConfigIn:      *const u8,
        pdwSizeOfConfigOut: *u32,
        ppConfigOut:    **u8,
        ppEapError:     **void
    ): u32

    @lib pub fn EapHostPeerQueryCredentialInputFields(
        userImpersonationToken: i64,
        eapMethodType:  *const EAP_METHOD_TYPE,
        dwFlags:        u32,
        dwEapConnDataSize: u32,
        pbEapConnData:  *const u8,
        pEapConfigInputFieldArray: *void,
        ppEapError:     **void
    ): u32

    @lib pub fn EapHostPeerInvokeIdentityUI(
        hwndParent:     HWND,
        dwVersion:      u32,
        dwFlags:        u32,
        eapMethodType:  *const EAP_METHOD_TYPE,
        dwSizeofConnectionData: u32,
        pConnectionData: *const u8,
        dwSizeofUserData: u32,
        pUserData:      *const u8,
        pdwSizeOfUserDataOut: *u32,
        ppUserDataOut:  **u8,
        ppwszIdentity:  **u16,
        ppEapError:     **void,
        ppvReserved:    **void
    ): u32

    /// Free any blob allocated by an EapHostPeer* call.
    @lib pub fn EapHostPeerFreeMemory(pData: *u8)

    @lib pub fn EapHostPeerFreeEapError(pEapError: *void)

    /// Get a friendly EAP error description in `*ppLogonProfile`
    /// (free with `EapHostPeerFreeMemory`).
    @lib pub fn EapHostPeerGetIdentity(
        dwVersion:                  u32,
        dwFlags:                    u32,
        eapMethodType:              *const EAP_METHOD_TYPE,
        dwSizeofConnectionData:     u32,
        pConnectionData:            *const u8,
        dwSizeofUserData:           u32,
        pUserData:                  *const u8,
        hTokenImpersonateUser:      i64,
        pfInvokeUI:                 *i32,
        pdwSizeOfUserDataOut:       *u32,
        ppUserDataOut:              **u8,
        ppwszIdentity:              **u16,
        ppEapError:                 **void
    ): u32

    /// Translate an `EAP_ERROR*` blob to a localised string.
    @lib pub fn EapHostPeerGetMethodProperties(
        dwVersion:        u32,
        dwFlags:          u32,
        eapMethodType:    *const EAP_METHOD_TYPE,
        hUserImpersonationToken: i64,
        dwEapConnDataSize: u32,
        pbEapConnData:    *const u8,
        dwUserDataSize:   u32,
        pbUserData:       *const u8,
        pMethodPropertyArray: *void,
        ppEapError:       **void
    ): u32
}
