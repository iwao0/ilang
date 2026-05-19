// NSMutableString — appendString / insert / replace / setString.

use test
use foundation { NSMutableString, nsstr, ilstr, makeRange }

let ms = NSMutableString.string()
test.expect(ms.length as i64, 0)

ms.appendString(nsstr("hello"))
test.expectStr(ilstr(ms), "hello")
test.expect(ms.length as i64, 5)

ms.appendString(nsstr(" world"))
test.expectStr(ilstr(ms), "hello world")

// insert at index.
ms.insertString(nsstr(", "), 5 as u64)
test.expectStr(ilstr(ms), "hello,  world")

// delete a range.
ms.deleteCharactersInRange(makeRange(5 as u64, 1 as u64))  // drop the ','
test.expectStr(ilstr(ms), "hello  world")

// replace a range.
ms.replaceCharactersInRange(makeRange(5 as u64, 2 as u64), nsstr(" "))
test.expectStr(ilstr(ms), "hello world")

// setString resets contents.
ms.setString(nsstr("reset"))
test.expectStr(ilstr(ms), "reset")
test.expect(ms.length as i64, 5)

0
