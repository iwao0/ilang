# ilang HANDOFF

新しいセッションへの引き継ぎ用。`/clear` 後にこのファイルを読めば現状の文脈が把握できる構成。

言語仕様の詳細は [`docs/syntax.md`](syntax.md) を参照。このファイルは **「実装の現状」と「次に何をやるか」の引き継ぎ** に絞る。

## プロジェクト概要

**ilang** はユーザーが新しく設計中のプログラミング言語。最終ゴール:

- **capability ベースのセキュリティ**: ライブラリ/クラスごとに `net`, `file` などの実行権限を持たせ、サプライチェーン攻撃を緩和する (核となる設計目標)
- **ARC** によるメモリ安全性 (所有権/`mut`/借用は採用しない)
- **JS / TypeScript / Rust 風** のハイブリッド構文。文末は **JS 風 ASI** (改行が `;` 代わり)
- 例外なし。失敗は `Result<T, E>`、回復不能エラーは panic

実装言語: **Rust 1.95**。実行モデル: AST → MIR (SSA) → **Cranelift JIT** が唯一の実行経路。ツリーウォーク インタプリタ (`ilang-eval`) と旧 ilang-codegen 経路は M1 Step 6 で撤去済み。AOT 経由のネイティブ実行は `ilang build` で行う。

## 現在地

`run_all_program_fixtures` (1278/1278) + `cocoa_foundation` + `cocoa_appkit` + workspace 全 525 test 全緑。 `crepr_struct_field_discard.il` (a6e9310e で意図的に赤いまま追加されていた fixture) は緑。 `examples/sdl_breakout/main.il` の起動も実機確認済み (`playing — ESC to quit`)。

直近のセッション (2026-06-10) で main に landing した変更:

- **内部 fn の CRepr struct return を sret 経路に倒す** (`4d1f97dc`)。 `crepr_struct_field_discard.il` の leak (= chunks return で callee の `new Box()` buffer が宙吊り) を塞いだ。 `Terminator::Return` に `release_value: bool` を追加し、 codegen が sret memcpy 後に callee 側 buffer を `__mir_free` する。 `is_c_abi` (= `Extern { .. } | ExternBody`) は従来の platform chunk → HFA → sret cascade を維持して SDL2 / wgpu / objc_msgSend を守る。
- **`Inst::VirtCall` も同じ sret 経路に統一** (本コミット)。 `call_dispatch.rs::VirtCall` が `struct_indirect_with_max` のままだったため、 vtable 経由で 16 byte 以下の CRepr struct (NSRange / NSRect 等) を返す `@objc method` の caller signature (chunks return) と callee signature (sret) が決定的にミスマッチし、 debug build で SIGSEGV を踏んでいた (`cocoa_foundation/calendar_test.il`、 `cocoa_appkit/drawing_test.il`)。 vtable に乗るのは構造的に `FunctionKind::Local` のみなので `struct_sret_for_internal` に統一すれば整合する。
- **CRepr struct の inline enum field を表す `MirTy::CReprEnum` を導入** (`28f7060f` → `65bb326a` → `14292c5e`、 前セッション)。
- **`match` / `if let` のアームバインディング tail-Var Retain** を `Binding::PatternBinding(_, _, needs_retain_on_tail)` で表現し直し (`ef1b9d35` → `838d2dc4`、 前セッション)。
- **closure body 内 cell store の rc** を 2 path に分離 (`50eb400a` + `46feb093`、 前セッション)。
- **`Binding::Ssa` 細分化と rc-slot 集約** (`4afd282e` → `d6b2e64f` → `838d2dc4`、 前セッション)。
- **CRepr fresh return の leak 調査用に `ILANG_HEAP_TRACE` env を追加** (`bcd3367f`、 前セッション)。

次のフェーズ候補は変わらず: **capability の enforce**、 **未実装の言語機能 (タプル / `?` 演算子 / Iterator など)**、 **C ヘッダから .il 自動生成のミニ bindgen**。

## 未解決の引き継ぎ事項

### `nested_generic.il` 系の SIGABRT は `cranelift_module::ModuleDeclarations` の drop が真因方向

**最小再現** (約 15% で SIGABRT を 16 並列 × 25 batch で再現):

```il
use std.test as test
class Box<T> {
    x: T
    init(v: T) { this.x = v }
    get(): T { x }
}
let b1 = new Box<i64>(7)
let b2 = new Box<Box<i64>>(b1)
test.expect(b2.get().get(), 7)
```

**必須条件の絞り込み(本セッション)**:

各要素を 1 つずつ削った縮小版を 400 並列で測ったところ:

| variant | 必須要素 | race率 |
| --- | --- | --- |
| `Box 4 段だけ` | (test なし) | 0/400 |
| `Pair だけ` | (test なし) | 0/400 |
| `Box 2 段 + Box 構築のみ` | (`.get()` なし) | 0/400 |
| `Box 4 段 + Pair + test なし` | string あり、 test なし | 0/400 |
| `test.expect 3 回` | (Box なし) | 0/400 |
| `Box 2 段 + test.expect(b2.get().get(), 7)` | **全部** | **61/400 (15%)** |

→ **必須条件は「`use std.test as test` の取り込み + 2 段以上の `.get()` chain + その結果を test.expect の引数として渡す」**。 string は無関係、 Pair も無関係。

**stack trace (本セッションの crash_handler で取得 + `atos` 解決)**:

最初に検出した stack:

```
hashbrown::raw::RawTable::drop                                  ← SIGABRT 発火点
drop_in_place<cranelift_module::module::ModuleDeclarations>
drop_in_place<ilang_mir_codegen::compile::Compiled>
ilang::main at main.rs:75
```

これは `crates/ilang-cli/src/main.rs::run_file` で `std::mem::forget(compiled)` を追加して回避済み(子プロセスは数µs 後に exit するので drop しなくても OS が JIT メモリを回収する)。

ただし forget 後にも **別の drop path で同じ確率 (~6%) で再発**:

```
drop_in_place<ilang_mir::program::Function> + 384 (= 内部 Vec の drop)
drop_in_place<ilang_mir::program::Program>
ilang::main at main.rs:75
```

`Program::drop` 経路でも heap corruption 検出 → `__mir_alloc` の `__ILANG_HEAP_GUARD=1` ではこの path も 400/400 PASS なので、 こちらも `__mir_alloc` overrun ではない。 ASLR 依存で「特定 layout のときに Function 内の Vec が壊れる」path。 cranelift JIT generated code が ilang コンパイラ side の Vec を踏みうる layout 衝突を疑っているが未確定。

**ilang 側の影響**:

- ilang-mir / mir-codegen / lower すべてに `unsafe` 一切なし(grep で再確認済み)
- `ILANG_HEAP_GUARD=1` で 400/400 PASS → ilang runtime の `__mir_alloc` 経由ではない
- panic hook も呼ばれない → Rust panic 経由ではない

つまり cranelift_module / hashbrown のクライアント API の呼び方が壊している(declare_function / declare_data の重複登録、 ライフタイム違反、 ASLR-aware な内部状態を踏むサイズ依存)もしくは cranelift_module 0.131.1 自身の bug。

**次にやるべきこと**:

- 残る `Program::drop` race の真因究明: heap guard が捕まえない以上、 ilang コンパイラ side の Vec の中身が壊れている = `__mir_alloc` 由来でない別の overrun(JIT generated code が cranelift JIT page 経由で書き出すアドレス計算の bug が一番濃い)。 一段切り分けには次の手:
  - `Program` 自身も `std::mem::forget` してみて race が消えるかで「真因が drop の中か別か」を判別(本セッションでは Plan を守って forget(compiled) のみ commit)
  - 残る方なら `process::exit(0)` で全 drop を skip し、 fixture suite の flake を完全に消す(JIT child は短命なので無害)
- 「2 段 .get() chain → test.expect」 path で mono 化された method の declare_function を全部 dump して、 重複 declare がないかを確認:
  - `Box_i64.get`, `Box_Box_i64.get` のような mangled name が unique か
  - declaration 順序、 linkage、 signature の一貫性
- cranelift / cranelift_module / cranelift_jit のバージョンを 0.131.1 から 0.132 / 0.133 等に上げて、 race が消えるかどうか試行(`crates/ilang-mir-codegen/Cargo.toml` の cranelift 依存)
- もしくは hashbrown のバージョン固定で minimal reproducer を作って cranelift / hashbrown 側に issue report

**確認手順(最小再現)**:

