/// Hierarchical list — Cocoa: `NSOutlineView` inside an
/// `NSScrollView`; Linux: `GtkTreeView` over a `GtkTreeStore`;
/// Win32: `SysTreeView32`. Nodes carry an optional checkbox the
/// caller opts into per-node (`node.checkable = true`).
///
/// Build the tree by constructing `TreeNode`s and pushing them
/// onto `tree.root`:
///
///     let tree = new TreeView()
///     let pics = new TreeNode("Pictures")
///     pics.addChild(new TreeNode("cat.png"))
///     tree.root.addChild(pics)
///     window.add(tree)
///
/// `tree.root` is a hidden anchor — only its descendants render.
/// Setting `node.checkable = true` before the tree is attached
/// reveals a checkbox on that row; flipping it fires `onCheck`.

use std.events { Signal }
use events {
    EventTarget,
    TreeSelectEvent, TreeExpandEvent, TreeCheckEvent,
    FocusEvent, BlurEvent, MouseEvent, KeyEvent,
}
use window { Window, Control, Container }
use gui_impl {
    createNativeTree, addTreeNode, insertTreeNodeAt,
    removeTreeNode, clearTreeNodes,
    setTreeNodeLabel, setTreeNodeExpanded, setTreeNodeChecked,
    readTreeSelectedNode, setTreeSelectedNode,
    treeExpandAll, treeCollapseAll,
    setTreeFrame, setTreeEnabled, setTreeVisible, setTreeTooltip,
    setOnTreeSelect, setOnTreeExpand, setOnTreeCollapse, setOnTreeCheck,
    setOnTreeFocus, setOnTreeBlur,
    setOnTreeMouseEnter, setOnTreeMouseLeave,
    setOnTreeMouseDown, setOnTreeMouseUp, setOnTreeContextMenu,
    setOnTreeMouseWheel,
    setOnTreeKeyDown, setOnTreeKeyUp,
    focusTree, blurTree,
}

/// A single row in a `TreeView`. Build a hierarchy by constructing
/// nodes and calling `addChild` / `insertChild`. Mutating `label`
/// / `expanded` / `checked` after the tree has been attached to a
/// window pushes the change down to the native control; before
/// attach, the value is just stored.
///
/// `checkable` controls whether a checkbox is rendered next to
/// this row. Only honored at attach time — flip it before
/// `window.add(tree)`. (Win32's `SysTreeView32` reserves the
/// checkbox column when the tree is created; per-row visibility is
/// then achieved by clearing the state-image bits, which all
/// backends collapse to "no checkbox" for rows where
/// `checkable = false`.)
pub class TreeNode : EventTarget {
    pub _label:     string
    pub _children:  TreeNode[]
    pub _expanded:  bool
    pub _checked:   bool
    pub _checkable: bool
    pub _tree:      TreeView?
    pub _parent:    TreeNode?
    pub handle:     i64

    pub init(label: string) {
        super()
        this._label     = label
        this._children  = []
        this._expanded  = false
        this._checked   = false
        this._checkable = false
        this._tree      = none
        this._parent    = none
        this.handle     = -1
    }

    pub get label(): string { this._label }
    pub set label(v: string) {
        this._label = v
        if let some(t) = this._tree {
            if this.handle >= 0 { setTreeNodeLabel(t.handle, this.handle, v) }
        }
    }

    pub get expanded(): bool { this._expanded }
    pub set expanded(v: bool) {
        this._expanded = v
        if let some(t) = this._tree {
            if this.handle >= 0 { setTreeNodeExpanded(t.handle, this.handle, v) }
        }
    }

    pub get checked(): bool { this._checked }
    pub set checked(v: bool) {
        this._checked = v
        if let some(t) = this._tree {
            if this.handle >= 0 { setTreeNodeChecked(t.handle, this.handle, v) }
        }
    }

    /// Whether this node renders with a checkbox. Mutating after
    /// the tree is attached is a no-op — flip it before
    /// `window.add(tree)`.
    pub get checkable(): bool { this._checkable }
    pub set checkable(v: bool) { this._checkable = v }

    pub get children(): TreeNode[] { this._children }
    pub get parent(): TreeNode? { this._parent }

    pub addChild(node: TreeNode) {
        this._attach_child_at(this._children.length as i32, node)
    }

    pub insertChild(at: i32, node: TreeNode) {
        this._attach_child_at(at, node)
    }

    pub removeChild(node: TreeNode) {
        let idx = this._children.indexOf(node)
        if idx < 0 as i64 { return }
        let _ = this._children.removeAt(idx)
        node._parent = none
        if let some(t) = this._tree {
            if node.handle >= 0 {
                removeTreeNode(t.handle, node.handle)
                t._forgetSubtree(node)
            }
        }
        node._tree = none
    }

    pub clear() {
        while this._children.length > 0 as i64 {
            let last = this._children.length - (1 as i64)
            match this._children.removeAt(last) {
                some(child) {
                    child._parent = none
                    if let some(t) = this._tree {
                        if child.handle >= 0 {
                            removeTreeNode(t.handle, child.handle)
                            t._forgetSubtree(child)
                        }
                    }
                    child._tree = none
                }
                none {}
            }
        }
    }

