// NSCharacterSet — predefined sets + characterIsMember.

use test
use cocoa { NSCharacterSet, nsstr }

let digits = NSCharacterSet.decimalDigit()
test.expectTrue(digits.characterIsMember(48 as u16))   // '0'
test.expectTrue(digits.characterIsMember(57 as u16))   // '9'
test.expectFalse(digits.characterIsMember(97 as u16))  // 'a'

let letters = NSCharacterSet.letter()
test.expectTrue(letters.characterIsMember(65 as u16))  // 'A'
test.expectTrue(letters.characterIsMember(122 as u16)) // 'z'
test.expectFalse(letters.characterIsMember(48 as u16)) // '0'

let whitespace = NSCharacterSet.whitespace()
test.expectTrue(whitespace.characterIsMember(32 as u16))  // space
test.expectFalse(whitespace.characterIsMember(97 as u16)) // 'a'

// Custom set + invertedSet.
let abc = NSCharacterSet.characterSetWithCharactersInString(nsstr("abc"))
test.expectTrue(abc.characterIsMember(97 as u16))   // 'a'
test.expectFalse(abc.characterIsMember(120 as u16)) // 'x'
let notabc = abc.invertedSet
test.expectFalse(notabc.characterIsMember(97 as u16))
test.expectTrue(notabc.characterIsMember(120 as u16))

0