```sh
cat > /tmp/nest_v6.il <<'EOF'
use std.test as test
class Box<T> {
    x: T
    init(v: T) { this.x = v }
    get(): T { x }
}
let b1 = new Box<i64>(7)
let b2 = new Box<Box<i64>>(b1)
test.expect(b2.get().get(), 7)
EOF
ILBIN=./target/release/ilang
mkdir -p /tmp/nest_v6_check
for batch in $(seq 1 25); do
  for j in $(seq 1 16); do
    idx=$(( (batch-1)*16 + j ))
    ( ILANG_TRACE_CRASH=1 $ILBIN run /tmp/nest_v6.il > /tmp/nest_v6_check/${idx}.out 2>&1; echo $? > /tmp/nest_v6_check/${idx}.code ) &
  done
  wait
done
cat /tmp/nest_v6_check/*.code | sort -n | uniq -c
# 期待: ~85/400 PASS、 ~15% で 134 と stack trace 付き
```

### [解決済み記録] `crepr_struct_assign_index_field.il` 系の確率的失敗

**現状の観測**(本セッションで `programs.rs::check()` に signal 出力を追加して観測力強化済み):

- `cargo nextest run -p ilang --test programs run_all_program_fixtures` の 20 連続実行で:
  - `crepr_struct_assign_index_field.il`: signal=11 (SIGSEGV) × 1
  - `nested_generic.il`: signal=6 (SIGABRT) × 2
- bash で `./target/release/ilang run <fixture> & × 16` × 13 batch (= 200 並列起動) でも `crepr_struct_assign_index_field.il` は 200 中 3〜15 件 (1.5%〜5%) で exit 134 (= SIGABRT)。 つまり **nextest harness 限定ではなく、 多数並列起動で素直に踏む**。

**stderr 空の謎は本セッションで部分解明**:

`crates/ilang-runtime/src/enums.rs:122-134` の「CRepr struct 経由で読んだ enum discriminant が不正」path は `eprintln! → process::abort()` だが、 `eprintln!` が pipe buffer に届く前に `abort()` が走り、 stderr が空のままプロセスが死ぬ。 本セッションで該当 4 箇所(enums.rs ×2 + regex.rs ×2)に `stderr().flush()` を入れたが、 並列再現時の stderr 空は **直らない** — つまり「abort 前 flush」では救えない別の経路で死んでいる(本物の SIGSEGV / heap corruption が `eprintln!` 到達前に発火)。

**最小再現と絞り込み**:

- 元 fixture を縮めて再現: `let s0 = new Slot(); let s1 = new Slot(); let arr: Slot[] = [s0, s1]; arr[1].kind = Mode.active; arr[1].seq = 99` (16 並列 × 25 batch = 400 試行で 6/400 SIGABRT/SIGSEGV)。
- **`arr[0]` への書き込みは安全、 `arr[1]` (idx ≥ 1) への書き込みのみ race**。
- MIR dump: `Slot[]` は inline CRepr struct array (stride 8 byte) として lower される。
- `__mir_alloc` を `Vec<u8>` → `Vec<u64>` に変えて alignment を厳密化しても **race は同確率で続行**(alignment 仮説は外れ)。

**lldb で stack trace を取得 (race の真因方向確定)**:

debug build で並列負荷下に lldb attach loop を回し、 2 系統の crash trace を捕まえた:

**系統 A**: `EnumLayout::repr: MirTy` 経由

```
frame #0:  drop_in_place<MirTy>((null)=0x8)
frame #5:  alloc::alloc::dealloc(ptr=stack address)
frame #10: Box<MirTy>::drop
frame #12: drop_in_place<MirTy>
frame #13: drop_in_place<EnumLayout>
frame #14: drop_in_place<[EnumLayout]>
frame #15: Vec<EnumLayout>::drop
frame #17: drop_in_place<Program>
```

**系統 B**: `Function::value_tys: Vec<MirTy>` 経由 (4 回連続観察)

```
frame #0: drop_in_place<MirTy>((null)=0x0000000000000008)
frame #1: drop_in_place<Box<MirTy>>((null)=0x15370f1a0)
frame #2: drop_in_place<MirTy>((null)=0x15370f190)        ← 最深 nest
frame #3: drop_in_place<Box<MirTy>>((null)=0x1540096a8)
frame #4: drop_in_place<MirTy>((null)=0x154009698)        ← 中間 nest
frame #5: drop_in_place<[MirTy]>
frame #6: Vec<MirTy>::drop                                ← value_tys
frame #8: drop_in_place<Function>
frame #9: drop_in_place<[Function]>
frame #10: Vec<Function>::drop
frame #12: drop_in_place<Program>
frame #13: ilang::run_file at main.rs:1082
```

malloc error: `pointer being freed was not allocated`, address `0x16fdf6700` (stack address) または `0x8` (null+offset)。 **共通パターン: Box<MirTy> の内部 pointer 値が 0x8**(= Vec の len field 等として読まれるサイズ)。

**race は ilang コンパイラ自身の memory corruption**:

- fixture 実行コード (= ilang JIT generated) ではなく、 ilang コンパイラの `Program::drop` 時に `EnumLayout::repr` または `Function::value_tys` の中の MirTy のネストした Box が **dangling pointer (stack address or null+offset)** を保持して dealloc で abort。
- 共通パターン: 2 段 `Box<MirTy>` nest の最深部 (`Array<X>` / `Optional<X>` / `Promise<X>` / `RawPtr<X>` / `Set<X>` / `Map<K,V>` / `FnTy::ret` のうち X 自身も `Box<MirTy>` を持つ variant)で内部 pointer 値が **`0x8`** に化けている。
- 最小化 fixture (`crepr_struct_assign_index_field.il`) の post-lower MIR には 2 段 nest が明示的に登場しない → mono / opt pass / codegen のどこかで動的に生成された MirTy が corrupt している強い疑い。
- 触る場所候補 (value_tys に push する箇所、 grep 済み):
  - [crates/ilang-mir/src/builder.rs:68](crates/ilang-mir/src/builder.rs:68) — lower 中の builder
  - [crates/ilang-mir/src/passes/inline.rs:270](crates/ilang-mir/src/passes/inline.rs:270) — inline pass の `caller.value_tys.push(ty)`
  - [crates/ilang-mir/src/lower/decl/extern_c.rs:442,539](crates/ilang-mir/src/lower/decl/extern_c.rs:442) — extern C struct synth
  - [crates/ilang-mir/src/lower/decl/enum_fn.rs:203](crates/ilang-mir/src/lower/decl/enum_fn.rs:203) — enum fn synth
- 他に `dce_fn.rs` の `mem::take(&mut prog.functions)` で再配置する path もある。
- ilang-mir / parser / mir-codegen に `unsafe` 一切なし(grep で確認済み)ので、 直接的な unsafe deref ではない → **論理的 buffer overrun** か **意図しない alias** が source。
- ASLR でメモリレイアウトが変わるので踏む / 踏まないが分かれる典型 UB pattern。

**ASAN は timing 変化で踏まず** (debug + release 両方確認):

`RUSTFLAGS="-Z sanitizer=address" cargo +nightly build -Z build-std --release --target aarch64-apple-darwin -p ilang` で ASAN release ビルドを試したが、 400 並列起動で 400/400 PASS。 ASAN は debug でも release でも race を踏ませない。

**真因 pass 切り分け結果(opt pass は無関係)**:

[main.rs:484-501](crates/ilang-cli/src/main.rs:484) の 6 つの opt pass 各々を env (`ILANG_NO_DCE_FN` / `ILANG_NO_PROMOTE_LOCALS` / `ILANG_NO_INLINE` / `ILANG_NO_CONST_FOLD` / `ILANG_NO_BRANCH_FOLD` / `ILANG_NO_DCE`) で個別 disable して 400 並列を測ったところ、 race 件数 (27〜39 / 400) は baseline (33 / 400) と統計的に有意差なし。 **6 つの opt pass はどれも真因ではない**。 残る容疑者:

- **MIR lower** (`crates/ilang-mir/src/lower/`、 `builder.rs::value_tys.push`)
- **mono pass** (`crates/ilang-mir/src/monomorphize/`、 env では disable できない)
- **mir-codegen** (cranelift IR 生成中の MirTy 走査)
- **ilang-runtime** の何か (heap-trace の OnceLock 等は可能性低い)

ただし `crepr_struct_assign_index_field.il` は generic を使わないので mono pass は事実上 no-op になるはず → **lower か codegen** が最有力。

**[真因確定 + 修正完了]** [crates/ilang-mir-codegen/src/compile/lower_inst/objects.rs:755](crates/ilang-mir-codegen/src/compile/lower_inst/objects.rs:755) の `StoreField` codegen が **field の宣言型 (`field_meta_ty`) ではなく value の型 (`val_ty_mir`) で store size を選んでいた** のが真因。 例えば `seq: i32 = 99` の lower:

- 右辺 `99` の MirTy は `I64` (const のデフォルト)
- 旧コード: `celem_clif_type_with_enum(prog, &val_ty_mir)` で I64 → `Some(I64)` の guard が `!= I64` 不一致で `_` arm に流れ、 **i64 store (8 byte)** を発行
- 結果: `Slot.seq` (offset 4..8) を書くつもりが、 store 命令は `c_off+8 = offset 4..12` を書く → CRepr struct buffer (8 byte) を 4 byte 越境
- `Slot[]` data buffer (= 2 × 8 byte) の場合、 `arr[1].seq = 99` で `(data_ptr + 8) + 4 = data_ptr + 12` に 8 byte 書き込み = **offset 12..20** = **buffer の offset 16..20 (4 byte) を tail に踏み出す**

heap guard で 100% 確認した「offset 16 への single byte zero write」 は、 実は **「offset 12..20 への 8 byte i64 store」の高位 4 byte (うち最下位 1 byte が `0xBE → 0x00` に書き換わって見えた)**だった。 (`99 = 0x00000000_00000063` の little-endian で高位 4 byte は全 0 なので、 guard の最下位 byte 0xBE が `0x00` に書かれて見える。)

**修正**: `field_meta_ty.as_ref().unwrap_or(&val_ty_mir)` で field 宣言型を優先して store 幅を選ぶ。 i32 field なら ireduce で 4 byte に truncate してから store。

**検証結果**:

- `ILANG_HEAP_GUARD=1` で 400 並列起動: **0/400 abort** (修正前 400/400)
- guard なしで 400 並列起動: **0/400 race** (修正前 ~33/400)
- nextest `run_all_program_fixtures` 10 連続: **10/10 PASS**
- workspace 全体: **525/525 PASS**

---

### [研究記録] heap guard 計装で真因 path を pin point

[crates/ilang-runtime/src/alloc.rs](crates/ilang-runtime/src/alloc.rs:34) に `ILANG_HEAP_GUARD=1` で各 alloc の前後に 16 byte ずつ `0xDEADBEEFCAFEBABE` の guard を埋める計装を追加。 free 時に guard を検査して破壊を検出。

並列 400 試行で **400 / 400 が exit 134** (= SIGABRT)、 全 run で **同一の corruption pattern**:

- 場所: **`size=16` の buffer の tail guard (offset 16)**
- 破壊値: `0xDEADBEEFCAFEBA00`
- 元値: `0xDEADBEEFCAFEBABE`
- 差分: **最下位 1 byte だけ `0xBE → 0x00`** = **off-by-one single byte zero write**

`ILANG_HEAP_TRACE=1 ILANG_HEAP_GUARD=1` の組合せで死亡直前の alloc 順序を取得:

```
[alloc] size=8   ptr=0x154009880   ← s0
[alloc] size=32  ptr=0x154009a70   ← Mode.idle cached enum box
[alloc] size=8   ptr=0x15400c1a0   ← s1
[alloc] size=48  ptr=0x15400c1d0   ← array header
[alloc] size=16  ptr=0x15400c220   ← array data
[alloc] size=32  ptr=0x15400c250   ← Mode.active cached enum box
[guard] CORRUPTION at ptr=0x15400c220 size=16: [("tail", 0, ...)]
```

`Mode.active` enum box alloc 後、 array data (size=16) を free しようとした時に guard check が発火。 つまり **array data alloc → Mode.active alloc → arr[1].kind = active 系の JIT generated 操作 → array data free** の流れで、 何かが 1 byte 0 を array data の offset 16 に write している。

`size=16` は `Slot[] = [s0, s1]` の 2 要素 × 8 byte stride の data buffer。 corruption が **offset 16 ジャスト**(= データ末尾の直後) ということは、 cranelift JIT が出した ARM64 命令の中に「`strb wzr, [data_ptr, #16]`」相当の **off-by-one byte store** がある。

最小化 fixture で 1 byte zero store を出しうる codegen path:

- `crepr_struct_copy` の最後の 1-byte ループ([lower_inst/mod.rs:58-62](crates/ilang-mir-codegen/src/compile/lower_inst/mod.rs:58)) — total=8 では到達しないはず
- StoreField の CReprEnum (kind: u16) は `store(I16, ...)` で 2 byte
- NewArray の `crepr_struct_copy(fb, raw, dst_addr, total=8)` を `i=0, 1` で 2 回
- ArrayLoad は読み出しのみ

cranelift IR dump (`fb.func.display()` 相当)を取って、 array data buffer 周辺の store 命令の offset を全部確認するのが次の手。

**全箇所レビュー結果(同ラウンド前半で実施)**:

**A. `Function::value_tys: Vec<MirTy>` を mutate する production code は 5 箇所のみ**:

| 場所 | 操作 | 入力 ty の出所 |
| --- | --- | --- |
| [builder.rs:68](crates/ilang-mir/src/builder.rs:68) | `value_tys.push(ty)` | lower 全箇所から(BodyCx::new_value) |
| [lower/decl/enum_fn.rs:203](crates/ilang-mir/src/lower/decl/enum_fn.rs:203) | `value_tys.push(params[i].clone())` | enum fn synth の param type |
| [lower/decl/extern_c.rs:442](crates/ilang-mir/src/lower/decl/extern_c.rs:442), [539](crates/ilang-mir/src/lower/decl/extern_c.rs:539) | `value_tys.push(pty.clone())` | extern C struct method synth |
| [passes/inline.rs:270](crates/ilang-mir/src/passes/inline.rs:270) | `caller.value_tys.push(ty)` | `cand.value_tys.get(v.0).cloned()` の結果 |
| [passes/dce_fn.rs:223](crates/ilang-mir/src/passes/dce_fn.rs:223) | `for ty in &mut f.value_tys { remap_ty(ty, ...) }` | in-place mutate (`Object(cid)` の cid 書き換えのみ) |

opt pass の elimination matrix から 4, 5 はすでに除外。 残るは **1, 2, 3 (lower 系) で push される ty の出所**。

**B. `Box<MirTy>` を持つ MirTy variant の構築箇所**:

- 静的 (`Box::new(MirTy::primitive)`): 22 箇所、 全部 1 段 nest、 safe。
- **動的** (`Box::new(self.resolve_ty(...))`): [lower_state.rs:210-281](crates/ilang-mir/src/lower/lower_state.rs:210), [body_cx.rs:973-1028](crates/ilang-mir/src/lower/body_cx.rs:973) で再帰呼び出し。 ilang ソースの型注釈の深さに応じて任意段の nest を構築 (例: `Array<Optional<Box<T>>>`)。

**C. 2 段 `Box<MirTy>` nest を生成しうる入口**:

- ソース内の `let x: Array<Optional<T>>` のような明示的多段型注釈
- 暗黙的に Optional を包む path (例: `Array<T?>`、 stdlib `Promise<T>` chain、 Map の key/val)
- 最小化 fixture (`crepr_struct_assign_index_field.il`) では明示的にはない → **`use std.test as test` 経由の stdlib lower で動的に 2 段 nest が作られる可能性**

**D. ilang-mir / mir-codegen / lower すべてに `unsafe` 一切なし** (改めて確認)。 つまり Rust safe code 範囲で起きる UB:

- 依存 crate (cranelift / hashbrown 等) の `unsafe` が source の可能性
- Vec の grow / shrink が valid な範囲を超える(`usize::MAX`)系の panic は別経路で死ぬ
- `mem::take` → `extend` → 各種 mutation で **意図しない alias** ができている path(grep 限界、 動的解析必要)

**E. opt pass の elimination matrix(本ラウンドで取得)**:

| 環境変数 | 異常終了率 |
| --- | --- |
| baseline (no env) | 33 / 400 (8.25%) |
| ILANG_NO_DCE_FN=1 | 36 / 400 |
| ILANG_NO_PROMOTE_LOCALS=1 | 31 / 400 |
| ILANG_NO_INLINE=1 | 35 / 400 + 1 SIGSEGV |
| ILANG_NO_CONST_FOLD=1 | 32 / 400 |
| ILANG_NO_BRANCH_FOLD=1 | 27 / 400 |
| ILANG_NO_DCE=1 | 39 / 400 |

統計的有意差なし。 opt pass はすべて真因から除外。

**次にやるべきこと**:

- **動的解析が必須**: 静的レビューでは pin point 不可。
  - **Miri** (`cargo +nightly miri run -p ilang -- run fixture.il`) — 但し cranelift JIT を含むため動作するかは未確認。
  - **AddressSanitizer の wall-clock 緩和**: ASAN debug / release ともに timing が変わり race 引かない既存実績。 `RUSTFLAGS=-Zsanitizer=address -C opt-level=3 -C target-cpu=native` 等のチューニングで縮められる可能性。
  - **printf debug**: `Vec<MirTy>` の grow タイミングごとに `len/cap/ptr` を stderr に吐く instrumentation を入れて、 並列起動で踏んだ run の Vec 状態を比較。