    _attach_child_at(at: i32, node: TreeNode) {
        node._parent = this
        let n = this._children.length
        let i: i64 = at as i64
        if i <= 0 as i64 {
            this._children.unshift(node)
        } elif i >= n {
            this._children.push(node)
        } else {
            let next: TreeNode[] = []
            let k: i64 = 0
            while k < i {
                next.push(this._children[k])
                k = k + 1
            }
            next.push(node)
            while k < n {
                next.push(this._children[k])
                k = k + 1
            }
            this._children = next
        }
        if let some(t) = this._tree {
            t._attachSubtree(node, this.handle, at)
        }
    }
}

pub class TreeView : Control {
    handle: i64
    pub root: TreeNode
    _nodeMap:  Map<i64, TreeNode>
    _x:         f64
    _y:         f64
    _width:     f64
    _height:    f64
    _isEnabled: bool
    _isVisible: bool
    _tooltip:   string
    pub onSelect:   Signal<TreeSelectEvent>
    pub onExpand:   Signal<TreeExpandEvent>
    pub onCollapse: Signal<TreeExpandEvent>
    pub onCheck:    Signal<TreeCheckEvent>
    pub onFocus:    Signal<FocusEvent>
    pub onBlur:     Signal<BlurEvent>
    pub onMouseEnter:  Signal<MouseEvent>
    pub onMouseLeave:  Signal<MouseEvent>
    pub onMouseDown:   Signal<MouseEvent>
    pub onMouseUp:     Signal<MouseEvent>
    pub onContextMenu: Signal<MouseEvent>
    pub onMouseWheel:  Signal<MouseEvent>
    pub onKeyDown:     Signal<KeyEvent>
    pub onKeyUp:       Signal<KeyEvent>

    pub init() {
        super()
        this.handle      = -1
        this.root        = new TreeNode("")
        this._nodeMap    = new Map<i64, TreeNode>()
        this._x          = 0.0
        this._y          = 0.0
        this._width      = 200.0
        this._height     = 200.0
        this._isEnabled  = true
        this._isVisible  = true
        this._tooltip    = ""
        this.onSelect    = new Signal<TreeSelectEvent>()
        this.onExpand    = new Signal<TreeExpandEvent>()
        this.onCollapse  = new Signal<TreeExpandEvent>()
        this.onCheck     = new Signal<TreeCheckEvent>()
        this.onFocus     = new Signal<FocusEvent>()
        this.onBlur      = new Signal<BlurEvent>()
        this.onMouseEnter  = new Signal<MouseEvent>()
        this.onMouseLeave  = new Signal<MouseEvent>()
        this.onMouseDown   = new Signal<MouseEvent>()
        this.onMouseUp     = new Signal<MouseEvent>()
        this.onContextMenu = new Signal<MouseEvent>()
        this.onMouseWheel  = new Signal<MouseEvent>()
        this.onKeyDown     = new Signal<KeyEvent>()
        this.onKeyUp       = new Signal<KeyEvent>()
    }

    pub get selectedNode(): TreeNode? {
        if this.handle < 0 { return none }
        let h = readTreeSelectedNode(this.handle)
        if h < 0 { return none }
        this._nodeMap.get(h)
    }

    pub setSelectedNode(node: TreeNode?) {
        if this.handle < 0 { return }
        match node {
            some(n) {
                if n.handle >= 0 { setTreeSelectedNode(this.handle, n.handle) }
            }
            none { setTreeSelectedNode(this.handle, -1) }
        }
    }

    pub expandAll() {
        if this.handle >= 0 { treeExpandAll(this.handle) }
    }
    pub collapseAll() {
        if this.handle >= 0 { treeCollapseAll(this.handle) }
    }

