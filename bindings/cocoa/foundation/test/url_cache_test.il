/// NSURLCache — shared / custom instances, capacity reads.

use std.test
use foundation { NSURLCache, NSURL, nsstr }

let shared = NSURLCache.shared()
test.expectFalse(shared.handle == 0)
// Shared cache has some byte capacity set by the system.
test.expectTrue(shared.memoryCapacity >= 0 as u64)

// Build a custom one with explicit capacities.
let custom = NSURLCache.alloc().initWithMemoryAndDiskCapacity(
    1024 as u64 * 1024 as u64, 0 as u64,
    NSURL.fromFilePath(nsstr("/tmp/ilang_test_url_cache"))
)
test.expectFalse(custom.handle == 0)

// removeAllCachedResponses doesn't crash even on the custom one.
custom.removeAllCachedResponses()

0