- ASAN なしの最終手段: ilang コンパイラ内の Vec / Box 全部を `with_capacity` で十分大きく確保 → grow を抑制 → 並列で再現するか比較(真因仮説「grow 中の何か」の検証)。

**真因追跡で試して効かなかった手段** (次セッションで違う方法を考えること):

- macOS の core dump (`ulimit -c unlimited` + `/cores/core.%P`): `sysctl kern.coredump=1` でも `/cores` に書き出されず。 macOS の code signing / `get-task-allow` entitlement 制約。
- lldb で attach (`lldb -- ilang run ...`): デバッガ attach すると timing が変わり、 race を引かなくなる。
- `RUST_BACKTRACE=1`: stderr 空のまま死ぬので backtrace も出ない。

**次にやるべきこと**:

- **ASAN(AddressSanitizer)で計装**: nightly Rust + `-Z sanitizer=address` で `ilang` を再ビルドして並列起動 → double free / use-after-free をピン点。 仮説では `Slot[] = [s0, s1]` の elem 値コピー後の release path で double-free が起きている。
- `ilang` バイナリに `get-task-allow` entitlement を付ける(`codesign --entitlements ent.plist -s - target/release/ilang`)→ core dump を `/cores` に書き出させる → 死亡時の core を lldb で読む。
- `MallocStackLogging=1` + `MallocScribble=1` 等の macOS malloc debug 環境変数で heap corruption を捕まえる(性能落ちて timing 変わるリスクあり)。
- 最小化済みの `let s0 = new Slot(); let s1 = new Slot(); let arr: Slot[] = [s0, s1]; arr[1].kind = X; arr[1].seq = Y` で集中的に再現してから上記。

**確認手順**:

```sh
# 並列起動 (200 個) で確率失敗を引く
FIXTURE=crates/ilang-cli/tests/programs/05_edge_cases/crepr_struct_assign_index_field.il
ILBIN=./target/release/ilang
mkdir -p /tmp/raceCheck; rm -f /tmp/raceCheck/*
for batch in $(seq 1 13); do
  for j in $(seq 1 16); do
    idx=$(( (batch-1)*16 + j ))
    ( $ILBIN run $FIXTURE > /tmp/raceCheck/${idx}.out 2>&1; echo $? > /tmp/raceCheck/${idx}.code ) &
  done
  wait
done
cat /tmp/raceCheck/*.code | sort -n | uniq -c
# 期待: 1〜5% の確率で 134 (SIGABRT) が出る
```

### 関連 commit 履歴 (時系列)

- `f2eea6e3` AssignField で CRepr-parent Enum field を retain/release から除外 (point fix、 papering over)
- `c97cc0b2` ↑の pin fixture (`crepr_struct_enum_field_assign.il`)
- `28f7060f` → `65bb326a` → `14292c5e` `MirTy::CReprEnum` 導入で papering over を解消
- `84f2eb6c` CReprEnum 関連 fixture 6 件 (全 PASS)
- `a6e9310e` 疑わしい path を網羅する fixture 9 件 (1 件 fail = `crepr_struct_field_discard.il`)
- `d4b44d2f` 修正試行 3 件 (実質効果なし、 撤回せずに残置)
- `bcd3367f` `ILANG_HEAP_TRACE` env と真因確定の trace 結果記録
- `4d1f97dc` 内部 fn CRepr struct return の sret 経路化 (`Inst::Call` 系)
- `3bb34848` `Inst::VirtCall` の sret 経路を内部 fn 規約に統一
- `c8a8f525` `Inst::CallIndirect` (closure 経由) の sret + by-value param 経路追加
- `56d5881e` CRepr struct return まわりの回帰防止 fixture 4 件
- **本セッション** (2026-06-10) `test.liveAllocCount` / `test.liveAllocBytes` で測定前に `pool::drain` を呼ぶことで async 系 leak 検知の確率性を解消。

## 実装済み機能 (一覧)

### コア
- 全 10 数値型 (`i8/i16/i32/i64/u8/u16/u32/u64/f32/f64`) + bool + string + Unit
- 整数リテラル: 10進 / 16進 (`0xff`) / 8進 (`0o755`) / 2進 (`0b1011`) + `_` 桁区切り
- 数値型サフィックス (`1_i32`, `1.5_f32`)
- 暗黙の型変換規則 (同符号整数間 / 整数→浮動 / 浮動↔浮動)、符号またぎと浮動→整数は `as` 必須
- **二項演算でのリテラル側型適応**: `u32_var != 0` のように相手の整数型にリテラルが収まれば自動でその型として扱う

### 制御フロー
- `if` / `elif` / `else` (式)
- `while` / `loop` / `for in`
- range 式 `a..b` (排他) / `a..=b` (包含) — for-in イテレータ位置のみ
- `break` / `continue` / `break v` (loop からの値付き脱出)
- `return` (値あり / なし) — **トップレベルでも使える** (早期 program exit、値は持てない)
- `match` (enum 上のパターンマッチ)
- `if let some(v) = x` (Optional 専用パターン)

### 関数
- `fn` 宣言 (引数型必須、戻り値型 `: T` (TS 風))
- ジェネリック関数 `fn id<T>(x: T): T { x }` — 推論ベースで JIT mono 化。`*const T` のような raw pointer 内の TypeVar も推論される
- 関数オーバーロード — best-match scoring、ambiguous エラー
- ファーストクラス関数 (`let f = add; f(2, 3)`)
- 匿名関数 `let inc = fn(x: i64): i64 { x + 1 }`
- クロージャ (読むだけ=値スナップショット / 代入=共有、全 capture 型対応)
- `@requires(...)` 等の属性 (パースのみ、enforce は未実装)
- `@override` (継承メソッドの override マーカー、必須)

### FFI (`@extern(C) { ... }`)
ブロック構文に統一済み。`@lib(...)` の dlopen は JIT 起動時に解決される。

ブロック内で書ける item:
- `fn name(...): T` — 関数宣言
  - `@lib("libfoo", "libfoo-fallback.so")` — dlopen するライブラリ名(複数指定 = フォールバック)。省略すると host 登録形(stdlib の math/os/test がこの形)
  - `@optional` — ライブラリやシンボルが見つからなくても JIT 構築は失敗せず、呼ぶとアボートするスタブにバインド。`os.libLoaded(name)` で事前ガードする
  - `@symbol("c_name")` — C 側のシンボル名と ilang 側の fn 名を分離
  - 末尾の `...` — printf 系 variadic
- `fn name(...): T { body }` — ilang 本体を C ABI で公開する関数(callback / 内部 wrapper)
- `struct Name { ... }` — C 互換構造体。空 struct = opaque handle として使う
- `union Name { ... }` — C union (全フィールド offset 0)
- `static name: T` — C グローバル変数
- `class Name { ... }` — ARC-managed wrapper クラス。method 本体は in_extern_c 文脈で動くので、生 extern fn と FFI ヘルパーを直接呼べる(deinit で C ハンドル自動 close できる)
- `@packed` (struct のみ) / `@bits(N)` (フィールド) — C のレイアウト調整に対応

ブロック**内のみ**で使える型:
- `*T` / `*const T` (raw pointer)
- `char` / `void` / `size_t` / `ssize_t`

これらの型はブロック外の式・型注釈に書けない。値も漏らせない (let バインディングで受けたり、call 結果に C-only 型が含まれると型エラー)。

ブロック内のみで呼べる **マーシャリングヘルパー**(自動的にビルトイン登録):
- `cstrFromString(s: string): *const char`
- `stringFromCstr(p: *const char): string`
- `freeCstr(p: *const char)`
- `bytesFromBuffer(p: *const void, n: size_t): u8[]`
- `arrayFromCArray<T>(p: *const T, n: size_t): T[]` (T は数値プリミティブ / bool)
- `cstrArrayToStrings(p: *const *const char): string[]`
- `errnoCheck(rc: i32): i32?` / `errnoCheckI64(rc: i64): i64?`

その他のキャスト規則 (ブロック内):
- `*T ↔ *U` — type-pun (`*const u8 → *const void` 等)
- `*T ↔ i64` — pointer ↔ アドレス値
- `T[] → *T` (Array→RawPtr 暗黙変換、data ポインタを渡す)
- struct 値渡し(< 16 B = chunks / HFA / > 16 B = sret)を自動で適用(旧 `byValue` フラグ相当)

