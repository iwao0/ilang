// Attributed-text bindings — NSAttributedString and its
// NSMutableAttributedString sibling. Text + per-character
// attribute dictionaries (foreground color, font, paragraph
// style, …). AppKit's NSTextView / NSTextField render these
// directly; you'll commonly build one to drive styled UI text.

use core { NSObject, NSString, NSDictionary, NSRange }

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSAttributedString` — immutable styled text. Construct
    /// from a plain `NSString` (no attributes), an `NSString` +
    /// attribute dictionary (uniform styling), or another
    /// attributed string (copy). Get the raw text via `.string`
    /// and the length / attribute interrogations below.
    @objc pub class NSAttributedString : NSObject {
        @objc("alloc")
        pub static alloc(): NSAttributedString
        @objc("initWithString:")
        pub initWithString(s: NSString): NSAttributedString
        @objc("initWithString:attributes:")
        pub initWithStringAndAttributes(
            s: NSString, attributes: NSDictionary
        ): NSAttributedString
        @objc("initWithAttributedString:")
        pub initWithAttributedString(
            other: NSAttributedString
        ): NSAttributedString

        @objc("string")
        pub get string(): NSString
        @objc("length")
        pub get length(): i64

        @objc("attributesAtIndex:effectiveRange:")
        pub attributesAtIndex(i: u64, effectiveRange: *u64): NSDictionary
        @objc("attribute:atIndex:effectiveRange:")
        pub attributeAtIndex(
            name: NSString, atIndex: u64, effectiveRange: *u64
        ): NSObject

        @objc("isEqualToAttributedString:")
        pub isEqualToAttributedString(other: NSAttributedString): bool
        @objc("attributedSubstringFromRange:")
        pub attributedSubstringFromRange(range: NSRange): NSAttributedString
    }

    /// `NSMutableAttributedString` — appendable / editable variant.
    /// Wrap mutations in `beginEditing` / `endEditing` brackets
    /// when you make a batch of changes so the layout engine
    /// only relays out once at the end.
    @objc pub class NSMutableAttributedString : NSAttributedString {
        @objc("alloc")
        pub static alloc(): NSMutableAttributedString
        @objc("init")
        pub init(): NSMutableAttributedString

        @objc("mutableString")
        pub get mutableString(): NSString

        @objc("addAttribute:value:range:")
        pub addAttribute(name: NSString, value: NSObject, range: NSRange)
        @objc("addAttributes:range:")
        pub addAttributes(attrs: NSDictionary, range: NSRange)
        @objc("setAttributes:range:")
        pub setAttributes(attrs: NSDictionary, range: NSRange)
        @objc("removeAttribute:range:")
        pub removeAttribute(name: NSString, range: NSRange)

        @objc("replaceCharactersInRange:withString:")
        pub replaceCharactersInRangeWithString(range: NSRange, with: NSString)
        @objc("replaceCharactersInRange:withAttributedString:")
        pub replaceCharactersInRangeWithAttributedString(
            range: NSRange, with: NSAttributedString
        )
        @objc("appendAttributedString:")
        pub appendAttributedString(s: NSAttributedString)
        @objc("insertAttributedString:atIndex:")
        pub insertAttributedString(s: NSAttributedString, atIndex: u64)
        @objc("deleteCharactersInRange:")
        pub deleteCharactersInRange(range: NSRange)

        @objc("beginEditing")
        pub beginEditing()
        @objc("endEditing")
        pub endEditing()
    }
}
