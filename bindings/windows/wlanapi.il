/// wlanapi.dll — Wi-Fi enumeration + connect / disconnect.
///
/// Boot:
///   WlanOpenHandle(WLAN_API_VERSION, NULL, &negotiatedVer, &h)
///   WlanEnumInterfaces(h, NULL, &pList)
///   ... iterate pList->InterfaceInfo[i] ...
///   WlanCloseHandle(h, NULL)
///
/// `WLAN_API_VERSION_2_0` = 2.

use winnt { HANDLE }
@extern(C, "wlanapi") {
    /// 144-byte info struct per wireless interface (GUID + name +
    /// state). Layout is simplified to what callers need most.
    pub struct WLAN_INTERFACE_INFO {
        InterfaceGuid: u8[16]                 // GUID
        strInterfaceDescription: u16[256]     // wchar_t[256]
        isState: u32                          // WLAN_INTERFACE_STATE
    }

    /// `WlanEnumInterfaces` returns this header followed by `dwNumberOfItems`
    /// `WLAN_INTERFACE_INFO` entries inline.
    pub struct WLAN_INTERFACE_INFO_LIST {
        dwNumberOfItems: u32
        dwIndex:         u32
        InterfaceInfo:   WLAN_INTERFACE_INFO   // first element; rest follow
    }

    /// `WlanOpenHandle(version, pReserved, pdwNegotiatedVersion, phClientHandle)`.
    /// Returns ERROR_SUCCESS (0) on success.
    @lib pub fn WlanOpenHandle(
        dwClientVersion:       u32,
        pReserved:             *void,
        pdwNegotiatedVersion:  *u32,
        phClientHandle:        *HANDLE
    ): u32

    @lib pub fn WlanCloseHandle(hClientHandle: HANDLE, pReserved: *void): u32

    @lib pub fn WlanEnumInterfaces(
        hClientHandle: HANDLE,
        pReserved:     *void,
        ppInterfaceList: **WLAN_INTERFACE_INFO_LIST
    ): u32

    /// Free a structure allocated by any `Wlan*` function.
    @lib pub fn WlanFreeMemory(pMemory: *void)

    /// Scan the named interface for visible BSSIDs / SSIDs.
    @lib pub fn WlanScan(
        hClientHandle:    HANDLE,
        pInterfaceGuid:   *const void,    // *GUID
        pDot11Ssid:       *const void,    // *DOT11_SSID — may be 0
        pIeData:          *const void,    // optional
        pReserved:        *void
    ): u32

    /// Get the available networks (post-scan).
    @lib pub fn WlanGetAvailableNetworkList(
        hClientHandle:           HANDLE,
        pInterfaceGuid:          *const void,
        dwFlags:                 u32,
        pReserved:               *void,
        ppAvailableNetworkList:  **void   // *WLAN_AVAILABLE_NETWORK_LIST
    ): u32

    /// Connect to a profile by name.
    @lib pub fn WlanConnect(
        hClientHandle:   HANDLE,
        pInterfaceGuid:  *const void,
        pConnectionParameters: *const void,   // *WLAN_CONNECTION_PARAMETERS
        pReserved:       *void
    ): u32

    @lib pub fn WlanDisconnect(
        hClientHandle:   HANDLE,
        pInterfaceGuid:  *const void,
        pReserved:       *void
    ): u32

    /// Get the current connection attributes (SSID, BSSID, signal,
    /// rx/tx rate). Free `*ppData` with `WlanFreeMemory`.
    @lib pub fn WlanQueryInterface(
        hClientHandle:  HANDLE,
        pInterfaceGuid: *const void,
        OpCode:         u32,
        pReserved:      *void,
        pdwDataSize:    *u32,
        ppData:         **void,
        pWlanOpcodeValueType: *u32
    ): u32

    /// Enumerate stored profiles.
    @lib pub fn WlanGetProfileList(
        hClientHandle:    HANDLE,
        pInterfaceGuid:   *const void,
        pReserved:        *void,
        ppProfileList:    **void
    ): u32

    /// Get a profile's XML (settings, plaintext key if requested
    /// + caller has the right). `pdwFlags` may carry
    /// WLAN_PROFILE_GET_PLAINTEXT_KEY = 4.
    @lib pub fn WlanGetProfile(
        hClientHandle: HANDLE,
        pInterfaceGuid: *const void,
        strProfileName: *const u16,
        pReserved:      *void,
        pstrProfileXml: **u16,
        pdwFlags:       *u32,
        pdwGrantedAccess: *u32
    ): u32
}
