# 攻撃面カバレッジ索引

バグあぶり出しラウンドが **どの攻撃面を確認済みか** を組合せ側から引くための表。
[BUG_HUNTING.md](BUG_HUNTING.md) §6 のマトリクス(値の種類 × 配置 × 制御フロー)の
「確認済み交点」を、弾・結果・代表 fixture で一覧にする。

**役割分担**: 本書は索引(攻撃面 → 確認済みか)、[HANDOFF.md](HANDOFF.md) は履歴と詳細、
[BUG_HUNTING.md](BUG_HUNTING.md) は手順。HANDOFF が大きくなりすぎたため索引だけ分離した。

## 使い方

- **ラウンド開始時**: §6 で狙う交点を決めたら、本書の該当領域を見る。既に行があれば
  別の交点へ移る(同じ場所を再 probe しない)。**表に無い交点が次の候補**。
- **ラウンド終了時**: 確認した交点を該当領域の表に 1 行追記する(下記「追記規約」)。
  これを怠ると索引が実態とズレて再 probe を招くため、[BUG_HUNTING.md](BUG_HUNTING.md)
  §11・§12 の必須項目にしてある。

## 追記規約

- 列は `攻撃面(配置・制御フロー) | 弾 | 結果 | fixture`。
- **結果**: `修正`(バグ検出・修正)/ `健全`(クリーンラウンド)/ `仕様`(ユーザー決定の仕様追加)。
- **fixture** は `crates/ilang-cli/tests/programs/` 配下への相対リンク。複数あるものは代表 1 本。
  fixture を増やさないクリーンラウンドは `—`。
- 詳細(症状・原因・修正・検証)は HANDOFF の `第 N 弾` 記録を参照。本書は重複させない。

---

## object / class(ARC・継承・static・this・closure 内 this)

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| メソッド内 closure の `this` 解決(bare field 代入 / bare メソッド呼び) | 32 | 修正 | [closure_field_no_spurious_this](crates/ilang-cli/tests/programs/05_edge_cases/closure_field_no_spurious_this.il) |
| implicit bare-name field 代入 × Optional / subtype wrap | 33 | 修正 | [subclass_optional_wrap](crates/ilang-cli/tests/programs/09_subtyping/subclass_optional_wrap.il) |
| bare field 代入 × 宣言型が要る RHS | 34 | 健全 | — |
| bare field 書き × 反復ミューテーション・多段連鎖 | 35 | 健全 | — |
| bare field 代入が composite リテラル要素 wrap を欠く | 36 | 修正 | — |
| heap static フィールド代入の retain(UAF) | 51 | 修正 | — |
| heap 値 property の ARC(owned 規約) | (増殖3) | 修正 | [property_heap_value_arc](crates/ilang-cli/tests/programs/08_properties/property_heap_value_arc.il) |
| 深い解放 cascade(stack overflow) | (増殖5) | 修正 | [deep_release_iterative](crates/ilang-cli/tests/programs/05_edge_cases/deep_release_iterative.il) |
| 継承時 `deinit` の自動連鎖(Swift 流) | — | 仕様 | — |

## interface / 仮想ディスパッチ / property

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| 分岐 join が共通 interface に合流しない | 46 | 修正 | [covariant_join_interface_result](crates/ilang-cli/tests/programs/09_subtyping/covariant_join_interface_result.il) |
| interface covariance + downcast | 47 | 健全 | — |
| 仮想ディスパッチの fresh heap 引数リーク | 119 | 修正 | [interface_dispatch_fresh_arg_release](crates/ilang-cli/tests/programs/09_subtyping/interface_dispatch_fresh_arg_release.il) |
| interface 実装クラスのサブクラスを親クラス型で呼ぶと SIGSEGV | 120 | 修正 | [interface_method_base_typed_receiver](crates/ilang-cli/tests/programs/09_subtyping/interface_method_base_typed_receiver.il) |
| property 仮想ディスパッチ × サブクラス継承スロット | 123 | 健全 | — |
| `is`/`as?` が第一基底以外の interface を認識しない | 124 | 修正 | [interface_is_as_additional_and_inherited](crates/ilang-cli/tests/programs/09_subtyping/interface_is_as_additional_and_inherited.il) |
| generic instance × interface covariance / 判別 | 125 | 健全 | [generic_instance_is_as_discrimination](crates/ilang-cli/tests/programs/09_subtyping/generic_instance_is_as_discrimination.il) |
| property accessor body から top-level let 参照 | 52 | 修正 | — |

