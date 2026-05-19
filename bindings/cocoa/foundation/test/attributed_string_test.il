// NSAttributedString / NSMutableAttributedString.

use test
use foundation { NSAttributedString, NSMutableAttributedString,
                 NSMutableDictionary, nsstr, ilstr, makeRange }

// Immutable, plain text.
let plain = NSAttributedString.alloc().initWithString(nsstr("hello"))
test.expect(plain.length, 5)
test.expectStr(ilstr(plain.string), "hello")

// substring via range.
let sub = plain.attributedSubstringFromRange(makeRange(1 as u64, 3 as u64))
test.expectStr(ilstr(sub.string), "ell")

// Mutable build: init() → append → splice.
let m = NSMutableAttributedString.alloc().init()
test.expect(m.length, 0)

m.appendAttributedString(plain)
test.expect(m.length, 5)
test.expectStr(ilstr(m.string), "hello")

m.appendAttributedString(
    NSAttributedString.alloc().initWithString(nsstr(", world"))
)
test.expectStr(ilstr(m.string), "hello, world")

m.deleteCharactersInRange(makeRange(5 as u64, 2 as u64))
test.expectStr(ilstr(m.string), "helloworld")

m.replaceCharactersInRangeWithString(
    makeRange(5 as u64, 0 as u64), nsstr("__")
)
test.expectStr(ilstr(m.string), "hello__world")

// isEqualToAttributedString — value equality (same text, same
// (empty) attributes).
let a = NSAttributedString.alloc().initWithString(nsstr("x"))
let b = NSAttributedString.alloc().initWithString(nsstr("x"))
test.expectTrue(a.isEqualToAttributedString(b))
let c = NSAttributedString.alloc().initWithString(nsstr("y"))
test.expectFalse(a.isEqualToAttributedString(c))

// initWithString:attributes: + initWithAttributedString.
let attrs = NSMutableDictionary.alloc().init()
attrs.setObject(nsstr("Helvetica"), nsstr("NSFontName"))
let styled = NSAttributedString.alloc().initWithStringAndAttributes(
    nsstr("hi"), attrs
)
test.expect(styled.length, 2)

let copy = NSAttributedString.alloc().initWithAttributedString(styled)
test.expectTrue(styled.isEqualToAttributedString(copy))

// beginEditing / endEditing — bracketed mutation batch.
m.beginEditing()
m.appendAttributedString(
    NSAttributedString.alloc().initWithString(nsstr("!"))
)
m.endEditing()
test.expectStr(ilstr(m.string), "hello__world!")

// mutableString accessor returns a string that reflects mutations.
test.expect(m.mutableString.length as i64, m.length as i64)

0
