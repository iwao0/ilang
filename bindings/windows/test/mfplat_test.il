/// mfplat — startup / shutdown + buffer creation smoke test.

use std.test
use mfplat {
    MFStartup, MFShutdown, MfStartupFlags,
    MFCreateMemoryBuffer, MFCreateSample, MFCreateMediaType,
    MFCreateAttributes,
}

@extern(C) {
    pub fn run() {
        // MF_VERSION = 0x00020070.
        test.expect(
            MFStartup(0x00020070 as u32, MfStartupFlags.full as u32) as i64,
            0 as i64
        )

        // Create a 64 KiB memory buffer.
        let buf: *void = 0 as *void
        test.expect(
            MFCreateMemoryBuffer(65536 as u32, &buf) as i64,
            0 as i64
        )
        test.expectTrue((buf as i64) != 0)

        // Create a media type (empty) + sample envelope.
        let mt: *void = 0 as *void
        test.expect(MFCreateMediaType(&mt) as i64, 0 as i64)
        test.expectTrue((mt as i64) != 0)

        let s: *void = 0 as *void
        test.expect(MFCreateSample(&s) as i64, 0 as i64)
        test.expectTrue((s as i64) != 0)

        // Attribute store (initial capacity 0).
        let attrs: *void = 0 as *void
        test.expect(MFCreateAttributes(&attrs, 0 as u32) as i64, 0 as i64)
        test.expectTrue((attrs as i64) != 0)

        // We deliberately leak the COM pointers — IUnknown.Release
        // dispatch isn't bound at the ilang-call level. MFShutdown
        // tears down the platform anyway.
        test.expect(MFShutdown() as i64, 0 as i64)
    }
}

run()
