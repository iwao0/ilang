// libc.assert / libc.assertMsg — happy-path no-op.
// (We can't easily exercise the abort path in a normal fixture,
// since aborting the process flags the run as a failure.)
use test
use libc { assert, assertMsg }

@extern(C) {
    pub fn run() {
        // `true` paths must be no-ops.
        assert(true)
        assert(2 + 2 == 4)
        assertMsg(true, "always true")
        assertMsg(1 + 1 == 2, "math broke")

        // Confirm we reached the end (no aborts fired).
        test.expectTrue(true)
    }
}

run()
