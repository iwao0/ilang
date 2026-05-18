// `NSUserActivity` — Handoff / Spotlight indexing / Siri
// suggestions integration. Create an activity describing what
// the user is doing, fill in `userInfo` with the state needed
// to resume it elsewhere, and `becomeCurrent` so the system can
// surface it on the user's other devices / inside Spotlight.

use core { NSObject, NSString, NSArray, NSDictionary }
use network { NSURL }

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSUserActivity` — opaque activity descriptor. The
    /// `activityType` reverse-DNS string must also appear in the
    /// app bundle's `NSUserActivityTypes` Info.plist key for the
    /// system to accept it.
    @objc pub class NSUserActivity : NSObject {
        @objc("alloc")
        pub static alloc(): NSUserActivity
        @objc("initWithActivityType:")
        pub initWithActivityType(activityType: NSString): NSUserActivity

        @objc("activityType")
        pub get activityType(): NSString
        @objc("title")
        pub get title(): NSString
        @objc("setTitle:")
        pub set title(s: NSString)
        @objc("userInfo")
        pub get userInfo(): NSDictionary
        @objc("setUserInfo:")
        pub set userInfo(info: NSDictionary)
        @objc("requiredUserInfoKeys")
        pub get requiredUserInfoKeys(): NSArray
        @objc("setRequiredUserInfoKeys:")
        pub set requiredUserInfoKeys(keys: NSArray)
        @objc("keywords")
        pub get keywords(): NSArray
        @objc("setKeywords:")
        pub set keywords(set: NSArray)
        @objc("webpageURL")
        pub get webpageURL(): NSURL
        @objc("setWebpageURL:")
        pub set webpageURL(url: NSURL)
        @objc("expirationDate")
        pub get expirationDate(): NSObject
        @objc("setExpirationDate:")
        pub set expirationDate(d: NSObject)

        @objc("isEligibleForHandoff")
        pub get isEligibleForHandoff(): bool
        @objc("setEligibleForHandoff:")
        pub set isEligibleForHandoff(v: bool)
        @objc("isEligibleForSearch")
        pub get isEligibleForSearch(): bool
        @objc("setEligibleForSearch:")
        pub set isEligibleForSearch(v: bool)
        @objc("isEligibleForPublicIndexing")
        pub get isEligibleForPublicIndexing(): bool
        @objc("setEligibleForPublicIndexing:")
        pub set isEligibleForPublicIndexing(v: bool)
        @objc("isEligibleForPrediction")
        pub get isEligibleForPrediction(): bool
        @objc("setEligibleForPrediction:")
        pub set isEligibleForPrediction(v: bool)

        @objc("becomeCurrent")
        pub becomeCurrent()
        @objc("resignCurrent")
        pub resignCurrent()
        @objc("invalidate")
        pub invalidate()
    }
}
