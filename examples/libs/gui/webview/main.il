// 🌐 libs/gui — WebView mini-browser.
//
// URL bar, back / forward / reload buttons, and a `WebView`
// filling the rest of the window. Hit Enter in the URL field to
// navigate; the load lifecycle prints to the console.
//
// macOS (WKWebView) and Linux (WebKitGTK 6.0) are live; the win32
// backend still ships as a no-op stub.
//
// Run with:
//   ilang run examples/libs/gui/webview/main.il
//
// Linux note: WebKitGTK runs its web content in a bubblewrap
// sandbox. Inside a container / VM where user namespaces are
// restricted it can't start (bwrap "Permission denied"); export
// WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 there.

use gui

let w = new gui.Window()
w.title  = "ilang × gui — WebView"
w.width  = 800.0
w.height = 600.0

let back = new gui.Button()
back.text   = "◀"
back.x      = 8.0
back.y      = 8.0
back.width  = 36.0
back.height = 28.0
w.addChild(back)

let fwd = new gui.Button()
fwd.text   = "▶"
fwd.x      = 48.0
fwd.y      = 8.0
fwd.width  = 36.0
fwd.height = 28.0
w.addChild(fwd)

let reload = new gui.Button()
reload.text   = "⟳"
reload.x      = 88.0
reload.y      = 8.0
reload.width  = 36.0
reload.height = 28.0
w.addChild(reload)

let addr = new gui.TextField()
addr.text   = "https://example.com"
addr.x      = 132.0
addr.y      = 10.0
addr.width  = 660.0
addr.height = 24.0
w.addChild(addr)

let view = new gui.WebView()
view.x      = 8.0
view.y      = 44.0
view.width  = 784.0
view.height = 548.0
w.addChild(view)

view.onNavigate.add(fn(e: gui.NavigateEvent) {
    console.log("navigate:", e.url)
    addr.text = e.url
})
view.onLoadFinish.add(fn(e: gui.NavigateEvent) {
    if e.isError {
        console.log("load failed:", e.url, "—", e.errorMessage)
    } else {
        console.log("loaded:", e.url)
    }
})

back.onClick.add(fn(e: gui.MouseEvent) { view.goBack() })
fwd.onClick.add(fn(e: gui.MouseEvent)  { view.goForward() })
reload.onClick.add(fn(e: gui.MouseEvent) { view.reload() })

// Submit the URL field on Enter.
addr.onKeyDown.add(fn(e: gui.KeyEvent) {
    if e.code == "Enter" {
        view.url = addr.text
    }
})

view.url = addr.text
w.show()
gui.run()
