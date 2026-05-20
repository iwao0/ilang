// Optional interface methods are only meaningful for Objective-C
// protocols, where the ObjC runtime probes `respondsToSelector:`
// at the call site. Plain (non-`@objc`) interfaces enforce a
// strict conformance contract, so the `@optional` attribute is
// rejected outright — there is no way to opt out of implementing
// a method on a regular ilang interface.
//
// expect-error: optional interface methods are only allowed inside `@objc` interfaces

interface Greet {
    hello(): string
    @optional goodbye(): string
}
