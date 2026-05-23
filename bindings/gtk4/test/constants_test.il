/// gtk — assorted constants the binding exposes. Pure-value checks
/// only; no DLL calls so this fixture runs everywhere.

use test
use gtk { G_APPLICATION_DEFAULT_FLAGS }

test.expect(G_APPLICATION_DEFAULT_FLAGS as i64, 0)