## string

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| inplace 文字列 concat の fresh rhs リーク | 43 | 修正 | — |
| string / array ARC 全方位 | 44 | 健全 | — |
| unicode(astral / 結合文字 / `\u{}` / NUL / split) | 139 | 健全 | [string_unicode_escapes_and_combining](crates/ilang-cli/tests/programs/05_edge_cases/string_unicode_escapes_and_combining.il) |
| 文字列の順序比較は型エラー | — | 仕様 | [string_ordering_error](crates/ilang-cli/tests/programs/05_edge_cases/string_ordering_error.il) |

## 動的配列

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| index 代入 RHS が composite 要素 wrap を欠き SIGSEGV | 41 | 修正 | — |
| 動的配列 static フィールドが codegen で crash | 89 | 修正 | [static_array_field_error](crates/ilang-cli/tests/programs/05_edge_cases/static_array_field_error.il) |
| 配列 `indexOf`/`includes`/`remove` が payload enum を構造的比較しない | 126 | 修正 | [array_search_enum_structural](crates/ilang-cli/tests/programs/03_collections/array_search_enum_structural.il) |
| 配列検索 × 構造的 == コンテナ(tuple/array/optional 要素) | 127 | 修正 | [array_search_structural_containers](crates/ilang-cli/tests/programs/03_collections/array_search_structural_containers.il) |

## 固定長配列 `T[N]`

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| heap 要素の ARC 未モデル → 正式サポート | 18–19 | 修正 | — |
| weak × 固定長配列 × wrap | 45 | 健全 | — |
| 固定長配列の `==` は対象外(型エラー) | — | 仕様 | [fixed_array_equality_error](crates/ilang-cli/tests/programs/05_edge_cases/fixed_array_equality_error.il) |

## Optional

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| strong → `Node.weak?` の bare coercion が配置ごとに破綻 | 76 | 修正 | [optional_weak_from_strong](crates/ilang-cli/tests/programs/05_edge_cases/optional_weak_from_strong.il) |
| `Set` の Optional wrap が return 位置で早期解放 | 77 | 修正 | [optional_wrap_set_retain](crates/ilang-cli/tests/programs/05_edge_cases/optional_wrap_set_retain.il) |
| f64 値を `f32?`(numeric optional)に代入すると SIGSEGV | 135 | 修正 | [numeric_optional_wrap_coerce](crates/ilang-cli/tests/programs/05_edge_cases/numeric_optional_wrap_coerce.il) |

## tuple

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| monomorphize が tuple 型の中の型パラメータを置換しない | 39–40 | 修正 | — |
| tuple 要素 wrap の残る store サイト | 42 | 健全 | — |
| tuple に構造的 `==`(入れ子・参照スロット) | 127 | 仕様 | [structural_eq_tuple_array_optional](crates/ilang-cli/tests/programs/05_edge_cases/structural_eq_tuple_array_optional.il) |

## Map

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| `m[missing]` が panic せず default を返す | 71 | 修正 | [map_index_missing_key_panics](crates/ilang-cli/tests/programs/05_edge_cases/map_index_missing_key_panics.il) |
| `{}` map リテラルのクラスキーが値等価でキーされない | 75 | 修正 | [map_object_key_brace_literal](crates/ilang-cli/tests/programs/03_collections/map_object_key_brace_literal.il) |
| join 共変を map リテラル値へ | 81 | 修正 | [map_literal_covariant_value](crates/ilang-cli/tests/programs/09_subtyping/map_literal_covariant_value.il) |
| index 読み取りでオブジェクトキーがリーク | 101 | 修正 | [map_index_read_object_key_release](crates/ilang-cli/tests/programs/03_collections/map_index_read_object_key_release.il) |
| slot 昇格 edge + forEach mutation | 53 | 健全 | — |
| payload enum を直接 Map キーに | 130 | 仕様 | [set_map_payload_enum_key](crates/ilang-cli/tests/programs/03_collections/set_map_payload_enum_key.il) |

## Set

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| オブジェクト要素の集合演算 ARC | 97 | 健全 | [set_algebra_object_arc](crates/ilang-cli/tests/programs/03_collections/set_algebra_object_arc.il) |
| `Set<f64>` の特殊値ビット列(NaN/±0/±Inf) | 102 | 健全 | [set_float_bit_pattern](crates/ilang-cli/tests/programs/03_collections/set_float_bit_pattern.il) |
| payload enum を直接 Set 要素に(演算・述語・entries) | 130–131 | 仕様/健全 | [set_payload_enum_operations](crates/ilang-cli/tests/programs/03_collections/set_payload_enum_operations.il) |

