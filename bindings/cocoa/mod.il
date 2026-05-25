/// Cocoa bindings — umbrella for the macOS Objective-C runtime
/// surface. Re-exports the `foundation` sub-package (NSObject /
/// NSString / collections / dates / URLs / NSData / NSError) and
/// the `appkit` sub-package (NSApplication / NSWindow / NSView and
/// every AppKit control / panel / drawing primitive).
///
/// Reach for the umbrella when a consumer needs both layers in one
/// `use cocoa { … }`; import a sub-package directly
/// (`use foundation { NSString }` / `use appkit { NSWindow }`) when
/// only one layer is involved — that keeps the dep DAG tight and
/// avoids pulling in the AppKit framework on Foundation-only hosts
/// (server / CLI tools).
///
/// macOS-only. Linked against `Foundation.framework` and
/// `AppKit.framework` at JIT / AOT time via each sub-package's
/// `@extern(ObjC, "…")` blocks.

pub use foundation.*
pub use appkit.*
