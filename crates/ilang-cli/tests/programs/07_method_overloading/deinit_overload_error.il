// deinit cannot be overloaded — there is exactly one deinit per class.
// expect-error: deinit
class Bad {
    init() {}
    deinit() {}
    deinit() {}
}
let _ = new Bad()
