/// Built-in `events` module — minimal Node.js-style EventEmitter.
///
/// Generic over the **single payload type** every listener
/// receives:
///
///     let bus = new events.EventEmitter<MouseEvent>()
///     bus.on("click", fn(e: MouseEvent) { ... })
///     bus.emit("click", new MouseEvent(...))
///
/// Pure ilang — no FFI, safe to call from anywhere.
///
/// Difference from Node.js's `EventEmitter`:
///   - One payload type per emitter (set by the generic `T`).
///     If you need heterogeneous events, give the emitter a
///     wrapping struct / class for `T`.

/// Generic single-payload event bus. Construct with the payload
/// type fixed, register listeners with `on`, fire them with
/// `emit`. Listeners run synchronously in registration order.
pub class EventEmitter<T> {
    /// Event name → listener array. Empty arrays get collapsed
    /// back to an absent key (via `delete`) so `listenerCount`
    /// and `emit` don't have to distinguish "absent" vs.
    /// "present but empty".
    listeners: Map<string, fn(T)[]>

    pub init() {
        this.listeners = new Map<string, fn(T)[]>()
    }

    /// Register `listener` for `event`.
    pub on(event: string, listener: fn(T)) {
        if let some(arr) = this.listeners.get(event) {
            arr.push(listener)
        } else {
            let fresh: fn(T)[] = []
            fresh.push(listener)
            this.listeners[event] = fresh
        }
    }

    /// Remove the listener registered for `event` whose `fn`
    /// value compares equal to `listener` (reference equality
    /// on the closure pointer — pass back the exact value you
    /// gave to `on`). Returns `true` when a matching listener
    /// was found, `false` otherwise. Removes the first match
    /// only; call again to drop further duplicates.
    pub off(event: string, listener: fn(T)): bool {
        if let some(arr) = this.listeners.get(event) {
            if !arr.remove(listener) { return false }
            if arr.length == 0 {
                let _ = this.listeners.delete(event)
            }
            true
        } else {
            false
        }
    }

    /// Synchronously invoke every listener registered for
    /// `event`, passing `value` as the single argument.
    /// Listeners fire in registration order.
    ///
    /// Iterates a SNAPSHOT taken before firing, so a listener that
    /// `on`s / `off`s during the emit doesn't corrupt the walk —
    /// removing a listener mid-emit would otherwise shrink the live
    /// array under the cached length and index out of bounds.
    /// (Matches Node.js: the set fired is the one present at emit.)
    pub emit(event: string, value: T) {
        if let some(arr) = this.listeners.get(event) {
            let snapshot = arr.slice(0, arr.length)
            let n = snapshot.length
            let i = 0
            while i < n {
                let f = snapshot[i]
                f(value)
                i = i + 1
            }
        }
    }

    /// Drop every listener registered for `event`. No-op when
    /// nothing was registered.
    pub removeAllListeners(event: string) {
        let _ = this.listeners.delete(event)
    }

    /// Number of listeners currently registered for `event`.
    pub listenerCount(event: string): i32 {
        if let some(arr) = this.listeners.get(event) {
            arr.length as i32
        } else {
            0 as i32
        }
    }
}

/// Single-event emitter — like `EventEmitter` but bound to one
/// event with one payload type. Designed for cases where the
/// containing object already partitions listeners by name (e.g. a
/// GUI `Window` that holds `onClose`, `onResize` as separate
/// `Signal<T>` fields, so callers write `window.onClose.add(fn)`
/// rather than `window.events.on("close", fn)`).
///
///     let sig = new Signal<MouseEvent>()
///     sig.add(fn(e: MouseEvent) { ... })
///     sig.emit(new MouseEvent(...))
pub class Signal<T> {
    /// Registration order = firing order.
    listeners: fn(T)[]

    pub init() {
        this.listeners = []
    }

    /// Register `listener`. Same listener may be added more than
    /// once; each registration fires once per `emit`.
    pub add(listener: fn(T)) {
        this.listeners.push(listener)
    }

    /// Remove the first registration whose `fn` value compares
    /// equal to `listener` (closure-pointer equality — pass back
    /// the exact value handed to `add`). Returns `true` when a
    /// matching listener was found, `false` otherwise. Call again
    /// to drop further duplicates.
    pub remove(listener: fn(T)): bool {
        this.listeners.remove(listener)
    }

    /// Synchronously fire every registered listener in
    /// registration order, passing `value` as the single argument.
    ///
    /// Iterates a SNAPSHOT taken before firing, so a listener that
    /// `add`s / `remove`s during the emit doesn't corrupt the walk
    /// (a mid-emit `remove` would otherwise shrink the live array
    /// under the cached length and index out of bounds).
    pub emit(value: T) {
        let snapshot = this.listeners.slice(0, this.listeners.length)
        let n = snapshot.length
        let i = 0
        while i < n {
            let f = snapshot[i]
            f(value)
            i = i + 1
        }
    }

    /// Drop every registered listener.
    pub removeAll() {
        this.listeners = []
    }

    /// Number of listeners currently registered. Property-style
    /// access: `sig.count` (no parens).
    pub get count(): i32 {
        this.listeners.length as i32
    }
}
