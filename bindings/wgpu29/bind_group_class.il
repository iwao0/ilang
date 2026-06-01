/// Friendly wrappers for the bind-group family — `Sampler`,
/// `BindGroup`, `BindGroupLayout`, `PipelineLayout` plus their
/// `*Options` / `*EntryOption` configuration classes — pulled
/// out of `bind_group.il`.

use core { WGPUStringView, TextureFormat }
use buffer { WGPUBuffer }
use buffer_class { Buffer }
use texture { WGPUTextureView, TextureViewDimension }
use texture_class { TextureView }
use bind_group {
    WGPUBindGroup, WGPUBindGroupLayout, WGPUPipelineLayout, WGPUSampler,
    AddressMode, FilterMode, MipmapFilterMode, CompareFunction,
    BufferBindingType, SamplerBindingType, TextureSampleType,
    StorageTextureAccess, ShaderStage,
    BindingResourceKind, BindingLayoutKind,
    wgpuSamplerRelease, wgpuSamplerSetLabel,
    wgpuBindGroupRelease, wgpuBindGroupSetLabel,
    wgpuBindGroupLayoutRelease, wgpuBindGroupLayoutSetLabel,
    wgpuPipelineLayoutRelease, wgpuPipelineLayoutSetLabel,
}
use std.ffi { cstrFromString }