    pub get x(): f64 { this._x }
    pub set x(v: f64) {
        this._x = v
        if this.handle >= 0 {
            setTreeFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get y(): f64 { this._y }
    pub set y(v: f64) {
        this._y = v
        if this.handle >= 0 {
            setTreeFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get width(): f64 { this._width }
    pub set width(v: f64) {
        this._width = v
        if this.handle >= 0 {
            setTreeFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub get height(): f64 { this._height }
    pub set height(v: f64) {
        this._height = v
        if this.handle >= 0 {
            setTreeFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }

    pub get isEnabled(): bool { this._isEnabled }
    pub set isEnabled(v: bool) {
        this._isEnabled = v
        if this.handle >= 0 { setTreeEnabled(this.handle, v) }
    }
    pub get isVisible(): bool { this._isVisible }
    pub set isVisible(v: bool) {
        this._isVisible = v
        if this.handle >= 0 { setTreeVisible(this.handle, v) }
    }
    pub get tooltip(): string { this._tooltip }
    pub set tooltip(v: string) {
        this._tooltip = v
        if this.handle >= 0 { setTreeTooltip(this.handle, v) }
    }

    pub override attachTo(parent: Container) {
        if this.handle >= 0 { return }
        this.handle = createNativeTree(
            parent.nativeChildHandle(),
            this._x, this._y, this._width, this._height,
            this._isEnabled, this._isVisible
        )
        this.root._tree = this
        let n = this.root._children.length
        let i: i64 = 0
        while i < n {
            this._attachSubtree(this.root._children[i], -1, i as i32)
            i = i + 1
        }

        setOnTreeSelect(this.handle, fn(h: i64) {
            let target: EventTarget? = if h < 0 {
                none
            } else {
                match this._nodeMap.get(h) {
                    some(n) { some(n as EventTarget) }
                    none    { none }
                }
            }
            this.onSelect.emit(new TreeSelectEvent(this, target))
        })
        setOnTreeExpand(this.handle, fn(h: i64) {
            match this._nodeMap.get(h) {
                some(n) {
                    n._expanded = true
                    this.onExpand.emit(new TreeExpandEvent(this, n as EventTarget))
                }
                none {}
            }
        })
        setOnTreeCollapse(this.handle, fn(h: i64) {
            match this._nodeMap.get(h) {
                some(n) {
                    n._expanded = false
                    this.onCollapse.emit(new TreeExpandEvent(this, n as EventTarget))
                }
                none {}
            }
        })
        setOnTreeCheck(this.handle, fn(h: i64, v: bool) {
            match this._nodeMap.get(h) {
                some(n) {
                    n._checked = v
                    this.onCheck.emit(new TreeCheckEvent(this, n as EventTarget, v))
                }
                none {}
            }
        })
        setOnTreeFocus(this.handle, fn() {
            this.onFocus.emit(new FocusEvent(this))
        })
        setOnTreeBlur(this.handle, fn() {
            this.onBlur.emit(new BlurEvent(this))
        })
        setOnTreeMouseEnter(this.handle, fn(x: f64, y: f64) {
            this.onMouseEnter.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnTreeMouseLeave(this.handle, fn(x: f64, y: f64) {
            this.onMouseLeave.emit(new MouseEvent(this, x, y, 0, false, false, false, false))
        })
        setOnTreeMouseDown(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseDown.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnTreeMouseUp(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onMouseUp.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnTreeContextMenu(this.handle,
            fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onContextMenu.emit(new MouseEvent(this, x, y, b, ck, sk, ak, mk))
            })
        setOnTreeMouseWheel(this.handle,
            fn(dx: f64, dy: f64, x: f64, y: f64,
               ck: bool, sk: bool, ak: bool, mk: bool) {
                let e = new MouseEvent(this, x, y, 0, ck, sk, ak, mk)
                e.wheelDx = dx
                e.wheelDy = dy
                this.onMouseWheel.emit(e)
            })
        setOnTreeKeyDown(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyDown.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        setOnTreeKeyUp(this.handle,
            fn(c: string, ck: bool, sk: bool, ak: bool, mk: bool) {
                this.onKeyUp.emit(new KeyEvent(this, c, ck, sk, ak, mk))
            })
        if this._tooltip != "" { setTreeTooltip(this.handle, this._tooltip) }
    }

    pub focus() { if this.handle >= 0 { focusTree(this.handle) } }
    pub blur()  { if this.handle >= 0 { blurTree(this.handle) } }

    pub override setFrame(x: f64, y: f64, w: f64, h: f64) {
        this._x = x; this._y = y; this._width = w; this._height = h
        if this.handle >= 0 {
            setTreeFrame(this.handle, this._x, this._y, this._width, this._height)
        }
    }
    pub override frameX():      f64 { this._x }
    pub override frameY():      f64 { this._y }
    pub override frameWidth():  f64 { this._width }
    pub override frameHeight(): f64 { this._height }

    /// Walks `node` and every descendant, allocating native rows
    /// under `parentHandle` and stashing the returned ids in
    /// `_nodeMap`. `index` is the insertion slot among the parent's
    /// existing children (`-1` for end-append at the top level
    /// during initial attach — backends fall back to append when
    /// index is out of range).
    pub _attachSubtree(node: TreeNode, parentHandle: i64, index: i32) {
        node._tree = this
        let h = insertTreeNodeAt(
            this.handle, parentHandle, index, node._label, node._checkable
        )
        node.handle = h
        this._nodeMap[h] = node
        if node._checked { setTreeNodeChecked(this.handle, h, true) }
        let n = node._children.length
        let i: i64 = 0
        while i < n {
            this._attachSubtree(node._children[i], h, i as i32)
            i = i + 1
        }
        if node._expanded { setTreeNodeExpanded(this.handle, h, true) }
    }

    /// Drops `node` and every descendant from `_nodeMap` after the
    /// native row has already been removed by the caller.
    pub _forgetSubtree(node: TreeNode) {
        if node.handle >= 0 {
            let _ = this._nodeMap.delete(node.handle)
            node.handle = -1
        }
        let n = node._children.length
        let i: i64 = 0
        while i < n {
            this._forgetSubtree(node._children[i])
            i = i + 1
        }
    }
}
