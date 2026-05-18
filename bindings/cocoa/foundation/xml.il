// XML bindings — NSXMLParser (event-based SAX). The
// tree-walking siblings NSXMLDocument / NSXMLNode / NSXMLElement
// are macOS-only (not on iOS) and rarely the right tool for new
// code, so they're intentionally not bound here.
//
// The delegate callbacks live on the `NSXMLParserDelegate` ObjC
// protocol; implement that interface and set it as the parser's
// delegate before calling `parse()`.

use core { NSObject, NSString, NSData, NSError, NSDictionary, NSArray }
use network { NSURL }

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSXMLParser` — event-based XML reader. Construct against
    /// an NSData / NSURL / NSInputStream, set a delegate, then
    /// call `.parse()`. The delegate (typically a class
    /// implementing the various
    /// `parser:didStartElement:namespaceURI:qualifiedName:attributes:`
    /// callbacks) receives each tag / character data / processing
    /// instruction event.
    @objc pub class NSXMLParser : NSObject {
        @objc("alloc")
        pub static alloc(): NSXMLParser
        @objc("initWithData:")
        pub initWithData(d: NSData): NSXMLParser
        @objc("initWithStream:")
        pub initWithStream(s: NSObject): NSXMLParser
        @objc("initWithContentsOfURL:")
        pub initWithContentsOfURL(url: NSURL): NSXMLParser

        @objc("delegate")
        pub get delegate(): NSObject
        @objc("setDelegate:")
        pub set delegate(d: NSObject)

        @objc("parse")
        pub parse(): bool
        @objc("abortParsing")
        pub abortParsing()

        @objc("parserError")
        pub get parserError(): NSError
        @objc("lineNumber")
        pub get lineNumber(): i64
        @objc("columnNumber")
        pub get columnNumber(): i64
        @objc("publicID")
        pub get publicID(): NSString
        @objc("systemID")
        pub get systemID(): NSString

        @objc("shouldProcessNamespaces")
        pub get shouldProcessNamespaces(): bool
        @objc("setShouldProcessNamespaces:")
        pub set shouldProcessNamespaces(v: bool)
        @objc("shouldReportNamespacePrefixes")
        pub get shouldReportNamespacePrefixes(): bool
        @objc("setShouldReportNamespacePrefixes:")
        pub set shouldReportNamespacePrefixes(v: bool)
        @objc("shouldResolveExternalEntities")
        pub get shouldResolveExternalEntities(): bool
        @objc("setShouldResolveExternalEntities:")
        pub set shouldResolveExternalEntities(v: bool)
    }

    /// `NSXMLParserDelegate` — protocol the parser dispatches
    /// SAX-style events into. Implement the methods you care
    /// about; everything is `@optional` so unimplemented ones
    /// stay as ObjC no-ops.
    @objc pub interface NSXMLParserDelegate {
        @optional parserDidStartDocument(parser: NSXMLParser)
        @optional parserDidEndDocument(parser: NSXMLParser)
        @optional @objc("parser:didStartElement:namespaceURI:qualifiedName:attributes:")
        parserDidStartElement(
            parser: NSXMLParser, didStartElement: NSString,
            namespaceURI: NSString, qualifiedName: NSString,
            attributes: NSDictionary
        )
        @optional @objc("parser:didEndElement:namespaceURI:qualifiedName:")
        parserDidEndElement(
            parser: NSXMLParser, didEndElement: NSString,
            namespaceURI: NSString, qualifiedName: NSString
        )
        @optional @objc("parser:foundCharacters:")
        parserFoundCharacters(parser: NSXMLParser, foundCharacters: NSString)
        @optional @objc("parser:foundIgnorableWhitespace:")
        parserFoundIgnorableWhitespace(
            parser: NSXMLParser, foundIgnorableWhitespace: NSString
        )
        @optional @objc("parser:foundProcessingInstructionWithTarget:data:")
        parserFoundProcessingInstruction(
            parser: NSXMLParser, target: NSString, data: NSString
        )
        @optional @objc("parser:foundComment:")
        parserFoundComment(parser: NSXMLParser, foundComment: NSString)
        @optional @objc("parser:foundCDATA:")
        parserFoundCDATA(parser: NSXMLParser, foundCDATA: NSData)
        @optional @objc("parser:parseErrorOccurred:")
        parserParseErrorOccurred(parser: NSXMLParser, parseError: NSError)
        @optional @objc("parser:validationErrorOccurred:")
        parserValidationErrorOccurred(parser: NSXMLParser, validationError: NSError)
    }
}
