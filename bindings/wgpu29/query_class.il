/// Friendly `QuerySet` + `QuerySetOptions` wrappers pulled out
/// of `query.il`.

use core { WGPUStringView }
use query {
    WGPUQuerySet, QueryType,
    wgpuQuerySetRelease, wgpuQuerySetDestroy, wgpuQuerySetSetLabel,
    wgpuQuerySetGetType, wgpuQuerySetGetCount,
}
use std.ffi { cstrFromString }

@extern(C, "wgpu_native") {
/// Wraps `WGPUQuerySet`. Built via `Device.createQuerySet`.
/// `release()` drops the host-side refcount; `destroy()`
/// drops the GPU-side storage eagerly.
pub class QuerySet {
    pub handle: i64

    pub init(handle: WGPUQuerySet) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuQuerySetRelease(this.handle as WGPUQuerySet)
            this.handle = 0
        }
    }
    pub destroy() {
        if this.handle != 0 {
            wgpuQuerySetDestroy(this.handle as WGPUQuerySet)
        }
    }
    pub setLabel(label: string) {
        wgpuQuerySetSetLabel(this.handle as WGPUQuerySet, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
    pub getType(): QueryType {
        wgpuQuerySetGetType(this.handle as WGPUQuerySet) as QueryType
    }
    pub getCount(): i64 {
        wgpuQuerySetGetCount(this.handle as WGPUQuerySet) as i64
    }
}

/// Configuration for `Device.createQuerySet`.
pub class QuerySetOptions {
    pub label: string
    pub type:  QueryType
    pub count: i64

    pub init(type: QueryType, count: i64) {
        this.label = ""
        this.type  = type
        this.count = count
    }
}
}
