// NSError — construct and read back domain / code / userInfo.

use test
use foundation { NSError, NSMutableDictionary, NSObject, nsstr, ilstr }

let info = NSMutableDictionary.alloc().init()
info.setObject(nsstr("file not found"), nsstr("NSLocalizedDescription"))
info.setObject(nsstr("verify the path"), nsstr("NSLocalizedRecoverySuggestion"))

let e = NSError.errorWithDomain(nsstr("MyDomain"), 404, info)
test.expect(e.code, 404)
test.expectStr(ilstr(e.domain), "MyDomain")
test.expectStr(ilstr(e.localizedDescription), "file not found")
test.expectStr(ilstr(e.localizedRecoverySuggestion), "verify the path")

// userInfo round-trip — pull a key back out.
let dict = e.userInfo
test.expect(dict.count, 2)

0
