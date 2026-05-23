/// GTK 4 bindings — umbrella module.
///
/// Scope: the subset libs/gui needs to back its cross-platform
/// widget set on Linux (window + 11 widgets + menu). Not a full
/// GTK surface; add files / fns here as broader use cases come up.
///
/// Library names follow GTK 4's shared-object naming. The loader's
/// `try_open_lib` walks `libgtk-4.so.<n>` (and `libgobject-2.0.so.<n>`
/// etc.) so `@lib("gtk-4")` resolves to whatever sonum the host
/// distro ships.
///
/// Most GTK / GLib functions are reachable through `libgtk-4`
/// itself (it pulls in libglib / libgobject / libgio at load time
/// and RTLD_GLOBAL makes their symbols visible via the runtime's
/// `dlsym(RTLD_DEFAULT, …)` lookup), so a single `@lib("gtk-4")`
/// annotation on every binding block is enough.

pub use core.*
pub use widgets.*
pub use menu.*
pub use input.*
