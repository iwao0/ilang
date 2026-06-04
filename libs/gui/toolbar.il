/// Toolbar — horizontal strip docked along the top edge of a
/// `Window` and filled with `ToolbarItem`s. Install through
/// `window.setToolbar(toolbar)`; the backend resizes and places
/// the strip automatically as the window changes shape.
///
///   let bar = new gui.Toolbar([
///       gui.ToolbarItem.button("New",  fn(e) { console.log("new")  }),
///       gui.ToolbarItem.button("Open", fn(e) { console.log("open") }),
///       gui.ToolbarItem.separator(),
///       gui.ToolbarItem.button("Save", fn(e) { console.log("save") }),
///       gui.ToolbarItem.flexibleSpace(),
///       gui.ToolbarItem.button("Help", fn(e) { console.log("help") }),
///   ])
///   window.setToolbar(bar)
///
/// `flexibleSpace` items expand to absorb any remaining width
/// — drop one between groups to push them apart, or several to
/// split the leftover space evenly.

use std.events { Signal }
use events { EventTarget, MouseEvent }

/// Discriminant for `ToolbarItem`. Matches the int the backend
/// reads from the array its `createNativeToolbar` receives.
pub enum ToolbarItemKind: i32 {
    button        = 0
    separator     = 1
    flexibleSpace = 2
}

pub class ToolbarItem : EventTarget {
    pub kind:    ToolbarItemKind
    pub text:    string
    pub onClick: Signal<MouseEvent>

    pub init(kind: ToolbarItemKind, text: string) {
        super()
        this.kind    = kind
        this.text    = text
        this.onClick = new Signal<MouseEvent>()
    }

    /// Clickable button with `text`. `handler` is appended to the
    /// item's `onClick` signal, so multiple subscribers stay
    /// supported through the usual `item.onClick.add(...)` API.
    pub static button(text: string, handler: fn(MouseEvent)): ToolbarItem {
        let item = new ToolbarItem(ToolbarItemKind.button, text)
        item.onClick.add(handler)
        item
    }
    /// Fixed-width vertical divider between items.
    pub static separator(): ToolbarItem {
        new ToolbarItem(ToolbarItemKind.separator, "")
    }
    /// Stretchy gap that absorbs any unused width. Multiple flexible
    /// spaces in the same toolbar split the leftover space evenly.
    pub static flexibleSpace(): ToolbarItem {
        new ToolbarItem(ToolbarItemKind.flexibleSpace, "")
    }
}

use gui_impl {
    createNativeToolbar, setToolbarItemTitle,
    setOnToolbarItemClick,
}

pub class Toolbar {
    pub handle: i64
    pub _items: ToolbarItem[]

    pub init() {
        this.handle = -1
        let empty: ToolbarItem[] = []
        this._items = empty
    }
    pub init(items: ToolbarItem[]) {
        this.handle = -1
        this._items = items
    }

    pub get items(): ToolbarItem[] { this._items }

    /// Update one button's title in place. Only `kind = button`
    /// items have a title to update; calls against separators /
    /// flexible spaces are no-ops.
    pub setText(index: i32, text: string) {
        let i = index as i64
        if i < 0 as i64 || i >= this._items.length { return }
        let item = this._items[i]
        item.text = text
        if this.handle >= 0 {
            setToolbarItemTitle(this.handle, index, text)
        }
    }

    /// Realise the bar inside `window`. Called by
    /// `Window.setToolbar`; user code shouldn't hit this directly.
    pub attachTo(windowHandle: i64) {
        if this.handle >= 0 { return }
        let n = this._items.length
        let kinds:  i32[]    = []
        let texts:  string[] = []
        let i = 0 as i64
        while i < n {
            kinds.push(this._items[i].kind as i32)
            texts.push(this._items[i].text)
            i = i + 1
        }
        this.handle = createNativeToolbar(windowHandle, kinds, texts)
        if this.handle >= 0 {
            // Wire each button's onClick: the backend fires
            // `setOnToolbarItemClick` for the item's index, and we
            // dispatch to that item's Signal.
            let items = this._items
            setOnToolbarItemClick(this.handle, fn(idx: i32) {
                let j = idx as i64
                if j < 0 as i64 || j >= items.length { return }
                let item = items[j]
                // Toolbar buttons carry no cursor / modifier
                // payload — surface a placeholder MouseEvent so
                // subscribers can rely on the standard shape.
                item.onClick.emit(new MouseEvent(
                    item, 0.0, 0.0, 0, false, false, false, false
                ))
            })
        }
    }
}
