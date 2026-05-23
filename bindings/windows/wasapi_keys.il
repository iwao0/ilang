/// SDK-published property keys + sub-format GUIDs that the WASAPI
/// surface in `wasapi.il` consumes.
///
/// Kept in a sibling file (same pattern as `bindings/directx12/
/// d3d12_iids.il`): consts that reference structs declared inside
/// another module's `@extern(C)` block need to live at module top
/// level with a `use ...` import to bring the type into scope.

use winnt { GUID }
use wasapi { PROPERTYKEY }

/// `PKEY_Device_FriendlyName = a45c254e-df1c-4efd-8020-67d146a850e0`, pid 14.
/// Pass to `IPropertyStore.GetValue` to read the human-readable
/// device name (returned as a `VT_LPWSTR`).
pub const PKEY_Device_FriendlyName: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID {
        Data1: 0xa45c254e, Data2: 0xdf1c, Data3: 0x4efd,
        Data4lo: 0x208020fd, Data4hi: 0xe050a846,
    },
    pid:   14,
    _pad0: 0,
}

/// `PKEY_Device_DeviceDesc = a45c254e-df1c-4efd-8020-67d146a850e0`, pid 2.
/// Device description (manufacturer-provided, often "Speakers" /
/// "Headphones" / "Microphone").
pub const PKEY_Device_DeviceDesc: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID {
        Data1: 0xa45c254e, Data2: 0xdf1c, Data3: 0x4efd,
        Data4lo: 0x208020fd, Data4hi: 0xe050a846,
    },
    pid:   2,
    _pad0: 0,
}

/// `KSDATAFORMAT_SUBTYPE_PCM` — `WAVEFORMATEXTENSIBLE.SubFormat`
/// value for PCM streams. Equals `00000001-0000-0010-8000-00aa00389b71`.
pub const KSDATAFORMAT_SUBTYPE_PCM: GUID = GUID {
    Data1: 0x00000001, Data2: 0x0000, Data3: 0x0010,
    Data4lo: 0xaa000080, Data4hi: 0x719b3800,
}

/// `KSDATAFORMAT_SUBTYPE_IEEE_FLOAT` — sub-format value for the
/// 32-bit-float PCM variant.
/// Equals `00000003-0000-0010-8000-00aa00389b71`.
pub const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID = GUID {
    Data1: 0x00000003, Data2: 0x0000, Data3: 0x0010,
    Data4lo: 0xaa000080, Data4hi: 0x719b3800,
}
