/// `WGPUBindGroup` + `WGPUBindGroupLayout` +
/// `WGPUPipelineLayout` + `WGPUSampler` and every descriptor
/// they need.
///
/// The four `*BindingLayout` value structs always appear nested
/// inside a `WGPUBindGroupLayoutEntry`; exactly one is non-zero
/// (the others stay zero-initialised).
///
/// Enum discriminants (raw u32):
///   WGPUBufferBindingType:    Undefined=0, Uniform=1,
///                             Storage=2, ReadOnlyStorage=3
///   WGPUSamplerBindingType:   Undefined=0, Filtering=1,
///                             NonFiltering=2, Comparison=3
///   WGPUTextureSampleType:    Undefined=0, Float=1,
///                             UnfilterableFloat=2, Depth=3,
///                             Sint=4, Uint=5
///   WGPUTextureViewDimension: Undefined=0, _1D=1, _2D=2,
///                             _2DArray=3, Cube=4, CubeArray=5,
///                             _3D=6
///   WGPUStorageTextureAccess: Undefined=0, WriteOnly=1,
///                             ReadOnly=2, ReadWrite=3
///   WGPUAddressMode:          Undefined=0, ClampToEdge=1,
///                             Repeat=2, MirrorRepeat=3
///   WGPUFilterMode:           Undefined=0, Nearest=1, Linear=2
///   WGPUMipmapFilterMode:     Undefined=0, Nearest=1, Linear=2
///   WGPUCompareFunction:      Undefined=0, Never=1, Less=2,
///                             Equal=3, LessEqual=4, Greater=5,
///                             NotEqual=6, GreaterEqual=7,
///                             Always=8
///   WGPUShaderStage flags:    None=0, Vertex=1, Fragment=2,
///                             Compute=4

use core { WGPUStringView, WGPUDevice, TextureFormat }
use buffer { WGPUBuffer, Buffer }
use texture { WGPUTextureView, TextureView, TextureViewDimension }
use std.ffi { cstrFromString }

pub enum BindingResourceKind: u32 {
    buffer      = 0x01
    sampler     = 0x02
    textureView = 0x03
}

pub enum BindingLayoutKind: u32 {
    buffer         = 0x01
    sampler        = 0x02
    texture        = 0x03
    storageTexture = 0x04
}

pub enum AddressMode: u32 {
    undefined    = 0x00
    clampToEdge  = 0x01
    repeat       = 0x02
    mirrorRepeat = 0x03
}

pub enum BufferBindingType: u32 {
    bindingNotUsed   = 0x00
    undefined        = 0x01
    uniform          = 0x02
    storage          = 0x03
    readOnlyStorage  = 0x04
}

pub enum CompareFunction: u32 {
    undefined    = 0x00
    never        = 0x01
    less         = 0x02
    equal        = 0x03
    lessEqual    = 0x04
    greater      = 0x05
    notEqual     = 0x06
    greaterEqual = 0x07
    always       = 0x08
}

pub enum FilterMode: u32 {
    undefined = 0x00
    nearest   = 0x01
    linear    = 0x02
}

pub enum MipmapFilterMode: u32 {
    undefined = 0x00
    nearest   = 0x01
    linear    = 0x02
}

pub enum SamplerBindingType: u32 {
    bindingNotUsed = 0x00
    undefined      = 0x01
    filtering      = 0x02
    nonFiltering   = 0x03
    comparison     = 0x04
}

pub enum StorageTextureAccess: u32 {
    bindingNotUsed = 0x00
    undefined      = 0x01
    writeOnly      = 0x02
    readOnly       = 0x03
    readWrite      = 0x04
}

pub enum TextureSampleType: u32 {
    bindingNotUsed    = 0x00
    undefined         = 0x01
    float             = 0x02
    unfilterableFloat = 0x03
    depth             = 0x04
    sint              = 0x05
    uint              = 0x06
}

@flags
pub enum ShaderStage: u64 {
    none     = 0x00
    vertex   = 0x01
    fragment = 0x02
    compute  = 0x04
}

