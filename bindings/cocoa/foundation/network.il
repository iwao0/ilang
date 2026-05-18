// Networking bindings. Covers the URL-session class tree, request
// / response wrappers, query-string building, cookies, HTTP auth
// credentials, plus the block-completion variants of the data /
// download task factories — the runtime ships a
// `void(^)(id, id, id)` trampoline (`makeThreeObjectBlock`) that
// fits the standard `(NSData *, NSURLResponse *, NSError *)`
// shape.

use core { NSObject, NSString, NSArray, NSDictionary, NSData, NSError,
           makeThreeObjectBlock, makeObjectBlock }
use datetime { NSDate }

// NSURLSessionTaskState — `state` value on every URLSessionTask.
// `running` / `suspended` are the two states a task spends most
// of its life in; `completed` is terminal. `canceling` means a
// cancel was issued and is still propagating.
pub enum URLSessionTaskState: i64 {
    running    = 0
    suspended  = 1
    canceling  = 2
    completed  = 3
}

// NSURLCacheStoragePolicy — request-side cache hint. The system
// reserves the right to override `allowed` (downgrading it to
// `notAllowed` for HTTPS responses missing cache-control
// headers, for instance).
pub enum URLCacheStoragePolicy: u64 {
    allowed         = 0
    allowedInMemoryOnly = 1
    notAllowed      = 2
}

// NSURLRequestCachePolicy — controls how a request consults
// the local cache and whether it revalidates upstream.
pub enum URLRequestCachePolicy: u64 {
    useProtocolCachePolicy            = 0
    reloadIgnoringLocalCacheData      = 1
    reloadIgnoringLocalAndRemoteCacheData = 4
    returnCacheDataElseLoad           = 2
    returnCacheDataDontLoad           = 3
    reloadRevalidatingCacheData       = 5
}