### モジュール / プロジェクトファイル
- `use module` (whole) / `use module { foo, bar }` (selective: bare 名 + 名前空間の両方が使える)
- **`use module as foo`** — 別名で名前空間を import (`foo.X` で参照、内部的には `module.X` に書き戻される)
- **`use module as _ { ... }`** — 名前空間を抑止し、selective 名のみ公開
- **`pub use module`** — re-export(umbrella module を作る用)。`as` の併用は不可
- **可視性**: top-level item とクラスメンバはデフォルトで module-private。`pub fn` / `pub class` / `pub enum` / `pub const` / `pub let` (top-level) / `pub` 付きの `@extern(C){}` 内アイテム、`pub init` / `pub <method>` / `pub <field>` / `pub static` / `pub get/set` で外部公開。loader は post-load の `validate_visibility` で selective import と `module.X` 参照を pub catalog に照合し、`pub use M` チェインを辿って可視性を伝播する。`pub use M` は M の **pub アイテムだけ** を再エクスポート
- **`ilang.toml`** プロジェクトファイル: `[deps] sdl2 = "path"` で `use` の探索パスを追加。CLI が entry file から上に辿って自動発見
- `const NAME: T = const_expr` — 算術 / ビット / 比較 / 論理 / `as` キャスト / 他の const 参照を**コンパイル時に折りたたみ**。型注釈付き const は substitute 時に Cast で wrap されて、参照箇所すべてに自動的に型が伝わる
- 同梱モジュール: `math` (sqrt/sin/cos/pi/e ほか) / `test` (expect/...)、`os` (errno / libLoaded / 定数群)

### クラス (OOP フル)
- `class C { fields; init(); methods; deinit() }` — `init` 可、`deinit` 可
- 暗黙 `this` (フィールド/メソッドを `this.` なしで参照可)
- `==` / `!=` は参照等値 (`Rc::ptr_eq`)
- ジェネリッククラス `class Box<T> { ... }` — JIT mono 化
- メソッド/init オーバーロード (best-match scoring)
- `get` / `set` プロパティ (`obj.x` がアクセサ呼び出し)
- `static` メソッド (`ClassName.method(args)`)
- `static` フィールド (`i64`/`f64`/`bool` のみ、定数式初期化、mutable)
- 継承 (`extends`): 単一継承、`@override` キーワード必須、`super.method(...)` / `super(...)` (init 連鎖)、仮想ディスパッチ (vtable)、サブタイプ

### コレクション
- 配列 `T[]` / `T[N]`: literal / index / push / pop / length / slice / indexOf / includes / map / filter / forEach
- Map `Map<K, V>` (K = string / int / bool): `m[k]` / `m[k] = v` / has / delete / size / keys / values / get
- Optional `T?`: `none` / `some(x)` / `if let` / `is_some` / `is_none` / `unwrap` / `T → T?` 自動 wrap
- Weak `T.weak`: `.get(): T?` / `Foo → Foo.weak` 自動 downgrade / 二重 rc
- enum + 構造体的 payload (`tuple` / `struct` / `unit` バリアント)
- Result<T, E>: 組み込みジェネリック enum、`Result.ok(v)` / `Result.err(e)` で構築

### 文字列
- リテラル + エスケープ (`\n` `\t` `\r` `\\` `\"` `\0`)
- `+` (連結)、`==` `!=` (構造的等値)
- メソッド: `length` (Unicode コードポイント) / `charAt` / `includes` / `startsWith` / `endsWith` / `toUpper` / `toLower` / `trim` / `split` / `replace` / `slice`
- 文字列補間 (テンプレートリテラル) 対応: `` `val=${x} sum=${x + 1}` ``

### メモリ管理 (ARC)
- 全ヒープ値は ref-counted: Object / String / Array / Optional / Weak / Map / closure / EnumHeap
- `deinit` がスコープ脱出時 / rc=0 時に発火
- 二重 rc (strong/weak) で循環参照を `T.weak` で解消可能
- フィールド / 配列要素 / capture の再帰 release

## 実行モデル

| モード | コマンド | 用途 |
| --- | --- | --- |
| **MIR JIT** | `ilang run path.il` | Cranelift ネイティブコード、唯一の実行経路 |
| **AOT** | `ilang build path.il -o out` | 同じ MIR→Cranelift 経路を ELF/Mach-O に焼き出す |
| **REPL** | `ilang` (引数なし) | 1 行ずつ評価 (MIR JIT を REPL スロット付きで実行) |

`ilang.toml` が entry の上の階層にあれば自動発見、`[deps]` のパスが `use` の探索先に追加される。`ilang run --mir-jit` は旧 CLI の互換フラグで現在はデフォルトと同じ。

現状の制約:
- 静的フィールドは `i64` / `f64` / `bool` のみ (string / object 等は未対応)
- ジェネリッククラスでの **継承** / **静的メンバー** / **プロパティ** は型パラメータ解決の制約により未対応

## ワークスペース構成

```
crates/
├── ilang-ast/       # AST 定義 (Span 含む)
├── ilang-lexer/     # 字句解析 (Token, leading_newline, numeric_suffix)
├── ilang-parser/    # Pratt 構文解析 + loader (use 解決 / pub use / ilang.toml dep paths) + normalize + const 折りたたみ
├── ilang-types/     # 型チェッカー (overload resolution / mangle / inheritance / closures / @extern(C) コンテキスト)
├── ilang-mir/       # AST→MIR (SSA + block-args)、モノモーフィゼーション、validator/printer
├── ilang-mir-codegen/ # MIR→Cranelift JIT 本体
│   ├── compile/           # ARC + FFI + REPL slot を含む lowering 一式
│   ├── aot/               # ELF/Mach-O を吐く `ilang build` 経路
│   └── ty.rs              # 内部 JIT 型 / クラスレイアウト
├── ilang-runtime/   # ランタイム (alloc, retain/release, str/array fns、math/os/test extern, native_extern)
├── ilang-lsp/       # LSP サーバー
└── ilang-cli/       # `ilang` バイナリ (REPL + run + build + ilang.toml 解決)

bindings/
├── cocoa/           # macOS Cocoa バインディング (foundation / appkit)
├── directx12/       # Windows DirectX 12 バインディング (テストフィクスチャ付き)
├── gtk4/            # Linux GTK 4 バインディング (テストフィクスチャ付き)
├── libc/            # POSIX libc バインディング
├── sdl2/            # 再利用可能な SDL2 バインディング (umbrella + 機能別 6 ファイル + README)
├── sqlite3/         # SQLite3 バインディング
└── windows/         # Windows Win32 バインディング

examples/
├── sdl_breakout/   # SDL2 を使ったゲーム画面サンプル (main.il + ilang.toml)
└── libs/gui/       # libs/gui のサンプル群 (controls / menus / window 等)

libs/
└── gui/             # クロスプラットフォーム GUI ライブラリ (cocoa/win32/linux backend)

docs/syntax.md       # ユーザー向け構文一覧 (常に最新に保つ)
crates/ilang-cli/tests/programs/  # 150 個の .il fixture (MIR JIT + AOT で実行、stdout parity 検証)
```

各 crate は `lib.rs` がほぼ re-export だけ。実体は役割別ファイル。**新コードを書くときも役割別モジュールを維持** すること。テストは `crates/<crate>/tests/<name>.rs` の統合テスト + `crates/ilang-cli/tests/programs/*.il` の言語レベル fixture。

### .il fixture の書き方
- `crates/ilang-cli/tests/programs/<カテゴリ>/<名前>.il` に `.il` ファイルを 1 つ置けば自動で MIR JIT + AOT ビルドで実行される
- マジックコメント:
  - `// expect: <line>` — stdout の行を順序通りマッチ
  - `// expect-error: <substr>` — 失敗を期待、stderr に substr が含まれること
  - `// jit: skip` — MIR JIT 実行をスキップ
  - `// aot: skip` — AOT ビルド経路をスキップ
- MIR JIT と AOT 両方が走った場合は stdout 一致も検証 (divergence 防止)
- アサーションは `use test; test.expect(actual, expected)` などで書く

## JIT メモリレイアウト (重要)

