// NSCharacterSet — predefined sets + characterIsMember.

use test
use foundation { NSCharacterSet, nsstr }

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

// Predefined more.
let alnum = NSCharacterSet.alphanumeric()
test.expectTrue(alnum.characterIsMember(65 as u16))   // 'A'
test.expectTrue(alnum.characterIsMember(48 as u16))   // '0'
test.expectFalse(alnum.characterIsMember(33 as u16))  // '!'

let punct = NSCharacterSet.punctuation()
test.expectTrue(punct.characterIsMember(46 as u16))   // '.'
test.expectFalse(punct.characterIsMember(65 as u16))  // 'A'

// URL allowed sets — fragment / query / host / path.
test.expectTrue(NSCharacterSet.urlPathAllowed()
    .characterIsMember(47 as u16))   // '/'
test.expectFalse(NSCharacterSet.urlPathAllowed()
    .characterIsMember(35 as u16))   // '#' is NOT allowed in path

// Newline + whitespace.
let wsnl = NSCharacterSet.whitespaceAndNewline()
test.expectTrue(wsnl.characterIsMember(32 as u16))   // space
test.expectTrue(wsnl.characterIsMember(10 as u16))   // '\n'

// Decomposable case-letter sets.
test.expectTrue(NSCharacterSet.uppercaseLetters().characterIsMember(65 as u16))
test.expectFalse(NSCharacterSet.uppercaseLetters().characterIsMember(97 as u16))
test.expectTrue(NSCharacterSet.lowercaseLetters().characterIsMember(97 as u16))
test.expectFalse(NSCharacterSet.lowercaseLetters().characterIsMember(65 as u16))

0
