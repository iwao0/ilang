// NSUserDefaults — set / get / remove on a custom suite name so
// we don't pollute the host user's defaults database.

use test
use foundation { NSUserDefaults, nsstr, ilstr }

let ud = NSUserDefaults.alloc().initWithSuiteName(nsstr("ilang.test.defaults"))

let key = nsstr("ilang.test.key")

// Clear any leftover from prior run.
ud.removeObjectForKey(key)

ud.setInteger(42, key)
test.expect(ud.integerForKey(key), 42)

ud.setDouble(3.14, key)
test.expectF64(ud.doubleForKey(key), 3.14)

ud.setBool(true, key)
test.expectBool(ud.boolForKey(key), true)

ud.setObject(nsstr("hello"), key)
test.expectStr(ilstr(ud.stringForKey(key)), "hello")

ud.removeObjectForKey(key)
// After removal, integerForKey returns 0.
test.expect(ud.integerForKey(key), 0)

0
