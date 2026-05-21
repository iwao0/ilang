/// vssapi.dll — Volume Shadow Copy Service: snapshot-based backup.
///
/// VSS is heavy COM. The headline entry points are flat C calls
/// that hand back an `IVssBackupComponents` (in-process role) or
/// `IVssExpressWriter` (writer plug-ins). Walk the COM vtables
/// from there.
///
/// Bring-up (backup app):
///   let bc: *void = 0 as *void
///   CreateVssBackupComponentsInternal(&bc)
///   bc.InitializeForBackup(NULL)
///   bc.SetBackupState(...)
///   bc.GatherWriterMetadata(...)
///   bc.StartSnapshotSet(&snapId)
///   bc.AddToSnapshotSet(L"C:\\", GUID_PROVIDER_DEFAULT, &snapId)
///   bc.DoSnapshotSet(...)
///   ...
///   bc.BackupComplete(...)
///   bc.Release()

// ─── VSS_OBJECT_TYPE ─────────────────────────────────────────────
pub enum VssObjectType: i32 {
    unknown    = 0
    none       = 1
    snapshotSet = 2
    snapshot   = 3
    provider   = 4
    type_count = 5
}

// ─── VSS_BACKUP_TYPE ─────────────────────────────────────────────
pub enum VssBackupType: i32 {
    undefined    = 0
    full         = 1
    incremental  = 2
    differential = 3
    log          = 4
    copy         = 5
    other        = 6
}

// ─── VSS_SNAPSHOT_STATE ──────────────────────────────────────────
pub enum VssSnapshotState: i32 {
    unknown                   = 0
    preparing                 = 1
    processingPrepare         = 2
    prepared                  = 3
    processingPrecommit       = 4
    precommitted              = 5
    processingCommit          = 6
    committed                 = 7
    processingPostcommit      = 8
    processingPreFinalCommit  = 9
    preFinalCommitted         = 10
    processingPostFinalCommit = 11
    created                   = 12
    aborted                   = 13
    deleted                   = 14
    postCommitted             = 15
    count                     = 16
}

// ─── VSS_VOLUME_SNAPSHOT_ATTRIBUTES — flags ──────────────────────
@flags
pub enum VssVolumeSnapshotAttr: u32 {
    persistent           = 0x01
    noAutorecovery       = 0x02
    clientAccessible     = 0x04
    noAutoRelease        = 0x08
    noWriters            = 0x10
    transportable        = 0x20
    notSurfaced          = 0x40
    notTransacted        = 0x80
    hardwareAssisted     = 0x10000
    differential         = 0x20000
    plex                 = 0x40000
    imported             = 0x80000
    exposedLocally       = 0x100000
    exposedRemotely      = 0x200000
}

@extern(C, "vssapi") {
    /// 16-byte VSS_ID — a GUID under a friendly name. Used for
    /// snapshot ids, provider ids, writer ids, etc.
    pub struct VSS_ID {
        Data: u8[16]
    }

    /// `CreateVssBackupComponentsInternal(&ppBackupComponents)`.
    /// The actual exported flat entry; clients normally use the
    /// inline `CreateVssBackupComponents` wrapper from vsbackup.h
    /// that calls this. Returns S_OK on success.
    @lib pub fn CreateVssBackupComponentsInternal(
        ppBackupComponents: **void           // *IVssBackupComponents
    ): i32

    /// Inverse — frees the wrapper. Provided because some sites
    /// can't call `Release` cleanly across module boundaries.
    @lib pub fn VssFreeSnapshotProperties(
        pProp:    *void
    )

    /// Snapshot properties — 432 bytes on x64 (huge union; pass
    /// `*void` and inspect the first few fields by hand). The
    /// docs spell it as `VSS_SNAPSHOT_PROP`.

    // ── Provider enumeration ────────────────────────────────────

    /// Load the default in-process provider list. Each provider
    /// is a COM object; the discovered `*IVssAdmin` is returned
    /// through `ppProviderAdmin`.
    @lib pub fn CreateVssExamineWriterMetadataInternal(
        bstrXml:                  *const u16,
        ppExamineWriterMetadata:  **void
    ): i32

    /// Walk every registered VSS writer and call `pfnCallback`
    /// for each. `pvParam` is opaque caller context.
    /// (Older API surface; modern code uses IVssBackupComponents.)
    @lib pub fn VssEnumerateWriters(
        pfnCallback: fn(*const void, *void): i32,
        pvParam:     *void
    ): i32
}