| 値 | レイアウト | ヘッダサイズ |
| --- | --- | --- |
| Object | `[strong_rc | weak_rc | drop_fn | vtable_ptr | fields...]` | 32 byte |
| String | `Box<StringRc { rc, s }>` (リテラルは saturated rc) | — |
| Array | `[rc | drop_fn | len | cap | data_ptr]` ヘッダ + 別領域データバッファ | 40 byte |
| Optional<heap> | T と同じ i64 ポインタ (0 = none) | — |
| Optional<primitive> | ヒープ box (rc + value) | 16 byte |
| Weak<T> | T と同じ i64 ポインタ。retain/release は weak 側 helper | — |
| Map | Box<HashMap<MapKey, i64>> | — |
| EnumHeap | `[rc | weak_rc | drop_fn | vtable | tag(i32) | padding | payload...]` | object と同じ |
| Closure | `[rc | drop_fn | total_size]` ヘッダ + `[fn_ptr | env_field0 | ...]` | 24 byte |
| Function value | closure ptr (top-level fn は trampoline closure に自動 wrap) | — |
| `@extern(C)` struct | ARC ヘッダ付きヒープ Object と同じ — C には負オフセットの ARC ヘッダは見えず、ユーザポインタ = フィールド領域先頭 | 32 byte |

### vtable (継承)
- per-class `Box<[i64]>` (typechecker が assign した slot に関数ポインタ)
- 子クラスの vtable は親の prefix を含む (slot 共有)
- `obj.method()` は `vtable_ptr → vtable[slot] → call_indirect`
- `super.method()` は親の特定関数への直接呼び出し (no indirection)

## 重要な設計決定 (引き継ぎ要)

| 領域 | 決定 | 理由 |
| --- | --- | --- |
| 所有権 / `mut` | 採用しない | ARC 前提、全変数再代入可 |
| 戻り値型構文 | `): T { ... }` | TS 風、`->` トークンなし |
| 例外 | 採用しない | Result + panic、Rust/Go/Zig 流 |
| ファイル拡張子 | `.il` | 確定 |
| capability | アノテーション + capability 値 (案C) | enforce はまだ |
| ネイティブコード | Cranelift 第一候補 | 軽量、ビルドコスト優先 |
| クラス継承 | 単一継承 + 仮想ディスパッチ | 採用済み |
| コンストラクタ名 | `init` (Swift 風) | 特殊メソッド名、キーワードではない |
| オブジェクト等価性 | 参照等価 (`Rc::ptr_eq`) | structural equality は将来トレイト経由 |
| クロージャキャプチャ | 読むだけ=値スナップショット / 代入=共有 (JS・Swift 既定) | 代入される capture は共有セル経由で外側・兄弟と共有。別 `let` は独立 |
| FFI 構文 | `@extern(C) { ... }` ブロック | per-fn フラグまみれの旧構文を捨てて Rust の extern "C" {} を踏襲 |
| FFI 型カプセル化 | raw pointer / C-only 型はブロック内のみ書ける + 値を外に漏らせない | 「ブロックの内側だけが unsafe」という Rust と同じ思想 |
| プロジェクトファイル | `ilang.toml` (Cargo 風) | binding 配布のため最小限の `[deps]` だけ |

## ARC まわりの核心ルール (JIT)

- **caller 側で aliased な heap 引数を retain**、callee 側で param を関数出口で release (deinit の `this` は除外 — `release_object` が lifecycle を持つので二重 release で無限再帰)
- ブロック終了時に新規 heap binding を **LIFO 順** で release
- `let y = x` / 関数引数 / `obj.field = x` / `a[i] = x` で borrowed 元 (Var/Field/Index/This) なら retain (fresh 値はそのまま rc=1 を譲渡)
- `emit_bind_retain` ヘルパが bind 時 rc 調整を集約 (heap 一般則 + fresh strong → weak 変換時の特殊ケース含む)
- 代入上書きは **新値 retain → store → 旧値 release** の順
- ブロック / `__main` の **tail retain は aliased heap 値のときだけ**
- 文字列 fresh オペランドは concat / eq 後に release
- `release_object` は drop_fn 中 weak_rc を sentinel +1 (back-edge weak の早期解放回避)
- **クラス宣言は 2 段階**: `declare_class_name` で名前→id だけ登録 → `finalize_class_layout` でフィールド型解決

### Closure ARC
- closure 専用のヘッダ (`[rc | drop_fn | total_size]`) + 専用 retain/release helper
- 各 closure wrapper に対し heap captures を release する drop fn を自動生成 (`__drop_closure_<name>`)
- `JitTy::Fn` は `is_heap()` に含まれる → 既存の let-bind retain / scope release 経路で自動管理
- 入れ子 closure: 内側 closure の construct site が外側の `closure_capture_env` を見て env から load して再 capture

## 開発フロー

```sh
# 全テスト (cargo-nextest 経由、~30 秒)。`.cargo/config.toml` の
# alias で `cargo t` = `nextest run --workspace`、`cargo tci` =
# `--profile ci` (リトライ + fail-fast オフ)。設定本体は
# `.config/nextest.toml`。doctest は別途 `cargo test --doc` 必要
# (~20 秒)
~/.cargo/bin/cargo t

# cargo-nextest が無いホスト用フォールバック
~/.cargo/bin/cargo test --workspace

# REPL (let / fn / class が永続化)
~/.cargo/bin/cargo run -p ilang

# ファイル実行 (MIR JIT)
~/.cargo/bin/cargo run -p ilang -- run path.il

# AOT ビルド (Mach-O / ELF を吐く)
~/.cargo/bin/cargo run -p ilang -- build path.il -o ./out

# 1 つの fixture を直接実行
./target/debug/ilang run crates/ilang-cli/tests/programs/04_modules/extern_cstr_array.il

# SDL サンプル (要 SDL2 インストール: brew install sdl2 / apt install libsdl2-dev)
./target/debug/ilang run examples/sdl_breakout/main.il

# cocoa バインディングテスト (macOS のみ。非 macOS では skip)
#   - foundation: NSString / NSArray / NSDate / NSURL / NSData / 他
#                 38 fixtures, 645/1989 selectors (32%), 136/179 classes (76%)
#   - appkit    : NSWindow / NSButton / NSColor / NSBezierPath / 他
#                 11 fixtures, 169/508 selectors (33%), 44/53 classes (83%)
# `-- --nocapture` でカバレッジレポートを stdout に流す
~/.cargo/bin/cargo test --release -p ilang --test cocoa_foundation -- --nocapture
~/.cargo/bin/cargo test --release -p ilang --test cocoa_appkit -- --nocapture

# 個別 fixture を直接実行
./target/release/ilang run bindings/cocoa/foundation/test/strings_test.il
./target/release/ilang run bindings/cocoa/appkit/test/drawing_test.il
```

`source "$HOME/.cargo/env"` を使うと権限プロンプトが出る (settings.local.json の Bash allow が `Bash` 単独だと効かない)。**`~/.cargo/bin/cargo` を直接呼ぶこと**。

### scanner / parser ベンチ

`crates/ilang-parser/benches/scan_parse.rs` に criterion ベンチがある。stdlib / `tests/programs` 全体 / 全プログラム連結 の 3 コーパスを lex 単独・lex+parse の 2 段で計測する。

```sh
# ベースライン保存 (最適化前に1回)
~/.cargo/bin/cargo bench -p ilang-parser --bench scan_parse -- --save-baseline before

# 変更後の比較 (criterion が before との差分を出す)
~/.cargo/bin/cargo bench -p ilang-parser --bench scan_parse -- --baseline before

# 単一グループだけ走らせる例
~/.cargo/bin/cargo bench -p ilang-parser --bench scan_parse programs -- --baseline before
```

サンプル数や測定時間は `--sample-size 50 --warm-up-time 2 --measurement-time 5` 等で増やせる。デフォルトは短時間 (1秒ウォームアップ・3秒測定) なので、有意差判定が「noise threshold」になりがちな場合は増やす。

### type-check / MIR-lower ベンチ

`crates/ilang-mir/benches/check_lower.rs` に criterion ベンチがある。`tests/programs` の中で load+check+lower が成功する全プログラムを 1 ラウンドとして:

- `check_lower/check` — `ilang_types::check` のみ
- `check_lower/lower` — `ilang_types::check` + `ilang_mir::lower_program` (実パイプラインと同じ順序)

```sh
~/.cargo/bin/cargo bench -p ilang-mir --bench check_lower -- --save-baseline before
~/.cargo/bin/cargo bench -p ilang-mir --bench check_lower -- --baseline before
```

このベンチは 343 個の小プログラムを直列実行する都合上、scan_parse より run-to-run の variance が大きい。意味のある差分判定をしたいときは `--sample-size 50 --warm-up-time 3 --measurement-time 6` 程度を指定する。

### コミット方針
- 機能単位で 1 コミット
- メッセージは英語、末尾に `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`
- ユーザーが「コミットして」と言うまでコミットしない (頻出パターン)

### コードスタイル
- コメントは「なぜ」だけ。「なに」は書かない
- 公開 API のみ `pub use` で再エクスポート
- 警告ゼロを維持
- 役割別モジュール分割を維持 (`lib.rs` を肥大化させない)
- 大きな機能を追加するときは fixture を `tests/programs/` に必ず追加 (MIR JIT + AOT 両方で動くか自動検証されるため)