// NSURLCredentialPersistence — how long a credential lives.
// `none` is per-request; `forSession` covers the process
// lifetime; `permanent` writes into the keychain; `synchronizable`
// also syncs through iCloud Keychain.
pub enum URLCredentialPersistence: u64 {
    none           = 0
    forSession     = 1
    permanent      = 2
    synchronizable = 3
}

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSURL` — URL value. Use `fromString` for `http://...`
    /// style URLs, `fromFilePath` for filesystem paths. Lives in
    /// the network module because most callers reach for it from
    /// HTTP / URL-loading contexts, even though `fileURLWithPath:`
    /// is just as legitimate.
    @objc pub class NSURL : NSObject {
        @objc("URLWithString:")
        pub static fromString(s: NSString): NSURL
        @objc("fileURLWithPath:")
        pub static fromFilePath(s: NSString): NSURL
        @objc("absoluteString")
        pub get absoluteString(): NSString
        @objc("path")
        pub get path(): NSString
    }

    /// `NSURLRequest` — immutable URL request. Build one with
    /// `requestWithURL:` for a GET, or the longer form with
    /// `requestWithURL:cachePolicy:timeoutInterval:` when you
    /// want bespoke cache / timeout behaviour. For mutating
    /// requests (custom method, header, body) drop down to
    /// `NSMutableURLRequest`.
    @objc pub class NSURLRequest : NSObject {
        @objc("requestWithURL:")
        pub static requestWithURL(url: NSURL): NSURLRequest
        @objc("requestWithURL:cachePolicy:timeoutInterval:")
        pub static requestWithURLAndPolicy(
            url: NSURL, cachePolicy: URLRequestCachePolicy, timeoutInterval: f64
        ): NSURLRequest

        @objc("URL")
        pub get url(): NSURL
        @objc("HTTPMethod")
        pub get httpMethod(): NSString
        @objc("allHTTPHeaderFields")
        pub get allHTTPHeaderFields(): NSDictionary
        @objc("valueForHTTPHeaderField:")
        pub valueForHTTPHeaderField(field: NSString): NSString
        @objc("HTTPBody")
        pub get httpBody(): NSData
        @objc("cachePolicy")
        pub get cachePolicy(): URLRequestCachePolicy
        @objc("timeoutInterval")
        pub get timeoutInterval(): f64
    }

    /// `NSMutableURLRequest` — writable variant. Set the method,
    /// headers, body, then hand it to an NSURLSession task
    /// factory. The setter pattern mirrors Foundation's other
    /// mutable types — no fluent chaining.
    @objc pub class NSMutableURLRequest : NSURLRequest {
        @objc("alloc")
        pub static alloc(): NSMutableURLRequest
        @objc("init")
        pub init(): NSMutableURLRequest

        @objc("setURL:")
        pub setURL(url: NSURL)
        @objc("setHTTPMethod:")
        pub setHTTPMethod(m: NSString)
        @objc("setValue:forHTTPHeaderField:")
        pub setValue(value: NSString, forHTTPHeaderField: NSString)
        @objc("addValue:forHTTPHeaderField:")
        pub addValue(value: NSString, forHTTPHeaderField: NSString)
        @objc("setAllHTTPHeaderFields:")
        pub setAllHTTPHeaderFields(headers: NSDictionary)
        @objc("setHTTPBody:")
        pub setHTTPBody(body: NSData)
        @objc("setCachePolicy:")
        pub setCachePolicy(p: URLRequestCachePolicy)
        @objc("setTimeoutInterval:")
        pub setTimeoutInterval(t: f64)
    }

    /// `NSURLResponse` — base response wrapper. For HTTP traffic
    /// you'll downcast to `NSHTTPURLResponse` (use
    /// `as NSHTTPURLResponse`) for the status code and headers.
    @objc pub class NSURLResponse : NSObject {
        @objc("URL")
        pub get url(): NSURL
        @objc("MIMEType")
        pub get mimeType(): NSString
        @objc("expectedContentLength")
        pub get expectedContentLength(): i64
        @objc("textEncodingName")
        pub get textEncodingName(): NSString
        @objc("suggestedFilename")
        pub get suggestedFilename(): NSString
    }

    /// `NSHTTPURLResponse` — HTTP-specific response carrying the
    /// status code and parsed header dictionary. The status-code
    /// → reason-phrase mapping is on the class itself
    /// (`localizedStringForStatusCode:`).
    @objc pub class NSHTTPURLResponse : NSURLResponse {
        @objc("statusCode")
        pub get statusCode(): i64
        @objc("allHeaderFields")
        pub get allHeaderFields(): NSDictionary
        @objc("valueForHTTPHeaderField:")
        pub valueForHTTPHeaderField(field: NSString): NSString
        @objc("localizedStringForStatusCode:")
        pub static localizedStringForStatusCode(code: i64): NSString
    }

    /// `NSURLSession` — entry point for HTTP / file / FTP /
    /// custom-scheme network calls. `shared()` is the
    /// process-wide default; for cookies / cache / auth
    /// configuration use `sessionWithConfiguration:` against an
    /// NSURLSessionConfiguration. Tasks start suspended — call
    /// `.resume()` to actually fire the request off the wire.
    @objc pub class NSURLSession : NSObject {
        @objc("sharedSession")
        pub static shared(): NSURLSession
        @objc("sessionWithConfiguration:")
        pub static sessionWithConfiguration(
            cfg: NSURLSessionConfiguration
        ): NSURLSession
        @objc("sessionWithConfiguration:delegate:delegateQueue:")
        pub static sessionWithConfigurationAndDelegate(
            cfg: NSURLSessionConfiguration, delegate: NSObject, queue: NSObject
        ): NSURLSession

        @objc("configuration")
        pub get configuration(): NSURLSessionConfiguration

        @objc("dataTaskWithURL:")
        pub dataTaskWithURL(url: NSURL): NSURLSessionDataTask
        @objc("dataTaskWithRequest:")
        pub dataTaskWithRequest(req: NSURLRequest): NSURLSessionDataTask
        @objc("downloadTaskWithURL:")
        pub downloadTaskWithURL(url: NSURL): NSURLSessionDownloadTask
        @objc("downloadTaskWithRequest:")
        pub downloadTaskWithRequest(req: NSURLRequest): NSURLSessionDownloadTask
        @objc("uploadTaskWithRequest:fromData:")
        pub uploadTaskWithRequestFromData(
            req: NSURLRequest, fromData: NSData
        ): NSURLSessionUploadTask
        @objc("uploadTaskWithRequest:fromFile:")
        pub uploadTaskWithRequestFromFile(
            req: NSURLRequest, fromFile: NSURL
        ): NSURLSessionUploadTask

        // ─── Block-completion variants ─────────────────────────
        // Raw `id`s come in pre-autoreleased; wrap them inside the
        // handler body via `NSData.__wrap_handle_unowned` etc. so
        // the surrounding ARC frame doesn't double-release.

        @objc("dataTaskWithRequest:completionHandler:")
        _dataTaskWithRequestAndHandler(
            req: NSURLRequest, handler: i64
        ): NSURLSessionDataTask
        @objc("dataTaskWithURL:completionHandler:")
        _dataTaskWithURLAndHandler(
            url: NSURL, handler: i64
        ): NSURLSessionDataTask
        @objc("downloadTaskWithRequest:completionHandler:")
        _downloadTaskWithRequestAndHandler(
            req: NSURLRequest, handler: i64
        ): NSURLSessionDownloadTask
        @objc("uploadTaskWithRequest:fromData:completionHandler:")
        _uploadTaskWithRequestFromDataAndHandler(
            req: NSURLRequest, fromData: NSData, handler: i64
        ): NSURLSessionUploadTask

        /// `dataTaskWithRequest:completionHandler:` block variant.
        /// The body is called with three raw `id` handles for
        /// `(NSData *, NSURLResponse *, NSError *)` — wrap them
        /// with `NSData.__wrap_handle_unowned`,
        /// `NSURLResponse.__wrap_handle_unowned`, and
        /// `NSError.__wrap_handle_unowned` inside if you want
        /// typed views. The returned task is still in the
        /// `suspended` state; call `.resume()` to fire.
        pub dataTaskWithRequestAndHandler(
            req: NSURLRequest, body: fn(i64, i64, i64)
        ): NSURLSessionDataTask {
            this._dataTaskWithRequestAndHandler(req, makeThreeObjectBlock(body))
        }
        pub dataTaskWithURLAndHandler(
            url: NSURL, body: fn(i64, i64, i64)
        ): NSURLSessionDataTask {
            this._dataTaskWithURLAndHandler(url, makeThreeObjectBlock(body))
        }
        /// `downloadTaskWithRequest:completionHandler:`. The block
        /// receives `(NSURL *destination, NSURLResponse *, NSError *)`
        /// — the destination URL points at a temp file the system
        /// will delete when the handler returns, so move / copy it
        /// inside the body if you want to keep the bytes.
        pub downloadTaskWithRequestAndHandler(
            req: NSURLRequest, body: fn(i64, i64, i64)
        ): NSURLSessionDownloadTask {
            this._downloadTaskWithRequestAndHandler(req, makeThreeObjectBlock(body))
        }
        /// `uploadTaskWithRequest:fromData:completionHandler:`.
        /// Same `(NSData *, NSURLResponse *, NSError *)` body
        /// shape as `dataTaskWithRequestAndHandler`.
        pub uploadTaskWithRequestFromDataAndHandler(
            req: NSURLRequest, fromData: NSData, body: fn(i64, i64, i64)
        ): NSURLSessionUploadTask {
            this._uploadTaskWithRequestFromDataAndHandler(
                req, fromData, makeThreeObjectBlock(body)
            )
        }

        @objc("invalidateAndCancel")
        pub invalidateAndCancel()
        @objc("finishTasksAndInvalidate")
        pub finishTasksAndInvalidate()
        @objc("resetWithCompletionHandler:")
        pub resetWithCompletionHandler(handler: i64)
        @objc("flushWithCompletionHandler:")
        pub flushWithCompletionHandler(handler: i64)
    }

    /// `NSURLSessionConfiguration` — session-level knobs:
    /// timeouts, cookie storage, cache, HTTP/2 enablement, etc.
    /// Use one of the typed factories (`default`,
    /// `ephemeral`, `backgroundSessionConfigurationWithIdentifier:`)
    /// rather than `[NSURLSessionConfiguration alloc] init` — the
    /// raw alloc/init path is private API.
    @objc pub class NSURLSessionConfiguration : NSObject {
        @objc("defaultSessionConfiguration")
        pub static default(): NSURLSessionConfiguration
        @objc("ephemeralSessionConfiguration")
        pub static ephemeral(): NSURLSessionConfiguration
        @objc("backgroundSessionConfigurationWithIdentifier:")
        pub static backgroundWithIdentifier(id: NSString): NSURLSessionConfiguration

        @objc("identifier")
        pub get identifier(): NSString
        @objc("timeoutIntervalForRequest")
        pub get timeoutIntervalForRequest(): f64
        @objc("setTimeoutIntervalForRequest:")
        pub set timeoutIntervalForRequest(t: f64)
        @objc("timeoutIntervalForResource")
        pub get timeoutIntervalForResource(): f64
        @objc("setTimeoutIntervalForResource:")
        pub set timeoutIntervalForResource(t: f64)
        @objc("HTTPMaximumConnectionsPerHost")
        pub get httpMaximumConnectionsPerHost(): i64
        @objc("setHTTPMaximumConnectionsPerHost:")
        pub set httpMaximumConnectionsPerHost(n: i64)
        @objc("waitsForConnectivity")
        pub get waitsForConnectivity(): bool
        @objc("setWaitsForConnectivity:")
        pub set waitsForConnectivity(v: bool)
        @objc("allowsCellularAccess")
        pub get allowsCellularAccess(): bool
        @objc("setAllowsCellularAccess:")
        pub set allowsCellularAccess(v: bool)
    }

    /// `NSURLSessionTask` — base class for all URL tasks. The
    /// state / countOfBytes properties report live progress;
    /// `resume` / `suspend` / `cancel` drive the lifecycle.
    @objc pub class NSURLSessionTask : NSObject {
        @objc("resume")
        pub resume()
        @objc("suspend")
        pub suspend()
        @objc("cancel")
        pub cancel()

        @objc("state")
        pub get state(): URLSessionTaskState
        @objc("error")
        pub get error(): NSError

        @objc("taskIdentifier")
        pub get taskIdentifier(): u64
        @objc("originalRequest")
        pub get originalRequest(): NSURLRequest
        @objc("currentRequest")
        pub get currentRequest(): NSURLRequest
        @objc("response")
        pub get response(): NSURLResponse

        @objc("countOfBytesReceived")
        pub get countOfBytesReceived(): i64
        @objc("countOfBytesSent")
        pub get countOfBytesSent(): i64
        @objc("countOfBytesExpectedToReceive")
        pub get countOfBytesExpectedToReceive(): i64
        @objc("countOfBytesExpectedToSend")
        pub get countOfBytesExpectedToSend(): i64

        @objc("taskDescription")
        pub get taskDescription(): NSString
        @objc("setTaskDescription:")
        pub set taskDescription(s: NSString)
    }

    /// `NSURLSessionDataTask` — task that receives data in memory.
    /// Use the delegate methods on `NSURLSessionDataDelegate` to
    /// observe bytes / response / completion (the block-completion
    /// variant is not bound yet — see top-of-file note).
    @objc pub class NSURLSessionDataTask : NSURLSessionTask {}

    /// `NSURLSessionDownloadTask` — task that streams its response
    /// to a temporary file. Observe `didFinishDownloadingToURL:`
    /// on `NSURLSessionDownloadDelegate` to move the file before
    /// it's auto-deleted.
    @objc pub class NSURLSessionDownloadTask : NSURLSessionTask {
        @objc("cancelByProducingResumeData:")
        pub cancelByProducingResumeData(handler: i64)
    }

    /// `NSURLSessionUploadTask` — task that sends data as the
    /// request body. The data comes from a `dataTaskWithURL:` or
    /// `dataTaskWithRequest:` factory call; the body is whatever
    /// you pass as the second argument.
    @objc pub class NSURLSessionUploadTask : NSURLSessionDataTask {}

    /// `NSURLComponents` — parse / build a URL by its named
    /// pieces (scheme / host / path / query). The `queryItems`
    /// array gives you structured access to query parameters
    /// instead of hand-rolled `?foo=bar&baz=qux` splitting.
    @objc pub class NSURLComponents : NSObject {
        @objc("alloc")
        pub static alloc(): NSURLComponents
        @objc("init")
        pub init(): NSURLComponents
        @objc("componentsWithURL:resolvingAgainstBaseURL:")
        pub static componentsWithURL(
            url: NSURL, resolvingAgainstBaseURL: bool
        ): NSURLComponents
        @objc("componentsWithString:")
        pub static componentsWithString(s: NSString): NSURLComponents

        @objc("URL")
        pub get url(): NSURL
        @objc("string")
        pub get string(): NSString

        @objc("scheme")
        pub get scheme(): NSString
        @objc("setScheme:")
        pub set scheme(s: NSString)
        @objc("host")
        pub get host(): NSString
        @objc("setHost:")
        pub set host(s: NSString)
        @objc("port")
        pub get port(): NSObject
        @objc("setPort:")
        pub set port(p: NSObject)
        @objc("path")
        pub get path(): NSString
        @objc("setPath:")
        pub set path(s: NSString)
        @objc("query")
        pub get query(): NSString
        @objc("setQuery:")
        pub set query(s: NSString)
        @objc("fragment")
        pub get fragment(): NSString
        @objc("setFragment:")
        pub set fragment(s: NSString)
        @objc("user")
        pub get user(): NSString
        @objc("setUser:")
        pub set user(s: NSString)
        @objc("password")
        pub get password(): NSString
        @objc("setPassword:")
        pub set password(s: NSString)

        @objc("queryItems")
        pub get queryItems(): NSArray
        @objc("setQueryItems:")
        pub set queryItems(items: NSArray)
    }

    /// `NSURLQueryItem` — name / value pair carried inside
    /// `NSURLComponents.queryItems`. Build with `queryItemWithName:value:`
    /// and hand a fresh `NSArray` of them into the components
    /// setter.
    @objc pub class NSURLQueryItem : NSObject {
        @objc("queryItemWithName:value:")
        pub static queryItem(name: NSString, value: NSString): NSURLQueryItem
        @objc("name")
        pub get name(): NSString
        @objc("value")
        pub get value(): NSString
    }

    /// `NSHTTPCookie` — single cookie value. Either parse a
    /// dictionary that mirrors the Set-Cookie header parts
    /// (`cookieWithProperties:`) or read off an existing one from
    /// the shared storage.
    @objc pub class NSHTTPCookie : NSObject {
        @objc("cookieWithProperties:")
        pub static cookieWithProperties(props: NSDictionary): NSHTTPCookie

        @objc("name")
        pub get name(): NSString
        @objc("value")
        pub get value(): NSString
        @objc("domain")
        pub get domain(): NSString
        @objc("path")
        pub get path(): NSString
        @objc("expiresDate")
        pub get expiresDate(): NSDate
        @objc("isSecure")
        pub get isSecure(): bool
        @objc("isHTTPOnly")
        pub get isHTTPOnly(): bool
        @objc("isSessionOnly")
        pub get isSessionOnly(): bool
        @objc("version")
        pub get version(): u64

        @objc("properties")
        pub get properties(): NSDictionary
    }

    /// `NSHTTPCookieStorage` — process-wide cookie jar.
    /// `sharedHTTPCookieStorage` is the default shared by every
    /// session that uses `NSURLSessionConfiguration.default()`;
    /// ephemeral sessions get an in-memory jar of their own.
    @objc pub class NSHTTPCookieStorage : NSObject {
        @objc("sharedHTTPCookieStorage")
        pub static shared(): NSHTTPCookieStorage

        @objc("cookies")
        pub get cookies(): NSArray
        @objc("cookiesForURL:")
        pub cookiesForURL(url: NSURL): NSArray
        @objc("setCookie:")
        pub setCookie(c: NSHTTPCookie)
        @objc("setCookies:forURL:mainDocumentURL:")
        pub setCookiesForURL(
            cookies: NSArray, forURL: NSURL, mainDocumentURL: NSURL
        )
        @objc("deleteCookie:")
        pub deleteCookie(c: NSHTTPCookie)
        @objc("removeCookiesSinceDate:")
        pub removeCookiesSinceDate(d: NSDate)
    }

    /// `NSURLCredential` — username / password / certificate
    /// credential handed to an auth challenge. The
    /// `persistence` controls how long the keychain remembers it.
    @objc pub class NSURLCredential : NSObject {
        @objc("credentialWithUser:password:persistence:")
        pub static credentialWithUserAndPassword(
            user: NSString, password: NSString,
            persistence: URLCredentialPersistence
        ): NSURLCredential

        @objc("user")
        pub get user(): NSString
        @objc("password")
        pub get password(): NSString
        @objc("hasPassword")
        pub get hasPassword(): bool
        @objc("persistence")
        pub get persistence(): URLCredentialPersistence
    }

    /// `NSURLProtectionSpace` — host / port / realm tuple
    /// describing where a credential applies. Auth challenges
    /// carry one of these; matching them is how the URL loading
    /// system knows whether to reuse a stored credential.
    @objc pub class NSURLProtectionSpace : NSObject {
        @objc("alloc")
        pub static alloc(): NSURLProtectionSpace
        @objc("initWithHost:port:protocol:realm:authenticationMethod:")
        pub initWithHost(
            host: NSString, port: i64, protocol: NSString,
            realm: NSString, authenticationMethod: NSString
        ): NSURLProtectionSpace

        @objc("host")
        pub get host(): NSString
        @objc("port")
        pub get port(): i64
        @objc("protocol")
        pub get protocolName(): NSString
        @objc("realm")
        pub get realm(): NSString
        @objc("authenticationMethod")
        pub get authenticationMethod(): NSString
        @objc("isProxy")
        pub get isProxy(): bool
        @objc("receivesCredentialSecurely")
        pub get receivesCredentialSecurely(): bool
    }

    /// `NSURLAuthenticationChallenge` — handed to the
    /// `URLSession:task:didReceiveChallenge:completionHandler:`
    /// delegate method. Pull the protection space off it, look up
    /// a credential, then call the completion handler with a
    /// disposition + optional NSURLCredential.
    @objc pub class NSURLAuthenticationChallenge : NSObject {
        @objc("protectionSpace")
        pub get protectionSpace(): NSURLProtectionSpace
        @objc("proposedCredential")
        pub get proposedCredential(): NSURLCredential
        @objc("previousFailureCount")
        pub get previousFailureCount(): i64
        @objc("failureResponse")
        pub get failureResponse(): NSURLResponse
        @objc("error")
        pub get error(): NSError
        @objc("sender")
        pub get sender(): NSObject
    }

    /// `NSURLSessionWebSocketTask` — WebSocket task. Open via
    /// `NSURLSession.webSocketTaskWithURL:` (added below); send
    /// and receive `NSURLSessionWebSocketMessage` values for
    /// either text or data frames.
    @objc pub class NSURLSessionWebSocketTask : NSURLSessionTask {
        @objc("sendPingWithPongReceiveHandler:")
        pub sendPingWithPongReceiveHandler(handler: i64)
        @objc("cancelWithCloseCode:reason:")
        pub cancelWithCloseCode(code: i64, reason: NSData)
        @objc("maximumMessageSize")
        pub get maximumMessageSize(): i64
        @objc("setMaximumMessageSize:")
        pub set maximumMessageSize(n: i64)
        @objc("closeCode")
        pub get closeCode(): i64
        @objc("closeReason")
        pub get closeReason(): NSData
    }

    /// `NSURLSessionStreamTask` — raw bidirectional byte stream
    /// over the URL loading system. Open via
    /// `NSURLSession.streamTaskWithHostName:port:`; `readDataOfMinLength:`
    /// / `writeData:` push bytes through.
    @objc pub class NSURLSessionStreamTask : NSURLSessionTask {
        @objc("startSecureConnection")
        pub startSecureConnection()
        @objc("stopSecureConnection")
        pub stopSecureConnection()
        @objc("closeRead")
        pub closeRead()
        @objc("closeWrite")
        pub closeWrite()
    }

    /// `NSURLCache` — request / response cache. `shared()` is
    /// the singleton every `NSURLSession.shared()` instance uses;
    /// build your own with capacity limits for an isolated cache.
    @objc pub class NSURLCache : NSObject {
        @objc("sharedURLCache")
        pub static shared(): NSURLCache
        @objc("setSharedURLCache:")
        pub static setShared(c: NSURLCache)

        @objc("alloc")
        pub static alloc(): NSURLCache
        @objc("initWithMemoryCapacity:diskCapacity:directoryURL:")
        pub initWithMemoryAndDiskCapacity(
            memoryCapacity: u64, diskCapacity: u64, directoryURL: NSURL
        ): NSURLCache

        @objc("cachedResponseForRequest:")
        pub cachedResponseForRequest(req: NSURLRequest): NSObject
        @objc("storeCachedResponse:forRequest:")
        pub storeCachedResponse(response: NSObject, forRequest: NSURLRequest)
        @objc("removeCachedResponseForRequest:")
        pub removeCachedResponseForRequest(req: NSURLRequest)
        @objc("removeAllCachedResponses")
        pub removeAllCachedResponses()
        @objc("removeCachedResponsesSinceDate:")
        pub removeCachedResponsesSinceDate(d: NSObject)

        @objc("memoryCapacity")
        pub get memoryCapacity(): u64
        @objc("setMemoryCapacity:")
        pub set memoryCapacity(c: u64)
        @objc("diskCapacity")
        pub get diskCapacity(): u64
        @objc("setDiskCapacity:")
        pub set diskCapacity(c: u64)
        @objc("currentMemoryUsage")
        pub get currentMemoryUsage(): u64
        @objc("currentDiskUsage")
        pub get currentDiskUsage(): u64
    }

    /// `NSNetService` — Bonjour / mDNS service publication +
    /// resolution. Construct with a service `type` like
    /// `"_http._tcp"`, a `name` for the human label, and a
    /// `port`; call `publish()` to advertise on the local
    /// network. To consume a service discovered through
    /// `NSNetServiceBrowser`, set the delegate and call
    /// `resolveWithTimeout:`.
    @objc pub class NSNetService : NSObject {
        @objc("alloc")
        pub static alloc(): NSNetService
        @objc("initWithDomain:type:name:port:")
        pub initWithDomain(
            domain: NSString, type: NSString, name: NSString, port: i32
        ): NSNetService
        @objc("initWithDomain:type:name:")
        pub initForBrowsing(
            domain: NSString, type: NSString, name: NSString
        ): NSNetService

        @objc("name")
        pub get name(): NSString
        @objc("type")
        pub get type(): NSString
        @objc("domain")
        pub get domain(): NSString
        @objc("hostName")
        pub get hostName(): NSString
        @objc("port")
        pub get port(): i64
        @objc("addresses")
        pub get addresses(): NSArray
        @objc("TXTRecordData")
        pub get txtRecordData(): NSData
        @objc("setTXTRecordData:")
        pub setTXTRecordData(data: NSData): bool

        @objc("delegate")
        pub get delegate(): NSObject
        @objc("setDelegate:")
        pub set delegate(d: NSObject)

        @objc("publish")
        pub publish()
        @objc("publishWithOptions:")
        pub publishWithOptions(options: u64)
        @objc("resolveWithTimeout:")
        pub resolveWithTimeout(timeout: f64)
        @objc("stop")
        pub stop()
    }

    /// `NSNetServiceBrowser` — Bonjour / mDNS service discovery.
    /// Set the delegate, call
    /// `searchForServicesOfType:inDomain:`, then receive
    /// `NSNetService` instances through delegate callbacks.
    /// `stop` cancels the search.
    @objc pub class NSNetServiceBrowser : NSObject {
        @objc("alloc")
        pub static alloc(): NSNetServiceBrowser
        @objc("init")
        pub init(): NSNetServiceBrowser

        @objc("delegate")
        pub get delegate(): NSObject
        @objc("setDelegate:")
        pub set delegate(d: NSObject)

        @objc("searchForServicesOfType:inDomain:")
        pub searchForServicesOfType(type: NSString, inDomain: NSString)
        @objc("searchForBrowsableDomains")
        pub searchForBrowsableDomains()
        @objc("searchForRegistrationDomains")
        pub searchForRegistrationDomains()
        @objc("stop")
        pub stop()
    }

    /// `NSURLProtocol` — abstract base for custom URL-scheme
    /// handlers. Subclass it (override `+canInitWithRequest:`,
    /// `+canonicalRequestForRequest:`, `-startLoading`,
    /// `-stopLoading`) then register your subclass with
    /// `+registerClass:` to intercept matching URLs across the
    /// process. Hand the request / response surface back through
    /// the protocol's `client`.
    @objc pub class NSURLProtocol : NSObject {
        @objc("registerClass:")
        pub static registerClass(cls: NSObject): bool
        @objc("unregisterClass:")
        pub static unregisterClass(cls: NSObject)
        @objc("canInitWithRequest:")
        pub static canInitWithRequest(req: NSURLRequest): bool
        @objc("canonicalRequestForRequest:")
        pub static canonicalRequestForRequest(req: NSURLRequest): NSURLRequest
        @objc("requestIsCacheEquivalent:toRequest:")
        pub static requestIsCacheEquivalent(a: NSURLRequest, toRequest: NSURLRequest): bool

        @objc("request")
        pub get request(): NSURLRequest
        @objc("cachedResponse")
        pub get cachedResponse(): NSObject
        @objc("client")
        pub get client(): NSObject

        @objc("startLoading")
        pub startLoading()
        @objc("stopLoading")
        pub stopLoading()
    }
}
