// setupapi.dll — Device installation, INF parsing, device-class
// enumeration.  Used to list installed devices, query their
// hardware IDs, drivers, and registry properties.

// ─── Class GUIDs are 16-byte GUID structs; use ole32's GUID type. ─

// ─── SetupDiGetClassDevs flags (DIGCF_*) ─────────────────────────
@flags
pub enum DiGetClassDevsFlags: u32 {
    default        = 0x00000001   // matched by class only
    present        = 0x00000002   // only currently-attached devices
    allClasses     = 0x00000004
    profile        = 0x00000008
    deviceInterface = 0x00000010
}

// ─── SetupDiGetDeviceRegistryProperty property IDs (SPDRP_*) ─────
pub enum DevRegProp: u32 {
    deviceDesc           = 0x00000000
    hardwareId           = 0x00000001
    compatibleIds        = 0x00000002
    unused               = 0x00000003
    service              = 0x00000004
    unused1              = 0x00000005
    unused2              = 0x00000006
    class                = 0x00000007
    classGuid            = 0x00000008
    driver               = 0x00000009
    configFlags          = 0x0000000A
    mfg                  = 0x0000000B
    friendlyName         = 0x0000000C
    locationInformation  = 0x0000000D
    physicalDeviceObjectName = 0x0000000E
    capabilities         = 0x0000000F
    uiNumber             = 0x00000010
    upperFilters         = 0x00000011
    lowerFilters         = 0x00000012
    busTypeGuid          = 0x00000013
    legacyBusType        = 0x00000014
    busNumber            = 0x00000015
    enumeratorName       = 0x00000016
    security             = 0x00000017
    securitySds          = 0x00000018
    devType              = 0x00000019
    exclusive            = 0x0000001A
    characteristics      = 0x0000001B
    address              = 0x0000001C
    uiNumberDescFormat   = 0x0000001D
    devicePowerData      = 0x0000001E
    removalPolicy        = 0x0000001F
    removalPolicyHwDefault = 0x00000020
    removalPolicyOverride = 0x00000021
    installState         = 0x00000022
    locationPaths        = 0x00000023
    baseContainerId      = 0x00000024
}

// ─── REG_* value type returned from GetDeviceRegistryProperty ────
// (same constants as advapi32's RegValueType)