@extern(C, "wgpu_native") {
/// Wraps `WGPUSampler`. Built via `Device.createDefaultSampler`
/// or `Device.createSampler(SamplerOptions)`.
pub class Sampler {
    pub handle: i64

    pub init(handle: WGPUSampler) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuSamplerRelease(this.handle as WGPUSampler)
            this.handle = 0
        }
    }
    pub setLabel(label: string) {
        wgpuSamplerSetLabel(this.handle as WGPUSampler, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
}

/// Wraps `WGPUBindGroup`.
pub class BindGroup {
    pub handle: i64

    pub init(handle: WGPUBindGroup) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuBindGroupRelease(this.handle as WGPUBindGroup)
            this.handle = 0
        }
    }
    pub setLabel(label: string) {
        wgpuBindGroupSetLabel(this.handle as WGPUBindGroup, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
}

/// Wraps `WGPUBindGroupLayout`.
pub class BindGroupLayout {
    pub handle: i64

    pub init(handle: WGPUBindGroupLayout) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuBindGroupLayoutRelease(this.handle as WGPUBindGroupLayout)
            this.handle = 0
        }
    }
    pub setLabel(label: string) {
        wgpuBindGroupLayoutSetLabel(this.handle as WGPUBindGroupLayout, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
}

/// Wraps `WGPUPipelineLayout`.
pub class PipelineLayout {
    pub handle: i64

    pub init(handle: WGPUPipelineLayout) {
        this.handle = handle as i64
    }
    pub release() {
        if this.handle != 0 {
            wgpuPipelineLayoutRelease(this.handle as WGPUPipelineLayout)
            this.handle = 0
        }
    }
    pub setLabel(label: string) {
        wgpuPipelineLayoutSetLabel(this.handle as WGPUPipelineLayout, WGPUStringView {
            data:   cstrFromString(label) as *void,
            length: 0xFFFFFFFFFFFFFFFF as u64,
        })
    }
}

/// Configuration for `Device.createSampler`. Defaults match
/// `wgpuDeviceCreateSampler(d, null)` — ClampToEdge / Nearest
/// / no compare / no anisotropy. Reassign fields before
/// passing in.
pub class SamplerOptions {
    pub label:         string
    pub addressModeU:  AddressMode
    pub addressModeV:  AddressMode
    pub addressModeW:  AddressMode
    pub magFilter:     FilterMode
    pub minFilter:     FilterMode
    pub mipmapFilter:  MipmapFilterMode
    pub lodMinClamp:   f32
    pub lodMaxClamp:   f32
    pub compare:       CompareFunction
    pub maxAnisotropy: i64

    pub init() {
        this.label         = ""
        this.addressModeU  = AddressMode.clampToEdge
        this.addressModeV  = AddressMode.clampToEdge
        this.addressModeW  = AddressMode.clampToEdge
        this.magFilter     = FilterMode.nearest
        this.minFilter     = FilterMode.nearest
        this.mipmapFilter  = MipmapFilterMode.nearest
        this.lodMinClamp   = 0.0
        this.lodMaxClamp   = 32.0
        this.compare       = CompareFunction.undefined
        this.maxAnisotropy = 1
    }
}

/// One entry of a `BindGroupOptions`. Mirrors
/// `WGPUBindGroupEntry` — three handle slots
/// (`buffer` / `sampler` / `textureView`) that are mutually
/// exclusive. Pick which slot is live via `kind`; assign the
/// matching field before passing to `Device.createBindGroup`.
///
/// For buffer bindings, leave `bufferSize` at `0` to mean
/// "to end of buffer" (translates to `WGPU_WHOLE_SIZE`).
pub class BindGroupEntryOption {
    pub binding:      i64
    pub kind:         BindingResourceKind
    pub buffer:       Buffer
    pub bufferOffset: u64
    pub bufferSize:   u64
    pub sampler:      Sampler
    pub textureView:  TextureView

    pub init(binding: i64, kind: BindingResourceKind) {
        this.binding      = binding
        this.kind         = kind
        this.buffer       = new Buffer(0 as WGPUBuffer)
        this.bufferOffset = 0 as u64
        this.bufferSize   = 0 as u64
        this.sampler      = new Sampler(0 as WGPUSampler)
        this.textureView  = new TextureView(0 as WGPUTextureView)
    }
}

/// Configuration for `Device.createBindGroup`.
pub class BindGroupOptions {
    pub label:   string
    pub layout:  BindGroupLayout
    pub entries: BindGroupEntryOption[]

    pub init(layout: BindGroupLayout, entries: BindGroupEntryOption[]) {
        this.label   = ""
        this.layout  = layout
        this.entries = entries
    }
}

/// One entry of a `BindGroupLayoutOptions`. Mirrors
/// `WGPUBindGroupLayoutEntry` — exactly one of the four
/// nested binding-layout sub-structs is "live" per entry,
/// selected by `kind`. Variant-specific fields are flattened
/// here; unused ones stay at their zero default.
pub class BindGroupLayoutEntryOption {
    pub binding:                  i64
    pub visibility:               ShaderStage
    pub kind:                     BindingLayoutKind

    // kind = buffer
    pub bufferType:               BufferBindingType
    pub bufferHasDynamicOffset:   bool
    pub bufferMinBindingSize:     u64

    // kind = sampler
    pub samplerType:              SamplerBindingType

    // kind = texture
    pub textureSampleType:        TextureSampleType
    pub textureViewDimension:     TextureViewDimension
    pub textureMultisampled:      bool

    // kind = storageTexture
    pub storageAccess:            StorageTextureAccess
    pub storageFormat:            TextureFormat
    pub storageViewDimension:     TextureViewDimension

    pub init(binding: i64, visibility: ShaderStage, kind: BindingLayoutKind) {
        this.binding                = binding
        this.visibility             = visibility
        this.kind                   = kind

        this.bufferType             = BufferBindingType.bindingNotUsed
        this.bufferHasDynamicOffset = false
        this.bufferMinBindingSize   = 0 as u64

        this.samplerType            = SamplerBindingType.bindingNotUsed

        this.textureSampleType      = TextureSampleType.bindingNotUsed
        this.textureViewDimension   = TextureViewDimension.undefined
        this.textureMultisampled    = false

        this.storageAccess          = StorageTextureAccess.bindingNotUsed
        this.storageFormat          = TextureFormat.undefined
        this.storageViewDimension   = TextureViewDimension.undefined
    }
}

/// Configuration for `Device.createBindGroupLayout`.
pub class BindGroupLayoutOptions {
    pub label:   string
    pub entries: BindGroupLayoutEntryOption[]

    pub init(entries: BindGroupLayoutEntryOption[]) {
        this.label   = ""
        this.entries = entries
    }
}

/// Configuration for `Device.createPipelineLayout`.
pub class PipelineLayoutOptions {
    pub label:   string
    pub layouts: BindGroupLayout[]

    pub init(layouts: BindGroupLayout[]) {
        this.label   = ""
        this.layouts = layouts
    }
}
}
