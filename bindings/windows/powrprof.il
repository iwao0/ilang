/// powrprof.dll — Power profile + sleep / hibernate control.
///
/// Two common uses:
///   1. Sleep / hibernate the machine — `SetSuspendState`.
///   2. Read or change the active power scheme (battery profile).

@extern(C, "powrprof") {
    /// `SetSuspendState(hibernate, forceCritical, disableWakeEvent)`.
    /// `hibernate`: 0 = sleep (S3), 1 = hibernate (S4).
    /// Returns non-zero on success.
    @lib pub fn SetSuspendState(
        hibernate:        i32,
        forceCritical:    i32,
        disableWakeEvent: i32
    ): i32

    /// Trigger a "Power Throttling" hint to the kernel for the
    /// current process.
    @lib pub fn PowerSetActiveScheme(
        UserRootPowerKey: *void,        // NULL = default
        SchemeGuid:       *const void   // *GUID
    ): u32

    @lib pub fn PowerGetActiveScheme(
        UserRootPowerKey: *void,
        ActivePolicyGuid: **void        // *GUID, freed with LocalFree
    ): u32

    /// Read a setting under the named scheme + subgroup. `BufferSize`
    /// is in/out: cap on input, actual on output. The value is
    /// stored at the end of `Buffer` as a u32 / u64 / blob.
    @lib pub fn PowerReadACValue(
        RootPowerKey:    *void,
        SchemeGuid:      *const void,
        SubGroupOfPowerSettingsGuid: *const void,
        PowerSettingGuid: *const void,
        Type:            *u32,
        Buffer:          *u8,
        BufferSize:      *u32
    ): u32

    @lib pub fn PowerReadDCValue(
        RootPowerKey:    *void,
        SchemeGuid:      *const void,
        SubGroupOfPowerSettingsGuid: *const void,
        PowerSettingGuid: *const void,
        Type:            *u32,
        Buffer:          *u8,
        BufferSize:      *u32
    ): u32

    @lib pub fn PowerWriteACValueIndex(
        RootPowerKey:    *void,
        SchemeGuid:      *const void,
        SubGroupOfPowerSettingsGuid: *const void,
        PowerSettingGuid: *const void,
        AcValueIndex:    u32
    ): u32

    @lib pub fn PowerWriteDCValueIndex(
        RootPowerKey:    *void,
        SchemeGuid:      *const void,
        SubGroupOfPowerSettingsGuid: *const void,
        PowerSettingGuid: *const void,
        DcValueIndex:    u32
    ): u32

    /// Apply pending writes to the active scheme so the system
    /// actually picks them up.
    @lib pub fn PowerApplySettingChanges(
        SubGroupOfPowerSettingsGuid: *const void,
        PowerSettingGuid: *const void
    ): u32

    /// Enumerate the schemes / subgroups / settings tree.
    /// `AccessFlags`: 16 = ACCESS_SCHEME, 17 = ACCESS_SUBGROUP,
    /// 18 = ACCESS_INDIVIDUAL_SETTING.
    @lib pub fn PowerEnumerate(
        RootPowerKey:    *void,
        SchemeGuid:      *const void,
        SubGroupOfPowerSettingsGuid: *const void,
        AccessFlags:     u32,
        Index:           u32,
        Buffer:          *u8,
        BufferSize:      *u32
    ): u32

    @lib pub fn PowerReadFriendlyName(
        RootPowerKey:    *void,
        SchemeGuid:      *const void,
        SubGroupOfPowerSettingsGuid: *const void,
        PowerSettingGuid: *const void,
        Buffer:          *u8,
        BufferSize:      *u32
    ): u32

    /// Returns the system's idle time in 100-ns ticks.
    @lib pub fn CallNtPowerInformation(
        InformationLevel: i32,        // POWER_INFORMATION_LEVEL
        InputBuffer:      *void,
        InputBufferLength: u32,
        OutputBuffer:     *void,
        OutputBufferLength: u32
    ): i32
}
