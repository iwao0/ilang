// Foundation collection types beyond NSArray / NSDictionary
// (which live in `core` because too many other classes use them
// directly). This file pulls in NSSet / NSCountedSet,
// NSIndexSet (+ Mutable), and NSOrderedSet (+ Mutable).
//
// Cross-file references resolve through `use core { ... }` at the
// top: `core` is the sibling umbrella `core.il`, found via the
// same-directory loader rule. `mod.il` re-exports everything.

use core { NSObject, NSString, NSArray, NSRange }

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSSet` — unordered collection of distinct objects. Two
    /// members compare equal under the receiver's `-hash` and
    /// `-isEqual:`. Use `NSMutableSet` for the writable variant;
    /// the static factories below build immutable values from an
    /// `NSArray` of objects.
    @objc pub class NSSet : NSObject {
        @objc("set")
        pub static set(): NSSet
        @objc("setWithArray:")
        pub static setWithArray(arr: NSArray): NSSet

        @objc("count")
        pub get count(): i64
        @objc("containsObject:")
        pub containsObject(o: NSObject): bool
        @objc("member:")
        pub member(o: NSObject): NSObject
        @objc("anyObject")
        pub get anyObject(): NSObject
        @objc("allObjects")
        pub get allObjects(): NSArray

        @objc("isSubsetOfSet:")
        pub isSubsetOfSet(other: NSSet): bool
        @objc("intersectsSet:")
        pub intersectsSet(other: NSSet): bool
        @objc("isEqualToSet:")
        pub isEqualToSet(other: NSSet): bool
    }

    /// `NSMutableSet` — `NSSet` you can add to / remove from
    /// in-place. Mint with `set()` (empty) or `setWithCapacity:`
    /// (preallocate).
    @objc pub class NSMutableSet : NSSet {
        @objc("alloc")
        pub static alloc(): NSMutableSet
        @objc("init")
        pub init(): NSMutableSet
        @objc("set")
        pub static set(): NSMutableSet
        @objc("setWithCapacity:")
        pub static setWithCapacity(capacity: u64): NSMutableSet

        @objc("addObject:")
        pub addObject(o: NSObject)
        @objc("removeObject:")
        pub removeObject(o: NSObject)
        @objc("removeAllObjects")
        pub removeAllObjects()

        @objc("unionSet:")
        pub unionSet(other: NSSet)
        @objc("minusSet:")
        pub minusSet(other: NSSet)
        @objc("intersectSet:")
        pub intersectSet(other: NSSet)
    }

    /// `NSCountedSet` — set that remembers how many times each
    /// member has been inserted. `count(for:)` returns the
    /// multiplicity; `addObject:` on an existing member bumps the
    /// counter rather than being a no-op.
    @objc pub class NSCountedSet : NSMutableSet {
        @objc("alloc")
        pub static alloc(): NSCountedSet
        @objc("init")
        pub init(): NSCountedSet
        @objc("initWithArray:")
        pub initWithArray(arr: NSArray): NSCountedSet
        @objc("initWithSet:")
        pub initWithSet(set: NSSet): NSCountedSet

        @objc("countForObject:")
        pub countForObject(o: NSObject): i64
    }

    /// `NSIndexSet` — set of NSUInteger indices, optimised for
    /// long contiguous runs (stored as ranges internally). Use
    /// when you want to talk about row / column / element
    /// positions efficiently — most AppKit collection APIs that
    /// take "the selected rows" want one of these.
    @objc pub class NSIndexSet : NSObject {
        @objc("indexSet")
        pub static indexSet(): NSIndexSet
        @objc("indexSetWithIndex:")
        pub static indexSetWithIndex(i: u64): NSIndexSet
        @objc("indexSetWithIndexesInRange:")
        pub static indexSetWithIndexesInRange(range: NSRange): NSIndexSet

        @objc("count")
        pub get count(): i64
        @objc("firstIndex")
        pub get firstIndex(): u64
        @objc("lastIndex")
        pub get lastIndex(): u64
        @objc("containsIndex:")
        pub containsIndex(i: u64): bool
        @objc("containsIndexesInRange:")
        pub containsIndexesInRange(range: NSRange): bool
        @objc("containsIndexes:")
        pub containsIndexes(other: NSIndexSet): bool
        @objc("intersectsIndexesInRange:")
        pub intersectsIndexesInRange(range: NSRange): bool
        @objc("isEqualToIndexSet:")
        pub isEqualToIndexSet(other: NSIndexSet): bool
        @objc("countOfIndexesInRange:")
        pub countOfIndexesInRange(range: NSRange): i64
    }

    /// `NSMutableIndexSet` — index set with explicit add / remove.
    /// All edits operate on either a single index or a contiguous
    /// range; the internal range-based storage keeps both shapes
    /// cheap.
    @objc pub class NSMutableIndexSet : NSIndexSet {
        @objc("alloc")
        pub static alloc(): NSMutableIndexSet
        @objc("init")
        pub init(): NSMutableIndexSet
        @objc("indexSet")
        pub static indexSet(): NSMutableIndexSet

        @objc("addIndex:")
        pub addIndex(i: u64)
        @objc("removeIndex:")
        pub removeIndex(i: u64)
        @objc("addIndexes:")
        pub addIndexes(other: NSIndexSet)
        @objc("removeIndexes:")
        pub removeIndexes(other: NSIndexSet)
        @objc("addIndexesInRange:")
        pub addIndexesInRange(range: NSRange)
        @objc("removeIndexesInRange:")
        pub removeIndexesInRange(range: NSRange)
        @objc("removeAllIndexes")
        pub removeAllIndexes()
        @objc("shiftIndexesStartingAtIndex:by:")
        pub shiftIndexesStartingAtIndex(start: u64, by: i64)
    }

    /// `NSOrderedSet` — set with deterministic iteration order
    /// (insertion order). Has both set-style membership and
    /// array-style indexing. Useful when you need uniqueness
    /// guarantees AND a stable order at the same time (e.g. a
    /// menu's selection history).
    @objc pub class NSOrderedSet : NSObject {
        @objc("orderedSet")
        pub static orderedSet(): NSOrderedSet
        @objc("orderedSetWithArray:")
        pub static orderedSetWithArray(arr: NSArray): NSOrderedSet

        @objc("count")
        pub get count(): i64
        @objc("firstObject")
        pub get firstObject(): NSObject
        @objc("lastObject")
        pub get lastObject(): NSObject
        @objc("objectAtIndex:")
        pub objectAtIndex(i: u64): NSObject
        @objc("indexOfObject:")
        pub indexOfObject(o: NSObject): i64
        @objc("containsObject:")
        pub containsObject(o: NSObject): bool
        @objc("isEqualToOrderedSet:")
        pub isEqualToOrderedSet(other: NSOrderedSet): bool

        @objc("array")
        pub get array(): NSArray
        @objc("set")
        pub get set(): NSSet
    }

    /// `NSMutableOrderedSet` — writable ordered set. Mirrors the
    /// NSMutableArray surface (`addObject` / `insertObject:atIndex:` /
    /// `removeObject:` / `removeObjectAtIndex:`) plus the
    /// set-style `addObject` that's a no-op on an existing member.
    @objc pub class NSMutableOrderedSet : NSOrderedSet {
        @objc("alloc")
        pub static alloc(): NSMutableOrderedSet
        @objc("init")
        pub init(): NSMutableOrderedSet
        @objc("orderedSet")
        pub static orderedSet(): NSMutableOrderedSet
        @objc("orderedSetWithCapacity:")
        pub static orderedSetWithCapacity(capacity: u64): NSMutableOrderedSet

        @objc("addObject:")
        pub addObject(o: NSObject)
        @objc("insertObject:atIndex:")
        pub insertObject(o: NSObject, atIndex: u64)
        @objc("removeObject:")
        pub removeObject(o: NSObject)
        @objc("removeObjectAtIndex:")
        pub removeObjectAtIndex(i: u64)
        @objc("removeAllObjects")
        pub removeAllObjects()
        @objc("moveObjectsAtIndexes:toIndex:")
        pub moveObjectsAtIndexes(indexes: NSIndexSet, toIndex: u64)
        @objc("exchangeObjectAtIndex:withObjectAtIndex:")
        pub exchangeObjectAtIndex(i: u64, withObjectAtIndex: u64)
    }

    /// `NSHashTable` — set-like container parameterised by
    /// pointer-personality `options` (default = strong references
    /// like NSMutableSet; `weakMemory()` factory drops entries
    /// when their referent dies). Useful for "observer" registries
    /// that shouldn't keep their observers alive.
    @objc pub class NSHashTable : NSObject {
        @objc("alloc")
        pub static alloc(): NSHashTable
        @objc("init")
        pub init(): NSHashTable
        @objc("hashTableWithOptions:")
        pub static hashTableWithOptions(options: u64): NSHashTable
        @objc("weakObjectsHashTable")
        pub static weakObjectsHashTable(): NSHashTable

        @objc("count")
        pub get count(): u64
        @objc("addObject:")
        pub addObject(o: NSObject)
        @objc("removeObject:")
        pub removeObject(o: NSObject)
        @objc("removeAllObjects")
        pub removeAllObjects()
        @objc("containsObject:")
        pub containsObject(o: NSObject): bool
        @objc("member:")
        pub member(o: NSObject): NSObject
        @objc("allObjects")
        pub get allObjects(): NSArray
    }

    /// `NSMapTable` — dictionary-like container parameterised by
    /// key / value pointer-personality options. The two most
    /// common shapes: `strongToWeakObjectsMapTable()` (strong
    /// keys → weak values, the textbook observer table) and
    /// `weakToStrongObjectsMapTable()`.
    @objc pub class NSMapTable : NSObject {
        @objc("alloc")
        pub static alloc(): NSMapTable
        @objc("mapTableWithKeyOptions:valueOptions:")
        pub static mapTableWithOptions(keyOptions: u64, valueOptions: u64): NSMapTable
        @objc("strongToWeakObjectsMapTable")
        pub static strongToWeakObjectsMapTable(): NSMapTable
        @objc("weakToStrongObjectsMapTable")
        pub static weakToStrongObjectsMapTable(): NSMapTable
        @objc("strongToStrongObjectsMapTable")
        pub static strongToStrongObjectsMapTable(): NSMapTable
        @objc("weakToWeakObjectsMapTable")
        pub static weakToWeakObjectsMapTable(): NSMapTable

        @objc("count")
        pub get count(): u64
        @objc("objectForKey:")
        pub objectForKey(k: NSObject): NSObject
        @objc("setObject:forKey:")
        pub setObject(o: NSObject, forKey: NSObject)
        @objc("removeObjectForKey:")
        pub removeObjectForKey(k: NSObject)
        @objc("removeAllObjects")
        pub removeAllObjects()
    }

    /// `NSPointerArray` — array variant with configurable
    /// pointer-personality. Used internally by NSHashTable /
    /// NSMapTable; surface here for the rare case where you
    /// genuinely want an ordered weak-reference array. NULLs
    /// are explicitly representable as slot values.
    @objc pub class NSPointerArray : NSObject {
        @objc("alloc")
        pub static alloc(): NSPointerArray
        @objc("pointerArrayWithOptions:")
        pub static pointerArrayWithOptions(options: u64): NSPointerArray
        @objc("strongObjectsPointerArray")
        pub static strongObjectsPointerArray(): NSPointerArray
        @objc("weakObjectsPointerArray")
        pub static weakObjectsPointerArray(): NSPointerArray

        @objc("count")
        pub get count(): u64
        @objc("setCount:")
        pub set count(n: u64)
        @objc("pointerAtIndex:")
        pub pointerAtIndex(i: u64): NSObject
        @objc("addPointer:")
        pub addPointer(p: NSObject)
        @objc("removePointerAtIndex:")
        pub removePointerAtIndex(i: u64)
        @objc("insertPointer:atIndex:")
        pub insertPointer(p: NSObject, atIndex: u64)
        @objc("replacePointerAtIndex:withPointer:")
        pub replacePointerAtIndex(i: u64, withPointer: NSObject)
        @objc("compact")
        pub compact()
        @objc("allObjects")
        pub get allObjects(): NSArray
    }

    /// `NSSortDescriptor` — single sort key + direction (and
    /// optionally a comparison selector). Hand an array of
    /// descriptors to `[NSArray sortedArrayUsingDescriptors:]`
    /// to get a stable multi-key sort: the first descriptor
    /// breaks ties first, then the second, and so on.
    /// `reversedSortDescriptor` is the inverted-direction sibling
    /// for the same key, handy for "click the column header to
    /// flip the sort".
    @objc pub class NSSortDescriptor : NSObject {
        @objc("sortDescriptorWithKey:ascending:")
        pub static sortDescriptorWithKey(
            key: NSString, ascending: bool
        ): NSSortDescriptor
        @objc("sortDescriptorWithKey:ascending:selector:")
        pub static sortDescriptorWithKeyAndSelector(
            key: NSString, ascending: bool, selector: i64
        ): NSSortDescriptor

        @objc("key")
        pub get key(): NSString
        @objc("ascending")
        pub get ascending(): bool
        @objc("selector")
        pub get selector(): i64

        @objc("reversedSortDescriptor")
        pub get reversedSortDescriptor(): NSSortDescriptor
    }
}
