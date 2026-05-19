// NSURLRequest / NSMutableURLRequest — construct + headers +
// body, no actual network traffic.

use test
use foundation { NSURL, NSURLRequest, NSMutableURLRequest, NSData,
                 NSURLRequestCachePolicy, nsstr, ilstr }

let url = NSURL.fromString(nsstr("https://example.com/api"))

// Default GET request.
let r = NSURLRequest.requestWithURL(url)
test.expectStr(ilstr(r.url.absoluteString), "https://example.com/api")
// Default cache policy is useProtocolCachePolicy = 0.
test.expect(r.cachePolicy as i64, NSURLRequestCachePolicy.useProtocolCachePolicy as i64)

// Long-form factory with explicit cache + timeout.
let r2 = NSURLRequest.requestWithURLAndPolicy(
    url, NSURLRequestCachePolicy.reloadIgnoringLocalCacheData, 7.5
)
test.expect(r2.cachePolicy as i64,
    NSURLRequestCachePolicy.reloadIgnoringLocalCacheData as i64)
test.expectF64(r2.timeoutInterval, 7.5)

// Mutable: set method, header, body.
let mr = NSMutableURLRequest.alloc().init()
mr.setURL(url)
mr.setHTTPMethod(nsstr("POST"))
mr.setValue(nsstr("application/json"), nsstr("Content-Type"))
mr.addValue(nsstr("Bearer x"), nsstr("Authorization"))
mr.setHTTPBody(NSData.fromBytes([123 as u8, 125 as u8]))  // "{}"

test.expectStr(ilstr(mr.httpMethod), "POST")
test.expectStr(ilstr(mr.valueForHTTPHeaderField(nsstr("Content-Type"))),
    "application/json")
test.expectStr(ilstr(mr.valueForHTTPHeaderField(nsstr("Authorization"))),
    "Bearer x")
test.expect(mr.httpBody.length, 2)

// Header dict has both fields.
test.expectTrue(mr.allHTTPHeaderFields.count >= 2)

0
