/// mfsensorgroup.dll — Media Foundation sensor groups.
///
/// Enumerate physical cameras / capture devices and synthesize
/// "sensor groups" so a single virtual source spans multiple
/// physical sensors (e.g. front + rear smartphone cameras).
///
/// Common pattern:
///   MFCreateSensorGroup(L"CameraGroup1", &group)
///   group.AddSource(srcA)
///   group.AddSource(srcB)
///   group.CreateMediaSource(&sensorSource)
///
/// `IMFSensorGroup` / `IMFSensorActivityMonitor` are accessed via
/// the IID values + CoCreateInstance / MFCreateSensorGroup.

@extern(C, "mfsensorgroup") {
    /// Create a sensor group identified by `sensorGroupSymbolicLink`.
    /// The symbolic link mirrors `IMFActivate.MF_DEVSOURCE_ATTRIBUTE_*`.
    @lib pub fn MFCreateSensorGroup(
        sensorGroupSymbolicLink: *const u16,
        ppSensorGroup:           **void           // *IMFSensorGroup
    ): i32

    /// Create the platform's default sensor stream.
    @lib pub fn MFCreateSensorStream(
        streamId:           u32,
        pAttributes:        *void,                // *IMFAttributes
        pMediaTypeCollection: *void,              // *IMFCollection
        ppStream:           **void                // *IMFSensorStream
    ): i32

    /// Create the sensor profile collection (used to assemble
    /// custom recording-mode profiles).
    @lib pub fn MFCreateSensorProfile(
        ProfileType: *const void,                 // *GUID
        ProfileIndex: u32,
        Constraints: *const u16,
        ppProfile:   **void                        // *IMFSensorProfile
    ): i32

    @lib pub fn MFCreateSensorProfileCollection(
        ppProfileCollection: **void                // *IMFSensorProfileCollection
    ): i32

    /// Create the device activity monitor (lights an icon in the
    /// system tray while the camera is in use).
    @lib pub fn MFCreateSensorActivityMonitor(
        pCallback:     *void,                     // *IMFSensorActivitiesReportCallback
        ppActivityMonitor: **void                  // *IMFSensorActivityMonitor
    ): i32

    /// Same but only watches a single sensor symbolic-link path.
    @lib pub fn MFCreateRelativePanelWatcher(
        viewType:    *const void,                 // *GUID
        ppWatcher:   **void
    ): i32
}
