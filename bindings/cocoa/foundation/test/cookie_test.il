/// NSHTTPCookieStorage + NSURLCredential + NSURLProtectionSpace.
/// Read-only / construct + accessor tests, no real network state
/// changes.

use std.test
use foundation { NSHTTPCookieStorage, NSURLCredential,
                 NSURLProtectionSpace, NSURLCredentialPersistence,
                 nsstr, ilstr }

// Shared cookie jar is always non-nil; cookies array is at
// least the empty array.
let jar = NSHTTPCookieStorage.shared()
test.expectFalse(jar.handle == 0)
let _ = jar.cookies  // ensure the property fires without crash

// NSURLCredential — construct + read back fields.
let cred = NSURLCredential.credentialWithUserAndPassword(
    nsstr("alice"), nsstr("s3cret"),
    NSURLCredentialPersistence.forSession
)
test.expectStr(ilstr(cred.user), "alice")
test.expectStr(ilstr(cred.password), "s3cret")
test.expectTrue(cred.hasPassword)
test.expect(cred.persistence as i64,
    NSURLCredentialPersistence.forSession as i64)

// NSURLProtectionSpace — host / port / realm / scheme.
let space = NSURLProtectionSpace.alloc().initWithHost(
    nsstr("example.com"), 443,
    nsstr("https"),
    nsstr("Admin"),
    nsstr("NSURLAuthenticationMethodHTTPBasic")
)
test.expectStr(ilstr(space.host), "example.com")

0