## enum payload / repr / `@flags`

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| enum ctor 型引数が store / 引数 / some / tuple / index 代入位置で refine されない | 48–54 | 修正 | — |
| match arm / クロージャ戻り値が yield する enum ctor の refine | 50, 61 | 修正 | — |
| enum-in-enum payload の refine | 64 | 修正 | — |
| generic enum リテラルの covariance(if/match join, some/tuple 入れ子) | 66–67, 79 | 修正/仕様 | [generic_enum_covariant_join](crates/ilang-cli/tests/programs/09_subtyping/generic_enum_covariant_join.il) |
| `@flags` の `has` 未 lower と `~` の SIGSEGV | 68 | 修正 | — |
| repr enum を int リテラルと比較できない非対称 | 69 | 修正 | [repr_enum_comparison](crates/ilang-cli/tests/programs/06_enums/repr_enum_comparison.il) |
| 無効 enum 値の wildcard 無し match が SIGILL | 72 | 修正 | — |
| combined `@flags` 値の `==`/Set/Map/leak | 73 | 修正 | [flags_enum_combined_identity](crates/ilang-cli/tests/programs/06_enums/flags_enum_combined_identity.il) |
| repr enum 同士の順序比較がポインタ比較で誤答 | 74 | 修正 | — |
| 直接 `==` の構造的比較(payload enum) | 105 | 修正 | [enum_structural_equality](crates/ilang-cli/tests/programs/06_enums/enum_structural_equality.il) |
| enum フィールドを `@derive(Eq, Hash)` で完全支援 | 129 | 仕様 | [derive_enum_field](crates/ilang-cli/tests/programs/02_classes/derive_enum_field.il) |

## Promise / async / event loop / timer

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| executor の leak / ICF 二重登録 / float ABI / TLS 破棄順 | 8–9 | 修正 | — |
| Promise.all/race の入力・結果配列所有権、fresh promise 引数 | 10 | 修正 | [promise_all_race_input_arc](crates/ilang-cli/tests/programs/04_modules/promise_all_race_input_arc.il) |
| await の rejection propagate と早期 return の scope release | 11 | 修正 | — |
| 実行モデルを JS 流(run-to-completion・単一スレッド)へ移行 | — | 仕様 | — |
| exit 時の event-loop drain が global 解放後に走り OOB | 37 | 修正 | — |
| async ARC 全方位 | 38 | 健全 | — |
| timer(setInterval / microtask 順序)の深掘り | 140–141 | 健全 | [timer_interval_multifire_arc](crates/ilang-cli/tests/programs/05_edge_cases/timer_interval_multifire_arc.il) |
| await を含まない async fn の `return` が Promise にラップされない | 149 | 修正 | [async_zero_await_return](crates/ilang-cli/tests/programs/05_edge_cases/async_zero_await_return.il) |

## weak

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| weak / property 本体 / fs / path / interface 配列の sweep | 17 | 健全 | — |
| `let w: T.weak = strongRef` が weak 共有を retain せず UAF | 145 | 修正 | [weak_bind_keeps_zombie_alive](crates/ilang-cli/tests/programs/05_edge_cases/weak_bind_keeps_zombie_alive.il) |
| weak 再代入の UAF と一時 weak レシーバのリーク | 146 | 修正 | — |
| weak 強制を全消費サイトで完了(fresh/join/return/field/array/tuple/arg)+ 順序バグ | 147–148 | 修正 | [weak_fresh_into_container_and_arg](crates/ilang-cli/tests/programs/05_edge_cases/weak_fresh_into_container_and_arg.il) |

## closure

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| capture cell の解放 / nested shared cell / loop per-iteration capture | 増殖2 | 修正 | — |
| 早期脱出 × 値の持ち出し(closure が capture を持って脱出) | 14 | 健全 | — |
| fn 本体内の自己再帰 closure(`ClosureSelf`) | — | 仕様 | — |

## 横断: 型推論 / generic 単一化

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| generic fn の戻り値位置 / 期待型 / 引数からの型パラメータ推論 | 59–62 | 修正 | — |
| 解決できない型引数を明示診断に | 63 | 仕様 | — |
| generic fn / class が builtin Result・generic enum を構築 | 57–58 | 修正 | — |
| tail 式の奥の `?` の err return が refine されない | 56 | 修正 | [try_op_heap_result_arc](crates/ilang-cli/tests/programs/05_edge_cases/try_op_heap_result_arc.il) |
| 再帰的 generic class で monomorphizer がハング | 91 | 修正 | [mono_recursion_limit_error](crates/ilang-cli/tests/programs/05_edge_cases/mono_recursion_limit_error.il) |
| generic クラス上の generic メソッド(主ケース) | 107 | 修正 | — |