## 次の候補

ユーザーと相談して選ぶこと。

### A. 言語機能 (重要度高)
- **タプル** `(i64, string)` — 匿名 product 型。複数戻り値 / 一時的なペアに毎回クラスを定義するのが冗長
- **`?` 演算子** — `let v = parse(s)?` で `Result.err` を即 return。Result 連鎖が match ネストにならない
- **文字列補間** — `"hello, ${name}"`
- **Iterator プロトコル** — ユーザ型に `next()` を実装させて for-in に乗せる仕組み。ジェネレータの基礎
- **デフォルト引数 / 名前付き引数** (デフォルト引数は実装済み、名前付き呼び出しは未)

### B. FFI / バインディング配布
- **C ヘッダから .il を自動生成するミニ bindgen** — 現状は手書き。3 段階(手動 YAML / clang JSON AST → スクリプト変換 / libclang フル統合)を [docs/syntax.md](syntax.md) ではなく相談済みのチャットで議論済み。当面は `bindings/sdl2/` を手書きでメンテ
- **`bindings/libc/`** など他のライブラリも同形式で整備
- **`bindings/sdl2/`** の拡張: SDL_Image (PNG 読み込み) / SDL_ttf (フォント) / イベントの SDL_PollEvent 構造体対応(現状は SDL_GetKeyboardState ベースのポーリングだけ)

### C. 言語機能 (重要度中)
- 演算子オーバーロード (`class Vec2 { + (other: Vec2): Vec2 { ... } }`)
- Trait / Interface — type-by-shape 抽象化、継承と直交
- デストラクチャリング (`let (a, b) = pair`)
- async/await
- ジェネリック制約 (bounds)

### D. capability の enforce ← **言語ビジョンの核**
- 呼び出し側にも `@requires(...)` を要求するチェック
- `use http_client with cap(net = env.net)` のような import 構文
- クラスに capability を持たせる構文設計
- MIR を挟むタイミングで一緒に入れるのが自然 (関数呼び出しグラフが扱いやすい)

### E. JIT 補完
- 静的フィールドの string / object 対応
- ジェネリッククラスでの継承 / 静的メンバー / プロパティ
- **`&CONST` (トップレベル const のアドレス取得) の最適化**:
  現状の lowering (`crates/ilang-mir/src/lower/ops.rs`
  `lower_addr_of_decomposed`) は loader が demote した repl_slot
  経由で値をロードし、CRepr Object の場合のみポインタ値を再タグする
  形で `&IID_X` を通している。毎回 `__repl_load_slot` を呼ぶので、
  ホットパスでは無駄。.rodata 相当の静的データシンボルに焼いて
  `symbol_value` で参照する形にすれば呼び出しオーバーヘッドが消える。
  CRepr 以外 (i64/f64/string などのプリミティブ const) は現状未対応
  で `&` がエラーになる — 必要になったらスタックコピー経路も追加

### F. MIR まわりの拡張 (中間表現は導入済み)
AST → MIR (SSA + block-args) → Cranelift IR の経路は完成済み。次に乗せやすいもの:
- LLVM / wasm 等の別バックエンド (Cranelift 経路と並走できる)
- MIR レベルでの constant folding / dead code elim
- capability enforce の検査箇所として活用
- 軽量バイトコード VM (起動高速化やデバッガ統合の足場として)

## 会話のトーンと言語

- ユーザーへの返答は **日本語** (このファイルも日本語)
- コード/識別子/コミットメッセージは英語
- 提案する場合は 2-3 案のトレードオフを示し、推奨を 1 つ明示
- ユーザーが選んだ後は実装まで進める (確認は重要なところだけ)
- 大きな機能追加では「Phase A→B→C と段階的に」を提案するパターンが多い (継承、static、closure、FFI リファクタはこの形で着地済み)

## 既知の細かい落とし穴

- **JIT の内部 typecheck**: `jit_run_inner` の中で TypeChecker をもう一度動かす (`define_main` 内)。第 2 パス用の side table (`closure_wrapper_captures`, `loop_break_types`, `class_method_slots`, `class_vtable_lens`) を毎回最新に保つこと
- **Hoist pass は MIR mono の一部**: `crates/ilang-mir/src/monomorphize/hoist.rs::hoist_anon_fns` が FnExpr → Closure 変換を行う。`ExprKind::Closure` は typechecker からは `unreachable!` で除外され、hoist 後の MIR でのみ現れる
- **`ExprKind` を追加したら walker を全部更新**: monomorphize.rs に 6+ の walker (hoist_in_expr / scan_expr / subst_expr / rewrite_expr / walk_expr_children / map_expr_children + rewrite_calls_in_expr / rewrite_enum_refs_in_expr) があり、checker / mangle / loader / normalize にも match 漏れチェックが効く
- **AST の `is_override`**: `FnDecl.is_override` は override メソッドのときだけ `true`。クローンする箇所が monomorphize に多数あるので忘れずに `f.is_override` を伝播
- **`extends` 周りの class_signature**: parent をオーバーレイしてから子の declarations をマージ。`init` / `deinit` は per-class なので override 必須チェックの対象外 (特殊条件あり)
- **`docs/syntax.md` を最新に保つ**: 機能追加するたびに必ず更新する (ユーザーが頻繁に参照)
- **`@extern(C)` の synth パイプライン**: ブロック内の struct / fn / class は `synthesize_extern_c_classes` / `synthesize_extern_c_fns` で AST レベルでトップレベル相当に展開される。ここを通る `@lib` 付き fn には自動で `byValue` / `variadic` / `optional` 属性が付く。下流の native_extern.rs や class registration はこの synth 結果を読む
- **`@lib` fn のシンボル名と ilang 名**: ilang 名は loader の prefix で `module.fn_name` に化けるが、dlsym はオリジナルの C シンボルでなければならない。loader が `c_symbol` フィールドを自動でセット保存する仕組みになっている (`@symbol("...")` が明示されていなければ元の bare name)
- **FFI ヘルパー (`cstrFromString` 等) は loader の prefix から除外**: `prefix_block_calls` の `is_builtin_callee` 判定で組み込みヘルパーのリストを持つ。新ヘルパーを追加するときはここも更新
- **`ilang.toml` の検索**: CLI が entry file の親ディレクトリから上に辿って `ilang.toml` を探す。プロジェクトを横断する CLI 統合テストは現状ないので、変更時は `examples/sdl_breakout/` で動作確認するのが手軽

## WebGPU PoC (`examples/wgpu_triangle`) — 3 OS で動かす手順

ilang から `wgpu-native` (WebGPU の C 実装) を叩く PoC。**SDL2 で独立ウィンドウを作り、その上に wgpu サーフェスを張って WGSL シェーダで三角形を描く**。「シェーダを環境非依存(WGSL 1本)で 3 OS」を狙う検証。**macOS / Windows / Linux すべて実機確認済み**。Linux は Ubuntu (VirtualBox) の **Wayland セッション + lavapipe (ソフトウェア Vulkan)** で `PASS: presented 120 frames` を確認。サーフェスソースは `os.platform` で出し分ける(macOS=CAMetalLayer / Windows=HWND / Linux=X11 or Wayland を `info.subsystem` で分岐)。バインドは `bindings/wgpu/` (wgpu-native **v29.0.0.0** の `webgpu.h`/`wgpu.h` に固定)。

