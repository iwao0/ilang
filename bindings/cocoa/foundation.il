// Foundation bindings — the ObjC base layer (NSObject / NSString /
// collections / dates / URLs) plus the ilang ↔ ObjC plumbing that
// every higher-level binding leans on. Doesn't touch AppKit; safe
// to import on macOS server contexts that don't open a window.
//
// Imported via the `cocoa` umbrella module (`use cocoa { ... }`)
// or directly with `use foundation { ... }`.

// ─── Foundation geometry types (HFA-passed) ──────────────────────
// NSPoint / NSSize / NSRect are plain f64 structs. The AArch64
// ABI rides them through v0..v3 (HFA path) when passed to
// objc_msgSend — same layout C uses, no shim needed.
pub struct NSPoint { x: f64; y: f64 }
pub struct NSSize  { width: f64; height: f64 }
pub struct NSRect  { origin: NSPoint; size: NSSize }

@extern(ObjC) {
    /// Root of the Cocoa class hierarchy. Bound (not subclassed)
    /// so users can call e.g. `obj.release()` on any returned
    /// instance.
    @objc pub class NSObject {
        @objc("release")
        pub release()
        @objc("retain")
        pub retain(): NSObject
        @objc("hash")
        pub hash(): i64

        /// Wrap a raw ObjC `id` (the `i64` pointer value) into an
        /// `NSObject` instance without going through `alloc + init`.
        /// Bridges to the @objc-desugar's hidden `__wrap_handle`
        /// helper, which simply stores `h` into the object's
        /// `handle` field. Use this for `nil` (handle = 0) or for
        /// adopting an `id` returned from a C-side bridge.
        pub static wrap(h: i64): NSObject {
            NSObject.__wrap_handle(h)
        }
    }

    /// `NSString` — Cocoa's UTF-16 string type. We expose the
    /// C-string constructor and `UTF8String` accessor; ilang code
    /// works in UTF-8 so the round-trip is `nsstr(s) ↔ ilstr(ns)`.
    @objc pub class NSString : NSObject {
        @objc("stringWithUTF8String:")
        pub static fromCStr(s: *const char): NSString
        @objc("UTF8String")
        pub utf8String(): *const char
        @objc("length")
        pub length(): i64
        @objc("isEqualToString:")
        pub isEqualToString(other: NSString): bool
    }

    /// `NSNumber` — boxed numeric value. Read back as `intValue`,
    /// `doubleValue`, or `boolValue` depending on what's inside.
    @objc pub class NSNumber : NSObject {
        @objc("numberWithLongLong:")
        pub static fromI64(v: i64): NSNumber
        @objc("numberWithDouble:")
        pub static fromF64(v: f64): NSNumber
        @objc("numberWithBool:")
        pub static fromBool(v: bool): NSNumber

        @objc("longLongValue")
        pub i64Value(): i64
        @objc("doubleValue")
        pub f64Value(): f64
        @objc("boolValue")
        pub boolValue(): bool
    }

    /// `NSArray` — immutable ordered collection of `id`. Returned
    /// from many Foundation APIs (`NSFileManager.contentsOfDirectory…`,
    /// `[NSString componentsSeparatedByString:]`, etc.). For
    /// construction use `NSMutableArray`.
    @objc pub class NSArray : NSObject {
        @objc("count")
        pub count(): i64
        @objc("objectAtIndex:")
        pub objectAtIndex(i: i64): NSObject
        @objc("containsObject:")
        pub containsObject(o: NSObject): bool
    }

    /// `NSMutableArray` — array that can be appended to. Initialise
    /// with `alloc().init()` or use the `array` class method.
    @objc pub class NSMutableArray : NSArray {
        @objc("alloc")
        pub static alloc(): NSMutableArray
        @objc("init")
        pub init(): NSMutableArray
        @objc("array")
        pub static array(): NSMutableArray

        @objc("addObject:")
        pub addObject(o: NSObject)
        @objc("removeObjectAtIndex:")
        pub removeObjectAtIndex(i: i64)
        @objc("insertObject:atIndex:")
        pub insertObject(o: NSObject, atIndex: i64)
        @objc("removeAllObjects")
        pub removeAllObjects()
    }

    /// `NSDictionary` — immutable id→id map.
    @objc pub class NSDictionary : NSObject {
        @objc("count")
        pub count(): i64
        @objc("objectForKey:")
        pub objectForKey(k: NSObject): NSObject
        @objc("allKeys")
        pub allKeys(): NSArray
    }

    /// `NSMutableDictionary` — dictionary you can write to. Keys
    /// should conform to NSCopying (NSString / NSNumber are fine).
    @objc pub class NSMutableDictionary : NSDictionary {
        @objc("alloc")
        pub static alloc(): NSMutableDictionary
        @objc("init")
        pub init(): NSMutableDictionary
        @objc("dictionary")
        pub static dictionary(): NSMutableDictionary

        @objc("setObject:forKey:")
        pub setObject(o: NSObject, forKey: NSObject)
        @objc("removeObjectForKey:")
        pub removeObjectForKey(k: NSObject)
        @objc("removeAllObjects")
        pub removeAllObjects()
    }

    /// `NSData` — immutable byte buffer.
    @objc pub class NSData : NSObject {
        @objc("dataWithBytes:length:")
        pub static fromBytes(bytes: *const char, length: i64): NSData
        @objc("length")
        pub length(): i64
        @objc("bytes")
        pub bytes(): *const char
    }

    /// `NSDate` — a moment in time. `date` is "now"; the interval
    /// methods give you the offset in seconds.
    @objc pub class NSDate : NSObject {
        @objc("date")
        pub static now(): NSDate
        @objc("timeIntervalSince1970")
        pub timeIntervalSince1970(): f64
        @objc("timeIntervalSinceNow")
        pub timeIntervalSinceNow(): f64
    }

    /// `NSURL` — URL value. Use `fromString` for `http://...`
    /// style URLs, `fromFilePath` for filesystem paths.
    @objc pub class NSURL : NSObject {
        @objc("URLWithString:")
        pub static fromString(s: NSString): NSURL
        @objc("fileURLWithPath:")
        pub static fromFilePath(s: NSString): NSURL
        @objc("absoluteString")
        pub absoluteString(): NSString
        @objc("path")
        pub path(): NSString
    }

    // ─── Convenience helpers ─────────────────────────────────────

    // libobjc's `sel_registerName` exposed under an ilang-side
    // name with an `i64` return so user code (which can't name
    // raw pointer types) can shuffle a SEL around. The cast back
    // to a typed selector happens automatically inside @objc
    // wrappers; user code just treats SEL as opaque i64.
    @lib("objc") @symbol("sel_registerName")
        fn _sel_register_name(name: *const char): i64

    // ObjC autorelease pool primitives. Mirror `@autoreleasepool {
    // ... }` in Cocoa: push a pool token, run the body, pop the
    // token. Inside the body any `autorelease`-flagged temporaries
    // (e.g. `[NSString stringWithUTF8String:]`) accumulate against
    // this pool instead of the outer thread-default one — pop
    // drains them.
    @lib("objc") @symbol("objc_autoreleasePoolPush")
        fn _autoreleasepool_push(): i64
    @lib("objc") @symbol("objc_autoreleasePoolPop")
        fn _autoreleasepool_pop(token: i64)

    // Heap-allocates an ObjC block whose `invoke` trampoline calls
    // the captured ilang closure. The block is autoreleased before
    // return so callers can pass it straight to APIs that take a
    // completion handler without manual `Block_release`.
    // Implemented in `ilang-runtime/objc_blocks.rs`.
    @lib("c") @symbol("__ilang_make_void_block")
        fn _make_void_block(closure: fn()): i64
    @lib("c") @symbol("__ilang_make_obj_block")
        fn _make_obj_block(closure: fn(i64)): i64
    @lib("c") @symbol("__ilang_make_obj_to_obj_block")
        fn _make_obj_to_obj_block(closure: fn(i64): i64): i64

    /// Intern an Objective-C selector by name and return its
    /// stable SEL pointer (as `i64`). Use it with
    /// `button.setAction(selectorPtr("clicked:"))`.
    pub fn selectorPtr(name: string): i64 {
        _sel_register_name(cstrFromString(name))
    }

    /// Build an `NSString` from an ilang `string`. Wraps the
    /// `cstrFromString` + `NSString.fromCStr` two-step.
    pub fn nsstr(s: string): NSString {
        NSString.fromCStr(cstrFromString(s))
    }

    /// Convert an `NSString` back to an ilang `string` — the
    /// mirror of `nsstr`. Internally `[NSString UTF8String]`
    /// returns a C `const char *` whose lifetime is tied to the
    /// autorelease pool, so call this inside an
    /// `autoreleasepool { ... }` if the string is long-lived or
    /// you'll race the pool drain.
    pub fn ilstr(s: NSString): string {
        stringFromCstr(s.utf8String())
    }

    /// Construct an `NSRect` from primitive components.
    pub fn makeRect(x: f64, y: f64, w: f64, h: f64): NSRect {
        let r = new NSRect()
        r.origin.x = x
        r.origin.y = y
        r.size.width = w
        r.size.height = h
        r
    }

    /// Build an ObjC `void(^)(void)` block from an ilang closure.
    /// Returns the block as an opaque `i64` pointer suitable for
    /// passing to any `@objc` method whose selector takes a
    /// `void(^)(void)` (completion handlers etc.). The block is
    /// autoreleased — let it die with the surrounding pool, or
    /// retain it via `objc_retain` if you need a longer lifetime.
    pub fn makeBlock(body: fn()): i64 {
        _make_void_block(body)
    }

    /// Build an ObjC `void(^)(id)` block — the callback shape used
    /// by AppKit / Foundation when handing a single object back
    /// (e.g. `addLocalMonitorForEventsMatchingMask:handler:`'s
    /// NSEvent argument). The ilang closure receives the object's
    /// raw `id` as `i64`; wrap it with `NSObject.wrap(arg)` (or a
    /// subclass equivalent) inside if you want a typed view.
    pub fn makeObjectBlock(body: fn(i64)): i64 {
        _make_obj_block(body)
    }

    /// Build an ObjC `id(^)(id)` block — the monitoring callback
    /// shape used by `addLocalMonitorForEventsMatchingMask:handler:`
    /// where the handler returns the event to forward (often the
    /// same one), a replacement, or `0` to swallow. The ilang
    /// closure is `fn(handle: i64): i64` — both sides traffic in
    /// raw `id`s, so wrap them with `NSObject.__wrap_handle_unowned`
    /// for AppKit-owned arguments to avoid double-release.
    pub fn makeObjectToObjectBlock(body: fn(i64): i64): i64 {
        _make_obj_to_obj_block(body)
    }

    /// Run `body` inside a fresh ObjC autorelease pool. Mirrors
    /// Cocoa's `@autoreleasepool { ... }` — any autorelease'd
    /// temporaries (e.g. the NSStrings minted by `nsstr`) get
    /// drained when `body` returns instead of leaking until the
    /// outer thread-default pool drains (which, in a long-running
    /// loop, may never happen).
    pub fn autoreleasepool(body: fn()) {
        let token = _autoreleasepool_push()
        body()
        _autoreleasepool_pop(token)
    }
}

/// `nil` sender — an NSObject wrapper around the zero pointer.
/// Pass to AppKit calls that want an arbitrary "no sender"
/// receiver (e.g. `win.makeKeyAndOrderFront(cocoa.nil)`).
pub const nil = NSObject.wrap(0)
