// IO and stream bindings — NSFileHandle, NSPipe, the NSStream
// tree (input / output), and NSCache.
//
// NSStream's `read:maxLength:` / `write:maxLength:` selectors
// traffic in `uint8_t *` buffers. The raw selectors stay
// module-private and the public wrappers expose
// `u8[]` (write side, copies the bytes in) or
// `bytesFromBuffer` round-trips (read side).

use core { NSObject, NSString, NSArray, NSDictionary, NSData, NSMutableData, NSError }
use collections { NSEnumerator }
use network { NSURL }

// NSStreamStatus — current state of an NSInputStream /
// NSOutputStream. `notOpen` → `opening` → `open` → `reading` /
// `writing` → `atEnd` / `error` / `closed`.
pub enum StreamStatus: u64 {
    notOpen = 0
    opening = 1
    open    = 2
    reading = 3
    writing = 4
    atEnd   = 5
    closed  = 6
    error   = 7
}

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSFileHandle` — wraps an open file descriptor. The
    /// reading / writing / updating factories open the file in
    /// the matching mode and hand back a ready-to-use handle.
    /// Use `NSFileHandle.standardOutput()` / `standardError()` to
    /// write into a process's existing stdout / stderr without
    /// opening them yourself.
    @objc pub class NSFileHandle : NSObject {
        @objc("fileHandleForReadingAtPath:")
        pub static fileHandleForReadingAtPath(path: NSString): NSFileHandle
        @objc("fileHandleForWritingAtPath:")
        pub static fileHandleForWritingAtPath(path: NSString): NSFileHandle
        @objc("fileHandleForUpdatingAtPath:")
        pub static fileHandleForUpdatingAtPath(path: NSString): NSFileHandle

        @objc("fileHandleWithStandardInput")
        pub static standardInput(): NSFileHandle
        @objc("fileHandleWithStandardOutput")
        pub static standardOutput(): NSFileHandle
        @objc("fileHandleWithStandardError")
        pub static standardError(): NSFileHandle
        @objc("fileHandleWithNullDevice")
        pub static nullDevice(): NSFileHandle

        @objc("readDataToEndOfFile")
        pub readDataToEndOfFile(): NSData
        @objc("readDataOfLength:")
        pub readDataOfLength(length: u64): NSData
        @objc("availableData")
        pub get availableData(): NSData

        @objc("writeData:")
        pub writeData(data: NSData)

        @objc("offsetInFile")
        pub get offsetInFile(): u64
        @objc("seekToFileOffset:")
        pub seekToFileOffset(offset: u64)
        @objc("seekToEndOfFile")
        pub seekToEndOfFile(): u64
        @objc("truncateFileAtOffset:")
        pub truncateFileAtOffset(offset: u64)
        @objc("synchronizeFile")
        pub synchronizeFile()
        @objc("closeFile")
        pub closeFile()

        @objc("fileDescriptor")
        pub get fileDescriptor(): i32

        // ─── Error-returning variants (10.15+) ────────────────
        // The plain `readDataOfLength:` / `writeData:` family
        // raise NSExceptions on failure; the
        // `…AndReturnError:` mirrors below take the error
        // through an NSError ** slot instead.
        @objc("readDataUpToLength:error:")
        pub readDataUpToLength(length: u64, error: i64): NSData
        @objc("readDataToEndOfFileAndReturnError:")
        pub readDataToEndOfFileAndReturnError(error: i64): NSData
        @objc("writeData:error:")
        pub writeDataAndReturnError(data: NSData, error: i64): bool
        @objc("getOffset:error:")
        pub getOffset(offset: *u64, error: i64): bool
        @objc("seekToOffset:error:")
        pub seekToOffset(offset: u64, error: i64): bool
        @objc("seekToEndReturningOffset:error:")
        pub seekToEndReturningOffset(offset: *u64, error: i64): bool
        @objc("truncateAtOffset:error:")
        pub truncateAtOffset(offset: u64, error: i64): bool
        @objc("synchronizeAndReturnError:")
        pub synchronizeAndReturnError(error: i64): bool
        @objc("closeAndReturnError:")
        pub closeAndReturnError(error: i64): bool

        // ─── Construction from descriptor / URL ──────────────
        @objc("fileHandleWithFileDescriptor:closeOnDealloc:")
        pub static fileHandleWithFileDescriptor(
            fd: i32, closeOnDealloc: bool
        ): NSFileHandle
        @objc("alloc")
        pub static alloc(): NSFileHandle
        @objc("initWithFileDescriptor:closeOnDealloc:")
        pub initWithFileDescriptor(
            fd: i32, closeOnDealloc: bool
        ): NSFileHandle

        // ─── Async readability handlers ──────────────────────
        // Block-based notifications fire on a background queue
        // when data / EOF / error becomes available.
        @objc("readabilityHandler")
        pub get readabilityHandler(): i64
        @objc("setReadabilityHandler:")
        pub set readabilityHandler(h: i64)
        @objc("writeabilityHandler")
        pub get writeabilityHandler(): i64
        @objc("setWriteabilityHandler:")
        pub set writeabilityHandler(h: i64)
    }

    /// `NSPipe` — anonymous pipe + a pair of NSFileHandle ends.
    /// The standard recipe for capturing a child process's
    /// stdout: build the pipe, hand the writing end to
    /// `NSTask.setStandardOutput:`, read off the reading end
    /// after the task exits.
    @objc pub class NSPipe : NSObject {
        @objc("pipe")
        pub static pipe(): NSPipe
        @objc("alloc")
        pub static alloc(): NSPipe
        @objc("init")
        pub init(): NSPipe

        @objc("fileHandleForReading")
        pub get fileHandleForReading(): NSFileHandle
        @objc("fileHandleForWriting")
        pub get fileHandleForWriting(): NSFileHandle
    }

    /// `NSStream` — abstract base for NSInputStream /
    /// NSOutputStream. Provides the lifecycle (`open` / `close`)
    /// and status interrogation; the actual byte read / write
    /// surface lives on the typed subclasses.
    @objc pub class NSStream : NSObject {
        @objc("open")
        pub open()
        @objc("close")
        pub close()

        @objc("streamStatus")
        pub get streamStatus(): StreamStatus
        @objc("streamError")
        pub get streamError(): NSError

        @objc("delegate")
        pub get delegate(): NSObject
        @objc("setDelegate:")
        pub set delegate(d: NSObject)

        @objc("scheduleInRunLoop:forMode:")
        pub scheduleInRunLoop(runLoop: NSObject, forMode: NSString)
        @objc("removeFromRunLoop:forMode:")
        pub removeFromRunLoop(runLoop: NSObject, forMode: NSString)

        @objc("propertyForKey:")
        pub propertyForKey(key: NSString): NSObject
        @objc("setProperty:forKey:")
        pub setProperty(value: NSObject, forKey: NSString): bool

        /// Pair of one input + one output stream connected to
        /// the host `hostname` on the given `port`. The result
        /// stays out-of-band; both streams come back via the
        /// pointer slots (which the caller is expected to fill
        /// in with `NSInputStream` / `NSOutputStream` pointers).
        @objc("getStreamsToHostWithName:port:inputStream:outputStream:")
        pub static getStreamsToHost(
            hostname: NSString, port: i64,
            inputStream: i64, outputStream: i64
        )
    }

    /// `NSInputStream` — byte-reading stream over a file, NSData,
    /// or NSURL. Construct via the typed factories; call
    /// `.open()` then loop `read:maxLength:` until
    /// `hasBytesAvailable` flips to `false`.
    @objc pub class NSInputStream : NSStream {
        @objc("inputStreamWithFileAtPath:")
        pub static inputStreamWithFileAtPath(path: NSString): NSInputStream
        @objc("inputStreamWithData:")
        pub static inputStreamWithData(data: NSData): NSInputStream
        @objc("inputStreamWithURL:")
        pub static inputStreamWithURL(url: NSURL): NSInputStream

        @objc("alloc")
        pub static alloc(): NSInputStream
        @objc("initWithFileAtPath:")
        pub initWithFileAtPath(path: NSString): NSInputStream
        @objc("initWithData:")
        pub initWithData(data: NSData): NSInputStream
        @objc("initWithURL:")
        pub initWithURL(url: NSURL): NSInputStream

        @objc("read:maxLength:")
        _read(buffer: *u8, maxLength: u64): i64
        @objc("hasBytesAvailable")
        pub get hasBytesAvailable(): bool

        /// Read up to `maxLength` bytes off the stream into a
        /// fresh `u8[]`. Returns the slice of bytes actually
        /// read (may be shorter than `maxLength`); a length-0
        /// slice means EOF (call `streamError` to disambiguate
        /// from a real error).
        pub readBytes(maxLength: u64): u8[] {
            let buf: u8[] = [0 as u8]
            let i: u64 = 1 as u64
            while i < maxLength {
                buf.push(0 as u8)
                i = i + (1 as u64)
            }
            let p: *u8 = buf
            let got = this._read(p, maxLength)
            if got < buf.length {
                let trimmed: u8[] = []
                let j = 0
                while j < got {
                    trimmed.push(buf[j])
                    j = j + 1
                }
                return trimmed
            }
            buf
        }
    }

    /// `NSOutputStream` — byte-writing stream over a file, an
    /// in-memory buffer, or a caller-supplied byte buffer. Same
    /// lifecycle as `NSInputStream`: open, write in a loop until
    /// the source data is consumed, close. Pull the accumulated
    /// bytes off an in-memory output stream via
    /// `propertyForKey("NSStreamDataWrittenToMemoryStreamKey")`.
    @objc pub class NSOutputStream : NSStream {
        @objc("outputStreamToFileAtPath:append:")
        pub static outputStreamToFileAtPath(
            path: NSString, append: bool
        ): NSOutputStream
        @objc("outputStreamToMemory")
        pub static outputStreamToMemory(): NSOutputStream
        @objc("outputStreamWithURL:append:")
        pub static outputStreamWithURL(
            url: NSURL, append: bool
        ): NSOutputStream

        @objc("alloc")
        pub static alloc(): NSOutputStream
        @objc("initToFileAtPath:append:")
        pub initToFileAtPath(path: NSString, append: bool): NSOutputStream
        @objc("initToMemory")
        pub initToMemory(): NSOutputStream
        @objc("initWithURL:append:")
        pub initWithURL(url: NSURL, append: bool): NSOutputStream

        @objc("write:maxLength:")
        _write(buffer: *const u8, maxLength: u64): i64
        @objc("hasSpaceAvailable")
        pub get hasSpaceAvailable(): bool

        /// Write `buf` to the stream; returns the number of bytes
        /// the stream accepted (may be less than `buf.length` for
        /// a partial write — loop until you've consumed all of
        /// `buf` if you need a complete send).
        pub writeBytes(buf: u8[]): i64 {
            let p: *u8 = buf
            this._write(p as *const u8, buf.length as u64)
        }
    }

    /// `NSCache` — bounded auto-evicting key/value store. Like
    /// NSMutableDictionary but with a `totalCostLimit` and
    /// `countLimit` the system enforces by dropping
    /// least-recently-used entries under memory pressure.
    /// Thread-safe for read + write.
    @objc pub class NSCache : NSObject {
        @objc("alloc")
        pub static alloc(): NSCache
        @objc("init")
        pub init(): NSCache

        @objc("name")
        pub get name(): NSString
        @objc("setName:")
        pub set name(s: NSString)
        @objc("countLimit")
        pub get countLimit(): u64
        @objc("setCountLimit:")
        pub set countLimit(n: u64)
        @objc("totalCostLimit")
        pub get totalCostLimit(): u64
        @objc("setTotalCostLimit:")
        pub set totalCostLimit(n: u64)

        @objc("objectForKey:")
        pub objectForKey(k: NSObject): NSObject
        @objc("setObject:forKey:")
        pub setObject(value: NSObject, forKey: NSObject)
        @objc("setObject:forKey:cost:")
        pub setObjectWithCost(value: NSObject, forKey: NSObject, cost: u64)
        @objc("removeObjectForKey:")
        pub removeObjectForKey(k: NSObject)
        @objc("removeAllObjects")
        pub removeAllObjects()
    }

    /// `NSFileManager` — filesystem operations. `defaultManager`
    /// returns the process-wide shared instance. Methods that take
    /// an `NSError **` out-parameter expose it as a raw `i64`
    /// (`0` discards). Returned `NSArray`s of names are UTF-8
    /// strings reachable via `ilstr(arr.objectAtIndex(i) as NSString)`.
    @objc pub class NSFileManager : NSObject {
        @objc("defaultManager")
        pub static defaultManager(): NSFileManager

        @objc("fileExistsAtPath:")
        pub fileExistsAtPath(path: NSString): bool
        @objc("contentsOfDirectoryAtPath:error:")
        pub contentsOfDirectoryAtPath(path: NSString, error: i64): NSArray
        @objc("createDirectoryAtPath:withIntermediateDirectories:attributes:error:")
        pub createDirectoryAtPath(
            path: NSString, intermediates: bool, attrs: NSObject, error: i64
        ): bool
        @objc("removeItemAtPath:error:")
        pub removeItemAtPath(path: NSString, error: i64): bool
        @objc("copyItemAtPath:toPath:error:")
        pub copyItemAtPath(src: NSString, dst: NSString, error: i64): bool
        @objc("moveItemAtPath:toPath:error:")
        pub moveItemAtPath(src: NSString, dst: NSString, error: i64): bool
        @objc("attributesOfItemAtPath:error:")
        pub attributesOfItemAtPath(path: NSString, error: i64): NSDictionary
        @objc("currentDirectoryPath")
        pub get currentDirectoryPath(): NSString
        @objc("changeCurrentDirectoryPath:")
        pub changeCurrentDirectoryPath(path: NSString): bool

        // ─── URL-based variants ───────────────────────────────
        // Prefer these over the path-based ones for new code:
        // NSURL flows through the system's security-scoped /
        // iCloud-aware code path without extra work.
        @objc("contentsOfDirectoryAtURL:includingPropertiesForKeys:options:error:")
        pub contentsOfDirectoryAtURL(
            url: NSURL, includingPropertiesForKeys: NSArray,
            options: u64, error: i64
        ): NSArray
        @objc("createDirectoryAtURL:withIntermediateDirectories:attributes:error:")
        pub createDirectoryAtURL(
            url: NSURL, withIntermediateDirectories: bool,
            attributes: NSDictionary, error: i64
        ): bool
        @objc("copyItemAtURL:toURL:error:")
        pub copyItemAtURL(srcURL: NSURL, toURL: NSURL, error: i64): bool
        @objc("moveItemAtURL:toURL:error:")
        pub moveItemAtURL(srcURL: NSURL, toURL: NSURL, error: i64): bool
        @objc("removeItemAtURL:error:")
        pub removeItemAtURL(url: NSURL, error: i64): bool
        @objc("trashItemAtURL:resultingItemURL:error:")
        pub trashItemAtURL(
            url: NSURL, resultingItemURL: NSURL, error: i64
        ): bool
        @objc("enumeratorAtURL:includingPropertiesForKeys:options:errorHandler:")
        pub enumeratorAtURL(
            url: NSURL, includingPropertiesForKeys: NSArray,
            options: u64, errorHandler: i64
        ): NSDirectoryEnumerator
        @objc("URLsForDirectory:inDomains:")
        pub urlsForDirectory(directory: u64, inDomains: u64): NSArray
        @objc("mountedVolumeURLsIncludingResourceValuesForKeys:options:")
        pub mountedVolumeURLs(
            includingResourceValuesForKeys: NSArray, options: u64
        ): NSArray

        // ─── Symbolic-link / hardlink primitives ──────────────
        @objc("createSymbolicLinkAtPath:withDestinationPath:error:")
        pub createSymbolicLinkAtPath(
            path: NSString, withDestinationPath: NSString, error: i64
        ): bool
        @objc("destinationOfSymbolicLinkAtPath:error:")
        pub destinationOfSymbolicLinkAtPath(
            path: NSString, error: i64
        ): NSString
        @objc("linkItemAtPath:toPath:error:")
        pub linkItemAtPath(path: NSString, toPath: NSString, error: i64): bool

        // ─── Path inspection / per-file predicates ────────────
        @objc("isExecutableFileAtPath:")
        pub isExecutableFileAtPath(path: NSString): bool
        @objc("isReadableFileAtPath:")
        pub isReadableFileAtPath(path: NSString): bool
        @objc("isWritableFileAtPath:")
        pub isWritableFileAtPath(path: NSString): bool
        @objc("isDeletableFileAtPath:")
        pub isDeletableFileAtPath(path: NSString): bool
        @objc("displayNameAtPath:")
        pub displayNameAtPath(path: NSString): NSString
        @objc("subpathsAtPath:")
        pub subpathsAtPath(path: NSString): NSArray
        @objc("subpathsOfDirectoryAtPath:error:")
        pub subpathsOfDirectoryAtPath(path: NSString, error: i64): NSArray
        @objc("createFileAtPath:contents:attributes:")
        pub createFileAtPath(
            path: NSString, contents: NSData, attributes: NSDictionary
        ): bool
        @objc("contentsAtPath:")
        pub contentsAtPath(path: NSString): NSData
        @objc("setAttributes:ofItemAtPath:error:")
        pub setAttributes(
            attributes: NSDictionary, ofItemAtPath: NSString, error: i64
        ): bool
        @objc("attributesOfFileSystemForPath:error:")
        pub attributesOfFileSystemForPath(
            path: NSString, error: i64
        ): NSDictionary
    }

    /// `NSBundle` — read-only access to a code+resources bundle.
    /// `mainBundle` is the running executable's bundle; useful for
    /// finding resource files shipped alongside the binary.
    @objc pub class NSBundle : NSObject {
        @objc("mainBundle")
        pub static mainBundle(): NSBundle
        @objc("alloc")
        pub static alloc(): NSBundle
        @objc("initWithPath:")
        pub initWithPath(path: NSString): NSBundle
        @objc("initWithURL:")
        pub initWithURL(url: NSURL): NSBundle
        @objc("bundleWithIdentifier:")
        pub static bundleWithIdentifier(id: NSString): NSBundle
        @objc("bundleForClass:")
        pub static bundleForClass(cls: NSObject): NSBundle
        @objc("allBundles")
        pub static allBundles(): NSArray
        @objc("allFrameworks")
        pub static allFrameworks(): NSArray

        @objc("bundlePath")
        pub get bundlePath(): NSString
        @objc("bundleURL")
        pub get bundleURL(): NSURL
        @objc("resourcePath")
        pub get resourcePath(): NSString
        @objc("resourceURL")
        pub get resourceURL(): NSURL
        @objc("executablePath")
        pub get executablePath(): NSString
        @objc("executableURL")
        pub get executableURL(): NSURL
        @objc("bundleIdentifier")
        pub get bundleIdentifier(): NSString
        @objc("infoDictionary")
        pub get infoDictionary(): NSDictionary
        @objc("localizedInfoDictionary")
        pub get localizedInfoDictionary(): NSDictionary
        @objc("objectForInfoDictionaryKey:")
        pub objectForInfoDictionaryKey(key: NSString): NSObject

        // ─── Resource lookup variants ─────────────────────────
        @objc("pathForResource:ofType:")
        pub pathForResource(name: NSString, ofType: NSString): NSString
        @objc("pathForResource:ofType:inDirectory:")
        pub pathForResourceInDir(
            name: NSString, ofType: NSString, inDirectory: NSString
        ): NSString
        @objc("pathForResource:ofType:inDirectory:forLocalization:")
        pub pathForResourceLocalized(
            name: NSString, ofType: NSString,
            inDirectory: NSString, forLocalization: NSString
        ): NSString
        @objc("URLForResource:withExtension:")
        pub urlForResource(name: NSString, withExtension: NSString): NSURL
        @objc("URLForResource:withExtension:subdirectory:")
        pub urlForResourceSubdir(
            name: NSString, withExtension: NSString, subdirectory: NSString
        ): NSURL
        @objc("URLsForResourcesWithExtension:subdirectory:")
        pub urlsForResources(
            withExtension: NSString, subdirectory: NSString
        ): NSArray

        // ─── Localization ────────────────────────────────────
        @objc("localizedStringForKey:value:table:")
        pub localizedStringForKey(
            key: NSString, value: NSString, table: NSString
        ): NSString
        @objc("preferredLocalizations")
        pub get preferredLocalizations(): NSArray
        @objc("localizations")
        pub get localizations(): NSArray
        @objc("developmentLocalization")
        pub get developmentLocalization(): NSString

        @objc("load")
        pub load(): bool
        @objc("loadAndReturnError:")
        pub loadAndReturnError(error: i64): bool
        @objc("isLoaded")
        pub get isLoaded(): bool
        @objc("unload")
        pub unload(): bool
    }

    /// `NSStreamDelegate` — hooks for `stream:handleEvent:`
    /// dispatching on an NSInputStream / NSOutputStream attached
    /// to a run loop. The `eventCode` argument encodes which of
    /// open / has-bytes / has-space / errored / ended happened
    /// (it's a bitmask but typical handlers branch on a single
    /// bit at a time).
    @objc pub interface NSStreamDelegate {
        @optional @objc("stream:handleEvent:")
        streamHandleEvent(stream: NSStream, eventCode: u64)
    }

    /// `NSCacheDelegate` — fires before NSCache evicts an entry,
    /// giving you one last chance to peek at the value (e.g. to
    /// flush dirty state to disk).
    @objc pub interface NSCacheDelegate {
        @optional @objc("cache:willEvictObject:")
        cacheWillEvictObject(cache: NSCache, object: NSObject)
    }

    /// `NSFileManagerDelegate` — per-operation override hooks.
    /// Each `should…` callback returns a `bool` deciding whether
    /// the corresponding copy / move / remove / link goes
    /// through; the `proceedAfterError:` variants run after a
    /// failure and let you continue past errors.
    @objc pub interface NSFileManagerDelegate {
        @optional @objc("fileManager:shouldCopyItemAtPath:toPath:")
        fileManagerShouldCopyItemAtPath(
            fileManager: NSFileManager, srcPath: NSString, toPath: NSString
        ): bool
        @optional @objc("fileManager:shouldMoveItemAtPath:toPath:")
        fileManagerShouldMoveItemAtPath(
            fileManager: NSFileManager, srcPath: NSString, toPath: NSString
        ): bool
        @optional @objc("fileManager:shouldRemoveItemAtPath:")
        fileManagerShouldRemoveItemAtPath(
            fileManager: NSFileManager, path: NSString
        ): bool
        @optional @objc("fileManager:shouldLinkItemAtPath:toPath:")
        fileManagerShouldLinkItemAtPath(
            fileManager: NSFileManager, srcPath: NSString, toPath: NSString
        ): bool
    }

    /// `NSDirectoryEnumerator` — NSEnumerator subclass that walks
    /// a directory tree. Returned by
    /// `NSFileManager.enumeratorAtURL:includingPropertiesForKeys:options:errorHandler:`;
    /// each `nextObject` yields the next NSURL in the traversal.
    /// `skipDescendants` tells the enumerator to skip the
    /// subtree below the most recently yielded directory.
    @objc pub class NSDirectoryEnumerator : NSEnumerator {
        @objc("fileAttributes")
        pub get fileAttributes(): NSDictionary
        @objc("directoryAttributes")
        pub get directoryAttributes(): NSDictionary
        @objc("level")
        pub get level(): u64
        @objc("skipDescendants")
        pub skipDescendants()
        @objc("skipDescendents")
        pub skipDescendents()
    }

    /// `NSPurgeableData` — NSMutableData variant whose bytes the
    /// system can discard under memory pressure. Wrap accesses in
    /// `beginContentAccess` / `endContentAccess` so the discard
    /// can't race a read; `isContentDiscarded` tells you whether
    /// the buffer is still backed.
    @objc pub class NSPurgeableData : NSMutableData {
        @objc("alloc")
        pub static alloc(): NSPurgeableData
        @objc("init")
        pub init(): NSPurgeableData

        @objc("beginContentAccess")
        pub beginContentAccess(): bool
        @objc("endContentAccess")
        pub endContentAccess()
        @objc("discardContentIfPossible")
        pub discardContentIfPossible()
        @objc("isContentDiscarded")
        pub get isContentDiscarded(): bool
    }

    /// `NSFileWrapper` — file / directory representation NSDocument
    /// uses for filesystem-bundle storage. A wrapper can be a
    /// regular file (bytes + attributes), a directory (children
    /// → wrappers map), or a symlink. Serialise to / from disk
    /// via `writeToURL:options:originalContentsURL:error:` and
    /// `initWithURL:options:error:`.
    @objc pub class NSFileWrapper : NSObject {
        @objc("alloc")
        pub static alloc(): NSFileWrapper
        @objc("initRegularFileWithContents:")
        pub initRegularFileWithContents(data: NSData): NSFileWrapper
        @objc("initDirectoryWithFileWrappers:")
        pub initDirectoryWithFileWrappers(wrappers: NSDictionary): NSFileWrapper
        @objc("initSymbolicLinkWithDestinationURL:")
        pub initSymbolicLinkWithDestinationURL(url: NSURL): NSFileWrapper
        @objc("initWithURL:options:error:")
        pub initWithURL(url: NSURL, options: u64, error: i64): NSFileWrapper

        @objc("isRegularFile")
        pub get isRegularFile(): bool
        @objc("isDirectory")
        pub get isDirectory(): bool
        @objc("isSymbolicLink")
        pub get isSymbolicLink(): bool

        @objc("filename")
        pub get filename(): NSString
        @objc("setFilename:")
        pub set filename(s: NSString)
        @objc("preferredFilename")
        pub get preferredFilename(): NSString
        @objc("setPreferredFilename:")
        pub set preferredFilename(s: NSString)

        @objc("regularFileContents")
        pub get regularFileContents(): NSData
        @objc("fileWrappers")
        pub get fileWrappers(): NSDictionary
        @objc("symbolicLinkDestinationURL")
        pub get symbolicLinkDestinationURL(): NSURL

        @objc("addFileWrapper:")
        pub addFileWrapper(child: NSFileWrapper): NSString
        @objc("removeFileWrapper:")
        pub removeFileWrapper(child: NSFileWrapper)

        @objc("writeToURL:options:originalContentsURL:error:")
        pub writeToURL(
            url: NSURL, options: u64, originalContentsURL: NSURL, error: i64
        ): bool
    }

    /// `NSFileCoordinator` — serialises reads / writes to shared
    /// files across processes. Wrap each access in a coordinate
    /// call; the system pauses competing accesses (including
    /// those from other apps via iCloud Drive / Document
    /// Provider) until your closure returns. The block
    /// `byAccessor:` receives the URL the system actually opened
    /// (may be canonicalised) and returns nothing — perform the
    /// IO inside it.
    @objc pub class NSFileCoordinator : NSObject {
        @objc("alloc")
        pub static alloc(): NSFileCoordinator
        @objc("init")
        pub init(): NSFileCoordinator
        @objc("initWithFilePresenter:")
        pub initWithFilePresenter(presenter: NSObject): NSFileCoordinator

        @objc("coordinateReadingItemAtURL:options:error:byAccessor:")
        pub coordinateReadingItemAtURL(
            url: NSURL, options: u64, error: i64, byAccessor: i64
        )
        @objc("coordinateWritingItemAtURL:options:error:byAccessor:")
        pub coordinateWritingItemAtURL(
            url: NSURL, options: u64, error: i64, byAccessor: i64
        )
        @objc("itemAtURL:didMoveToURL:")
        pub itemAtURL(url: NSURL, didMoveToURL: NSURL)
        @objc("addFilePresenter:")
        pub static addFilePresenter(p: NSObject)
        @objc("removeFilePresenter:")
        pub static removeFilePresenter(p: NSObject)
        @objc("filePresenters")
        pub static filePresenters(): NSArray
    }

    /// `NSFilePresenter` — protocol the coordinator dispatches
    /// "another process changed this file" events into. Implement
    /// the methods you care about on your class and register the
    /// instance with `NSFileCoordinator.addFilePresenter(...)`.
    @objc pub interface NSFilePresenter {
        presentedItemURL(): NSURL
        presentedItemOperationQueue(): NSObject

        @optional presentedItemDidChange()
        @optional @objc("presentedItemDidMoveToURL:")
        presentedItemDidMoveToURL(newURL: NSURL)
        @optional accommodatePresentedItemDeletion(completionHandler: i64)
        @optional savePresentedItemChangesWithCompletionHandler(completionHandler: i64)
        @optional @objc("relinquishPresentedItemToReader:")
        relinquishPresentedItemToReader(reader: i64)
        @optional @objc("relinquishPresentedItemToWriter:")
        relinquishPresentedItemToWriter(writer: i64)
    }

    /// `NSFileSecurity` — opaque value carrying a file's owner /
    /// group / permission ACL. Pulled off an NSURL via the
    /// `NSURLFileSecurityKey` resource value; mutate (typically
    /// by going through CoreFoundation's `CFFileSecurity` API)
    /// and write back to set permissions.
    @objc pub class NSFileSecurity : NSObject {}

    /// `NSFileVersion` — a single saved revision of a document.
    /// The system retains them automatically when the user enables
    /// "auto-save" (the macOS document model); apps reach them via
    /// `+otherVersionsOfItemAtURL:` for older snapshots or
    /// `+currentVersionOfItemAtURL:` for the canonical current
    /// one. `replaceItemAtURL:options:error:` rolls back to a
    /// stored version; `removeAndReturnError:` discards one.
    @objc pub class NSFileVersion : NSObject {
        @objc("currentVersionOfItemAtURL:")
        pub static currentVersionOfItemAtURL(url: NSURL): NSFileVersion
        @objc("otherVersionsOfItemAtURL:")
        pub static otherVersionsOfItemAtURL(url: NSURL): NSArray
        @objc("unresolvedConflictVersionsOfItemAtURL:")
        pub static unresolvedConflictVersionsOfItemAtURL(url: NSURL): NSArray

        @objc("URL")
        pub get url(): NSURL
        @objc("localizedName")
        pub get localizedName(): NSString
        @objc("localizedNameOfSavingComputer")
        pub get localizedNameOfSavingComputer(): NSString
        @objc("modificationDate")
        pub get modificationDate(): NSObject
        @objc("persistentIdentifier")
        pub get persistentIdentifier(): NSObject

        @objc("isConflict")
        pub get isConflict(): bool
        @objc("isResolved")
        pub get isResolved(): bool
        @objc("setResolved:")
        pub set isResolved(v: bool)
        @objc("isDiscardable")
        pub get isDiscardable(): bool
        @objc("setDiscardable:")
        pub set isDiscardable(v: bool)

        @objc("replaceItemAtURL:options:error:")
        pub replaceItemAtURL(url: NSURL, options: u64, error: i64): NSURL
        @objc("removeAndReturnError:")
        pub removeAndReturnError(error: i64): bool
    }

    /// `NSDistributedLock` — file-backed cross-process lock. Pass
    /// a path two cooperating processes both know; `tryLock`
    /// creates the file atomically (fails fast if another holder
    /// owns it). `unlock` deletes the file; `breakLock` removes
    /// it forcibly if the previous holder crashed.
    @objc pub class NSDistributedLock : NSObject {
        @objc("alloc")
        pub static alloc(): NSDistributedLock
        @objc("initWithPath:")
        pub initWithPath(path: NSString): NSDistributedLock
        @objc("lockWithPath:")
        pub static lockWithPath(path: NSString): NSDistributedLock

        @objc("tryLock")
        pub tryLock(): bool
        @objc("unlock")
        pub unlock()
        @objc("breakLock")
        pub breakLock()
        @objc("lockDate")
        pub get lockDate(): NSObject
    }
}
