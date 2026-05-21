/// virtdisk.dll — Virtual Hard Disk (VHD / VHDX / ISO) operations.
///
/// Lifecycle:
///   1. OpenVirtualDisk      — open an existing .vhd / .vhdx / .iso
///   2. AttachVirtualDisk    — surface as a system drive (admin)
///   3. ... usage via the new drive letter ...
///   4. DetachVirtualDisk
///   5. CloseHandle (kernel32) — release the handle
///
/// Create a new VHD:
///   CreateVirtualDisk(&VIRTUAL_STORAGE_TYPE, L"new.vhdx", ...,
///                     &CREATE_VIRTUAL_DISK_PARAMETERS, NULL,
///                     &handle)
///
/// All entry points are admin-only on production systems.

// ─── VIRTUAL_DISK_ACCESS_MASK ────────────────────────────────────
@flags
pub enum VirtualDiskAccessMask: u32 {
    none           = 0
    attachRO       = 0x00010000
    attachRW       = 0x00020000
    detach         = 0x00040000
    getInfo        = 0x00080000
    create         = 0x00100000
    metaops        = 0x00200000
    read           = 0x000D0000
    all            = 0x003F0000
}

// ─── ATTACH_VIRTUAL_DISK_FLAG ────────────────────────────────────
@flags
pub enum AttachVirtualDiskFlag: u32 {
    none                  = 0
    readonly              = 0x1
    noDriveLetter         = 0x2
    permanentLifetime     = 0x4
    noLocalHost           = 0x8
    noSecurityDescriptor  = 0x10
    bypassDefaultEncryptionPolicy = 0x20
}

// ─── DETACH_VIRTUAL_DISK_FLAG ────────────────────────────────────
@flags
pub enum DetachVirtualDiskFlag: u32 {
    none = 0
}

// ─── OPEN_VIRTUAL_DISK_FLAG ──────────────────────────────────────
@flags
pub enum OpenVirtualDiskFlag: u32 {
    none                = 0
    noParents           = 0x1
    blankFile           = 0x2
    bootDrive           = 0x4
    cachedIO            = 0x8
    customDiffChain     = 0x10
    parentCachedIO      = 0x20
    vhdsetFileOnly      = 0x40
    ignoreRelativeParentLocator = 0x80
}

@extern(C, "virtdisk") {
    /// 20 bytes (DeviceId + GUID). VendorId for Microsoft VHDs:
    /// {EC984AEC-A0F9-47e9-901F-71415A66345B}.
    pub struct VIRTUAL_STORAGE_TYPE {
        DeviceId:   u32           // 1 = ISO, 2 = VHD, 3 = VHDX
        VendorId:   u8[16]        // GUID
    }

    /// CreateVirtualDisk parameters (V1 — 56 bytes on x64).
    pub struct CREATE_VIRTUAL_DISK_PARAMETERS {
        Version:    u32
        _pad:       u32
        UniqueId:   u8[16]
        MaximumSize: u64
        BlockSizeInBytes: u32
        SectorSizeInBytes: u32
        ParentPath:    *const u16
        SourcePath:    *const u16
    }

    /// `OpenVirtualDisk(storageType, path, accessMask, flags,
    ///                  parameters, handle)`. Returns 0 on success.
    @lib pub fn OpenVirtualDisk(
        VirtualStorageType: *const VIRTUAL_STORAGE_TYPE,
        Path:           *const u16,
        VirtualDiskAccessMask: u32,
        Flags:          u32,
        Parameters:     *const void,        // *OPEN_VIRTUAL_DISK_PARAMETERS
        Handle:         *i64
    ): u32

    /// Attach the VHD to the running system as a real disk.
    /// `securityDescriptor` may be NULL.
    @lib pub fn AttachVirtualDisk(
        VirtualDiskHandle: i64,
        SecurityDescriptor: *const void,
        Flags:             u32,
        ProviderSpecificFlags: u32,
        Parameters:        *const void,    // *ATTACH_VIRTUAL_DISK_PARAMETERS
        Overlapped:        *void
    ): u32

    @lib pub fn DetachVirtualDisk(
        VirtualDiskHandle: i64,
        Flags:             u32,
        ProviderSpecificFlags: u32
    ): u32

    @lib pub fn CreateVirtualDisk(
        VirtualStorageType: *const VIRTUAL_STORAGE_TYPE,
        Path:           *const u16,
        VirtualDiskAccessMask: u32,
        SecurityDescriptor: *const void,
        Flags:          u32,
        ProviderSpecificFlags: u32,
        Parameters:     *const CREATE_VIRTUAL_DISK_PARAMETERS,
        Overlapped:     *void,
        Handle:         *i64
    ): u32

    /// Compact / merge / expand existing VHDs.
    @lib pub fn CompactVirtualDisk(
        VirtualDiskHandle: i64,
        Flags:             u32,
        Parameters:        *const void,
        Overlapped:        *void
    ): u32

    @lib pub fn MergeVirtualDisk(
        VirtualDiskHandle: i64,
        Flags:             u32,
        Parameters:        *const void,
        Overlapped:        *void
    ): u32

    @lib pub fn ExpandVirtualDisk(
        VirtualDiskHandle: i64,
        Flags:             u32,
        Parameters:        *const void,
        Overlapped:        *void
    ): u32

    /// `GetVirtualDiskInformation` — info classes:
    ///   1 GET_VIRTUAL_DISK_INFO_SIZE
    ///   2 GET_VIRTUAL_DISK_INFO_IDENTIFIER
    ///   3 GET_VIRTUAL_DISK_INFO_PARENT_LOCATION
    ///   4 GET_VIRTUAL_DISK_INFO_PARENT_IDENTIFIER
    ///   ...
    @lib pub fn GetVirtualDiskInformation(
        VirtualDiskHandle: i64,
        VirtualDiskInfoSize: *u32,
        VirtualDiskInfo:    *void,
        SizeUsed:           *u32
    ): u32

    /// Return the physical disk path that the VHD is attached to.
    @lib pub fn GetVirtualDiskPhysicalPath(
        VirtualDiskHandle: i64,
        DiskPathSizeInBytes: *u32,
        DiskPath:          *u16
    ): u32

    /// Take / release a snapshot of the disk's data block layout.
    @lib pub fn TakeSnapshotVhdSet(
        VirtualDiskHandle: i64,
        Parameters:        *const void,
        Flags:             u32
    ): u32

    @lib pub fn DeleteSnapshotVhdSet(
        VirtualDiskHandle: i64,
        Parameters:        *const void,
        Flags:             u32
    ): u32
}
