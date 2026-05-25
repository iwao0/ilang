/// NSProcessInfo — read-only properties on the running process.

use std.test as test
use foundation { NSProcessInfo, ilstr }

let pi = NSProcessInfo.processInfo()

// processName is non-empty (the binary name or whatever the
// shell set as argv[0]).
test.expectTrue(pi.processName.length > 0)

// hostName is non-empty too.
test.expectTrue(pi.hostName.length > 0)

// processIdentifier is positive.
test.expectTrue((pi.processIdentifier as i64) > 0)

// processorCount >= 1.
test.expectTrue((pi.processorCount as i64) >= 1)
test.expectTrue((pi.activeProcessorCount as i64) >= 1)
test.expectTrue((pi.activeProcessorCount as i64) <= (pi.processorCount as i64))

// physicalMemory at least 256 MB.
test.expectTrue((pi.physicalMemory as i64) > (256 * 1024 * 1024))

// globallyUniqueString — 36-char UUID + "-pid-time" suffix; just
// verify it's non-empty.
test.expectTrue(pi.globallyUniqueString.length > 0)

// arguments has at least the program path at index 0.
let args = pi.arguments
test.expectTrue(args.count >= 1)

// environment is a non-empty dictionary on any sane host.
test.expectTrue(pi.environment.count > 0)

0
