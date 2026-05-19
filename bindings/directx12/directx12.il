// Umbrella module for DirectX 12 bindings.
//
// Re-exports every DLL-per-file binding so callers can write
// `use directx12 { D3D12CreateDevice, ... }` instead of naming
// each module.
//
// DirectX 12 requires Windows 10 1507+ and a compatible GPU /
// driver. The flat entry points here bootstrap COM interfaces
// (ID3D12Device, ID3D12CommandQueue, etc.); reaching the full
// rendering pipeline requires vtable calls on those interfaces.
//
// Layered on top of dxgi (in bindings/windows): DXGI provides
// the adapter, output, and swap-chain factory used by D3D12.

pub use d3d12.*
pub use d3d12_iids.*
pub use d3d12_vtables.*
pub use d3d12_vtable_structs.*
pub use d3dcompiler.*
