// NSMenu / NSMenuItem — construct, populate, inspect.

use test
use appkit { NSMenu, NSMenuItem }
use foundation { nsstr, ilstr }

let menu = NSMenu.alloc().initWithTitle(nsstr("File"))
test.expect(menu.itemArray.count, 0)

// addItemWithTitle returns the freshly-created NSMenuItem.
let openItem = menu.addItemWithTitle(
    nsstr("Open"), 0, nsstr("o")
)
test.expectFalse(openItem.handle == 0)
test.expect(menu.itemArray.count, 1)

// addItem on a pre-built NSMenuItem.
let quit = NSMenuItem.alloc().init(
    nsstr("Quit"), 0, nsstr("q")
)
quit.setEnabled(true)
menu.addItem(quit)
test.expect(menu.itemArray.count, 2)

// separator.
menu.addItem(NSMenuItem.separatorItem())
test.expect(menu.itemArray.count, 3)

// Submenu wiring.
let sub = NSMenu.alloc().initWithTitle(nsstr("Recent"))
sub.addItemWithTitle(nsstr("foo.txt"), 0, nsstr(""))
let recentParent = NSMenuItem.alloc().init(nsstr("Recent"), 0, nsstr(""))
recentParent.setSubmenu(sub)
menu.addItem(recentParent)
test.expect(menu.itemArray.count, 4)

0