@extern(C, "setupapi") {
    // ── SP_DEVINFO_DATA — 32 bytes on x64 ────────────────────────

    /// Identifies one device in a device-information set.
    /// Set `cbSize` = 32 before passing to enumeration functions.
    pub struct SP_DEVINFO_DATA {
        cbSize:    u32
        // 4 bytes implicit padding before the inline GUID
        ClassGuid_d1:    u32
        ClassGuid_d2:    u16
        ClassGuid_d3:    u16
        ClassGuid_d4lo:  u32
        ClassGuid_d4hi:  u32
        DevInst:   u32
        Reserved:  u64
    }

    // ── SP_DEVICE_INTERFACE_DATA — 32 bytes ──────────────────────

    pub struct SP_DEVICE_INTERFACE_DATA {
        cbSize:           u32
        InterfaceClassGuid_d1: u32
        InterfaceClassGuid_d2: u16
        InterfaceClassGuid_d3: u16
        InterfaceClassGuid_d4lo: u32
        InterfaceClassGuid_d4hi: u32
        Flags:            u32
        _pad:             u32
        Reserved:         u64
    }

    // ── Device-info set creation ──────────────────────────────────

    /// Returns a handle to a device-information set containing all
    /// devices matching the criteria.  Free with `SetupDiDestroyDeviceInfoList`.
    ///
    /// `ClassGuid`: NULL = all classes (DIGCF_ALLCLASSES required).
    /// `Enumerator`: e.g. "USB", "PCI", or NULL.
    /// `hwndParent`: 0.
    /// `Flags`: DiGetClassDevsFlags.* (typically `present | allClasses`).
    @lib pub fn SetupDiGetClassDevsA(
        ClassGuid:  *const void,    // *GUID, or NULL
        Enumerator: *const char,
        hwndParent: *void,
        Flags:      u32
    ): *void

    @lib pub fn SetupDiDestroyDeviceInfoList(DeviceInfoSet: *void): i32

    // ── Enumeration ───────────────────────────────────────────────

    /// Enumerates devices in the set.  `MemberIndex` starts at 0;
    /// returns FALSE with GetLastError() = ERROR_NO_MORE_ITEMS (259)
    /// when exhausted.
    @lib pub fn SetupDiEnumDeviceInfo(
        DeviceInfoSet: *void,
        MemberIndex:   u32,
        DeviceInfoData: *SP_DEVINFO_DATA
    ): i32

    @lib pub fn SetupDiEnumDeviceInterfaces(
        DeviceInfoSet: *void,
        DeviceInfoData: *const SP_DEVINFO_DATA,
        InterfaceClassGuid: *const void,
        MemberIndex:   u32,
        DeviceInterfaceData: *SP_DEVICE_INTERFACE_DATA
    ): i32

    // ── Property retrieval ────────────────────────────────────────

    /// Returns a registry property for `DeviceInfoData`.
    /// `Property`: DevRegProp.*.
    /// `PropertyRegDataType`: receives the REG_* type.
    /// `PropertyBuffer`/`PropertyBufferSize`/`RequiredSize`: standard
    /// "call with NULL to size, then re-call with buffer" pattern.
    @lib pub fn SetupDiGetDeviceRegistryPropertyA(
        DeviceInfoSet:       *void,
        DeviceInfoData:      *const SP_DEVINFO_DATA,
        Property:            u32,
        PropertyRegDataType: *u32,
        PropertyBuffer:      *u8,
        PropertyBufferSize:  u32,
        RequiredSize:        *u32
    ): i32

    @lib pub fn SetupDiSetDeviceRegistryPropertyA(
        DeviceInfoSet:    *void,
        DeviceInfoData:   *SP_DEVINFO_DATA,
        Property:         u32,
        PropertyBuffer:   *const u8,
        PropertyBufferSize: u32
    ): i32

    /// Returns the device-instance ID string (e.g.
    /// "USB\\VID_046D&PID_C534\\7&33567B85&0&1").
    @lib pub fn SetupDiGetDeviceInstanceIdA(
        DeviceInfoSet:  *void,
        DeviceInfoData: *const SP_DEVINFO_DATA,
        DeviceInstanceId: *char,
        DeviceInstanceIdSize: u32,
        RequiredSize:   *u32
    ): i32

    // ── Device interface detail ───────────────────────────────────

    /// Returns the device-path string for an interface (the path you
    /// pass to CreateFileA to open the device).
    /// `DeviceInterfaceDetailData`: caller-allocated buffer that
    /// begins with a u32 `cbSize` field set to 8 on x64 (one DWORD +
    /// alignment), followed by the path string.
    @lib pub fn SetupDiGetDeviceInterfaceDetailA(
        DeviceInfoSet:  *void,
        DeviceInterfaceData: *const SP_DEVICE_INTERFACE_DATA,
        DeviceInterfaceDetailData: *void,
        DeviceInterfaceDetailDataSize: u32,
        RequiredSize:   *u32,
        DeviceInfoData: *SP_DEVINFO_DATA
    ): i32

    // ── INF parsing (driver installation) ─────────────────────────

    /// Opens an INF file.  `InfFileName`: full path to .inf,
    /// `InfClass`: NULL to read class from the file.
    /// `InfStyle`: 2 (INF_STYLE_WIN4) for modern INF.
    /// `ErrorLine`: receives the line of the first parse error.
    @lib pub fn SetupOpenInfFileA(
        FileName:  *const char,
        InfClass:  *const char,
        InfStyle:  u32,
        ErrorLine: *u32
    ): *void

    @lib pub fn SetupCloseInfFile(InfHandle: *void)

    @lib pub fn SetupFindFirstLineA(
        InfHandle: *void,
        Section:   *const char,
        Key:       *const char,
        Context:   *void          // *INFCONTEXT
    ): i32

    @lib pub fn SetupFindNextLine(
        ContextIn:  *void,
        ContextOut: *void
    ): i32

    @lib pub fn SetupGetStringFieldA(
        Context:   *void,
        FieldIndex: u32,
        ReturnBuffer: *char,
        ReturnBufferSize: u32,
        RequiredSize: *u32
    ): i32
}
