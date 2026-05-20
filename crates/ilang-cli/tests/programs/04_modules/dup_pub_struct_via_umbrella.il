// Cross-module duplicate: the umbrella `lib_dup_pub_struct.il`
// re-exports two siblings (`_a.il` / `_b.il`) that both declare
// `pub struct Shared`. After the loader merges them they both
// land at `lib_dup_pub_struct.Shared`, and the `dup_pub` pass must
// surface the conflict as a load-time error. Mirrors the original
// `directx12.ID3DBlob` collision (opaque struct in `d3dcompiler`
// + `@com pub interface` in `d3d12_com`).
// expect-error: duplicate `pub` struct `lib_dup_pub_struct.Shared`

use lib_dup_pub_struct

0
