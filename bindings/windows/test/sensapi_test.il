/// sensapi — IsNetworkAlive smoke test.

use std.test
use sensapi { IsNetworkAlive }

@extern(C) {
    pub fn run() {
        // Returns non-zero when any network connection is up. We
        // don't assert the result (CI without networking is fine);
        // just verify the symbol resolves + the call returns.
        let flags: u32 = 0
        let _ = IsNetworkAlive(&flags)
        // Touch `flags` so the compiler keeps the local.
    }
}

run()
