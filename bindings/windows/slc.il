/// slc.dll — Software Licensing Client.
///
/// Reads / writes Windows activation state, license info, and
/// digital-rights metadata. Used by `slmgr.vbs`, by the system
/// activation UI, and by enterprise patching tools.

pub enum SlGenuineState: i32 {
    isGenuine     = 0
    invalidLicense = 1
    tamperedBinaries = 2
    offline       = 4
    lastEntry     = 5
}

pub enum SlDataType: i32 {
    sz       = 1        // string
    dword    = 4        // u32
    binary   = 3        // BYTE*
    multisz  = 7
}

@extern(C, "slc") {
    /// Open a licensing handle. Free with `SLClose`. Returns 0 on
    /// success.
    @lib pub fn SLOpen(hSLC: *i64): u32
    @lib pub fn SLClose(hSLC: i64): u32

    /// `SLGetWindowsInformationDWORD(name, value)` — read a
    /// well-known DWORD value. `name` examples:
    ///   "Kernel-ProductInfo"
    ///   "Kernel-BrandingInfo"
    /// Returns 0 on success.
    @lib pub fn SLGetWindowsInformationDWORD(
        pwszValueName: *const u16,
        pdwValue:      *u32
    ): u32

    /// `SLGetWindowsInformation(name, &type, &cbValue, &pbValue)`.
    /// `pbValue` is allocated by the system; free with `LocalFree`.
    @lib pub fn SLGetWindowsInformation(
        pwszValueName: *const u16,
        pdwValueType:  *u32,            // SlDataType
        pcbValue:      *u32,
        ppbValue:      **u8
    ): u32

    /// Activation status. `pGenuineState` receives `SlGenuineState`.
    @lib pub fn SLIsWindowsGenuineLocal(pGenuineState: *i32): u32

    /// Acquire activation grace period (returns minutes remaining).
    @lib pub fn SLGetGracePeriodRemaining(
        hSLC:           i64,
        pSkuId:         *const void,            // *SLID (16 bytes)
        pdwGraceRemaining: *u32
    ): u32

    /// Decompose / inspect the SKU / channel / family currently
    /// licensed.
    @lib pub fn SLGetProductSkuInformation(
        hSLC:           i64,
        pProductSkuId:  *const void,            // *SLID
        pwszValueName:  *const u16,
        peDataType:     *i32,
        pcbValue:       *u32,
        ppbValue:       **u8
    ): u32

    @lib pub fn SLGetApplicationInformation(
        hSLC:           i64,
        pApplicationId: *const void,
        pwszValueName:  *const u16,
        peDataType:     *i32,
        pcbValue:       *u32,
        ppbValue:       **u8
    ): u32

    /// Enumerate the SKUs that match a filter (license state +
    /// application id).
    @lib pub fn SLGetSLIDList(
        hSLC:        i64,
        eQueryIdType: i32,
        pQueryId:    *const void,
        eReturnIdType: i32,
        pnReturnIds: *u32,
        ppReturnIds: **void                     // *SLID*
    ): u32

    /// License-state predicates.
    @lib pub fn SLGetLicensingStatusInformation(
        hSLC:        i64,
        pAppId:      *const void,
        pProductSkuId: *const void,
        pwszRightName: *const u16,
        pnStatusCount: *u32,
        ppLicensingStatus: **void               // *SL_LICENSING_STATUS*
    ): u32

    /// Generate a Confirmation ID from an Installation ID (used by
    /// phone activation).
    @lib pub fn SLGenerateOfflineInstallationId(
        hSLC:            i64,
        pProductSkuId:   *const void,
        ppwszInstallationId: **u16
    ): u32

    @lib pub fn SLDepositOfflineConfirmationId(
        hSLC:               i64,
        pProductSkuId:      *const void,
        pwszInstallationId: *const u16,
        pwszConfirmationId: *const u16
    ): u32

    /// Server-side licensing — install / uninstall product keys.
    @lib pub fn SLInstallProductKey(
        hSLC:        i64,
        pwszProductKey: *const u16
    ): u32

    @lib pub fn SLUninstallProductKey(
        hSLC:        i64,
        pProductSkuId: *const void
    ): u32
}
