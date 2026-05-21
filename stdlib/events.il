// Built-in `events` module — minimal Node.js-style EventEmitter.
//
// Generic over the **single payload type** every listener
// receives:
//
//     let bus = new events.EventEmitter<MouseEvent>()
//     bus.on("click", fn(e: MouseEvent) { ... })
//     bus.emit("click", new MouseEvent(...))
//
// Pure ilang — no FFI, safe to call from anywhere.
//
// Difference from Node.js's `EventEmitter`:
//   - One payload type per emitter (set by the generic `T`).
//     If you need heterogeneous events, give the emitter a
//     wrapping struct / class for `T`.

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
            let n = arr.length
            let i = 0
            let found_idx = -1
            while i < n {
                if arr[i] == listener {
                    found_idx = i
                    i = n
                } else {
                    i = i + 1
                }
            }
            if found_idx < 0 { return false }
            // Rebuild without the matching slot — arrays don't
            // have a `splice` primitive.
            let next: fn(T)[] = []
            let j = 0
            while j < n {
                if j != found_idx { next.push(arr[j]) }
                j = j + 1
            }
            if next.length == 0 {
                let _ = this.listeners.delete(event)
            } else {
                this.listeners[event] = next
            }
            true
        } else {
            false
        }
    }

    /// Synchronously invoke every listener registered for
    /// `event`, passing `value` as the single argument.
    /// Listeners fire in registration order.
    pub emit(event: string, value: T) {
        if let some(arr) = this.listeners.get(event) {
            let n = arr.length
            let i = 0
            while i < n {
                let f = arr[i]
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
