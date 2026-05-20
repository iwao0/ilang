/// XPC bindings — Foundation's cross-process RPC machinery. Two
/// processes that want to talk to each other declare a shared
/// `@protocol` (the wire contract); each side wraps a connection
/// in an `NSXPCConnection`, sets the remote and exported
/// interfaces from `NSXPCInterface.interfaceWithProtocol:`, and
/// then drives method calls through `remoteObjectProxy`. The
/// system handles auth, marshalling, and forwarding under the hood.
///
/// On macOS, XPC services live in `Contents/XPCServices/*.xpc`
/// inside a bundle, or are vended by `launchd` agents through
/// `NSXPCListener.serviceListener` (mach-service-name mode).

use core { NSObject, NSString, NSArray, NSError }

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSXPCConnection` — one end of a bidirectional RPC pipe.
    /// `+alloc + -initWithServiceName:` is the easiest path:
    /// pass the bundled XPC service's bundle ID. Set
    /// `remoteObjectInterface` to the wire contract, then
    /// `-resume` and start sending. `remoteObjectProxy` returns
    /// a forwarding proxy on which method calls cross the wire.
    @objc pub class NSXPCConnection : NSObject {
        @objc("alloc")
        pub static alloc(): NSXPCConnection
        @objc("initWithServiceName:")
        pub initWithServiceName(name: NSString): NSXPCConnection
        @objc("initWithMachServiceName:options:")
        pub initWithMachServiceName(name: NSString, options: u64): NSXPCConnection
        @objc("initWithListenerEndpoint:")
        pub initWithListenerEndpoint(ep: NSXPCListenerEndpoint): NSXPCConnection

        @objc("remoteObjectInterface")
        pub get remoteObjectInterface(): NSXPCInterface
        @objc("setRemoteObjectInterface:")
        pub set remoteObjectInterface(iface: NSXPCInterface)
        @objc("exportedInterface")
        pub get exportedInterface(): NSXPCInterface
        @objc("setExportedInterface:")
        pub set exportedInterface(iface: NSXPCInterface)
        @objc("exportedObject")
        pub get exportedObject(): NSObject
        @objc("setExportedObject:")
        pub set exportedObject(o: NSObject)

        @objc("remoteObjectProxy")
        pub get remoteObjectProxy(): NSObject
        @objc("remoteObjectProxyWithErrorHandler:")
        pub remoteObjectProxyWithErrorHandler(
            handler: ObjCBlock<fn(i64)>
        ): NSObject
        @objc("synchronousRemoteObjectProxyWithErrorHandler:")
        pub synchronousRemoteObjectProxyWithErrorHandler(
            handler: ObjCBlock<fn(i64)>
        ): NSObject

        @objc("interruptionHandler")
        pub get interruptionHandler(): ObjCBlock<fn()>
        @objc("setInterruptionHandler:")
        pub set interruptionHandler(handler: ObjCBlock<fn()>)
        @objc("invalidationHandler")
        pub get invalidationHandler(): ObjCBlock<fn()>
        @objc("setInvalidationHandler:")
        pub set invalidationHandler(handler: ObjCBlock<fn()>)

        @objc("resume")
        pub resume()
        @objc("suspend")
        pub suspend()
        @objc("invalidate")
        pub invalidate()

        @objc("auditSessionIdentifier")
        pub get auditSessionIdentifier(): u32
        @objc("processIdentifier")
        pub get processIdentifier(): i32
        @objc("effectiveUserIdentifier")
        pub get effectiveUserIdentifier(): u32
        @objc("effectiveGroupIdentifier")
        pub get effectiveGroupIdentifier(): u32
    }

    /// `NSXPCInterface` — describes the protocol two ends of an
    /// XPC connection use to talk. Build with
    /// `+interfaceWithProtocol:` against an `@protocol` declared
    /// in shared code; for methods whose argument / return types
    /// aren't ObjC primitives, also call
    /// `setClasses:forSelector:argumentIndex:ofReply:` to
    /// register the allowed types (Foundation's secure-coding
    /// gate).
    @objc pub class NSXPCInterface : NSObject {
        @objc("interfaceWithProtocol:")
        pub static interfaceWithProtocol(proto: NSObject): NSXPCInterface

        @objc("protocol")
        pub get protocolName(): NSObject
        @objc("setProtocol:")
        pub set protocolName(proto: NSObject)

        @objc("setClasses:forSelector:argumentIndex:ofReply:")
        pub setClasses(
            classes: NSObject, forSelector: i64,
            argumentIndex: u64, ofReply: bool
        )
        @objc("classesForSelector:argumentIndex:ofReply:")
        pub classesForSelector(
            sel: i64, argumentIndex: u64, ofReply: bool
        ): NSObject
    }

    /// `NSXPCListener` — server side. Vend a service either as a
    /// bundled XPC service (`+serviceListener`), as a launchd
    /// mach-service (`+initWithMachServiceName:`), or anonymously
    /// for inter-process testing (`+anonymousListener`). Set the
    /// delegate and `-resume` to start accepting connections.
    @objc pub class NSXPCListener : NSObject {
        @objc("serviceListener")
        pub static serviceListener(): NSXPCListener
        @objc("anonymousListener")
        pub static anonymousListener(): NSXPCListener
        @objc("alloc")
        pub static alloc(): NSXPCListener
        @objc("initWithMachServiceName:")
        pub initWithMachServiceName(name: NSString): NSXPCListener

        @objc("delegate")
        pub get delegate(): NSObject
        @objc("setDelegate:")
        pub set delegate(d: NSObject)

        @objc("endpoint")
        pub get endpoint(): NSXPCListenerEndpoint

        @objc("resume")
        pub resume()
        @objc("suspend")
        pub suspend()
        @objc("invalidate")
        pub invalidate()
    }

    /// `NSXPCListenerEndpoint` — secure-codable handle pointing
    /// at a running listener. Send one across an existing
    /// connection so the receiver can open a new connection
    /// directly back to that listener (the standard "introduce
    /// two clients to each other" pattern).
    @objc pub class NSXPCListenerEndpoint : NSObject {}

    /// `NSXPCListenerDelegate` — implement
    /// `listener:shouldAcceptNewConnection:` to vet incoming
    /// connections. Set the new connection's exported interface
    /// + exported object, call `-resume`, and return `true` to
    /// accept; return `false` to drop.
    @objc pub interface NSXPCListenerDelegate {
        @optional @objc("listener:shouldAcceptNewConnection:")
        listenerShouldAcceptNewConnection(
            listener: NSXPCListener, newConnection: NSXPCConnection
        ): bool
    }
}