@extern(C, "wgpu_native") {
    @handle pub struct WGPUBindGroup {}
    @handle pub struct WGPUBindGroupLayout {}
    @handle pub struct WGPUPipelineLayout {}
    @handle pub struct WGPUSampler {}

    pub struct WGPUBufferBindingLayout {
        nextInChain:      *void
        type:             BufferBindingType
        hasDynamicOffset: u32      // WGPUBool
        minBindingSize:   u64
    }
    pub struct WGPUSamplerBindingLayout {
        nextInChain: *void
        type:        SamplerBindingType
    }
    pub struct WGPUTextureBindingLayout {
        nextInChain:   *void
        sampleType:    TextureSampleType
        viewDimension: TextureViewDimension
        multisampled:  u32         // WGPUBool
    }
    pub struct WGPUStorageTextureBindingLayout {
        nextInChain:   *void
        access:        StorageTextureAccess
        format:        TextureFormat
        viewDimension: TextureViewDimension
    }
    pub struct WGPUBindGroupLayoutEntry {
        nextInChain:      *void
        binding:          u32
        visibility:       ShaderStage
        bindingArraySize: u32      // 0 = scalar; >0 = array binding
        buffer:           WGPUBufferBindingLayout
        sampler:          WGPUSamplerBindingLayout
        texture:          WGPUTextureBindingLayout
        storageTexture:   WGPUStorageTextureBindingLayout
    }
    pub struct WGPUBindGroupLayoutDescriptor {
        nextInChain: *void
        label:       WGPUStringView
        entryCount:  u64
        entries:     *WGPUBindGroupLayoutEntry
    }

    pub struct WGPUBindGroupEntry {
        nextInChain: *void
        binding:     u32
        // For BUFFER bindings: set `buffer` (+ `offset` / `size`)
        // and leave `sampler` / `textureView` null. For SAMPLER
        // / TEXTURE bindings: clear `buffer` and set the matching
        // handle. `size` = u64::MAX means "to end of buffer".
        buffer:      WGPUBuffer
        offset:      u64
        size:        u64
        sampler:     WGPUSampler
        textureView: WGPUTextureView
    }
    pub struct WGPUBindGroupDescriptor {
        nextInChain: *void
        label:       WGPUStringView
        layout:      WGPUBindGroupLayout
        entryCount:  u64
        entries:     *WGPUBindGroupEntry
    }

    pub struct WGPUPipelineLayoutDescriptor {
        nextInChain:          *void
        label:                WGPUStringView
        bindGroupLayoutCount: u64
        bindGroupLayouts:     *WGPUBindGroupLayout
    }
    pub struct WGPUSamplerDescriptor {
        nextInChain:   *void
        label:         WGPUStringView
        addressModeU:  AddressMode
        addressModeV:  AddressMode
        addressModeW:  AddressMode
        magFilter:     FilterMode
        minFilter:     FilterMode
        mipmapFilter:  MipmapFilterMode
        lodMinClamp:   f32
        lodMaxClamp:   f32
        compare:       CompareFunction
        maxAnisotropy: u16
    }

    @lib pub fn wgpuDeviceCreateBindGroupLayout(
        device: WGPUDevice, descriptor: *WGPUBindGroupLayoutDescriptor
    ): WGPUBindGroupLayout
    @lib pub fn wgpuDeviceCreateBindGroup(
        device: WGPUDevice, descriptor: *WGPUBindGroupDescriptor
    ): WGPUBindGroup
    @lib pub fn wgpuDeviceCreatePipelineLayout(
        device: WGPUDevice, descriptor: *WGPUPipelineLayoutDescriptor
    ): WGPUPipelineLayout
    // `descriptor` may be null — wgpu then uses the default
    // sampler (ClampToEdge / Nearest / no compare).
    @lib pub fn wgpuDeviceCreateSampler(
        device: WGPUDevice, descriptor: *WGPUSamplerDescriptor
    ): WGPUSampler

    @lib pub fn wgpuBindGroupSetLabel(bindGroup: WGPUBindGroup, label: WGPUStringView)
    @lib pub fn wgpuBindGroupAddRef(bindGroup: WGPUBindGroup)
    @lib pub fn wgpuBindGroupRelease(bindGroup: WGPUBindGroup)

    @lib pub fn wgpuBindGroupLayoutSetLabel(layout: WGPUBindGroupLayout, label: WGPUStringView)
    @lib pub fn wgpuBindGroupLayoutAddRef(layout: WGPUBindGroupLayout)
    @lib pub fn wgpuBindGroupLayoutRelease(layout: WGPUBindGroupLayout)

    @lib pub fn wgpuPipelineLayoutSetLabel(layout: WGPUPipelineLayout, label: WGPUStringView)
    @lib pub fn wgpuPipelineLayoutAddRef(layout: WGPUPipelineLayout)
    @lib pub fn wgpuPipelineLayoutRelease(layout: WGPUPipelineLayout)

    @lib pub fn wgpuSamplerSetLabel(sampler: WGPUSampler, label: WGPUStringView)
    @lib pub fn wgpuSamplerAddRef(sampler: WGPUSampler)
    @lib pub fn wgpuSamplerRelease(sampler: WGPUSampler)

    // ─── friendly wrappers ────────────────────────────────────

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
