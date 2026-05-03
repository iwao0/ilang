// Write-only property (setter, no getter). Reading errors at typecheck.
// expect-error: no getter
class Logger {
    last: string
    init() { this.last = "" }
    set msg(s: string) { this.last = s }
}

let l = new Logger()
l.msg = "hi"
let _ = l.msg
