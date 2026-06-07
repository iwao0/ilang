// 3-step `pub use` chain — the entry reaches a fn declared 3 hops
// deep through the canonical names of each umbrella. The parser's
// `renormalize_merged` registers every dotted prefix
// (`lib_pubchain_top`, `lib_pubchain_top.lib_pubchain_mid`,
// `lib_pubchain_top.lib_pubchain_mid.lib_pubchain_core`) as a
// module so the chain resolves. Locks in fix 61978206.
use std.test as test
use lib_pubchain_top

test.expectStr(lib_pubchain_top.lib_pubchain_mid.lib_pubchain_core.echo3(), "leaf3")