**Linux 実機検証で潰した本物の codegen バグ (x86_64 System V ABI)** — macOS aarch64 / Windows では別 ABI 経路のため顕在化していなかったもの。詳細は [`crates/ilang-mir-codegen/src/compile/abi.rs`](../crates/ilang-mir-codegen/src/compile/abi.rs):
- **>16 B の値渡し構造体引数**: 旧実装は「呼び出し側でコピーを作りそのポインタを渡す」隠しポインタ方式だった (AArch64 AAPCS64 / Win64 では正しい)。SysV では MEMORY クラスとしてスタックへ直接コピーするのが正で、Cranelift の `ArgumentPurpose::StructArgument` を使うよう修正 (`clif_signature_for`)。wgpu の `WGPURequestDeviceCallbackInfo` (40 B 値渡し) がこれで壊れ、`onDevice` のはずが `onAdapter` が呼ばれて device 取得が失敗していた。
- **構造体の戻り値**: SysV は戻り値レジスタが整数 2 本 (rax:rdx)・浮動 2 本 (xmm0:xmm1)。ilang ABI の 64 B chunk 上限のままだと 32 B 構造体が 4 戻り値になり Cranelift が拒否 (#9510)。戻り値専用キャップ `ret_chunk_max` / `struct_hfa_ret` を導入し、超過分は sret に落とす。
- **クロージャの bool 戻り値**: x86_64 の `setcc` は下位 1 バイトしか書かず上位はゴミ (aarch64 `cset` はゼロ化)。ランタイムが述語結果を i64 全幅で読んでいたため `filter`/`find`/`every`/`some` が誤動作。`call_predicate_1` で下位バイトのみ読むよう修正 (`crates/ilang-runtime/src/arrays.rs`)。
- **固定長配列 (`T[N]`) の解放カスケード**: 固定長配列はヘッダ無しのインラインデータなのに KIND_ARRAY 扱いで「ヒープ配列として free」していた → glibc が `munmap_chunk(): invalid pointer` で abort (macOS のアロケータは見逃す)。`kind_tag_of` / AOT `field_kind_tag` で `len: Some` を KIND_NONE に。enum ペイロードのカスケードタグも MIR 型ベースに変更 (`jit_setup.rs`)。

### 共通の準備

1. **ライブラリ取得**: `third_party/wgpu/fetch.sh` を実行 (`gh` CLI 必須)。OS/arch を自動判定して該当リリースを DL し、dylib/so/dll + ヘッダを `third_party/wgpu/<os-arch>/` に展開、巨大な `.a` と zip は削除する。バイナリは **未コミット** (`.gitignore` 済み)。
   - Windows で bash が無ければ git-bash で `fetch.sh` を実行するか、`gh release download v29.0.0.0 -R gfx-rs/wgpu-native -p "wgpu-windows-x86_64-msvc-release.zip"` を手動展開する。
2. **ライブラリ検索パス**を立てて実行 (バイナリは標準ディレクトリに無いため):
   - macOS:   `DYLD_LIBRARY_PATH=third_party/wgpu/macos-aarch64/lib ./target/debug/ilang run examples/wgpu_triangle/main.il`
   - Linux:   `LD_LIBRARY_PATH=third_party/wgpu/linux-x86_64/lib ./target/debug/ilang run examples/wgpu_triangle/main.il`
   - Windows: `wgpu_native.dll` (+ `SDL2.dll`) を PATH に通すか、entry / exe と同じディレクトリへ置く。`@extern(C, "wgpu_native")` の bare 名解決で拾われる。確認済みの起動例(git-bash):
     `PATH="$PWD/third_party/wgpu/windows-x86_64/lib:$PWD/target/release:$PATH" ./target/debug/ilang.exe run examples/wgpu_triangle/main.il`
     (SDL2.dll は `target/release` に同梱されている)。`fetch.sh` は git-bash/MSYS でも Windows asset を取得する。

### OS 別 SurfaceSource (3 OS すべて実装済み・実機確認済み)

`examples/wgpu_triangle/main.il` は `os.platform` を見て SurfaceSource を出し分ける。共通部 (adapter/device/pipeline/draw) は OS 非依存。3 種とも struct は `bindings/wgpu/mod.il` に定義済み。

- **macOS**: `SDL_Metal_CreateView` → `SDL_Metal_GetLayer` → `WGPUSurfaceSourceMetalLayer` (sType=4)。
- **Windows**: `SDL_GetWindowWMInfo` で HWND/HINSTANCE を取り `WGPUSurfaceSourceWindowsHWND` (sType=5) をチェイン。ウィンドウフラグは macOS だけ METAL を付け、それ以外は SHOWN のみ。
- **Linux**: `info.subsystem` で X11 (=2) / Wayland (=6) を分岐。X11 は `WGPUSurfaceSourceXlibWindow` (sType=6)、Wayland は `WGPUSurfaceSourceWaylandSurface` (sType=7)。**実機確認済み** (Wayland セッションで `info.subsystem==6` 経路を通過)。GPU 3D アクセラレーションが無い環境では wgpu が自動で lavapipe (ソフトウェア Vulkan) を選ぶ。

ネイティブハンドルは SDL から取得する (`bindings/sdl2/sdl_window.il`)。`SysWMinfo.version` を埋めてから `SDL_GetWindowWMInfo(win, &info)`、`info.subsystem` (windows=1 / x11=2 / wayland=6) と `info.handle1..4` を読む。各 SurfaceSource の handle 対応:

- **Windows** (`subsystem==1`): hwnd=handle1, hinstance=handle3
- **Linux/X11** (`subsystem==2`): display=handle1 (`*void`), window=handle2 (`u64` の XID)
- **Linux/Wayland** (`subsystem==6`): display=handle1 (`*void`), surface=handle2 (`*void`)

SDL の wayland/x11 の選択は環境変数や `SDL_VIDEODRIVER` に依存するので、コード側は `info.subsystem` を見て分岐している。

### この PoC で判明した ilang FFI の落とし穴 (踏み直さないこと)

- **`@extern(C)` struct の `@handle` フィールド**: 以前は値が壊れたが commit `8ecbef08` (`mir-codegen: store @handle struct fields as pointer-sized values`) で修正済み。**ハンドル型フィールドはそのまま `WGPUXxx` 型で宣言してよい** (`*void` 回避策は不要)。ハンドル配列 (`WGPUCommandBuffer[]`) の要素も正しく直列化される。
- **ハンドル型は必ず `@handle pub struct WGPUXxx {}` を宣言する**。戻り値で使うだけだと型は通っても `handle as i64` 等のキャストが「expected i64, got WGPUXxx」で落ちる。
- **構造体 out パラメータは `&local` で渡す** (値ローカル/`new` どちらでも可)。`@extern(C)` struct を `*T` 引数へ**直接**渡すのは**設計上禁止** (型エラー) で、アドレス取得を明示させる方針 (`ops.rs::assignable` のコメント参照)。配列だけは `T[] → *T` が暗黙。caps 取得・`getCurrentTexture` はこの `&` 方式で動く。
- **コマンドバッファ配列は `WGPUCommandBuffer[]` を渡す** (`let cmds: WGPUCommandBuffer[] = [cmd]; wgpuQueueSubmit(queue, 1, cmds)`)。`T[] → *T` 変換でポインタが渡る。バインドの `commands` 引数は `*WGPUCommandBuffer`。
- **値渡しの CallbackInfo + コールバックの `WGPUStringView`**: `wgpuInstanceRequestAdapter`/`RequestDevice` は CallbackInfo を**値渡し**、コールバックは `WGPUStringView` を**値渡し**で受ける。コールバックは `WGPUStringView` を **2つの i64 に展開** (`fn(i32, i64, i64, i64, i64, i64)`) して受けると ABI が合う。wgpu-native はコールバックを同期的に発火するので `wgpuInstanceProcessEvents` 後にスロットから読めばよい。
- **`WGPUStringView.data` は `*void`**。`cstrFromString(s) as *void` を入れ、`length` は `0xFFFFFFFFFFFFFFFF`(=WGPU_STRLEN, SIZE_MAX) にすると wgpu 側で strlen される。
- **enum 値はヘッダ準拠の flat 値**でOK (`fmt`/`alpha`/`present` は `wgpuSurfaceGetCapabilities` の実値を使うのが堅い)。`WGPUTextureUsage`/`WGPUColorWriteMask` は **64bit (WGPUFlags)** なので struct フィールドは `u64`。
- **ドローアブル取得**: `wgpuSurfaceGetCurrentTexture` はウィンドウが**画面に合成されるまで** texture=null を返す。`SDL_ShowWindow` + `SDL_RaiseWindow` でウィンドウを前面化し、`SDL_PumpEvents` を回しつつ **texture が取れるまでリトライ**する (PoC のループ参照)。

### 既知の未解決/保留

- 取得ライブラリと同梱ヘッダで **`WGPUSurfaceGetCurrentTextureStatus` の値が食い違って見える瞬間がある** (texture 取得失敗時に `0x00030001` が観測された)。texture 取得成功時は `status=1` で header と一致するので、**status の数値で判定せず `texture != 0` で判定**している。
- **ライブラリ取得は `gh` 必須だが、無ければ `curl -fSL https://github.com/gfx-rs/wgpu-native/releases/download/v29.0.0.0/wgpu-linux-x86_64-release.zip` で直接 DL → `unzip` でも可** (`fetch.sh` は `gh` 前提)。
- formatter (`ilang-lsp`) はバッククォート テンプレートリテラルを `TmplStart` / `TmplLit` / `TmplEnd` の 3 トークンに分けて lex する。以前は content と閉じバッククォートの間に空白が入って WGSL 文字列を壊していた(`main.il` で `formatter_preserves_lexability_on_corpus` が落ちていた)。`needs_space` でこの 3 トークン同士を密着させて修正済み。
