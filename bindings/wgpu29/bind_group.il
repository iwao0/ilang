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
use buffer { WGPUBuffer }
use texture { WGPUTextureView, TextureViewDimension }

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
}

// `Sampler`, `BindGroup`, `BindGroupLayout`, `PipelineLayout` and
// their `*Options` / `*EntryOption` wrapper classes live in
// `bind_group_class.il`.

