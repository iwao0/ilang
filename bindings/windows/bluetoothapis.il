/// bthprops.cpl — Bluetooth discovery + device / service inquiry.
///
/// Both classic Bluetooth (RFCOMM, SDP) and Bluetooth LE (GATT
/// services / characteristics) are accessible from this surface.
/// LE GATT specifics route through `BluetoothAPIs.h` exports that
/// physically live in `bthprops.cpl`.

@flags
pub enum BluetoothSearchFlag: u32 {
    authenticated     = 0x01
    remembered        = 0x02
    unknown_          = 0x04
    connected         = 0x08
    issueInquiry      = 0x10
    returnAll         = 0x1F
}

use user32 { HWND }

@extern(C, "BluetoothApis", "bthprops.cpl") {
    /// Opaque radio handle (returned by BluetoothFindFirstRadio).
    @handle pub struct HBLUETOOTH_RADIO {}

    /// Opaque device-enumeration handle (BluetoothFindFirstDevice).
    @handle pub struct HBLUETOOTH_DEVICE_FIND {}

    /// Opaque radio-enumeration handle (BluetoothFindFirstRadio).
    @handle pub struct HBLUETOOTH_RADIO_FIND {}


    /// 48-bit MAC address (8 bytes — 6 active + 2 padding).
    pub struct BLUETOOTH_ADDRESS {
        ullLong: u64
    }

    /// `BLUETOOTH_DEVICE_SEARCH_PARAMS` (32 bytes on x64).
    pub struct BLUETOOTH_DEVICE_SEARCH_PARAMS {
        dwSize:               u32
        fReturnAuthenticated: i32
        fReturnRemembered:    i32
        fReturnUnknown:       i32
        fReturnConnected:     i32
        fIssueInquiry:        i32
        cTimeoutMultiplier:   u8
        _pad:                 u8[3]
        hRadio:               HBLUETOOTH_RADIO
    }

    /// 560 bytes on x64 — minus the name buffer this is mostly
    /// status flags + the device GUID.
    pub struct BLUETOOTH_DEVICE_INFO {
        dwSize:        u32
        _pad:          u32
        Address:       BLUETOOTH_ADDRESS
        ulClassofDevice: u32
        fConnected:    i32
        fRemembered:   i32
        fAuthenticated: i32
        stLastSeen:    u8[16]            // SYSTEMTIME
        stLastUsed:    u8[16]
        szName:        u16[248]          // BLUETOOTH_MAX_NAME_SIZE = 248
    }

    /// Open a search handle. Returns 0 on failure; otherwise pass
    /// the result to `BluetoothFindNextDevice` until that returns
    /// FALSE, then close via `BluetoothFindDeviceClose`.
    @lib pub fn BluetoothFindFirstDevice(
        pbtsp:        *const BLUETOOTH_DEVICE_SEARCH_PARAMS,
        pbtdi:        *BLUETOOTH_DEVICE_INFO
    ): HBLUETOOTH_DEVICE_FIND

    @lib pub fn BluetoothFindNextDevice(
        hFind:        HBLUETOOTH_DEVICE_FIND,
        pbtdi:        *BLUETOOTH_DEVICE_INFO
    ): i32

    @lib pub fn BluetoothFindDeviceClose(hFind: HBLUETOOTH_DEVICE_FIND): i32

    /// `BluetoothFindFirstRadio(params, &handle)`.
    @lib pub fn BluetoothFindFirstRadio(
        pbtfrp:       *const void,        // *BLUETOOTH_FIND_RADIO_PARAMS
        phRadio:      *HBLUETOOTH_RADIO
    ): HBLUETOOTH_RADIO_FIND

    @lib pub fn BluetoothFindNextRadio(hFind: HBLUETOOTH_RADIO_FIND, phRadio: *HBLUETOOTH_RADIO): i32
    @lib pub fn BluetoothFindRadioClose(hFind: HBLUETOOTH_RADIO_FIND): i32

    /// Radio info — 528 bytes.
    @lib pub fn BluetoothGetRadioInfo(
        hRadio:       HBLUETOOTH_RADIO,
        pRadioInfo:   *void
    ): u32

    @lib pub fn BluetoothIsDiscoverable(hRadio: HBLUETOOTH_RADIO): i32
    @lib pub fn BluetoothEnableDiscovery(hRadio: HBLUETOOTH_RADIO, fEnabled: i32): i32

    @lib pub fn BluetoothIsConnectable(hRadio: HBLUETOOTH_RADIO): i32
    @lib pub fn BluetoothEnableIncomingConnections(hRadio: HBLUETOOTH_RADIO, fEnabled: i32): i32

    // ── Pairing / authentication ────────────────────────────────

    @lib pub fn BluetoothAuthenticateDeviceEx(
        hwndParentIn: HWND,
        hRadioIn:     HBLUETOOTH_RADIO,
        pbtdiInOut:   *BLUETOOTH_DEVICE_INFO,
        pbtOobData:   *void,
        authMethod:   i32              // BLUETOOTH_AUTHENTICATION_METHOD
    ): u32

    @lib pub fn BluetoothRemoveDevice(
        pAddress:     *const BLUETOOTH_ADDRESS
    ): u32

    @lib pub fn BluetoothUpdateDeviceRecord(
        pbtdi:        *const BLUETOOTH_DEVICE_INFO
    ): u32

    /// Display the system "Add a Bluetooth device" wizard.
    @lib pub fn BluetoothSelectDevices(
        pbtsdp:       *void              // *BLUETOOTH_SELECT_DEVICE_PARAMS
    ): i32
}