## 横断: join 共変

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| join 共変を配列リテラル / some 包みへ | 80 | 修正 | — |
| join 共変を map / tuple リテラルへ | 81 | 修正 | — |
| 入れ子コンテナ covariance(some/tuple 降下) | 67 | 修正 | [nested_container_covariance](crates/ilang-cli/tests/programs/09_subtyping/nested_container_covariance.il) |

## 横断: 制御フロー型付け / divergence

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| for-in × 早期 return(fresh iterable leak / 要素借用の過剰解放) | 13 | 修正 | — |
| continue sweep / fresh scrutinee の早期脱出 / string match | 12 | 修正 | — |
| 全 arm が return する match を関数末尾に置くと型エラー | 144 | 修正 | [match_all_arms_return_tail](crates/ilang-cli/tests/programs/06_enums/match_all_arms_return_tail.il) |
| break 無し `loop` を関数 tail にすると `body produces ()` | 150 | 修正 | [loop_no_break_diverges](crates/ilang-cli/tests/programs/05_edge_cases/loop_no_break_diverges.il) |
| `todo()` の divergence(任意位置で型検査・到達で abort) | 154 | 仕様 | [todo_unreached_compiles](crates/ilang-cli/tests/programs/05_edge_cases/todo_unreached_compiles.il) |

## 横断: 数値 / repr / SIMD / const

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| const の div/mod by zero がコンパイル時に検出されない | 90 | 修正 | [const_div_zero_error](crates/ilang-cli/tests/programs/05_edge_cases/const_div_zero_error.il) |
| float リテラルが整数 SIMD レーンに無検査で通る | 88 | 修正 | [simd_int_lane_float_literal_error](crates/ilang-cli/tests/programs/04_modules/simd_int_lane_float_literal_error.il) |
| u64 の div/mod/shift(リテラル)と表示が符号付き | 132 | 修正 | [u64_signedness](crates/ilang-cli/tests/programs/01_basics/u64_signedness.il) |
| f64 値を `f32` フィールドに直接代入すると 0.0 | 134 | 修正 | [f32_field_from_f64_value](crates/ilang-cli/tests/programs/02_classes/f32_field_from_f64_value.il) |
| f32←f64 を全格納位置で(field/optional/arg/return) | 134–135 | 修正 | [f32_from_f64_all_positions](crates/ilang-cli/tests/programs/05_edge_cases/f32_from_f64_all_positions.il) |
| unsigned 整数の端(境界比較・narrow 除算・キャスト) | 133 | 健全 | [unsigned_literal_and_cast](crates/ilang-cli/tests/programs/01_basics/unsigned_literal_and_cast.il) |

## 横断: FFI / `@extern(C)` / repr(C)

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| `@extern(C) struct` の `@bits` 検証欠落 | 84 | 修正 | [extern_struct_bitfield_signed_error](crates/ilang-cli/tests/programs/04_modules/extern_struct_bitfield_signed_error.il) |
| `@extern(C) union` のフィールド検証欠落(SIGSEGV) | 85 | 修正 | [extern_union_heap_field_error](crates/ilang-cli/tests/programs/04_modules/extern_union_heap_field_error.il) |
| `@extern(C) struct` の field-type 検証欠落(leak/panic) | 86 | 修正 | [extern_struct_heap_field_error](crates/ilang-cli/tests/programs/04_modules/extern_struct_heap_field_error.il) |

## 横断: capability / security

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| capability enforcement(ilang.toml)を実装 | 142 | 仕様 | — |
| extern を値に束ねて間接呼び出しすると capability ゲートを回避 | 143 | 修正 | — |
| trojan source(双方向 unicode)の拒否 | — | 仕様 | [trojan_source_rejected](crates/ilang-cli/tests/programs/05_edge_cases/trojan_source_rejected.il) |

## 横断: REPL

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| loader 相当の normalize 経路へ乗せ替え(enum/async/const/generic slot/use) | 15 | 修正 | — |
| 再定義セマンティクス(型違い re-let の健全性穴) | 16 | 修正 | — |
| bare 式 echo を全型 auto-print に | 153 | 仕様 | — |

## 横断: std ライブラリ

| 攻撃面 | 弾 | 結果 | fixture |
|---|---|---|---|
| `std.math` の `sign(±0)` と `min`/`max` の NaN | 151 | 修正 | [math_sign_and_nan](crates/ilang-cli/tests/programs/04_modules/math_sign_and_nan.il) |
| `std.events` の emit 中リスナ削除で OOB | 152 | 修正 | [events_emit_reentrancy](crates/ilang-cli/tests/programs/04_modules/events_emit_reentrancy.il) |
