# ilang HANDOFF

新しいセッションへの引き継ぎ用。`/clear` 後にこのファイルを読めば現状の文脈が把握できる構成。

言語仕様の詳細は [`docs/syntax.md`](syntax.md) を参照。このファイルは **「実装の現状」と「次に何をやるか」の引き継ぎ** に絞る。バグあぶり出しラウンドの実施手順(probe の書き方・計測の罠・検証儀式・fixture 規約)は [`docs/BUG_HUNTING.md`](BUG_HUNTING.md) を参照。

## プロジェクト概要

**ilang** はユーザーが新しく設計中のプログラミング言語。最終ゴール:

- **capability ベースのセキュリティ**: ライブラリ/クラスごとに `net`, `file` などの実行権限を持たせ、サプライチェーン攻撃を緩和する (核となる設計目標)
- **ARC** によるメモリ安全性 (所有権/`mut`/借用は採用しない)
- **JS / TypeScript / Rust 風** のハイブリッド構文。文末は **JS 風 ASI** (改行が `;` 代わり)
- 例外なし。失敗は `Result<T, E>`、回復不能エラーは panic

実装言語: **Rust 1.95**。実行モデル: AST → MIR (SSA) → **Cranelift JIT** が唯一の実行経路。ツリーウォーク インタプリタ (`ilang-eval`) と旧 ilang-codegen 経路は M1 Step 6 で撤去済み。AOT 経由のネイティブ実行は `ilang build` で行う。

## 現在地

`run_all_program_fixtures` (1289/1289) + `cocoa_foundation` + `cocoa_appkit` + workspace 全 539 test 全緑。 `crepr_struct_field_discard.il` (a6e9310e で意図的に赤いまま追加されていた fixture) は緑。 `examples/sdl_breakout/main.il` の起動も実機確認済み (`playing — ESC to quit`)。

直近のセッション (2026-06-11) で main に landing した変更:

- **第 64 弾**。 第 63 弾の明示診断が **enum-in-enum ctor の refine 漏れ**という実バグを表面化させ、 修正。 `Result.ok(Maybe.nope)` を `Result<Maybe<Box>, string>` に対して構築すると、 内側 `Maybe.nope` の T が refine されず (`refine_enum_ctor_args` が ctor の**自分の型引数**は埋めるのに **payload 引数へ再帰していなかった**)、 第 63 以前は Type::Any クラッシュ・第 63 以降は「cannot infer」診断が出ていた (が型は宣言から確定可能)。 `refine_enum_ctor_args` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) で (1) ネスト Any を含む slot (`Maybe<Any>`) を target の具体 arg で置換、 (2) **payload 引数を置換後の payload 型へ再帰 refine** するよう拡張。 let 注釈・map 値・fn 戻り・深い入れ子 (`Result<Maybe<Maybe<Box>>>`)・heap ARC で動作。 詳細は下の解決済み記録。
- **第 63 弾** (ユーザー決定 = 第 62 弾の残る限界を明示診断に)。 **型引数を解決できない generic 呼び出し / enum ctor を、 lowering クラッシュでなく checker の明示診断にした**。 これまで `match f(Result.err("e")) { .. }` (bare scrutinee) や `let r = Result.ok(5)` (E 未決定) は型引数が `Any` のまま lowering に届き「Type::Any (variadic builtins)」で停止していた。 `check()` ([check.rs](../crates/ilang-types/src/checker/check.rs)) の末尾に `report_unresolved_type_args` を追加し、 `enum_ctor_type_args` / `fn_call_type_args` の stash で **型引数に `Type::Any` が残るエントリ**を span 付きで報告 (「cannot infer the type parameter(s) of ... — add a type annotation」)。 `TypeVar` (generic テンプレート本体) は除外。 **挙動変更**: 従来コンパイルが通っていた「未呼び出し fn 内の曖昧 ctor」も明示エラーになる (`let r = Result.ok(5)` は到達性に関わらず曖昧 = Rust の `let r = Ok(5)` と同じく注釈必須)。 既存 fixture は曖昧 ctor に依存しておらず全緑。 詳細は下の解決済み記録。
- **第 62 弾** (第 60 弾の確認済み記録 (2) を大部分解消)。 **型を決める引数を持たない generic fn 呼び出し** (`f(Result.err("e"))`、 `fn f<T>(r: Result<T,string>)`) を、 **期待型が分かる位置 (let 注釈 / fn 戻り / 引数) で解ける**ようにした。 第 59 弾の `refine_fn_call_type_args` は fn の T を期待型から解いていたが、 **インライン引数 `Result.err("e")` 自身の stash が `[Any, string]` のまま**残り (まだ generic な param に対して check されたため) 引数が Type::Any で lower 失敗していた。 `refine_fn_call_type_args` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) に、 T を解いた後**各引数を具体化した param 型で再 `refine_enum_ctor_args`** する処理を追加 (引数は fn の T を共有するため)。 let 注釈・return・引数位置で動作、 heap T ARC 健全。 詳細は下の解決済み記録。 **残る限界**: 期待型が皆無の bare match scrutinee (`match f(Result.err("e")) { .. }`) は依然 Type::Any (本質的に曖昧・注釈が必要)。
- **第 61 弾** (第 60 弾の確認済み記録 (1) を解消)。 **match の arm が enum を yield し注釈なし let に束縛すると arm ctor が refine されず Type::Any** になる既存バグを修正 (非 generic でも再現)。 `let res = match r { ok(v) { Result.ok(v) } err(e) { Result.err(e) } }` は join で `Result<i64,string>` に解決するが、 各 arm の `Result.ok(v)` (E=Any) / `Result.err(e)` (T=Any) は片方しか pin せず、 `res` に注釈が無いため refine されなかった。 `check_match_expr` ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)) / `check_match_optional` / `check_match_primitive` ([match_.rs](../crates/ilang-types/src/checker/match_.rs)) で join 後の結果型を各 arm に push する `refine_match_arm_ctors` を追加。 enum / Optional / primitive scrutinee の全 3 種に適用。 heap T ARC も健全。 詳細は下の解決済み記録。 **確認済み記録の (2)** (T を決める引数なしの generic 呼び出し) は未対応のまま。
- **第 60 弾**。 `?` を generic fn 内で使う形 (`fn unwrapOr<T>(r: Result<T,string>, fallback: T) { let v = r?; Result.ok(v) }`) を probe して、 **generic fn の型引数推論が引数の最初の binding を優先し Any を残す**既存バグを検出・修正。 `unwrapOr(Result.err("boom"), 0)` は arg1 `Result.err` が T=Any を入れ、 arg2 `0: T` の i64 で上書きされず、 fn が `<Any>` で具体化され monomorphizer が Type::Any で停止 (`?` 非依存。 `pick<T>(a: Result<T,string>, b: T)` でも再現)。 原因: `collect_type_var_bindings` ([sigs.rs](../crates/ilang-types/src/checker/sigs.rs)) が `or_insert_with` で最初の binding を優先。 **具体型が既存の Any を上書きする** (具体同士は従来どおり最初優先) ように修正。 詳細は下の解決済み記録。 **2 件の別系統の既存バグを記録**: (1) `let res = match someResult { ok(v){Result.ok(v)} err(e){Result.err(e)} }; res` (match arm が enum を yield・let に注釈なし) は arm ctor の型引数が refine されず Type::Any (非 generic でも再現。 match の join 結果型を各 arm に refine する必要)。 (2) generic fn を T を決める引数なしで呼ぶ (`f(Result.err("e"))` only) と `<Any>` 具体化で失敗 (match 文脈からの双方向推論が要る)。
- **第 59 弾** (第 57 弾の判断待ち記録 (2) を解消)。 **generic fn の型パラメータを戻り値位置から推論できる**ようにした。 `fn makeArr<T>(): T[]` / `fn wrapErr<T>(): Result<T,string>` など T が戻り型にしか現れない fn は引数から T を決められず Any のままだった (`let xs: i64[] = makeArr()` が「expected i64[], got any[]」、 enum 系は monomorphizer で Type::Any)。 `refine_fn_call_type_args(call, target)` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) を新設し、 fn の宣言戻り型を期待型に対して unify して残りの型パラメータを解き、 stash (`fn_call_type_args`) を更新して補正後の戻り型を返す。 期待型が分かる 3 位置 — let 注釈 ([stmt.rs](../crates/ilang-types/src/checker/stmt.rs))・fn 戻り位置 ([decls.rs](../crates/ilang-types/src/checker/decls.rs))・呼び出し引数 ([method.rs](../crates/ilang-types/src/checker/method.rs)) — で適用。 部分推論 (片方を引数・片方を注釈) ・heap T の ARC も健全。 詳細は下の解決済み記録。 **これで第 57 弾の判断待ち記録 2 件は両方解消。**
- **第 58 弾** (第 57 弾の判断待ち記録 (1) を解消)。 **generic class メソッドが generic enum を構築すると「unknown enum 〜」で lower 失敗**する既存バグを修正。 class 単一化 ([class.rs](../crates/ilang-mir/src/monomorphize/class.rs)) の `subst_expr` は specialized method body の型を置換するが **enum ctor の `enum_name` を再 mangle しなかった**ため、 `class Wrap<T> { asSome(): Maybe<T> { Maybe.some(this.v) } }` を `Wrap<i64>` で使うと `Maybe.some` が bare のまま (builtin Result も同症状)。 fn 経路 (fns.rs) は再 mangle するが class 経路に無かった。 checker の `enum_ctor_type_args` を thread-local 経由で class pass に渡し、 `subst_expr` が span 記録の型引数を class の T→具体で置換して mangle (fn 経路と同形)。 builtin Result・user enum・入れ子 (`Result.ok(Maybe.some(this.v))`)・複数インスタンス化・match 消費・heap T payload ARC を網羅。 詳細は下の解決済み記録。 **判断待ち記録の (2)** (generic fn 戻り型のみからの型推論) は未対応のまま。
- **第 57 弾**。 generic fn × enum ctor の継ぎ目を突き、 **generic fn が builtin `Result` を構築/返却すると「unknown enum Result」で lower 失敗**する既存バグを検出・修正。 `Result<T,E>` は宣言が無く call site ごとに monomorphize されるが、 `monomorphize_fns` は specialize 時に generic-enum の `EnumCtor.enum_name` を mangle する判定材料 (`generic_enums`) を**プログラム宣言の enum だけ**から作っていたため builtin Result が漏れ、 `fn wrapOk<T>(x: T): Result<T, string> { Result.ok(x) }` の `Result.ok` が `enum_name="Result"` のまま残った (user 宣言の `Maybe<T>` は含まれるので動く)。 `monomorphize_fns` の `generic_enums` に Result テンプレートを seed して修正 (monomorphize_enums パスと同じ対処)。 型パラメータが引数で決まる形 (ok arg / err arg / 両腕) で動作・heap payload ARC 厳密。 詳細は下の解決済み記録。 **2 件の別系統の既存バグを記録** (本弾では未対応): (1) generic **class メソッド**が generic enum (user の `Maybe<T>` も builtin `Result` も) を構築すると「unknown enum 〜」 — class 単一化経路が method body の enum ctor を mangle しない別バグ。 (2) generic fn の型パラメータを**戻り値注釈のみ**から推論できない (`let xs: i64[] = makeArr<T>()` も `Maybe`/`Result` も同様) — generic-fn 戻り型推論一般の制約。
- **第 56 弾**。 `?` 演算子 × enum ctor 型引数精緻化の継ぎ目を突き、 **`?` が呼び出し引数の中に入れ子になると Type::Any で lower 失敗**する既存バグを検出・修正。 `?` は `err(e) { return Result.err(e) }` (T=Any, E=string) を含むブロックに desugar されるが、 `Result.ok(take(g(ok)?))` のように `?` が tail 式の奥 (call 引数 / some / tuple / array 要素) に埋まると、 その `return Result.err` が refine されず T=Any のまま monomorphizer に届いていた。 原因: `refine_enum_ctor_args_in_block` が**文**の値は `refine_returns` で深く歩くのに、 **tail** は自身の値 ctor しか refine していなかった。 tail にも `refine_returns` を適用して修正。 `let b = g()?` (自前の文) は元から動いていた。 詳細は下の解決済み記録。
- **第 55 弾** (ユーザー決定 = 第 54 弾の別件に対し「`{}` でも書けるように実装」)。 空マップリテラル `{}` を **型注釈が `Map<K,V>` の位置で空マップとして解釈**するようにした (JS 風の利便性。 `new Map<K,V>()` は従来どおり)。 パーサは `{}` を空ブロックとして出すので型情報の要る checker + lowering で対応: checker `value_assignable` が空ブロック (stmt/tail なし) を Map ターゲットへ受理、 lowering `lower_composite_with_hint` が `(Block 空, MirTy::Map)` を空 `NewMap` に、 `is_fresh_object_expr` が空ブロックを fresh 扱い (scope-exit 解放のため。 unit 文脈では no-op)。 let / 再代入 / field 代入 / fn 戻り値 / fn 引数 / ネスト map 値で動作、 unit 文脈の空ブロックは不変。 詳細は下の解決済み記録。
- **第 54 弾**。 第 48〜50 弾の enum ctor 型引数精緻化の **残る store 位置 3 系統**を検出・修正。 (1) **enum payload 引数** (`Wrapper.wrap(Result.err(..))` tuple / `WrapperS.wrapS { r: Result.err(..) }` struct) — `check_enum_ctor` が自分の引数を payload 型へ refine していなかった。 (2) **built-in 配列メソッド引数** (`xs.push(Result.err(..))` / unshift / fill / remove / indexOf / includes) — ハードコード経路が element 型へ refine していなかった (Map.set / Set.add は check_args 経由で既に健全)。 (3) **index 代入** (`xs[0] = Result.err(..)` / `m["k"] = Result.err(..)`) — `check_assign_index` が element / value 型へ refine していなかった。 いずれも `Result<_, string>` 等の T=Any が monomorphizer に届き「Type::Any (variadic builtins)」で停止していた既存バグ。 詳細は下の解決済み記録。 **別件の既存ギャップを記録**: 空マップリテラル `{}` は型注釈に対して `()` (空ブロック) と推論され `let m: Map<K,V> = {}` が型エラー — パーサが `{}` を空ブロックとして出すため。 (※当初「空マップの構文が無い」と記したのは誤り。 `new Map<K,V>()` が既存の確立した形。 `{}` でも書けるようにする利便性として第 55 弾で対応。)
- **第 53 弾** (クリーンラウンド)。 第 52 弾の slot 昇格修正の周辺と Map forEach の反復中 mutation を網羅 probe — **新規バグなし**。 メソッド内/accessor 内クロージャ・init・static メソッド・default 引数式からのグローバル参照は全て昇格される。 forEach の add-during / future-key-delete (snapshot が +1 で生存させ UAF なし) / nested forEach は deinit 厳密 + delta=0。 forEach mutation の ARC を pin。 詳細は下の確認済み記録。
- **第 52 弾**。 property accessor を probe して、 **getter/setter 本体から top-level let (グローバル) を参照すると「unbound variable」で lower 失敗**する既存バグを検出・修正。 slot 昇格判定 (`build_slot_table` → `collect_fn_free_var_refs`) がクラスの method / static method は走査するのに **property accessor body を走査していなかった**ため、 getter/setter だけが参照する let が host slot に昇格されなかった (method は動くのに accessor は不可)。 `collect_fn_free_var_refs` の `walk_class` に getter/setter body 走査を追加。 詳細は下の解決済み記録。
- **第 51 弾**。 static フィールドを probe して、 **heap (string) static フィールド代入が値を retain せず use-after-free**する既存バグを検出・修正。 `Cls.s = arg` (借用 param に fresh string) が static slot を所有せず、 源の transient +1 解放後に slot が dangling → 後続 read が解放済みバッファ (fresh string が空印字)。 `StoreStatic` に instance field 同様の retain-new (非 fresh) / release-old を追加。 詳細は下の解決済み記録。
- **第 50 弾**。 第 48/49 弾の enum ctor 型引数精緻化の **最後の穴 = クロージャ本体の戻り値**を是正。 top-level fn の戻り値は refine していたが、 `fn(): Result<i64,string> { Result.err(..) }` や `array.map(fn -> Result<...>)` のクロージャ tail / 早期 return が未配線で Type::Any だった。 `check_fn_expr` に `refine_enum_ctor_args_in_block` を追加。 詳細は下の解決済み記録。
- **第 49 弾**。 第 48 弾の enum ctor 型引数精緻化の **残る store 位置**を是正。 引数位置 (fn / method / init)・`some(..)`・tuple 要素でも `Result.err(..)` 等が Type::Any で失敗していた。 `refine_enum_ctor_args` を some / tuple / array へ**再帰**させ (入れ子も伝播)、 call-arg checker (`check_args` / generic fn arg / fn 型 call) に refine を追加。 詳細は下の解決済み記録。
- **第 48 弾**。 Result `?` を probe する過程で、 **型パラメータを引数から推論できない enum コンストラクタ (`Result.err(..)` は T、 `Maybe.nope` は両方) を field / 配列リテラル / Map 値に格納すると Type::Any で lower 失敗**する既存バグを検出・修正。 `let f: T = ctor` は注釈で enum ctor の型引数を精緻化 (`refine_enum_ctor_args`) するのに、 field 代入 (明示 + bare)・配列リテラル要素・Map 値・local 再代入は精緻化していなかった。 結果 2 型パラメータ enum (Result / Either) を field/配列に置けなかった。 5 箇所に refine を追加。 詳細は下の解決済み記録。
- **第 47 弾** (クリーンラウンド)。 第 46 弾で開いた interface 周辺の **covariance と downcast** を網羅 probe — **新規バグなし**。 interface 実装クラスを Optional wrap / enum payload / Map 値 / 入れ子コンテナ / generic 型引数 / tuple 要素 (wrap 込み) に流し込む covariance、 `as?` による interface→具象 downcast (成功=共有・失敗=none) を deinit 厳密 + delta=0 で確認。 subclass の wrap/covariance/downcast 機構が interface にも generalize。 詳細は下の確認済み記録。
- **第 46 弾**。 interface dispatch を probe して、 **異なる interface 実装クラスの if/else・match・配列リテラルが共通 interface に合流できない**型推論バグを検出・修正 (ユーザー判断 = 「あるべき形に修正」)。 分岐 join が `common_ancestor` (クラス階層) しか見ず、 共通 interface を join 先にしていなかった (subclass は動くのに interface は不可)。 `common_object_join` を新設し全 object-join 箇所 (if / match / 配列 / Map リテラル) を切替。 唯一の共通 interface に合流、 複数共通 interface は曖昧として型エラー。 詳細は下の解決済み記録。
- **第 45 弾** (クリーンラウンド)。 weak 参照と固定長配列 × wrap を probe — **新規バグなし**。 weak.get() の昇格 (生存=値 / 死後=none)・weak? back-ref サイクル・parent-owns-child cascade (二重解放なし)・weak 配列、 および **固定長配列 `T[N]` の要素 wrap** (`Box?[2]` リテラル/index store、 `(Box?, Box)[2]` への tuple index store) を deinit 厳密 + delta=0 で網羅。 wrap 修正 (第 36/41) が固定長表現にも generalize していることを確認。 詳細は下の確認済み記録。
- **第 44 弾** (クリーンラウンド)。 第 43 弾の周辺を string/array ARC 全方位で probe — **新規バグなし**。 string メソッド連鎖の fresh 中間・template literal の heap 補間・`+=` desugar・**self-concat `s = s + s`** (aliased rhs を解放しない正しい挙動)・split・array push/unshift/map の fresh 要素・heap-kind 変数の fresh 再代入を、 `liveStringCount` / deinit 厳密で網羅。 string-ARC 形を pin。 詳細は下の確認済み記録。
- **第 43 弾**。 string バッファ ARC を probe して、 **inplace concat `s = s + n.toString()` が fresh な rhs 文字列を 1/iter リーク**する既存バグを検出・修正。 `StrConcatInplace` は rhs を `s` のバッファに**コピー**するだけで消費しないため、 fresh rhs (`toString()` / fresh concat) の +1 が宙に浮く。 リテラル rhs (intern 済み) や借用 var rhs は無事。 op 後に fresh rhs を Release。 詳細は下の解決済み記録。
- **第 42 弾** (クリーンラウンド)。 composite 要素 wrap の **残る store サイト**を網羅 probe — **新規バグなし**。 return 位置・`some(tuple)`・enum payload・引数位置・local 再代入・明示 field 代入・入れ子 `((Box?, Box), Box?)`・tuple 内 weak 要素を、 Optional 越し match で実体確認しつつ deinit 厳密 + delta=0。 第 36 (bare field) / 第 41 (index store) で未 pin の位置を 1 本に pin。 詳細は下の確認済み記録。
- **第 41 弾**。 tuple をコンテナに格納する ARC を probe して、 **index 代入 `coll[i] = (box, b)` が tuple 要素の wrap を欠く** 既存 SIGSEGV を検出・修正。 `arr[0] = (box, b)` (`(Box?, Box)[]`) / `m["k"] = (box, b)` (`Map<_, (Box?, Box)>`) が slot0 を wrap せず生 Box を `Box?` slot に格納 → 解放時クラッシュ (リテラル `[(box,b)]` は元から hint 済みで無事)。 `AssignIndex` の RHS を要素型ヒント付き lowering に変更 (第 36 弾と同型)。 詳細は下の解決済み記録。
- **第 40 弾**。 第 39 弾の tuple subst 修正の **同族探索が不完全**だったのを是正。 型パラメータ置換 (`subst_type`) だけでなく、 **concrete な generic instantiation を mangle する rewrite 群**も tuple を見落としていた: `rewrite_type` (generic class)・`rewrite_enum_refs_in_type` (generic enum)・`walk_types_pre` (instantiation 発見)。 `(Inner<i64>, i64)` / `(Maybe<i64>, i64)` を field / param / return に使うと「unsupported in M1: user-defined generic types」で停止。 3 関数に `Type::Tuple` arm を追加。 詳細は下の解決済み記録。
- **第 39 弾**。 generic + heap を probe して **monomorphize が tuple 型の中の型パラメータを置換しない**既存バグを検出・修正。 `(T, T)` / `(T?, T)` を generic fn/method のシグネチャや field 型に使うと「unknown type: T」で lowering 停止 (Optional / array / Map は置換されるのに tuple だけ漏れ)。 `subst_type` / `contains_type_var` に `Type::Tuple` arm を追加。 詳細は下の解決済み記録。
- **第 38 弾** (クリーンラウンド)。 第 37 弾の exit-drain 修正の周辺を async 全方位で probe — **新規バグなし**。 timer (setTimeout) at exit・Promise.all over heap・Promise を field 保持・multi-await chain・never-settled promise の executor heap capture・**await rejection が await 前の heap local を解放**・reject 経路の shutdown drain を網羅し、 全て deinit 厳密 + leak なし + clean exit。 await-rejection ARC と reject 経路 drain を pin。 詳細は下の確認済み記録。
- **第 37 弾**。 async へ攻撃面を移し、 **exit 時の event-loop drain がグローバル解放の後に走る順序バグ**を検出・修正。 pending promise 継続が保持する Box の `deinit` が top-level 配列 `deinits[0]` に触ると、 シャットダウンで解放済みグローバルを参照して **ランタイム OOB panic** (`deinit` 持ちの heap を await 跨ぎで保持する最小形で再現)。 drain を `__main` の top-level let 解放の **前**に emit して修正。 詳細は下の解決済み記録。
- **第 36 弾**。 bare field 代入の **composite リテラル要素 wrap 欠落** という既存 SIGSEGV を検出・修正。 `pair = (box, b)` (field 型 `(Box?, Box)`) が tuple を `(Box, Box)` として構築し生 Box を `Box?` slot に格納 → 解放時に不整列ポインタ参照でクラッシュ。 bare 経路がヒント無し lowering を再利用していたのが原因 (第 33 弾の設計の取りこぼし)。 field 型を composite ヒントに渡して修正 (array `Box?[]` / map `Map<_,Box?>` の同族も同時に解消)。 詳細は下の解決済み記録。
- **第 35 弾** (クリーンラウンド)。 bare field 書き (第 32 弾) × 連鎖レシーバ解放 (第 29/30 弾) の継ぎ目を反復ミューテーション・多段連鎖で攻めた — **新規バグなし**。 init 内 bare 再代入の `is_init` 安全性 (checker が初回 `this.` を要求して担保)、 fixed-array bare 代入の copyShallow、 4 段連鎖・field 返し連鎖、 **エスケープしたステートフルクロージャの heap field 反復付け替え** (deinit 700 厳密) を pin。 詳細は下の確認済み記録。
- **第 34 弾** (クリーンラウンド)。 第 33 弾で統合した `store_value_to_field` の継ぎ目を「bare field 代入 × 宣言型が要る RHS」で攻めた — **新規バグなし**。 共変 map / 配列リテラル・fresh Optional・Optional widen の bare 代入を厳密 deinit + churn delta=0 で pin。 併せて **第 33 弾の誤記録を訂正** (「weak の bare 代入は明示と非対称」は誤り。 実体は `none → plain Box.weak` の拒否で bare/明示共通・意図的制限)。 詳細は下の確認済み記録。
- **第 33 弾**。 第 32 弾で触った **implicit bare-name field 代入** arm が、明示 `this.f = v` (AssignField) と違って **`T → T?` / subtype の Optional 自動 wrap を欠く** 既存バグを検出・修正。 `slot = box` (field 型 `Box?`) が生オブジェクトを Optional slot に格納し解放時に **SIGSEGV**。 両経路を共通ヘルパー `store_value_to_field` に統合。 詳細は下の解決済み記録。
- **第 32 弾**。 第 31 弾のクロージャ `this` 捕獲の継ぎ目を攻めて **コンパイラ panic 2 件** を検出・対処。 (1) メソッド内クロージャが **bare field に代入** (`slot = nb`) すると lowering が panic (修正)。 (2) クロージャ内の **bare メソッド呼び出し** (`compute()`) が panic → クリーンな診断に (ユーザー決定)。 詳細は下の解決済み記録。
- **Promise/async 実行モデルを JS 型 (run-to-completion・シングルスレッド) へ移行**。 worker pool を撤去し、 継続はメインスレッドの FIFO queue + 期限順 timer heap (`pool.rs` 書き換え)、 executor は構築時に同期実行、 非ブロッキング pump の **`time.tick()`** を新設。 詳細は下の解決済み記録。
- **gui ライブラリの platform イベントループに `time.tick()` pump を組み込み** (cocoa = NSTimer common modes / win32 = SetTimer TIMERPROC / linux = g_timeout_add、 各 ~15ms)。 GUI 表示中も std.time タイマーと Promise 継続が発火する。 **win32 / linux は macOS 環境では型検査されないため未検証** — 詳細は下の解決済み記録。
- **fixture 増殖ラウンド第 8 弾** (新実行モデル周辺)。 `new Promise(executor)` の 3 セル leak (移行前から) と、 release ビルドの関数マージ (ICF) による capture 二重登録 → promise 二重解放 (leak 修正で顕在化した潜在バグ) を検出・修正。 回帰 fixture 4 件、 `ILANG_DEBUG_PROMISE/CLOSURE/TIMER` トレースを常設化。 詳細は下の解決済み記録。
- **fixture 増殖ラウンド第 9 弾**。 float 型 promise の executor 経路の ABI 不一致 (garbage / SIGSEGV、 既存)、 fresh 引数 post-release の残り 2 経路 (クロージャ間接呼び出し / 暗黙 `this.method`、 既存) + promise `.then`/`.catch` の fresh receiver release 欠落 (既存)、 armed timer を残した panic 終了の TLS 破棄順 abort (新モデル起因) を検出・修正。 回帰 fixture 4 件。 詳細は下の解決済み記録。
- **fixture 増殖ラウンド第 10 弾**。 `Promise.all`/`race` の入力配列未消費 (既存)、 all 結果配列の要素所有権欠落 (1 の修正で UAF 顕在化)、 fresh promise 引数の release 欠落 (5 サイトを共通述語 `fresh_arg_needs_post_release` に一本化) を検出・修正。 回帰 fixture 3 件。 REPL が `use` 文未対応という独立の既存制限も記録。 詳細は下の解決済み記録。
- **第 11 弾**。 (1) **await の rejection propagate を実装** (ユーザー決定 = JS 意味論: await 先が reject したら async fn の結果 promise も同じ msg で reject。 旧挙動は無言で永久 pending)。 (2) **早期 `return` が生きている heap 束縛を release しない一般バグを修正** (`lower_return` に sweep が無かった — fn 直下 / match arm / loop body / async poll fn 全部で 1 個/call leak)。 回帰 fixture 2 件。 詳細は下の解決済み記録。
- **第 12 弾**。 `continue` の sweep 欠落 (既存)、 fresh match scrutinee が diverging arm で leak (既存)、 string match の fresh scrutinee 全面未解放 (既存)、 async fn 本体の早期 `return` の実装 (旧: 型エラーで不支持)、 引数なし fn で第 11 弾 sweep が空振りする取りこぼし (deinit 不発で発見) を修正。 回帰 fixture 2 件。 詳細は下の解決済み記録。
- **第 13 弾**。 for-in × 早期 return の両方向: fresh iterable の leak (既存) と、 **第 11 弾 sweep が入れた要素借用の過剰解放 (UAF) の退行**を修正。 要素束縛は `PatternBinding` (借用) へ再分類、 `env.bind` 全箇所を監査済み。 回帰 fixture 1 件。 詳細は下の解決済み記録。
- **第 14 弾**。 deinit カウントを過剰解放検出器に使い、 早期脱出 × 値の持ち出し (payload の return/break・closure escape・入れ子 for-in 貫通・async 内制御構造) を網羅 — **新規バグなし**。 pin 用 fixture 1 件。 詳細は下の確認済み記録。
- **第 15 弾**。 REPL のパイプラインを loader 相当の normalize 経路 + fresh TypeChecker + slot 型の monomorphize 注入に乗せ替え — enum / async fn / const / generic 型 let の chunk 跨ぎを修復、 `use` は明示診断に。 `?` 演算子は実は Result 用に実装済みで健全 (HANDOFF の未実装リストが古かった)。 未知属性の素通り・turbofish 誤パース・`?` on Optional の誤診断を記録。 repl.rs +6 件。 詳細は下の解決済み記録。
- **第 16 弾**。 REPL の**型違い re-let が無言の型穴** (string ポインタ生値の印字) になっていたのを明示エラーで封鎖。 fn / class の再定義セマンティクス (現状: 拒否 / 生エラーで旧定義が残る) は設計判断待ちとして記録。 repl.rs +3 件。 詳細は下の解決済み記録。
- **第 17 弾**。 weak × 早期 return・property accessor 本体・fs/path/Unicode・interface 配列 × for-in × return・repr/@flags enum を網羅 probe — **新規バグなし** (クリーンラウンド 2 回目)。 pin 用 fixture 1 件。 詳細は下の確認済み記録。
- **第 18 弾**。 **固定長配列 `T[N]` × heap 要素の ARC 未モデル** (codegen が自認していたギャップ) が実害として確認された: scope exit / 早期脱出 / field drop / field 上書きで要素が漏れる (store の rc と escape は正常 — fixture で pin)。 詳細は下の解決済み記録 (第 19 弾で解決)。
- **配列リテラルの推論規則を変更 (ユーザー決定)**: 注釈なしの `let xs = [...]` は**動的配列** `T[]`、固定長 `T[N]` は宣言型 (注釈・field・引数) が要求した時だけ。旧規則 (無注釈リテラル = `i64[3]` 固定長、push には `i64[]` 注釈が必要) を廃止。`literal_assignable` が式の長さを直接見るため固定長注釈への代入は無変更で通る。これに伴い generic 型引数への配列リテラル直渡し (`hold([new Box(1), new Box(2)])`) は T=Box[] に束縛され自然に通るようになり、式レベル検査のリテラル除外ハックを削除。fixture: `01_basics/array_literal_dynamic_inference.il`。
- **第 31 弾**。 第 30 弾で記録した **capturing closure を class field に持つとリーク** するバグを修正。 原因は closure の自由変数収集 ([collect.rs](../crates/ilang-mir/src/lower/collect.rs)) が **bare `Var` ごとに `this` を投機的に候補追加**していたこと (「その名前が field なら `this.field` だから」)。 フィルタは「`this` が env で解決可能なら残す」だけなので、 メソッド本体内のクロージャでは `this` が常に解決可能 → **plain local / param しか参照しないクロージャまで `this` を捕獲**。 `this.f = fn(){ new Box(x) }` (x は param) が `this → closure → this` のサイクルを作り、 オブジェクトごとリークしていた。 修正: 投機的 `this` 追加を撤廃し、 [fn_expr.rs](../crates/ilang-mir/src/lower/fn_expr.rs) で **free var が enclosing class の実メンバ (field / property、 親チェーン込み) に解決された時だけ on-demand で `this` を捕獲** (`name_is_this_member`)。 明示 `this.` アクセスは従来どおり `frees` 経由で捕獲。 method は対象外 (bare メソッド呼び出し `compute()` は元から captured-this 経由で解決されず、 含めると新規 panic を生む — `this.compute()` を使う既存制限のまま)。 診断は MIR dump で `make_closure func#3 (v1, v0)` の v0=this が未使用 capture と判明、 HEAP_TRACE で「オブジェクト+closure が共に free されない」ことを確認。 bare field 参照クロージャ (genuine な `this` 捕獲) は維持 — それを field に格納すれば真の ARC サイクル (設計上のリーク、 weak で断つ) になる点も probe 済み。 fixture: `05_edge_cases/closure_field_no_spurious_this.il` (param/local 捕獲・build-only・bare-field escape を厳密 deinit 300 + churn delta=0)。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 31 弾の回帰修正**。 第 31 弾の on-demand `this` 捕獲が `lookup_var` だけで `this` を解決していたため、 **nested closure (内側 closure が field を bare 参照)** が panic していた (内側の `this` は外側 closure の capture 経由で local ではない)。 修正: needs_this を後段で個別 capture するのをやめ、 **`this` を `frees` に追加して通常の free-var 解決 (local も `captures_in_scope` forward も両対応) に通す** 方式へ ([fn_expr.rs](../crates/ilang-mir/src/lower/fn_expr.rs))。 direct method body (local this) と nested closure (forwarded this) の両方が動作。 fixture: `05_edge_cases/nested_closure_field_capture.il` (2 段ネスト + 明示 this を厳密 deinit 200 + churn delta=0)。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 30 弾**。 第 29 弾で直した連鎖メソッドの fresh-receiver リークの **同族 2 経路** (interface dispatch / closure-field call) に同じガードが残っていたのを検出・修正。 `lower_iface_dispatch` と `lower_fn_field_call` ([object.rs](../crates/ilang-mir/src/lower/calls/object.rs)) が共に `obj_is_fresh && !matches!(ret, Object)` で、 `mkHolder().grab().n` (interface 経由) と `mkObj().fnField().n` (closure-field 経由) の中間 receiver をリークしていた。 修正: Object 戻り値でも fresh receiver を解放。 interface VirtCall は ilang クラスにしか届かない (COM は別経路 `lower_com_iface_dispatch`、 @objc は objc_msgSend) ため @objc 除外不要、 closure-field call は戻り値が closure の所有戻り値で receiver 別名ではないため同じく不要。 static メソッド連鎖 (`Factory.make().grab()`) は第 29 弾の通常メソッド経路を通り元から健全 (回帰ガードとして fixture に保持)。 fixture: `05_edge_cases/chained_receiver_iface_fnfield.il`。 検証: nextest 539/539 (cocoa 含む)、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 **別系統の既存バグを記録** (背景タスク化): **capturing closure を class field に持つとオブジェクト解放時に closure block がリーク** (`class F { f: fn(): Box; init(x){ this.f = fn(): Box { new Box(x) } } }` をローカルに作るだけで 56 bytes/round 漏れる。 非 capturing closure field は健全)。 連鎖修正とは独立、 closure-capture 系の解放漏れ。
- **第 29 弾**。 **連鎖メソッド呼び出し `a.m1().m2()` で、m1 が Object を返す時に中間 receiver がリークする** 一般バグを検出・修正。 当初 generic クラス (`Cell<Cell<Box>>`) の leak として浮上したが、 切り分けで **generic 非依存**と判明 — 非 generic の `o.get().get().n` でも漏れる。 原因: object メソッド lowering の fresh-receiver 解放 ([object.rs](../crates/ilang-mir/src/lower/calls/object.rs)) が **戻り値が非 Object の時だけ** Release を出していた (`!matches!(sig.ret, Object)`)。 「フィールド別名を返す場合に過剰解放しないため」の保守的ガードだったが、 ilang メソッドの heap 戻り値は常に所有 (+1 — tail-borrow / bare-`this` retain) なので、 返り値は receiver の drop cascade を生き残る。 結果 `obj.getThing().use()` ・ builder `mk().bump().bump()` ・ 入れ子 generic の `cc.get().get()` がすべて中間値をリークしていた (`let m = a.m1(); m.m2()` の名前付き中間は scope-exit 解放で無事)。 修正: Object 戻り値でも fresh receiver を解放、 **ただし `@objc` receiver は除外** (ObjC の Object 戻り値は autorelease の借用で +1 所有ではない — `handle: i64` フィールドの有無で判定、 `lower_field` の receiver-temp 判定と同じ)。 診断は HEAP_TRACE + `__release_object` の rc トレースで「中間 Cell が rc=1 で残る」ことを確認して確定。 fixture: `05_edge_cases/chained_method_receiver_release.il` (field 連鎖・builder・入れ子 generic・名前付き中間ガードを厳密 deinit 500 + churn delta=0)。 検証: nextest 539/539 (cocoa_foundation/appkit 含む — ObjC 除外が効いている)、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 28 弾**。 第 26 弾の wrap 拡張が **狭すぎた** ことによる既存バグを probe で検出・修正。 第 26 弾は `T → T?` 自動 wrap を「plain `Object → Optional<Object>`」に限定したため、 **object 形だが Object でない subtype 源** — 特に `Dog[] → Animal[]?` (`Array<Dog> → Optional<Array<Animal>>`) — が抜け、 `let m: Map<string, Animal[]?> = {"a": [new Dog()]}` が lower 時「no coercion from obj[] to obj[]?」で停止 (checker はリテラル covariance で通すのに)。 store 位置なら第 26 弾と同じく crash 系統。 修正: wrap 述語を 3 箇所 (`coerce` [coerce.rs](../crates/ilang-mir/src/lower/coerce.rs)・`release_owned_wrap_source` [body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs)・`AssignField` の `needs_optional_wrap` [expr.rs](../crates/ilang-mir/src/lower/expr.rs)) で「inner も from も object 形 (Object/Array/Tuple/Map/Optional)・ただし from 自身は `Optional<_>` でない」へ拡張 (Optional 源は Optional→Optional widen のため除外し、 `none` / `Box??` を不変に保つ)。 同族確認: map-of-arrays・nested map・array-of-maps・`Animal[]?` wrap を let/field/arg/return で値・ARC 厳密一致 (deinit 900 + churn delta=0)、 none/混在/`Box??` の回帰なし、 第 25〜27 弾の probe 群も全て再確認。 fixture: `09_subtyping/nested_container_covariance.il`。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 27 弾**。 **map リテラルが宣言型に対して covariant でない** checker の不整合を修正 (ユーザー要望 = 「本来こうあるべき形」)。 `let m: Map<string, Animal> = {"a": new Dog()}` は配列の `[new Dog()]: Animal[]` が通るのに checker が「got Map<string, Dog>」で拒否していた (lowering には第22弾の `lower_map_literal_with_hint` があり宣言 K/V で構築できるのに、 mod.rs の `literal_assignable_with` が **MapLit を意図的に除外**し陳腐化したコメントを残していた)。 修正 2 点: (1) `literal_assignable_with` に MapLit arm を追加 ([mod.rs](../crates/ilang-types/src/checker/mod.rs)) — 各 key/value を宣言 K/V へ配列/タプルと同じ subtype + リテラル coerce で検査 (let / 引数 / return をカバー)。 (2) let 束縛が `Map<K,V>` 注釈付きリテラルを新設 `check_map_lit_with_hint` ([casts.rs](../crates/ilang-types/src/checker/expr/casts.rs)) 経由で検査 ([stmt.rs](../crates/ilang-types/src/checker/stmt.rs)、 配列の `check_array_with_hint` と対称) — `{"a": new Dog(), "b": none}` のような some(child)/none 混在を親型へ unify (推論経路は Dog と none を unify 不能)。 **covariance はリテラル限定で健全**: 別名 `Map<string, Dog>` 変数は依然 `Map<string, Animal>` へ代入不可 (親型越しの mutation が unsound)。 全位置 (let/arg/return) で値・ARC 厳密一致 (deinit 700 + churn delta=0)・Optional 越しの仮想ディスパッチ正常を確認。 fixture: `09_subtyping/map_literal_covariant_value.il` (homo/optWrap/mixed/arg/return を pin)、 `09_subtyping/map_literal_covariant_value_alias_error.il` (別名拒否を expect-error で pin)。 syntax.md / syntax_ja.md の暗黙変換表を更新。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 26 弾**。 **subclass を `Optional<Parent>` へ包む `T → T?` 自動 wrap が lowering で全位置未対応** だった既存バグを probe で検出・修正 (`inner == source` の**厳密一致**を 3 箇所が open-code していたため、 `Dog` を `Animal?` slot へ流すと wrap されなかった)。 症状: (1) `coerce` (let / 引数 / return / 再代入 / tuple リテラル / 配列リテラル) は checker が通すのに lower 時「no coercion from Dog to Animal」で停止。 (2) Map index 代入 / 配列 index 代入 / **field 代入** は生オブジェクトを Optional slot に格納し、 解放時に Optional kind で誤カスケードして不整列ポインタ参照で **SIGABRT**。 (3) `release_owned_wrap_source` も wrap を見落とし、 値が正しい経路でも fresh 源の +1 を leak。 修正: wrap 述語を 3 箇所 — `coerce` の Optional wrap arm ([coerce.rs](../crates/ilang-mir/src/lower/coerce.rs))・`release_owned_wrap_source` ([body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs))・`AssignField` の `needs_optional_wrap` ([expr.rs](../crates/ilang-mir/src/lower/expr.rs)) — で「plain `Object → Optional<Object>`」(subclass) を厳密一致に追加。 `Optional<_>` 源は Optional→Optional widen のため除外 (`none` / `Dog? → Animal?` は不変)。 同族探索: let / return / 再代入 / tuple / 配列リテラル / 配列 index / Map index / field / enum payload / borrowed 源 の全 11 位置で deinit 厳密一致・churn delta=0、 Optional 越しの仮想ディスパッチ (Dog.val() = n*10) も正しいことを確認。 fixture: `09_subtyping/subclass_optional_wrap.il` (11 位置 + none/widen 回帰ガードを厳密 deinit 1200 + byte delta=0 で pin)。 **別件として記録**: `let m: Map<string, Animal?> = {"a": new Dog()}` は **checker** が `Map<string, Dog>` を `Map<string, Animal?>` に推論できず明示拒否 (lowering 以前の型推論ギャップ。 crash/leak ではない — 対応するなら map リテラル値の subclass+Optional 統合)。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 25 弾**。 **`T → T?` / `T → T.weak` 暗黙 wrap coerce の所有元解放が「コンテナ store 位置」4 箇所で欠落** していた既存バグを probe で検出・修正 (第 20〜22 弾の同族で未踏だった配置)。 第 20〜22 弾は let / 引数 / return / 再代入 / field 代入 / Map リテラルを `release_owned_wrap_source` (または `lower_arg_to`) 経由に統一したが、 以下 4 つは独自に coerce + retain を open-code し wrap 源の +1 を落としていなかった: (1) **`m[k] = box`** (Map index 代入 against `Map<_, Box?>`) — **そもそも coerce 不在**で生 Box を Optional として格納 → `$map.release` が Optional kind で誤カスケード解放し不整列ポインタ参照で **SIGABRT** (最も深刻)。 (2) **`arr[i] = box`** (配列 index 代入 against `Box?[]`) — coerce はするが源 +1 を leak (fresh) / wrapper セルを二重計上 (borrowed)。 (3) **`[box]: Box?[]`** (配列リテラル要素)・(4) **`(box, _): (Box?, _)`** (タプルリテラル要素) — 同型 leak。 修正: 各 store サイトが wrap (`coerced != source`) を検出して aliased-element Retain の代わりに `release_owned_wrap_source` を呼ぶ ([expr.rs](../crates/ilang-mir/src/lower/expr.rs) AssignIndex の Array/Map arm、 [literals.rs](../crates/ilang-mir/src/lower/literals.rs) の `lower_array_literal_with_hint` / `lower_tuple_literal_with_hint`)。 Map arm は宣言 V へ coerce してから格納し、 host_map_set の retain 後に残る transient セルも解放。 同族探索で問題なし確認: `m.set(k, box)` (lower_arg_to 経由)・明示 `some(..)`・enum payload wrap・field 代入 (第 22 弾) は元から健全、 return が配列リテラルを返す形・入れ子 (tuple 内 `Box?[]`)・weak のコンテナリテラル/index 代入も修正経路で正しく解放。 fixture: `05_edge_cases/wrap_coerce_container_stores.il` (4 バグ + 健全経路の回帰ガードを厳密 deinit 1000 + churn delta=0 で pin)。 検証: nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。
- **第 24 弾**。 第 23 弾で入れた Map/Set 挿入順機構 (`order: Vec<i64>`、非所有ハンドル) の **ARC 次元**を厳密 deinit + churn delta=0 で網羅 — **新規バグなし** (クリーン 4 回目)。 probe: 文字列キー Map の上書き+delete+再挿入 (3 deinit/round)、 object キー Map (delete が key share を落とす・2/round)、 object 要素 Set の union/intersection/difference (eq 等価で別アロケーション混在 — survivor の二重 retain も早期解放も無し・4/round)、 forEach 反復中の現在キー delete (snapshot の +1 が値を callback 終了まで生存させる・3/round)、 Optional 値の上書き+delete カスケード、 文字列 Set union の順序+ARC。 全 probe で deinit 厳密一致・churn 100 周 delta=0。 挿入順意味論 (上書き=位置維持 / delete+再挿入=末尾) と第 23 弾で直したプロセス間決定性 (5 回実行で同一順序) も再確認。 fixture: `03_collections/map_set_order_arc_churn.il` (上記 4 形を厳密 deinit + byte delta=0 で pin)。 fixture 追加のみ (ソース変更なし) のため workspace 全体と nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認 (PASS、 byte delta=0 が両経路で成立)。
- **第 23 弾**。 breadth probe (entries 反復・@derive Set churn・weak 昇格・template×join・field 格納 closure・Promise.all・継承・脱出 closure) は **ARC 新規バグなし** (クリーン 3 回目、 churn 1300 まで正確)。 代わりに **Map / Set の反復順がプロセスごとに非決定** という言語仕様レベルの問題を検出 (同じプログラムで entries() の順序が実行ごとに変わる — Rust HashMap の per-process SipHash シード)。 **ユーザー決定: JS と同じ挿入順保証**。 `ManagedMap` / `ManagedSet` に非所有の挿入順リスト `order: Vec<i64>` を追加 (handle = Int は生値 / Str は初回挿入の orig ポインタ / Object は格納ポインタ)、 keys/values/entries/forEach/印字/集合演算 (union は第1集合→第2集合の順) を order 走査に統一。 上書きは位置維持・delete+再挿入は末尾 (JS 同様)。 印字の「文字列ソートで安定化」は挿入順表示に置換 (数値キーが `1, 2, 10` と自然に並ぶ)。 fixture: `03_collections/map_set_insertion_order.il` (Map/Set/object 要素/float 要素/集合演算)、 console_log_map.il と第 23 弾 breadth fixture の期待値を挿入順に更新。 order 機構の leak 無しは 200 周 churn (delta=0・deinit 1200 正確) で確認。 検証: nextest 539/539、 AOT PASS、 nested_generic 50 並列 0 fail。
- **第 22 弾**。 wrap coerce × セル格納の組合せを probe して **既存バグ 4 系統を検出・修正**: (1) `h.opt = new Box(1)` (field 型 `Box?`) — AssignField 自前の wrap coerce に所有元解放が無い (第 21 弾の 5 箇所に続く 6 箇所目。 strong→weak field も同時に修正)。 (2) `takeOpt(h.b)` — **借用ソース**の wrap が引数位置で作る fresh セルを post-release 判定が見ない (判定が元式の freshness のみ) — `last_arg_wrapped` フラグを `lower_arg_to` が立て、 5 つの呼び出し形 (fn / method / init / extern / variadic) の判定に OR。 (3) `let o: Box? = h.b` / ブロック式ソース — bind retain が元式の freshness で判定し、 **wrap が作った fresh セルを二重 retain** して leak (Assign も同型) — coerce が新値を作ったら fresh 扱いに。 (4) `let m: Map<string, Box?> = {"a": none, ...}` — **checker が通すのに lowering が落ちる不整合** (「no coercion from Map<string, ()?>」) — `lower_composite_with_hint` に MapLit arm が無かった。 `lower_map_literal_with_hint` 新設 (K/V を宣言型で構築、 none が V を採用、 wrap・nested hint・transient 解放込み)。 問題なし確認: property setter への wrap、 enum payload `Box?`、 `Box?[]` push、 return 位置の借用 wrap。 fixture: `05_edge_cases/wrap_cells_and_hinted_map.il`。 検証: nextest 539/539、 AOT PASS、 nested_generic 100 並列 0 fail。
- **第 21 弾**。 **`T → T?` / `T → T.weak` 包み coerce の所有元解放が let 以外の全位置で欠落** していたのを probe で検出・修正 (1 個/call leak): weak の join (`let w: Box.weak = if c { a } else { b }` — join 正規化が立てた強 +1 が weak 束縛に流れて行き場なし)・引数位置・代入・return 両経路 (末尾式 / 明示 `return`)。 修正は共通ヘルパー `release_owned_wrap_source` (所有判定 = fresh ∨ block tail retain 済み) を let / `lower_arg_to` / Assign / `lower_return` / `finalise_return` (tail_owned を呼び出し元 4 箇所から供給) に適用。 fixture: `05_edge_cases/wrap_coerce_fresh_release.il`。 **既知の制限を記録**: async fn 内の Result `?` は checker で型エラーになる (「expected Promise<Result<...>>, got Result<any, string>」という分かりにくい文言 — Optional の async 診断と違い専用メッセージなし。 対応するなら ? の async 対応一式とセット)。 join を return / break / 文破棄 / 入れ子 / field 代入 / tuple / 文字列で churn する probe は全て正確 (1300/449 まで一致)。 検証: nextest 539/539、 AOT PASS、 nested_generic 100 並列 0 fail。
- **第 20 弾**。 **混合 freshness の join が片側を 1 個/評価 leak する一般バグ** (if / match / if let すべて、 `?` 以前からの既存) を probe で検出・修正。 `if c { new Box(1) } else { fallback }` のような「片腕 fresh・片腕借用」の join は freshness 述語が「全腕 fresh」を要求するため非 fresh 扱いになり、 消費側の retain が fresh 腕を過剰計上していた。 修正は **join の所有正規化**: `lower_block_hinted` が tail retain を発行したかを `last_block_tail_owned` で公開し、 if / match (optional / enum / int / bool / str) / if-let の全 join (11 箇所) で「非所有の heap 結果に join 側 Retain」を入れ、 join 値は一律 +1 所有に。 これにより `is_fresh_object_expr` の If / Match / IfLet 規則は複雑な分岐 (arm_returns_own_binding 等) ごと `true` に単純化・削除。 join coerce が新セルを作る場合 (T→T? 包み / fixed copy) は所有元の +1 を解放。 併せて **fresh 引数の T→T? 自動包み leak** (`takeOpt(new Box(42))` — stmt-let 側は前ラウンドで修正済みだった残り) を `lower_arg_to` に同型の解放で修正。 fixture: `05_edge_cases/mixed_freshness_join.il` (if/match/iflet × deinit 数 + churn delta=0)。 probe 済みで問題なし: `?` の全配置 (引数位置・束縛経由・loop 内・文として破棄・arm 内・連鎖)、 固定長配列の新経路の組合せ (fresh 戻り値→push/map.set/fill、 for-in 早期 return、 Optional<Box[2]> field、 Map values() 反復、 再代入×所有者先死、 ?×固定長 payload) — churn 1150 まで正確。 検証: nextest 539/539、 AOT PASS、 nested_generic 100 並列 0 fail。
- **第 19 弾**。 ユーザー決定 (a) を受けて **固定長配列 × heap 要素を正式サポート**: fresh リテラル束縛が所有・エイリアスは借用・field 代入は fresh=ポインタ転送 / 非 fresh=値コピー (`$array.copyFixed`)・drop cascade は合成タグ `KIND_FIXED_BASE + len*16 + elem_kind`。 置き場所は field / ローカル / param に checker で限定 (戻り値・コンテナ構成要素・capture・再代入は型エラー)。 fixture 2 件 (包括 pin + placement エラー)。 詳細は下の解決済み記録。

それ以前のセッション (2026-06-10、 ARC ラウンド) で main に landing した変更:

- **fixture 増殖ラウンド** (`480ed47a`〜`ac91f68b`、 後続セッション)。 leak / 別名健全性の probe を Map / Set / Array / Optional / weak / template literal へ網羅的に当てて 5 系統のバグを検出・修正。 回帰 fixture 8 件追加。 詳細は下の解決済み記録「fixture 増殖ラウンドで検出した 5 系統」:
  - `arc_peephole` が「他値の Release は跨いで安全」と誤判定 → `makeMap()["k"].n` が解放済みメモリを読む (**正しさバグ**、 `480ed47a`)
  - `__release_object` の field cascade 中に weak back-ref の `__release_weak` が本体を解放 → 二重解放 SIGSEGV (**既存の潜在バグ**、 `d3b1d2cf`)
  - 配列 `indexOf` / `includes` / `remove` が string をポインタ比較 → 実行時生成文字列で不一致 (**正しさバグ**、 `ebb95b4a`)
  - Map/Set の `get`/`has`/`delete` fresh needle 引数、 fresh receiver (string/array/Optional のメソッド・`.length`・`.isSome`) の transient +1 が release されず leak (`8b9f8c31`)
  - template literal が評価ごとに registry string を 2+ 個 leak (`aace25c7`)
- **`m[k]` の Map index 読みを `ArrayLoad` と同じ borrow 規約に統一** (`f3d0a899`、 後続セッション)。 `__map_get` の retain-on-read (`d8c7f548`) と borrow 前提の消費側 (束縛 Retain / tail-borrow Retain / arc_peephole の whitelist) が二重 retain になり、 overwrite された entry が 24 bytes/iter で leak していたのを解消。 詳細は下の解決済み記録。
- **cranelift 0.131 → 0.132.1 へ依存上げ** (`12d171d4`)。 API breaking なし。 nested_generic.il race の調査用に試行したが race 確率は不変 (cranelift_module 側に修正は入っていない)。 修正とは独立で温存。
- **class method body の bare-var tail を borrow として扱う** (`4e4e6851`)。 [crates/ilang-mir/src/lower/body_cx.rs::lower_block_hinted](crates/ilang-mir/src/lower/body_cx.rs:695) の `tail_is_borrow` が `Index/Field` のみ matching していたところに、 `Var(name)` でも「class method 内 + env/capture 未解決」 なら暗黙の `this.field` として retain を発行するよう拡張。 `nested_generic.il` の race を根本解決。 同時に dd40bc49 の `forget(compiled)` も撤去。
- **property getter を bare-var borrow retain の対象外に** (`ac6e787e`)。 method 用の retain は property access の caller-side retain と二重になるため、 `BodyCx::is_property_getter` を追加して getter body だけ skip。 200 iter で 24 bytes/iter leak していた回帰を解消。
- **closure body 内の bare-var field を `LoadCapture` 経由で解決** (`a0c2a854`)。 [crates/ilang-mir/src/lower/collect.rs](crates/ilang-mir/src/lower/collect.rs) の `Var` arm で free var に `this` を candidate に追加 + [crates/ilang-mir/src/lower/expr.rs::lower_var_expr](crates/ilang-mir/src/lower/expr.rs) の field path で env に `this` がなければ captures からフォールバック。 `class Holder { inner: Box; grab(): fn(): Box { fn(): Box { inner } } }` で `lower_var_expr` の `Option::unwrap()` panic していたのを解消。
- **borrow-tail retain を fn-body の outermost block でだけ発火** (`775c6026`)。 [crates/ilang-mir/src/lower/body_cx.rs](crates/ilang-mir/src/lower/body_cx.rs) に `in_fn_body_top` flag + `lower_block_for_fn_body` wrapper を追加。 if-arm / match-arm / loop-body の sub-block では retain skip。 `pick(): Box { if flag { a } else { b } }` で 24 bytes/iter leak していたのを解消。
- **top-level let が class field 名を hijack する shadow bug を解消** (`a78bfd2a`)。 [crates/ilang-mir/src/lower/expr.rs::lower_var_expr](crates/ilang-mir/src/lower/expr.rs) で `repl_slots` lookup より先に implicit `this.field` を解決する順序に変更 (= OOP の「class members shadow globals」 ルール)。 `let base = test.liveAllocBytes()` で `Forge.base` field が誤って REPL slot 経由になり、 200 iter ループ内で `test.expect(g().n, i)` が常に 0 を返す wrong-value bug を解消。

regression fixture 9 件 (`05_edge_cases/method_tail_bare_var_if_arm.il`、 `05_edge_cases/method_tail_bare_var_match_arm.il`、 `05_edge_cases/method_tail_match_enum_payload.il`、 `05_edge_cases/method_tail_bare_top_level_fn.il`、 `05_edge_cases/method_field_shadows_top_level_let.il`、 `08_properties/getter_tail_bare_var_heap.il`、 `09_subtyping/method_tail_bare_var_parent_field.il`、 `10_closures_arc/closure_tail_bare_var_field.il`、 `059303f5` の match-arm/payload 2 件) を追加。

それ以前に同セッションで landing 済み (sret ラウンド):

- **内部 fn の CRepr struct return を sret 経路に倒す** (`4d1f97dc`)。 `crepr_struct_field_discard.il` の leak (= chunks return で callee の `new Box()` buffer が宙吊り) を塞いだ。 `Terminator::Return` に `release_value: bool` を追加し、 codegen が sret memcpy 後に callee 側 buffer を `__mir_free` する。 `is_c_abi` (= `Extern { .. } | ExternBody`) は従来の platform chunk → HFA → sret cascade を維持して SDL2 / wgpu / objc_msgSend を守る。
- **`Inst::VirtCall` も同じ sret 経路に統一** (本コミット)。 `call_dispatch.rs::VirtCall` が `struct_indirect_with_max` のままだったため、 vtable 経由で 16 byte 以下の CRepr struct (NSRange / NSRect 等) を返す `@objc method` の caller signature (chunks return) と callee signature (sret) が決定的にミスマッチし、 debug build で SIGSEGV を踏んでいた (`cocoa_foundation/calendar_test.il`、 `cocoa_appkit/drawing_test.il`)。 vtable に乗るのは構造的に `FunctionKind::Local` のみなので `struct_sret_for_internal` に統一すれば整合する。
- **CRepr struct の inline enum field を表す `MirTy::CReprEnum` を導入** (`28f7060f` → `65bb326a` → `14292c5e`、 前セッション)。
- **`match` / `if let` のアームバインディング tail-Var Retain** を `Binding::PatternBinding(_, _, needs_retain_on_tail)` で表現し直し (`ef1b9d35` → `838d2dc4`、 前セッション)。
- **closure body 内 cell store の rc** を 2 path に分離 (`50eb400a` + `46feb093`、 前セッション)。
- **`Binding::Ssa` 細分化と rc-slot 集約** (`4afd282e` → `d6b2e64f` → `838d2dc4`、 前セッション)。
- **CRepr fresh return の leak 調査用に `ILANG_HEAP_TRACE` env を追加** (`bcd3367f`、 前セッション)。

次のフェーズ候補: **capability の enforce** (`@requires` はパース済み・未 enforce)、 **未実装の言語機能 (Iterator プロトコル、 `?` の Optional 対応など — タプルと Result 用 `?` は実装済みと第 15 弾で確認)**、 **C ヘッダから .il 自動生成のミニ bindgen**、 **REPL の `use` 対応 (loader overlay 方式の素案は第 15 弾の記録参照)**。

## 未解決の引き継ぎ事項

### [解決済み記録] 第 64 弾: enum-in-enum ctor の payload が refine されず Type::Any (2026-06-13)

第 63 弾の明示診断が表面化させた実バグ:

- **症状**: `let r: Result<Maybe<Box>, string> = Result.ok(Maybe.nope)` や `m["k"] = Result.ok(Maybe.nope)` (m が `Map<_, Result<Maybe<Box>,string>>`) で、 内側 `Maybe.nope` の T が決まらず **「cannot infer the type parameter(s) of \`Maybe\`」** (第 63 以前は Type::Any クラッシュ)。 宣言型 `Result<Maybe<Box>,_>` から内側の Maybe は Box と確定できるのに refine されていなかった。
- **原因**: `refine_enum_ctor_args` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) の EnumCtor arm は ctor の**自分の型引数**を target から埋めるだけで、 **payload 引数 (`Maybe.nope`) へ再帰していなかった**。 さらに ctor の自分の slot も「bare `Any` のみ置換」だったため、 引数から推論された `Maybe<Any>` (ネスト Any) が直らなかった (check_enum_ctor は Result の T を Maybe.nope の型 = Maybe<Any> と推論する)。
- **修正**: EnumCtor arm を 2 点拡張。 (1) `type_contains_any` ヘルパーで slot が Any を**ネスト含む**なら target の具体 arg で置換 (`Maybe<Any>` → `Maybe<Box>`)。 (2) enum sig から variant の payload 型を引き、 ctor の (置換後) 型引数で subst して **各 payload 引数を再帰 refine** (`Maybe.nope` を `Maybe<Box>` に対して refine → 内側 stash が Box に埋まる)。 既存の some / tuple / array 再帰と合わせ、 任意の深さの enum-in-enum / enum-in-some/tuple/array が refine される。
- **検証**: let 注釈・fn 戻り・map 値 (index store + 上書き)・二重入れ子 (`Result<Maybe<Maybe<Box>>>`) を heap payload 込みで網羅。 値の正しさ + deinit 厳密 (200/round) + churn delta=0。 fixture: `05_edge_cases/nested_enum_ctor_refine.il`。 checker のみの変更。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 63 弾: 解決できない型引数を明示診断にした (2026-06-13、 ユーザー決定)

第 62 弾の残る限界 (期待型が皆無の bare match scrutinee 等) を、 ユーザー判断で**明示診断**にした (双方向推論は入れず、 曖昧なら注釈を促す):

- **背景**: `match f(Result.err("e")) { .. }` (期待型の無い bare scrutinee) や `let r = Result.ok(5)` (E が未決定) は、 型引数が `Any` のまま monomorphizer に届き **「mir lower: unsupported in M1: Type::Any (variadic builtins)」** で停止していた (源コード位置の無い不親切なエラー)。
- **調査**: checker は型引数を `enum_ctor_type_args` / `fn_call_type_args` の stash (span → (name, [type args])) に記録し、 解決できなかった param は `Type::Any` を残す。 到達コードではこの Any が lowering でクラッシュするが、 **未呼び出し fn 内の Any はコンパイルが通っていた** (テンプレートが lowering 前に破棄/未 monomorphize のため)。
- **修正**: `check()` ([check.rs](../crates/ilang-types/src/checker/check.rs)) の末尾に `report_unresolved_type_args` を追加。 両 stash を走査し、 型引数に `Type::Any` を含むエントリを `Unsupported { what, span }` で報告: enum ctor は「cannot infer the type parameter(s) of \`Result\` here — add a type annotation」、 generic fn 呼び出しは「cannot infer the type parameter(s) of generic fn \`f\` at this call」。 **`TypeVar` は除外** (generic テンプレート本体は instantiation で解決されるため正当)。 全ての refine (let / return / 引数 / match arm join / 第 54〜62 弾) が走った**後**の最終 stash を見るので、 解決済みの呼び出しは Any を持たず誤検知しない。
- **挙動変更**: 従来コンパイルが通っていた **未呼び出し fn 内の曖昧 ctor** (`fn unused() { let r = Result.ok(5); .. }`) も明示エラーになる。 これは到達性に関わらず曖昧 (Rust の `let r = Ok(5)` が注釈必須なのと同じ) なので妥当な厳格化。 既存 fixture / cocoa / std はいずれも曖昧 ctor に依存しておらず全緑。
- **検証**: 失敗していた全形 (bare match scrutinee・注釈なし let・`Result.ok(5)`) が span 付き診断に変わり、 正しい形 (注釈あり・T-fixing 引数・第 59/61/62 弾の各位置) は無影響。 fixture: `05_edge_cases/unresolved_type_arg_diagnostic.il` (`// expect-error` で診断を pin)。 workspace nextest 539/539 (REPL 含む)、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 **残る既知の課題**: 双方向推論 (match arm の使われ方から scrutinee の T を逆算) は未実装 — 曖昧なら注釈で回避する方針。

### [解決済み記録] 第 62 弾: 期待型から generic 呼び出しの T を解いた後に引数も再 refine (2026-06-13)

第 60 弾の確認済み記録 (2) を大部分解消:

- **症状**: `fn f<T>(r: Result<T,string>): Result<T,string>` を `f(Result.err("e"))` (T を決める引数なし) で呼び、 結果に**期待型がある**形 — `let x: Result<i64,string> = f(Result.err("e"))`、 fn 戻り位置、 引数位置 — でも **Type::Any** で lower 失敗。
- **原因**: 第 59 弾の `refine_fn_call_type_args` は fn の T を期待型から解いて呼び出しの stash を更新するが、 **インライン引数 `Result.err("e")` 自身の `enum_ctor_type_args` stash は `[Any, string]` のまま**だった。 引数は check 時にまだ generic な param 型 (`Result<Any,string>`、 inferred_args が Any) に対して refine されたため。 fn の T が後から i64 に解けても引数の stash は更新されず、 引数の lowering が Type::Any。
- **修正**: `refine_fn_call_type_args` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) で T を解いた後、 **各引数を「解いた型引数で具体化した param 型」に対して `refine_enum_ctor_args` で再 refine** する処理を追加 (引数は fn の T を共有する)。 これで `Result.err("e")` の T も i64 に埋まる。 borrow 衝突回避のため sig.params / type_params を clone し tbl を drop してから実施。
- **検証**: let 注釈・fn 戻り・引数位置で `f(Result.err(..))` / `f(Result.ok(Box))` を heap T 込みで網羅。 値の正しさ + deinit 厳密 (300/round = 3/round × 100) + churn delta=0。 fixture: `05_edge_cases/generic_fn_call_solved_from_context.il`。 **core 推論変更**のため workspace nextest 539/539 で回帰なし確認。 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。 **残る限界**: 期待型が皆無の bare match scrutinee は未対応 (上の確認済み記録)。

### [解決済み記録] 第 61 弾: match の arm が yield する enum ctor が refine されず Type::Any (2026-06-13)

第 60 弾の確認済み記録 (1) を解消:

- **症状**: `let res = match r { ok(v) { Result.ok(v) } err(e) { Result.err(e) } }` の後 `res` を返す/使う形で **「mir lower: unsupported in M1: Type::Any (variadic builtins)」**。 **非 generic でも再現** (`r: Result<i64,string>` 固定でも失敗)。 primitive scrutinee (`match flag { 0 { Result.ok(..) } _ { Result.err(..) } }`) も同様。
- **原因**: match の各 arm が enum ctor を yield するとき、 `Result.ok(v)` は T のみ・`Result.err(e)` は E のみ pin し、 自分では片方を Any のまま残す。 match の結果型は arm の join で `Result<i64,string>` と正しく出るが、 その型は各 arm ctor の stash (`enum_ctor_type_args`) に **push back されない**。 let / return / 引数位置の refine は「値そのもの」を対象にするが、 ここでは値が **兄弟 arm によって補完される** ため、 注釈なし let に束縛すると refine の入口が無かった。
- **修正**: `refine_match_arm_ctors(arms, result_ty)` ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)) を新設し、 join 後の結果型 (Generic / Optional のときのみ) を各 arm body に `refine_enum_ctor_args` で push。 match の 3 種の検査経路すべてに適用: enum scrutinee (`check_match_expr`)・Optional scrutinee (`check_match_optional`)・primitive scrutinee (`check_match_primitive` [match_.rs](../crates/ilang-types/src/checker/match_.rs))。 arm body が Block でも `refine_enum_ctor_args` が tail を辿る。
- **検証**: enum scrutinee の rewrap (heap `Box`)・generic 版 (`rewrapG<T>`)・primitive scrutinee の classify を網羅。 値の正しさ + deinit 厳密 (200/round = 2/round × 100) + churn delta=0。 fixture: `05_edge_cases/match_arm_yields_enum_refine.il`。 checker のみの変更。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 60 弾: generic fn の引数型推論が Any を具体型より優先 (2026-06-13)

`?` を generic fn 内で使う形を probe して発見 (当初 `?` 起因に見えたが `?` 非依存):

- **症状**: `fn unwrapOr<T>(r: Result<T,string>, fallback: T): Result<T,string> { let v = r?; Result.ok(v) }` を `unwrapOr(Result.err("boom"), 0)` で呼ぶと **「mir lower: unsupported in M1: Type::Any (variadic builtins)」**。 `Result.ok(5)` の呼びは通る。 `?` 無しの `fn pick<T>(a: Result<T,string>, b: T): T { b }` を `pick(Result.err("e"), 42)` で呼んでも同症状。
- **原因**: generic fn 呼び出しの型引数推論 ([calls.rs](../crates/ilang-types/src/checker/expr/calls.rs)) が各引数から `collect_type_var_bindings` で bind を集めるが、 同関数 ([sigs.rs](../crates/ilang-types/src/checker/sigs.rs)) が `bindings.entry(name).or_insert_with(..)` で **最初の binding を優先**。 arg1 `Result.err("boom")` は `Result<T,string>` vs `Result<Any,string>` で **T=Any** を入れ、 arg2 `0`/`42` の `T` vs `i64` が **上書きされない**。 結果 fn が `<Any>` で具体化され、 monomorphizer が `Result<Any,string>` の layout で `ty_to_mir(Any)` に達して停止。 (切り分け: DBG で fn の specialize が T=Any と T=i64 の両方で走っていることを確認。 非 generic の同形 (`Result<i64,string>` 固定) は通るので generic 推論固有。)
- **修正**: `collect_type_var_bindings` の `TypeVar` arm で、 **既存 binding が `Any` なら具体型で上書き** (具体同士は従来どおり最初を保持し、 conflict を黙って再推論しない)。 これで arg2 の i64 が arg1 の Any に勝ち、 fn が `<i64>` で具体化される。
- **検証**: `pick(Result.err("e"), 42)` / `pick(Result.ok(7), 9)` の値、 `?` helper の ok/err 経路を heap (`Box`) payload で deinit 厳密 (300/round = 3/round × 100) + churn delta=0 (err 経路の未使用 fallback Box 解放も確認)。 fixture: `05_edge_cases/generic_fn_arg_infer_prefers_concrete.il`。 **core 推論変更**のため workspace nextest 539/539 で回帰なしを確認。 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 59 弾: generic fn の型パラメータを戻り値位置から推論 (2026-06-13)

第 57 弾の判断待ち記録 (2) を解消 (これで判断待ち 2 件とも解決):

- **症状**: 型パラメータが戻り型にしか現れない generic fn は引数から T を決められず、 期待型から解かれなかった。 `fn makeArr<T>(): T[] { [] }` に `let xs: i64[] = makeArr()` で「expected i64[], got any[]」、 `fn makeNope<T>(): Maybe<T>` / `fn wrapErr<T>(): Result<T,string>` は (value_assignable は通るが) monomorphizer で Type::Any。 配列・user enum・builtin Result すべてで再現。
- **原因**: generic fn 呼び出しの型推論 ([calls.rs](../crates/ilang-types/src/checker/expr/calls.rs) `check_call_expr`) は **引数からのみ** 型パラメータを bind し、 残りを `Type::Any` にして stash + 戻り型に subst する。 期待型 (let 注釈等) を使う経路が無かった。
- **修正**: `refine_fn_call_type_args(call_expr, target)` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) を新設。 generic fn 呼び出しの stash に Any が残るとき、 引数で既に決まった bind を seed し、 宣言戻り型 `sig.ret` を target に `collect_type_var_bindings` で unify して残りを解き、 `fn_call_type_args` を更新して補正後の戻り型を返す (enum-ctor の `refine_enum_ctor_args` と同じ post-hoc 方式)。 期待型が分かる 3 位置で適用: **let 注釈** ([stmt.rs](../crates/ilang-types/src/checker/stmt.rs)、 vt を補正)・**fn 戻り位置** ([decls.rs](../crates/ilang-types/src/checker/decls.rs)、 tail を expected に対して解き body_ty を補正)・**呼び出し引数** ([method.rs](../crates/ilang-types/src/checker/method.rs) `check_args`、 at を param 型で補正)。 引数で T が既に決まる呼び出し (`wrapOk(5)`) は stash に Any が無いので無影響。
- **検証**: let (配列 / Maybe / Result)・return 位置・引数位置・**部分推論** (`pair<A,B>(a: A): (A,B)[]` で A=引数・B=注釈)・heap T (`emptyBoxes<T>(): T[]` に Box) を網羅。 値の正しさ + deinit 厳密 (200/round = 2/round × 100) + churn delta=0。 fixture: `05_edge_cases/generic_fn_return_type_inference.il`。 checker のみの変更。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 58 弾: generic class メソッドが generic enum を構築すると「unknown enum 〜」 (2026-06-13)

第 57 弾の判断待ち記録 (1) を解消:

- **症状**: `class Wrap<T> { v: T; asSome(): Maybe<T> { Maybe.some(this.v) }; asOk(): Result<T,string> { Result.ok(this.v) } }` を `Wrap<i64>` で使うと **「mir lower: unknown enum Maybe」 / 「unknown enum Result」**。 user enum も builtin Result も同症状。
- **原因**: class 単一化 ([class.rs](../crates/ilang-mir/src/monomorphize/class.rs)) の `subst_expr` は specialized method body の式の**型**を T→具体に置換するが、 **enum ctor の `enum_name` を再 mangle しなかった**。 そのため `Maybe.some(this.v)` の `enum_name="Maybe"` がそのまま lower に届き、 resolve_ty が bare `Type::Enum("Maybe")` を引けず失敗。 generic fn 経路 ([fns.rs](../crates/ilang-mir/src/monomorphize/fns.rs)) は `rewrite_calls_and_enums_in_expr` で enum_ctor_type_args + outer 置換から再 mangle するが、 class 経路に同等が無かった (span 記録の型引数は generic で [TypeVar(T), ..]、 specialize の clone は span を共有するので、 置換は specialize 時にしかできない)。
- **修正**: checker の `enum_ctor_type_args` を **thread-local** (`ENUM_CTOR_TYPE_ARGS` [mod.rs](../crates/ilang-mir/src/monomorphize/mod.rs)、 既存の `GENERIC_ENUM_NAMES` と同パターン) 経由で class pass に渡す。 `monomorphize` / `monomorphize_with_requests` に `enum_ctor_type_args` 引数を追加し、 pass 開始時に thread-local へ stash。 `subst_expr` が EnumCtor を見たら `remangle_generic_enum_ctor` で span 記録の型引数を class の params→args で置換し、 concrete なら `mangle_enum` で `Maybe<i64>` / `Result<i64,string>` に (fn 経路と同形)。 ctor 引数は再帰 subst で内側 ctor も処理 (`Result.ok(Maybe.some(this.v))`)。 非 generic class は `subst_expr` を通らない (rewrite_item 経由) ので無影響。 caller 3 箇所 (main.rs 通常 2 + REPL 1) に `tc.enum_ctor_type_args()` を渡す。
- **検証**: builtin Result・user enum Maybe を generic class メソッドで構築 (heap T `Box` 込み)、 複数インスタンス化 (`Wrap<i64>` / `Wrap<string>` / `Wrap<Box>`)、 入れ子 (`Result<Maybe<T>, string>`)、 explicit pattern での match 消費を網羅。 値の正しさ + deinit 厳密 (200/round = 2/round × 100) + churn delta=0。 fixture: `05_edge_cases/generic_class_method_builds_enum.il`。 **別の既存制限を記録**: generic class の **static メンバ**は checker が「static members on generic classes are not supported」で拒否 (本バグと無関係の意図的制限)。 monomorphize pipeline を触ったので nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 57 弾: generic fn が builtin Result を構築すると「unknown enum Result」 (2026-06-13)

generic fn × enum ctor 型引数精緻化 (第 48〜56 弾) の継ぎ目を probe して発見:

- **症状**: `fn wrapOk<T>(x: T): Result<T, string> { Result.ok(x) }` を `wrapOk(5)` で呼ぶと **「mir lower: unknown enum Result」**。 `Result.err(e)` 構築も同様。 user 宣言の generic enum (`fn wrapM<T>(x: T): Maybe<T> { Maybe.some(x) }`) は動く。
- **原因**: builtin `Result<T,E>` は宣言が無く call site ごとに monomorphize される (`monomorphize_enums` が `Result<i64,string>` 等の `Item::Enum` を合成)。 `monomorphize_fns` ([fns.rs](../crates/ilang-mir/src/monomorphize/fns.rs)) は generic fn を specialize する際、 body 内の generic-enum `EnumCtor.enum_name` を具体形 (`Result.ok` → `Result<i64,string>.ok`) に再 mangle するが、 「どの enum 名が generic か」の集合 `generic_enums` を**プログラム宣言の `Item::Enum` だけ**から作っていた。 builtin Result はそこに無いため mangle されず、 specialized body の `Result.ok` が `enum_name="Result"` のまま lower に届き、 resolve_ty が bare `Type::Enum("Result")` を引けず失敗。
- **修正**: `monomorphize_fns` の `generic_enums` 構築後に `result_template()` ([enums.rs](../crates/ilang-mir/src/monomorphize/enums.rs)、 `pub(super)`) を seed (`monomorphize_enums` が line 66 で行うのと同じ対処)。 これで specialized body の Result ctor が enum_table (enum_ctor_type_args) + outer 置換から `Result<i64,string>` に mangle される。
- **検証**: T を ok arg / err arg / 両腕で決める形を i64 と heap (`Box`) payload で網羅し、 値の正しさ + deinit 厳密 (300/round = 3/round × 100) + churn delta=0 (err 腕が破棄する heap arg の解放も確認)。 fixture: `05_edge_cases/generic_fn_returns_result.il`。 monomorphize (lowering pipeline) を触ったので nested_generic 儀式も実施。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 56 弾: tail 式の奥に入れ子の `?` の err return が refine されず Type::Any (2026-06-13)

`?` 演算子 × enum ctor 型引数精緻化 (第 48〜54 弾) の継ぎ目を probe して発見:

- **症状**: `Result.ok(take(g(ok)?))` のように `?` が **tail 式の奥** (call 引数 / `some(..)` / tuple 要素 / array 要素 / 二重ネスト call) に埋まると **「mir lower: unsupported in M1: Type::Any (variadic builtins)」**。 `let b = g(ok)?` のように `?` が**自前の文**にあるときや、 `let b = g()?; Result.ok(b.n)` は元から動いていた。
- **原因**: `?` は [postfix.rs](../crates/ilang-parser/src/expr/postfix.rs) で `{ let __try = expr; match __try { ok(v) { v } err(e) { return Result.err(e) } } }` に desugar される。 この `return Result.err(e)` は E (=e の string) は埋まるが **T=Any** で、 enclosing fn の戻り型から refine される必要がある。 `refine_enum_ctor_args_in_block` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) は **block の文**の値式には `refine_returns` (全子孫を歩いて Return を refine) を呼ぶのに、 **tail** には自身の値 ctor を refine する `refine_enum_ctor_args` しか呼んでいなかった。 そのため tail 式の奥に埋まった `return Result.err` が見つからず T=Any のまま lower に届いていた。
- **修正**: tail にも `refine_returns(self, t, target)` を適用 ([utils.rs](../crates/ilang-types/src/checker/utils.rs))。 `refine_returns` は式の全子孫を再帰的に歩くので、 call 引数 / some / tuple / array / 二重ネストのどの深さに `?` があってもその err return が refine される。 tail 自身の値 ctor refine (既存の `refine_enum_ctor_args`) と相補的 (前者は Return ノードのみ、 後者は tail 値)。
- **検証**: `?` を call 引数 / some / tuple / array / 二重ネスト call に入れ子にした 4 形を、 ok 経路 (heap payload を `?` が unwrap → 解放) と err 経路 (入れ子位置から早期 return) の両方で deinit 厳密 (400/round = 4/round × 100) + churn delta=0。 値の正しさ (21) も確認。 fixture: `05_edge_cases/try_nested_in_call_arg_refine.il`。 checker のみの変更のため nested_generic 儀式は対象外。 workspace nextest 539/539、 AOT 全 fixture PASS。

### [解決済み記録] 第 55 弾: 空マップリテラル `{}` を型注釈ありで空マップと解釈 (2026-06-13、 ユーザー決定)

第 54 弾で記録した「`let m: Map<K,V> = {}` が型エラー」を、 ユーザー判断 (= `{}` でも書けるように実装) を受けて対応。 **`new Map<K,V>()` は従来から空マップを作れる確立した形** (第 54 弾の当初記録の「構文が無い」は誤りで訂正済み)。 本弾は JS 風に `{}` でも書ける利便性の追加:

- **症状 (= 第 54 弾の別件)**: `{}` はパーサが**空ブロック** (値 `()`) として出すため (`control.rs::parse_map_literal` は 1 エントリ以上のときだけ `MapLit` を出す)、 `let m: Map<K,V> = {}` が「expected Map<K,V>, got ()」。
- **対応**: 型情報の要る checker + lowering で「Map ターゲット位置の空ブロック = 空マップ」とした。 (1) checker `value_assignable` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) が **空ブロック (stmts 空 + tail なし) を `Map<K,V>` ターゲットへ受理** (`empty_block_as_map`)。 非空 `{k: v}` は元から `MapLit` なので無関係、 空ブロックの unit 用法は Map 以外のターゲットでは不変。 (2) lowering `lower_composite_with_hint` ([literals.rs](../crates/ilang-mir/src/lower/literals.rs)) が `(Block 空, MirTy::Map)` を **空 `NewMap`** に lower (`lower_map_literal_with_hint(&[], ..)`)。 (3) `is_fresh_object_expr` ([body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs)) が**空ブロックを fresh 扱い** — 空ブロックは map スロットで rc=1 の fresh `NewMap` になり、 束縛が scope-exit で解放する必要があるため。 unit 文脈では fresh フラグは arc_heap でないので no-op (安全)。
- **検証**: let 注釈 / 再代入 (`m = {}` で旧マップ解放) / field 代入 (`this.m = {}`) / fn 戻り値 / fn 引数 / ネスト map 値 (`m.set("o", {})`) の全位置で heap value 込み deinit 厳密 (600/round = 6/round × 100) + churn delta=0。 空ブロックの unit 用法 (`if c {}`、 `let x = {}`) の回帰なし。 fixture: `03_collections/empty_map_literal_brace.il`。 checker + lowering 両方を触ったので nested_generic 儀式も実施。 workspace nextest 539/539、 ilang-types 75/75、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 54 弾: enum ctor 型引数精緻化を payload 引数 / built-in 配列メソッド / index 代入へ拡張 (2026-06-13)

第 48〜50 弾で enum ctor 型引数精緻化を let / field / 配列・Map リテラル / fn・method・init 引数 / some・tuple / fn・closure 戻り値へ広げたが、 同じ「型パラメータを引数から埋められない enum ctor」が **3 系統の残る store 位置**で Type::Any のまま lower に届いていた:

- **症状**: `Result<_, string>` 等 (T=Any) を以下に置くと **「mir lower: unsupported in M1: Type::Any (variadic builtins)」** (到達可能 = monomorphize される時)。 (1) **enum payload 引数**: `Wrapper.wrap(Result.err("e"))` (tuple payload) / `WrapperS.wrapS { r: Result.err("e") }` (struct payload)。 (2) **built-in 配列メソッド引数**: `xs.push(Result.err("e"))`・unshift・fill・remove・indexOf・includes。 (3) **index 代入**: `xs[0] = Result.err("e")` (配列) / `m["k"] = Result.err("e")` (Map)。 `Result.ok(1)` 等 T が埋まる形・到達不能な未呼び出し fn は元から通っていた。
- **原因**: refine は「値が宣言型に出会う store 位置」ごとに明示的に呼ぶ設計だが、 上記 3 経路が呼び漏れていた。 (1) `check_enum_ctor` ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)) は引数を payload 型へ `value_assignable` で検証するのに `refine_enum_ctor_args` を呼んでいなかった。 (2) ハードコードされた配列メソッド ([calls.rs](../crates/ilang-types/src/checker/expr/calls.rs) の `Type::Array` arm) は element 型へ検証するのに refine 無し (Map.set / Set.add は builtin クラス → `check_args` 経由で第 49 弾の refine が効いて健全)。 (3) `check_assign_index` ([access.rs](../crates/ilang-types/src/checker/expr/access.rs)) の Map value / 配列 element 検証に refine 無し。
- **修正**: 3 ファイルに `refine_enum_ctor_args(arg, &target)` を追加。 `check_enum_ctor` は tuple / struct 両 payload の検証ループで各引数を payload 型へ refine (各 ctor が自分の引数を refine するので **ネストした ctor-in-ctor も自然に伝播**)。 配列メソッド 6 種 (push/unshift/fill/remove/indexOf/includes) は element 型へ。 `check_assign_index` は Map value 型と配列 element 型へ。
- **検証**: payload (tuple/struct)・push・index 代入 (配列/Map) に `Result<Box, string>` を heap payload 込みで格納し deinit 厳密 (600/round = 6/round × 100) + churn delta=0。 ok/err 混在・値の正しさ (5/6/3/-1/9) を確認。 fixture: `05_edge_cases/enum_ctor_type_arg_refine_payload_and_index.il`。 **これで enum ctor 型引数精緻化は全 store / 引数位置で揃った。** workspace nextest 539/539、 ilang-types 75/75、 AOT 全 fixture PASS。 checker のみの変更のため nested_generic 儀式は対象外。
- **別件の既存ギャップを記録** (バグでなく設計判断待ち): 空マップリテラル `{}` は **パーサが空ブロックとして出す**ため (`casts.rs:484` のコメント「parser only ever emits MapLit when there's at least one entry」)、 `let m: Map<K,V> = {}` が「expected Map<K,V>, got ()」で型エラー。 空マップを作る構文が無い。 対応するなら空マップ構文の決定 (`{}` を注釈ありなら空マップと解釈する / `{:}` 等の専用構文 / `Map()` コンストラクタ) が要り、 `{}` (空ブロック) との曖昧性解決を含む言語仕様の選択。

### [確認済み記録] 第 53 弾: slot 昇格 edge + Map forEach mutation — 全て健全 (2026-06-13)

第 52 弾 (accessor の slot 昇格) の周辺と、 Map forEach の反復中 mutation を probe。 **新規バグなし**:

- **slot 昇格の他経路**: メソッド内クロージャ (`makeReader(): fn(){ g[0] }`)・accessor 内クロージャ (`get reader(): fn(){ g[0] }`)・init (`init() { g[0] = g[0]+1 }`)・static メソッド内クロージャ・default 引数式 (`fn f(x = g+1)`) からのグローバル参照が全て host slot に昇格され動作。 `walk_block` がクロージャ本体へ再帰し、 第 52 弾の accessor 走査と合わせて漏れなし。
- **Map.forEach の反復中 mutation**: forEach は callback ループ開始時に entry 順を **snapshot** するため安全。 (1) **add-during**: 反復中に `m.set(..)` した値は map drop 時に解放、 当該反復では未訪問。 (2) **future-key delete**: 反復中に未訪問キーを delete しても snapshot の +1 が callback 終了まで値を生存させ **UAF なし**・なお訪問される (sum に含まれる)。 (3) **nested forEach** (同一 map 二重反復): 二重計上も leak も無し。 全形 deinit 厳密 (800/round) + churn delta=0。

fixture: `03_collections/map_foreach_mutation_arc.il` (add/future-delete/nested を厳密 deinit + churn delta=0)。 slot 昇格 edge は第 52 弾 fixture と本ラウンドの probe で確認済みのため fixture 追加なし。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [解決済み記録] 第 52 弾: property accessor body から top-level let を参照すると unbound (2026-06-13)

property accessor を probe して発見:

- **症状**: property getter / setter の本体から **top-level let (グローバル)** を読み書きすると **「mir lower: unbound variable: g」**。 同じ body を **regular method** に書けば動く (`deinit() { deinits[0] = .. }` が全 probe で効いていたのと同じ機構)。 getter/setter だけが参照する let で発生。
- **原因**: CLI の slot 昇格パス `build_slot_table` ([main.rs](../crates/ilang-cli/src/main.rs)) が `collect_fn_free_var_refs` ([walk.rs](../crates/ilang-cli/src/walk.rs)) で「fn / method body から参照される top-level let」を集めて host slot に昇格するが、 `walk_class` が `c.methods` / `c.static_methods` は走査するのに **`c.properties` (getter/setter) を走査していなかった**。 結果、 getter/setter からしか参照されない let は `slot_table` → `repl_slots` に入らず、 lowering の `lower_var_expr` が repl_slots でも解決できず unbound。 debug で getter を持つプログラムの `repl_slots` が空 (=昇格漏れ) と確認。
- **修正**: `walk_class` に `c.properties` の getter/setter body 走査を追加 (`this` + params を locals に、 method と同形)。 これで accessor 内のグローバル参照が昇格される。
- **検証**: getter がグローバル配列を read / write (counter インクリメント)、 setter がグローバルを参照、 getter の副作用が 1 アクセス 1 評価 (getCalls=3)・setter 呼び出し回数 (setCalls=2) を確認。 scalar グローバルも (昇格漏れで repl_slots 空だったのが) 解消。 fixture: `08_properties/property_accessor_reads_global.il`。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 51 弾: heap static フィールド代入が retain せず use-after-free (2026-06-13)

static フィールドを probe して発見した ARC バグ:

- **症状**: `Cls.s = arg` (s が string static フィールド、 arg が fresh string を持つ借用 param) の後 `Cls.s` を読むと **空文字を印字するのに length は正しい** = 解放済みバッファを参照する **UAF**。 fresh string を **直接** (`Cls.s = "a" + b`) なら動く (fresh が +1 を所有)、 リテラルや束縛 local も動く (長生き)。 fresh を**パラメータ経由**で渡したときだけ壊れる。
- **原因**: static フィールド代入の lowering ([expr.rs](../crates/ilang-mir/src/lower/expr.rs) の `StoreStatic` 経路) が **retain も release もしていなかった**。 instance field (`StoreField`) は retain-new / release-old するのに、 static slot は値をそのまま store。 借用源は slot が +1 を所有しないため、 源 (caller の fresh arg transient) が解放されると slot が dangling。
- **修正**: heap static slot (`is_arc_slot`) のとき instance field と同じ **retain-new (非 fresh) / release-old (init store は除く — const-baked 初期値を持つため)** を `StoreStatic` の前に追加。 fresh 源は +1 を slot の share に流用、 借用源は retain。 旧値 (前回の文字列 or const-init "init") を release (interned リテラルの release は安全と churn で確認)。
- **検証**: fresh 直接・fresh via param (旧 UAF)・借用 local を string static に代入し、 値の非破壊 (`hello` が `hello` のまま) と `liveStringCount` churn 600 で flat (retain/release 均衡)・read が生きたバッファを返すことを確認。 fixture: `02_classes/static_string_field_arc.il`。
- **別の既存制限を記録** (バグでなく codegen ギャップ): **数値配列の static フィールド** (`static data: i64[]`) は checker が許可するのに **codegen が「unsupported in M1: static slot type」で拒否**。 string static のみ実用。 対応するなら static slot codegen に array 型を追加。
- workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 50 弾: クロージャ戻り値の enum ctor 型引数を精緻化 (2026-06-13)

第 48/49 弾で enum ctor 型引数精緻化を store / 引数 / some / tuple へ広げたが、 **クロージャ本体の戻り値**だけ未配線だった:

- **症状**: `let f: fn(): Result<i64,string> = fn(): Result<i64,string> { Result.err("e") }` や `xs.map(fn(x): Result<...> { ... Result.err(..) })` が **「Type::Any (variadic builtins)」**。 早期 return (`fn() { if .. { return Result.err(..) } .. }`) も同様。 top-level fn `fn f(): Result<..> { Result.err(..) }` は decls.rs:637 で refine 済みなので動いていた。
- **原因**: top-level fn 本体は [decls.rs](../crates/ilang-types/src/checker/decls.rs) の `refine_enum_ctor_args_in_block(&f.body, &expected)` で戻り値型から refine するが、 **クロージャ (`check_fn_expr` [casts.rs](../crates/ilang-types/src/checker/expr/casts.rs)) は同等の refine を呼んでいなかった**。 クロージャ本体の tail / return の `Result.err(..)` が T=Any のまま lower に届いた。
- **修正**: `check_fn_expr` の body 検査後に `self.refine_enum_ctor_args_in_block(body, &expected)` を追加 (`expected` = クロージャの宣言戻り値型)。 `refine_enum_ctor_args_in_block` は tail と全 return を走査するので tail / 早期 return の両方をカバー。
- **検証**: クロージャ tail err・早期 return err・`map` コールバックが Result を返す形を heap payload 込みで deinit 厳密 (300/round) + churn delta=0。 第 48/49 弾と既存 fixture の回帰なし。 fixture: `10_closures_arc/enum_ctor_refine_closure_return.il`。 **これで enum ctor 型引数精緻化は let / field (明示+bare) / 配列 / Map / 再代入 / 引数 (fn/method/init/default) / some / tuple / 入れ子 / top-level fn 戻り値 / クロージャ戻り値の全位置で揃った。** workspace nextest 539/539、 ilang-types 75/75、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 49 弾: enum ctor 型引数精緻化を引数 / some / tuple 位置へ拡張 (2026-06-13)

第 48 弾で field / 配列 / Map / 再代入を直したが、 同じ「型パラメータを埋めない enum ctor」が**残りの位置**でも Type::Any で失敗していた:

- **症状**: `consume(Result.err("e"))` (fn 引数)・`s.handle(Result.err("e"))` (method 引数)・`new Svc(Result.err(..))` (init 引数)・`some(Result.err(..))`・`(Result.err(..), 5)` (tuple 要素) が **「Type::Any (variadic builtins)」**。 入れ子 (`some((Result.err(..), 5))`、 `[(Result.ok(1), 10), (Result.err("e"), 20)]`) も同様。
- **原因**: 第 48 弾の `refine_enum_ctor_args` は if/match/block/return には再帰したが **`some` / tuple / array リテラルには再帰しなかった**ため、 `some(ctor)` の内側や tuple 要素の ctor が精緻化されなかった。 さらに **call-arg checker** (非 generic fn の `check_args` [method.rs](../crates/ilang-types/src/checker/method.rs)、 generic fn の inline arg loop、 fn 型 call) は refine を一切呼んでいなかった。
- **修正**: (1) `refine_enum_ctor_args` ([utils.rs](../crates/ilang-types/src/checker/utils.rs)) に `Some(inner)` (Optional の inner へ)・`Tuple(elems)` (各要素を tuple 型の対応要素へ)・`Array(elems)` (各要素を elem 型へ) の再帰 arm を追加 — これで let / field / 引数のどの入口から呼ばれても入れ子の ctor まで届く。 (2) call-arg の 3 経路 ([method.rs](../crates/ilang-types/src/checker/method.rs) の `check_args`、 [calls.rs](../crates/ilang-types/src/checker/expr/calls.rs) の generic fn arg loop と fn 型 call) で param 型を target に refine。
- **検証**: fn / method / init 引数・some・tuple 要素・入れ子 (some of tuple、 array of tuples) に Result を heap payload 込みで渡し deinit 厳密 (300/round) + churn delta=0。 第 48 弾 fixture と既存 fixture の回帰なし。 fixture: `05_edge_cases/enum_ctor_type_arg_refine_arg_some_tuple.il`。 workspace nextest 539/539、 ilang-types 75/75、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 48 弾: enum ctor の型引数が store 位置で精緻化されず Type::Any (2026-06-13)

Result `?` を probe する過程で発見した型推論バグ:

- **症状**: `Result<i64, string>` や `Either<A, B>` (= **2 型パラメータ enum**) を **class field 型 / 配列要素型 / Map 値型**に使うと **「mir lower: unsupported in M1: Type::Any (variadic builtins)」**。 1 型パラメータ enum (`Maybe<T>`) は field/配列とも動く。 **local / 戻り値**はどちらも動く。 array リテラルでは `[Result.ok(1), Result.err("e")]` のように **`err`/`nope` 等パラメータを埋めない variant** を混ぜると 1 型パラメータでも失敗。
- **原因**: enum コンストラクタ `check_enum_ctor` ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)) は引数から型パラメータを推論し、 **埋まらない param は `Any`** として span キーの `enum_ctor_type_args` に stash する (monomorphize がこれを読む)。 `Result.err("e")` は E しか埋まらず T=Any、 `Maybe.nope` は何も埋まらない。 `let f: T = ctor` と fn return は `refine_enum_ctor_args` で**注釈から Any slot を埋め直す**が、 **field 代入 (明示 `check_assign_field` / bare implicit-this)・配列リテラル要素 (`check_array_with_hint`)・Map 値 (`check_map_lit_with_hint`)・local 再代入**は refine を呼んでおらず Any が残ったまま lower に届いていた。
- **修正**: 5 箇所すべてで宣言型を target に `refine_enum_ctor_args(value, &decl_ty)` を呼ぶ — `check_assign_field` ([access.rs](../crates/ilang-types/src/checker/expr/access.rs))、 bare field 代入と local 再代入 ([expr/mod.rs](../crates/ilang-types/src/checker/expr/mod.rs) の Assign arm 2 経路)、 `check_array_with_hint` 各要素、 `check_map_lit_with_hint` 各値 ([casts.rs](../crates/ilang-types/src/checker/expr/casts.rs))。 これで 2 型パラメータ enum を field/配列/Map に格納でき、 `err`/`nope` 混在配列も通る。
- **検証**: Result/Either を field (明示+bare init+再代入)・配列リテラル (ok+err 混在)・Map 値に heap payload 込みで格納し deinit 厳密 (400/round) + churn delta=0。 1 型パラメータ enum・local/return は回帰なし。 fixture: `05_edge_cases/enum_ctor_type_arg_refine_in_stores.il`。 workspace nextest 539/539、 ilang-types 75/75、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [確認済み記録] 第 47 弾: interface covariance + downcast — 全て健全 (2026-06-13)

第 46 弾で interface 分岐 join を直した後、 interface の **covariance (実装クラスを親 interface スロットへ)** と **downcast** を網羅 probe。 **新規バグなし** — subclass 用の wrap/covariance/downcast 機構が interface にも generalize:

- **covariance**: `Circle → Shape?` (Optional wrap・bare field 代入)、 enum payload `shape: (Shape)` への Circle、 `Map<string, Shape>` への Circle/Square 混在リテラル、 `Circle[]/Square[] → Shape[]?` 入れ子コンテナ (第 28 の interface 版)、 `Map<string, Shape[]?>`、 generic `Holder<Shape>` への Circle、 `(Shape, i64)` tuple 要素、 `(Shape?, i64)[]` への tuple index store (slot0 wrap + interface)。 全て Optional/コンテナ越し dispatch で実体確認しつつ deinit 厳密。
- **downcast**: `as?` (NOT `as` — `as` は upcast/プリミティブ専用で downcast は型エラー)。 `sh as? Circle` で interface→具象に downcast、 成功は `some(circle)` で**共有** (二重解放なし)、 失敗 (別実装) は `none` (leak なし)。 クラス源 `a as? Dog` も同様。 `is T` の型テストは動くが**フロー絞り込みは無い** (`if a is Dog { a.fetch() }` は不可・`a as? Dog` で明示 downcast する仕様)。
- 計測の `delta=56` は §4-1 の `acc` 罠、 deinit は全形で厳密一致。

fixture: `09_subtyping/interface_covariance_and_downcast.il` (Optional/enum/Map/tuple/generic covariance + `as?` downcast を厳密 deinit 700 + churn delta=0)。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [解決済み記録] 第 46 弾: 分岐 join が共通 interface に合流しない (2026-06-13、 ユーザー判断 = あるべき形に修正)

interface dispatch を probe して発見した型推論バグ:

- **症状**: `if c { new Circle() } else { new Square() }` のように **異なる interface 実装クラス**を返す `if`/`match`/配列リテラル/Map リテラルが、 両者が共通 interface `Shape` を実装していても **「type mismatch: expected Circle, got Square」で拒否**。 戻り値型 `Shape` や `let r: Shape = if..` の明示注釈があっても同じ。 **共通親クラスのサブクラス** (`Dog`/`Cat` → `Animal`) の同じ分岐は**動く**ので、 「クラス階層は join するが interface は join しない」非対称。
- **原因**: 分岐/リテラルの object-join が [utils.rs](../crates/ilang-types/src/checker/utils.rs) の `common_ancestor` (= クラス階層の共通祖先) しか見ておらず、 共通 interface を join 先候補にしていなかった。 該当箇所は `unify_branch_obj` (match)、 `check_if_expr` の class_join (typed if)、 `unify_optional_branches` の Object arm、 配列リテラル要素 join / Map 値 join ([casts.rs](../crates/ilang-types/src/checker/expr/casts.rs))、 非 typed if の join ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)) の計 6 箇所。
- **修正 (ユーザー判断 = あるべき形)**: `common_object_join(a, b)` を新設 — まず `common_ancestor`、 無ければ **両クラスが実装する共通 interface**を探し、 **唯一なら**それを返す。 6 箇所すべてを `common_ancestor` から `common_object_join` へ切替。 これで if/match/配列/Map のどの分岐でも「親クラス or 唯一の共通 interface」へ合流。 **複数共通 interface**は join 先が曖昧 (この時点で期待型が無く一意に選べない) なので `None` を返し従来どおり型エラー — 注釈/構造変更で回避 (`branch_join_common_interface_ambiguous_error.il` で pin)。 共通点が無ければ従来どおりエラー (回帰なし)。
- **検証**: 唯一共通 interface の if/match/配列リテラル合流 + interface 経由 heap 返しメソッドの dispatch を deinit 厳密 (400/round) + delta=0。 複数共通 interface = clean な型エラー、 共通なし = 従来エラー。 multi-interface 実装クラス・interface 配列の for-in dispatch も健全。 fixture: `09_subtyping/branch_join_common_interface.il` (if/match/array 合流 + ARC)、 `09_subtyping/branch_join_common_interface_ambiguous_error.il` (複数 interface 曖昧)。 syntax.md / syntax_ja.md の interface 節に branch join を追記。 workspace nextest 539/539、 ilang-types 75/75、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [確認済み記録] 第 45 弾: weak 参照 + 固定長配列 × wrap — 全て健全 (2026-06-13)

weak と固定長配列 (第 19 弾の rc 表現) を、 最近の wrap 修正と交差させて probe。 **新規バグなし**:

- **weak.get() の昇格**: strong 生存中は `some(値)`、 死後は `none` (dangling weak の .get() は crash せず死を検知)。
- **weak? back-ref サイクル** (child が parent を weak 参照): 両ノード解放・サイクル leak なし。
- **parent-owns-child (strong) + child-weak-parent**: parent 死亡カスケードが child を解放する最中に child の weak-parent 解放が走っても **二重解放しない** (d3b1d2cf の修正が保持)。
- **weak 配列** `Box.weak[]`: strong は所有者の scope で死に、 weak 配列 drop は strong を release しない (deinit 厳密)。
- **固定長配列 × 要素 wrap**: `Box?[2]` のリテラル (`[some(box), none]`) と index store (`arr[i] = box` の `Box → Box?`)、 `(Box?, Box)[2]` への tuple index store (slot0 wrap)。 第 36/41 の wrap 修正が **固定長配列の rc 表現にも generalize** していることを Optional 越し match で実体確認しつつ deinit 厳密 (700/round) + delta=0。

fixture: `05_edge_cases/fixed_array_optional_tuple_wrap.il` (固定長 Optional/tuple 要素 wrap を厳密 deinit 700 + churn delta=0)。 weak 系は既存 fixture (`arc_cycle_via_weak` / `weak_backref_cascade_release_order` / `leak_weak_*`) が pin 済みのため追加せず。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [確認済み記録] 第 44 弾: string / array ARC 全方位 — 第 43 弾以外は健全 (2026-06-13)

第 43 弾の inplace concat 修正の周辺と、 触っていなかった string/array 操作を網羅 probe。 **新規バグなし**:

- **string メソッド連鎖** `("v"+n).toUpper().slice(0,3)`: fresh 中間文字列が全て解放 (delta=0)。
- **template literal の heap 補間** `\`box ${b.toString()} val ${b.n}\``: 補間 temp と Box を解放 (string delta=0・box deinit 厳密)。
- **`+=` desugar**: `s += n.toString()` は `s = s + ...` に展開され第 43 弾の fix が効く (delta=0・値正しい)。
- **self-concat `s = s + s`**: inplace パターンに合致 (lhs Var==target) するが rhs は **借用** (`is_fresh_object_expr(Var)` = false) なので解放しない — rhs が lhs バッファをエイリアスし結果になるため正しい。 prepend `s = fresh + s` は inplace 非マッチで通常 concat 経路 (既存 fix)。 両者 delta=0。
- **array push / unshift** の fresh 要素、 **map** コールバックの fresh heap 返り、 **split** の string[]: 全て deinit 厳密・leak なし。
- **heap-kind 変数の fresh 再代入** (`m = mkMap(...)` / `arr = mkArr(...)`): 旧値解放・新 fresh 非 leak (deinit 厳密)。

fixture: `05_edge_cases/string_arc_chains_template_selfconcat.il` (メソッド連鎖・template heap 補間・`+=`・self-concat・split を churn 500 で `liveStringCount` flat + deinit 厳密)。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [解決済み記録] 第 43 弾: inplace 文字列 concat が fresh rhs をリーク (2026-06-13)

tuple/generic を離れて string バッファ ARC を probe して発見:

- **症状**: `s = s + n.toString()` を回すと **1 文字列/反復のリニアな leak** (`liveStringCount` で N=100→100・200→200・400→400)。 `s = s + "-end"` (リテラル rhs) や `let s2 = s + n.toString()` (再代入なし) は無事 — leak は **inplace 再代入 × fresh rhs** に限定。
- **原因**: `s = s + <expr>` (s が string Local) は [expr.rs](../crates/ilang-mir/src/lower/expr.rs) の `Assign` arm で **`StrConcatInplace`** にルートされ、 `s` のバッファを doubling realloc で伸ばして rhs バイトを**追記**する。 rhs を**コピーするだけで消費しない**のに、 lowering が rhs を解放していなかった。 fresh rhs (`toString()` の temp・fresh concat) は +1 を持つので、 それが宙に浮いて毎反復 registry に溜まる。 リテラルは intern 済み (非所有)、 借用 var は所有者が解放するので、 どちらも fresh でなく無事だった。
- **修正**: `StrConcatInplace` の直後に **rhs が fresh なら Release** (`is_fresh_object_expr(rhs)` で判定、 op が rhs を読み終えた後)。 借用 rhs・リテラル rhs は非 fresh なので不変 (過剰解放しない)。
- **同族確認**: fresh rhs (toString / `(a + b)` の fresh concat) は churn 1000 で delta=0、 借用 rhs (`s = s + other` を 2 回) は値正しく (`aBBBB2`) churn delta=0 で過剰解放なし、 値の非破壊 (`v5-end`) を確認。 非再代入の `let s = a + b` は別パス (lower_binary StrConcat、 既存 fixture `leak_string_concat_loop.il` が pin 済み) で元から健全。
- fixture: `05_edge_cases/leak_string_concat_inplace_reassign.il` (fresh / 借用 rhs を churn 1000 で `liveStringCount` 増加 < 50)。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [確認済み記録] 第 42 弾: tuple 要素 wrap の残る store サイト — 全て健全 (2026-06-13)

第 36 弾 (bare field) / 第 41 弾 (index store) で composite リテラルの要素 wrap を直した後、 **同じ `(Box?, Box)` 形を残りの全 store サイト**で攻めた。 **新規バグなし** — これらは元から `lower_composite_with_hint` / `lower_arg_to` 経由でヒントを受けており健全:

- **return 位置** `fn(): (Box?, Box) { (box, b) }`、 **`some(tuple)`** (`(Box?, Box)?` への some)、 **enum payload** (`variant: ((Box?, Box))`)、 **引数位置** `consume((box, b))`、 **local 再代入** `t = (box, b)`、 **明示 field 代入** `this.pair = (box, b)`: 全て slot0 を `Box → Box?` wrap して値正しく、 deinit 厳密・delta=0。
- **入れ子** `((Box?, Box), Box?)` (内側 slot0 + 外側 slot1 の二重 wrap): 値正しく ARC 均衡。
- **tuple 内 weak 要素** `(Box.weak, Box)`: slot0 の strong→weak coerce が strong を **retain で漏らさない** (weak ref が指す strong box は所有者の scope で死ぬ・deinit 2/round)。 weak への直接 `match` は第 34 弾どおり checker が拒否 (バグでなく既存制約)。

これで composite (tuple) 要素 wrap は **全 store サイト** (let / 引数 / return / 再代入 / field (bare + 明示) / index (array + map) / some / enum payload / 入れ子 / weak) で網羅確認済み。 fixture: `05_edge_cases/tuple_element_wrap_all_stores.il` (return/some/enum/arg/reassign/field/nested を厳密 deinit 1700 + churn delta=0)。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [解決済み記録] 第 41 弾: index 代入の RHS が composite 要素 wrap を欠いて SIGSEGV (2026-06-13)

tuple をコンテナに入れる ARC を probe する過程で発見。 第 36 弾 (bare field 代入) と同じ「composite リテラルの要素 wrap 欠落」が、 **index 代入経路**に残っていた:

- **症状**: `arr[0] = (new Box(7), new Box(3))` (`arr: (Box?, Box)[]`) や `m["k"] = (...)` (`Map<_, (Box?, Box)>`) が **SIGSEGV**。 配列/Map の **リテラル**形 (`[(box, b)]` / `{"k": (box, b)}`) と **bare tuple** (`let t: (Box?, Box) = (box, b)`) は無事、 **index 代入だけ**落ちる。
- **原因**: [expr.rs](../crates/ilang-mir/src/lower/expr.rs) の `AssignIndex` が RHS を **ヒント無し**で lower し (`lower_expr(value)`)、 その後 slot 型へ **値全体を `coerce`** していた。 `coerce` は値自身の `T → T?` wrap しか見ず、 tuple の **内側要素** (`(Box, Box) → (Box?, Box)` の slot0) を wrap しないため、 生 Box が `Box?` slot に入り解放カスケードで不整列ポインタ参照 → crash。
- **修正**: `AssignIndex` の RHS を **slot の要素型 (Array の elem / Map の val) をヒントに `lower_composite_with_hint`** で lower (第 36 弾の bare field・第 25 弾の container store と同じ役割分担)。 tuple/array/map リテラルが宣言要素型で構築され、 slot0 が `lower_tuple_literal_with_hint` で wrap される。 非 composite RHS は従来どおり `lower_expr` にフォールバック。
- **同族確認**: array index 代入・Map index 代入の両方で `(Box?, Box)` slot0 wrap を Optional 越し match で実体確認しつつ deinit 厳密 (array 500/round・map churn)・delta=0。 array/Map の heap tuple リテラル・要素上書き (旧 tuple の Box 解放)・Optional<heap tuple> も probe して**元から健全**。
- fixture: `05_edge_cases/index_store_tuple_element_wrap.il` (array/Map の index 代入 tuple 要素 wrap を厳密 deinit 500 + churn delta=0)。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 40 弾: tuple 再帰ギャップの同族 3 関数 — 第 39 弾の取りこぼし (2026-06-13)

第 39 弾は `subst_type` / `contains_type_var` (型パラメータ `T` の置換) に tuple arm を足したが、 **§8-6 同族探索が不完全**で、 同じ tuple 再帰ギャップを持つ rewrite 群を取りこぼしていた。 generic enum を probe する過程で顕在化:

- **症状**: `(Inner<i64>, i64)` (generic class) や `(Maybe<i64>, i64)` (generic enum) を **class field / fn param / fn return** に使うと **「mir lower: unsupported in M1: user-defined generic types」で停止**。 第 39 弾の `T` 置換と違い、 これは **concrete instantiation の mangle** (`Generic(Inner,[i64]) → Object("Inner_i64")`) の漏れ。 `(T, T)` の **local** 注釈や、 **tuple でない** `Inner<i64>` field は動く (前者は binding 型を rhs から再導出、 後者は tuple を経由しない) ため第 39 弾に隠れていた。
- **原因**: tuple を再帰しない関数が **3 つ**残っていた — [class.rs](../crates/ilang-mir/src/monomorphize/class.rs) の `rewrite_type` (user generic class を mangled Object 名へ)、 [enums.rs](../crates/ilang-mir/src/monomorphize/enums.rs) の `rewrite_enum_refs_in_type` (generic enum を mangle)、 [walk.rs](../crates/ilang-mir/src/monomorphize/walk.rs) の `walk_types_pre` (tuple 内にしか現れない instantiation の発見)。 いずれも `Type::Tuple` arm が無く `_ => clone()` / `_ => {}` に落ちていた。
- **修正**: 3 関数すべてに `Type::Tuple` arm を追加 (要素を再帰)。 `seed_enums_in_type` は `walk_types_pre` 経由なので自動的に直る。 monomorphize モジュールの型再帰関数 (`Type::Optional` を持つもの) を全数監査し、 残る漏れが無いことを確認。
- **同族確認**: generic class / generic enum が **heap Box を保持**して tuple field に入る形を deinit 厳密 (200/round) + churn delta=0、 nested `((Inner<i64>, i64), i64)`、 fn param/return、 generic enum tuple payload (`yep: (T)`) の再確認。 全て値正しく ARC 均衡。
- fixture: `05_edge_cases/generic_in_tuple_field.il` (generic class/enum を heap 込みで tuple field・param・return に置いて厳密 deinit 200 + churn delta=0)。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 39 弾: monomorphize が tuple 型の中の型パラメータを置換しない (2026-06-13)

generic × heap × 第 36 弾の tuple wrap を probe する過程で発見:

- **症状**: generic fn / method のシグネチャ (param / return) や generic class の field に **tuple of T** (`(T, T)` / `(T?, T)` / `((T, T), T)`) を使うと、 呼び出し (= monomorphize) 時に **「mir lower: unknown type: T」で lowering 停止**。 `T?` / `T[]` / `Map<string, T>` は置換されるのに tuple だけ。 `let p: (T, T)` という **local 注釈は動く** (local はシグネチャに乗らない = monomorphize 対象外) ため見つかりにくかった。
- **原因**: [monomorphize/class.rs](../crates/ilang-mir/src/monomorphize/class.rs) の `subst_type` が Object/TypeVar・Generic・Array・Optional・Weak・Fn は再帰置換するが **`Type::Tuple` の arm が無く** `_ => t.clone()` に落ちて、 tuple の要素にある `Object("T")` がそのまま生き残る。 兄弟関数 `contains_type_var` も同じく Tuple を見落としており、 tuple を包む generic の mangle 判定も誤っていた。
- **修正**: 両関数に `Type::Tuple(elems)` arm を追加 (`subst_type` は各要素を再帰置換して再構築、 `contains_type_var` は要素のいずれかが型変数を含むか)。 Map は `Type::Generic` (base=Map) 経由で元から置換されていた。
- **同族確認**: generic fn の `(T, T)` param / return、 method 戻り値 `(T, T)`、 入れ子 `((T, T), T)`、 tuple 内 Optional/array `(T?, T[])`、 generic class の `(T?, T)` field を bare 代入 (monomorphize 後の T→T? 要素 wrap も含めて第 36 弾の経路と合流) — 全て値正しく ARC 厳密 (deinit 500/round + churn delta=0)。 generic 本体内で opaque T の `.n` には触れない (checker の既存制約、 バグではない)。
- fixture: `05_edge_cases/generic_tuple_type_param.il` (fn param/return・nested・generic class tuple field wrap を厳密 deinit 500 + churn delta=0)。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [確認済み記録] 第 38 弾: async ARC 全方位 — 第 37 弾以外は健全 (2026-06-13)

第 37 弾の exit-drain 修正の周辺と、 触っていなかった async 経路を網羅 probe。 **新規バグなし**:

- **timer (setTimeout) at exit**: heap を捕獲した closure を持つ timer を未 tick で残しても、 exit 時の blocking drain (`pool::drain` の `TimerStep::Wait => sleep`) が期限を待って発火 → closure・heap を解放。 `time.sleep` + `time.tick` で観測すると log 反映 + deinit 1。 clean exit (第 37 弾の drain が timer 経路も覆う)。
- **Promise.all over heap** (3 個の `Promise.resolve(new Box(_))` を then で集約): 値正しく (1+2+3=6)・deinit 3・leak なし。
- **Promise を class field 保持** + then、 **multi-await chain** (`await fetch(10)` → `await fetch(20)`): 値・ARC とも健全。
- **never-settled promise が executor closure で heap Box を捕獲**して即捨て: churn 100 で deinit 100・delta=0 (promise/scope 死亡で capture も解放)。
- **await rejection が await 前の heap local を解放**: `risky(pr)` が `let guard = new Box(99)` の後 `await pr` で reject すると残り (`guard.n + v`) は実行されないが、 guard は解放される (第 11 弾の早期脱出 sweep が await-reject 経路でも効く)。 churn で guard 数と deinit が厳密一致・delta=0。
- **reject 経路の shutdown drain**: reject する promise を未 tick で exit に残し、 guard の deinit が top-level 配列に触っても **clean exit** (第 37 弾の drain は resolve だけでなく reject の継続も流す)。

fixture: `04_modules/await_reject_releases_pre_await_heap.il` (await-rejection の guard 解放を明示 tick で deinit 検証 + 未 tick の rejecting promise で reject 経路 shutdown drain を踏む)。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [解決済み記録] 第 37 弾: exit 時の event-loop drain がグローバル解放の後に走り OOB panic (2026-06-13)

bare field 系統を離れ async を probe した初手で発見した**プロセス終了時の解放順序バグ**:

- **症状**: pending な Promise 継続が保持する heap オブジェクトの `deinit` が top-level グローバル (`deinits: i64[]`) に触ると、 プログラム終了時に **ランタイム OOB panic (`index out of bounds`、 Rust backtrace なし)**。 最小再現は「async fn が heap 返り async fn を `await` (Box を継続が跨いで保持)、 Box の `deinit` が top-level 配列にアクセス」で `.then` を 1 個登録するだけ (N=1 で再現)。 `time.tick()` で終了前にドレインすれば回避できる (= 順序の問題と確定)。
- **原因**: drain ([`$promise.drain`](../crates/ilang-runtime/src/promises.rs)) が **`__main` の return 後**に走っていた — JIT は [`run_main`](../crates/ilang-mir-codegen/src/compile/mod.rs) が `f()` の後に `__promise_drain()`、 AOT は C `main` ラッパが `entry` の後に呼ぶ。 ところが `__main` のエピローグは **return 前に top-level let を解放** する ([bodies.rs](../crates/ilang-mir/src/lower/decl/bodies.rs) の top_scope release)。 結果、 グローバル `deinits` 配列が先に free され、 その後の drain が継続を完了 → Box 解放 → `deinit` が解放済み配列を参照 → abort。
- **修正**: `__main` 本体の **top-level let 解放ループの直前**に `promise_drain` を emit ([bodies.rs](../crates/ilang-mir/src/lower/decl/bodies.rs))。 グローバル生存中に継続をドレインするので `deinit` が安全。 共有 MIR lowering 経由なので JIT / AOT 両方に効く。 外側の drain (`run_main` / AOT ラッパ) はキュー空で no-op として残置 (安全網)。 builtin 名は generic テーブルの `promise_drain` (`$promise.drain` ではない) を使用。
- **検証**: `time.tick()` で明示ドレインすると `deinit` 数が観測でき (3 promises → deinits=3、 100 churn → 100、 leak なし)、 未ドレインの pending promise を残しても **clean exit (panic 解消)**。 fixture: `04_modules/pending_promise_exit_drain_order.il` (明示 tick で deinit 数検証 + 未 tick の pending promise で shutdown drain を踏む)。 workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 36 弾: bare field 代入が composite リテラルの要素 wrap を欠いて SIGSEGV (2026-06-13)

第 34/35 弾で「bare field 代入は健全」と確認したが、 **composite リテラルの内側要素が wrap を要する**形は未踏だった。 enum payload / tuple を probe する過程で発見:

- **症状**: `pair = (a, b)` (bare、 field 型 `(Box?, Box)`) が **SIGSEGV (exit 139)**。 明示 `this.pair = (a, b)` は正常 (10)。 同型 array (`arr = [box]`、 field `Box?[]`)・map (`m = {"k": box}`、 field `Map<string, Box?>`) も同根。
- **原因**: bare field 代入は早期 lowering 済みの値を再利用する (第 33 弾) が、 その早期 lowering は target が field のとき **ヒント無し** (`target_ty` が None) で走るため、 tuple リテラル `(a, b)` が宣言要素型 `(Box?, Box)` でなく**推論型 `(Box, Box)` で構築**され、 slot 0 の `Box → Box?` 要素 wrap が起きない。 生 Box が `Box?` slot に入り、 解放カスケードが Optional として走査 → 不整列ポインタ参照でクラッシュ。 `store_value_to_field` の coerce は**値全体**の `T → T?` しか見ないため内側要素は救えない。 明示 AssignField は `lower_composite_with_hint(value, &fty)` を常に使うので無事だった。
- **修正**: bare field のとき (`target_ty` が None かつ `this_class` の field) に **field 型を composite リテラルのヒントとして供給** ([expr.rs](../crates/ilang-mir/src/lower/expr.rs) の Assign arm、 `bare_field_hint`)。 `lower_composite_with_hint` が tuple/array/map の要素を宣言型へ wrap して構築する。 外側 `T → T?` wrap は従来どおり `store_value_to_field` が担当 (役割分担: 内側 = リテラル構築ヒント、 外側 = store ヘルパー)。
- **同族確認**: tuple `(Box?, Box)`・array `Box?[]`・map `Map<string, Box?>` の bare 代入を、 各 Optional 越しの match で実体が壊れていないことを確認しつつ deinit 厳密 (500/round) + churn delta=0。 第 33〜35 弾の bare 代入 probe 群 (p5/q1/x1/s8) も全て回帰なし。 enum payload を field に持つ bare 代入・再代入 churn・match 持ち出し・Optional<enum>、 tuple の destructuring・入れ子 tuple return は別途 probe して**元から健全**。
- fixture: `05_edge_cases/bare_field_assign_composite_element_wrap.il` (tuple/array/map 要素 wrap を厳密 deinit 500 + churn delta=0)。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [確認済み記録] 第 35 弾: bare field 書き × 反復ミューテーション・多段連鎖 — 新規バグなし (2026-06-13)

第 32 弾の bare field 書きと第 29/30 弾の連鎖レシーバ解放の継ぎ目を、 反復・escape・多段で攻めた。 **全て健全**:

- **init 内 bare 再代入の `is_init` 安全性**: implicit-this bare 代入 arm は `is_init` を常に `false` で渡す (= 旧スロットを LoadField して Release)。 一見「init のゼロスロットを誤 release」しそうだが、 **checker が「init での初回 field 代入は `this.field = ...` の形」を要求** するため (`slot = ..` だけの init は「field is not assigned」で拒否)、 bare 書きが届く時点でスロットは既に実値を持つ → 旧値 release は正しい。 `this.slot = a` の後 `slot = b` で再代入する形を deinit 厳密 (2/round) で確認。
- **fixed-array field の bare 代入** (`items = other.items`、 非 fresh エイリアス源): `store_value_to_field` の fixed-array arm が copyShallow で値コピー (新バッファ + 要素 retain)、 旧バッファ release。 deinit 厳密 (4/round)。
- **多段メソッド連鎖**: 4 段 `mk().bump().bump().bump()` の中間 receiver 3 個解放、 **field を返す連鎖** `h.getInner().val()` (tail-borrow で +1 された中間)、 self 連鎖、 fresh holder 連鎖。 全て deinit 厳密。
- **エスケープしたステートフルクロージャの heap field 反復付け替え**: `s.updater()` が返すクロージャを 4 回呼んで毎回 `slot = new Box(x)`。 各呼び出しで前の Box を release、 object + closure 死亡で最後を drop。 1 round 5 Box 全死亡を deinit 厳密で確認 (第 32 弾修正の反復ミューテーション下の堅牢性)。
- 計測で繰り返し出た `delta=56` は全て **§4-1 の罠** (計測開始後に確保した `acc: i64[]`)。 deinit が常に厳密一致でオブジェクト leak なし。

fixture: `05_edge_cases/closure_bare_field_reassign_churn.il` (ステートフルクロージャ反復 + init bare 再代入を厳密 deinit 700 + churn delta=0)。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [確認済み記録] 第 34 弾: bare field 代入 × 宣言型が要る RHS — 新規バグなし + 第 33 弾の訂正 (2026-06-13)

第 33 弾で bare / 明示の field 代入を `store_value_to_field` に統合した直後、 その継ぎ目を「bare 名の field 代入に、 plain `T → T?` を超えて宣言型が要る RHS」で攻めた。 bare 経路は早期 lowering 済みの値 (field ヒント無し) を再利用するため、 ヒントが要る形が壊れないかを確認 — **全て健全**:

- **共変 map リテラル** `animals = {"d": new Dog()}` (field `Map<string, Animal>`)、 **共変配列リテラル** `members = [new Dog(), new Dog()]` (field `Animal[]`): 値正しく (Optional/コンテナ越しに `Dog.val()=10` でディスパッチ確認)、 deinit 厳密・churn delta=0。 ヒント無し lowering でも要素はオブジェクトなので runtime repr が一致し、 helper の store がそのまま通る。
- **fresh Optional RHS** `slot = makeOpt(7)` (Optional→Optional 同型、 二重 retain なし)、 **Optional widen** `pet = d` (`Dog? → Animal?`、 borrowed): deinit 厳密・delta=0。
- 計測で出た `delta=56` は **§4-1 の罠** (計測開始後に確保した `acc: i64[]` 自身) で、 `acc` を計測前に移すと delta=0。 deinit が常に厳密一致でオブジェクト leak が無いことは独立に確認済み。

**第 33 弾の誤記録を訂正**: 第 33 弾の同族確認に「weak field の bare 代入は checker が拒否・明示 `this.w = b` は通る非対称」と書いたが**誤り**。 実際に拒否されていたのは `this.w = none` (plain `Box.weak` を `none` で初期化) で、 これは **bare / 明示によらず同じ**挙動 (`w = b` strong→weak は両方通る)。 plain weak は nullable でなく、 nullable な weak は `Box.weak?` (Optional weak) を使う設計 — `Box.weak?` への `none` は通る。 docs/syntax_ja.md:988「空マップは `new Map<K,V>()` で構築 (`{}` は空ブロック扱い)」と同様、 これらは意図的な制限であってバグではない。

fixture: `05_edge_cases/bare_field_assign_composite_widen.il` (共変 map/配列・fresh Optional・widen を厳密 deinit 500 + churn delta=0)。 **ソース変更なし**のため第 24 弾と同じく workspace / nested_generic 儀式は省略、 programs fixture を JIT・AOT 両経路で確認。

### [解決済み記録] 第 33 弾: implicit bare-name field 代入が Optional / subtype wrap を欠いて SIGSEGV (2026-06-13)

第 32 弾で bare field 代入の `this` 解決を直した直後、 同じ implicit `this.<field>` 代入 arm を「代入する値の型 × field 型」軸で攻めて **既存のクラッシュ**を検出。 明示 `this.f = v` (`AssignField` arm) は第 22 / 26 弾で `T → T?` / subtype の Optional 自動 wrap を入れたのに、 **bare 名の `f = v` 経路は退化した store を別に持っていて wrap を一切しなかった**。

- **症状**: `slot = box` (field 型 `Box?`、 `this.` なし) が **生オブジェクトを Optional slot に格納**し、 オブジェクト解放時のカスケードがそれを Optional として走査 → 不整列ポインタ参照で **SIGSEGV (exit 139)**。 明示 `this.slot = box` は正常 (AssignField が wrap する) という非対称で確定。 subtype (`Dog` を `Animal?` field へ) も同様。
- **原因**: bare field 代入は他の代入経路 (local / cell capture / repl slot) を全部抜けた後の implicit-this arm に **早期 lowering 時の生の `v` / `vty`** で到達し、 field 型への coerce/wrap を踏まずに `StoreField` していた。 AssignField が持つ wrap / weak / fixed-array value-copy / CReprEnum drop を全部欠いていた。
- **修正**: AssignField の store ロジック (wrap 述語 → fixed-array → retain/release → StoreField → CReprEnum drop) を共通ヘルパー **`store_value_to_field`** ([expr.rs](../crates/ilang-mir/src/lower/expr.rs)) に抽出し、 **AssignField と implicit-this bare 代入の両方から呼ぶ**。 bare 経路は早期 lowering 済みの `v` / `vty` を再利用 (再 lower すると `new Box()` を二重確保するため) し、 ヘルパーの coerce が Optional wrap を行う。
- **同族確認**: `T → T?` 同型 wrap・subtype `Dog → Animal?` (Optional 越しの仮想ディスパッチ `Dog.val()=10` で実体が壊れていないことを確認)・**クロージャ内の bare Optional 代入** (第 32 弾の capture 経路と合流) を値・ARC 厳密一致 (deinit 300 + churn delta=0)。 fixed-array field の bare 代入 (fresh リテラル源) と primitive field は元から修正経路で健全。 strong→weak の bare 代入 (`w = b`、 field 型 `Box.weak`) は通る。 **(第 34 弾で訂正)** 当初ここに「weak の bare 代入は checker が拒否・明示は通る非対称」と書いたが誤り — 拒否されていたのは `this.w = none` (plain `Box.weak` を `none` で初期化) で、 bare/明示によらず同じ挙動。 plain weak は nullable でなく nullable weak は `Box.weak?` を使う設計 (第 34 弾で確認、 意図的制限)。
- fixture: `05_edge_cases/bare_field_assign_optional_wrap.il` (同型 wrap / subtype / closure 内を厳密 deinit 300 + churn delta=0)。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] 第 32 弾: メソッド内クロージャの `this` 解決が欠けていた 2 経路 — bare field 代入 (修正) と bare メソッド呼び出し (診断化) (2026-06-13)

第 31 弾で「メソッド内クロージャが field を **bare 名で読む**」経路に `this` の on-demand 捕獲を入れた。その同族として **`this` を必要とする残り 2 経路**が `lookup_var("this").unwrap()` を素のまま使っており、クロージャ内 (= `this` が local でなく capture 経由) で **`Option::unwrap()` on None のコンパイラ panic** を踏むことを probe で確認。両者とも「読み経路は capture フォールバックを持つのに、この経路は持たない」同根:

1. **bare field 代入 `slot = nb` が panic (修正)** — [expr.rs](../crates/ilang-mir/src/lower/expr.rs) の implicit `this.<field>` 代入 arm。 bare field **読み** (lower_var_expr) は `lookup_var → captures_in_scope` のフォールバックを持つのに、代入の store サイトは `lookup_var("this").unwrap()` 直書きだった。 `this` 自体は既に捕獲されている (代入 target が free var として収集され [collect.rs](../crates/ilang-mir/src/lower/collect.rs)、 `name_is_this_member` 経由で `this` が `frees` に入る) ので、 **読み経路と同じフォールバックを store サイトにミラーするだけ**で動く。 ARC は従来どおり (旧値 release・新値 own)。 fixture: `05_edge_cases/closure_bare_field_assign.il` (heap field 付け替え + primitive field を厳密 deinit 200 + churn delta=0)。

2. **bare メソッド呼び出し `compute()` が panic → クリーンな診断 (ユーザー決定)** — [call_fn.rs](../crates/ilang-mir/src/lower/call_fn.rs) の implicit `this.<method>(args)` arm。 第 31 弾の記録にあった「メソッドを `name_is_this_member` に含めると新規 panic を生む」の正体がこの素の `unwrap()` だった (含めると `this` 捕獲が起きてこの arm に到達し panic)。 **ユーザー判断 = メソッドは capture 対象外の既存決定を維持し、 panic を明示エラーに置換** (「bare method call \`compute(...)\` inside a closure is not supported; write \`this.compute(...)\`」)。 明示形 `this.compute()` はクロージャ内でも従来どおり動く。 fixture: `05_edge_cases/closure_bare_method_call_error.il` (expect-error)。

同族確認: 同型の素 `unwrap()` は grep で上記 2 箇所のみ。 bare field の field-of-field 書き (`box.n = ..`) は AssignField 経由で obj が読み経路を通るため元から健全、 primitive field 書きも修正経路で正しい。 syntax.md / syntax_ja.md のクロージャ節に「メソッド内クロージャの `this` メンバ参照」を追記。 検証: workspace nextest 539/539、 AOT 全 fixture PASS、 nested_generic 100 並列 0 fail。

### [解決済み記録] gui ライブラリが platform イベントループから `time.tick()` を pump するようにした (2026-06-11、 ユーザー判断 = 案 (a))

JS 型移行の積み残しだった「`gui.run()` がネイティブループでブロックして drain ポイントを通らず、 GUI アプリの `time.setTimeout` / `setInterval` / Promise 継続がイベントループ実行中に発火しない」問題。 3 プラットフォームとも `runEventLoop` に ~15ms 周期の platform タイマーを仕込んで `time.tick()` を呼ぶ形で解消:

- **cocoa** ([libs/gui/cocoa/window.il](../libs/gui/cocoa/window.il)): repeating `NSTimer` (0.015s、 tolerance 0.005s) を **common modes** (`kCFRunLoopCommonModes`) で main run loop に登録。 default mode だとウィンドウドラッグ / メニュー追跡中に止まるため
- **win32** ([libs/gui/win32/window.il](../libs/gui/win32/window.il)): `SetTimer` を **TIMERPROC 付き** (`SetTimerWithProc` — `@symbol("SetTimer")` で別シグネチャを [bindings/windows/user32.il](../bindings/windows/user32.il) に追加) で登録。 NULL-hwnd の queue タイマーはモーダルループ (ドラッグ / メニュー) 中に捨てられるが、 TIMERPROC は `DispatchMessage` が直接呼ぶので届く。 加えてメッセージ dispatch ごとに `time.tick()` (0ms timeout / 新規継続の即時サービス)。 ループ終了後 `KillTimer`
- **linux** ([libs/gui/linux/window.il](../libs/gui/linux/window.il)): `g_timeout_add(15, linux_pump_tick, ...)`。 `g_timeout_add` は GSourceFunc callback 型のため GIR generator が出せず、 [bindings/gtk4/manual.il](../bindings/gtk4/manual.il) に手書き追加 (signal-connect 変種と同じ理由)

**検証**: macOS は実機確認済み — headless の Foundation run loop テスト (`NSTimer` common modes pump + `NSRunLoop.runUntilDate` 中に `setTimeout` 発火、 AppKit/ウィンドウなし) と `window_state_demo` の AOT ビルド。 **win32 / linux はこの macOS 環境では型検査自体が走らない** (gui_impl の per-OS deps で cocoa しか load されない + gtk4 / pkg-config なし) ため未検証 — 既存コードの型付け慣例 (callback は top-level `pub fn` を名前渡し、 i32 戻りは `0 as i32` 形) に合わせて書いたが、 Windows / Linux 機で `gtk4_bindings` テストと GUI ビルドを通すこと。

タイマー分解能は pump 周期の ~15ms (gui.run の doc コメントに明記)。 sdl_breakout のような自前フレームループのアプリは従来どおり自分で `time.tick()` を呼ぶ。

### [解決済み記録] fixture 増殖ラウンド第 10 弾: Promise.all/race の入力配列・結果配列の所有権、fresh promise 引数 (2026-06-11)

probe 対象を Promise.all/race の ARC (途中 reject の部分値・race 敗者の値・settle されないまま捨てる形)、 enum/tuple payload の promise、 promise を持つ容器 / class field + deinit、 await × deferred の churn、 REPL チャンク跨ぎへ広げた。 3 系統のバグ (すべて移行前から — ただし 2 は第 9 弾までの修正で顕在化):

1. **`Promise.all` / `race` が入力配列を消費しない** (既存)。 lowering ([builtin_static.rs](../crates/ilang-mir/src/lower/calls/builtin_static.rs) `lower_promise_combinator`) は executor と同じ譲渡規約 (非 fresh に Retain) なのに runtime が release せず、 呼び出しごとに入力配列が leak。 fresh literal 入力では upstream promise (と settled 値) も配列に pin されたまま永久に残った。 修正: `__promise_all` / `__promise_race` が登録完了後に `release_input_array` (KIND_ARRAY cascade)。 pending upstream は「escape した resolve callback の capture」経由で生存するか、 waiter ごと死ぬ (発火し得ないので観測上同一)。
2. **`Promise.all` の結果配列が要素の所有権を持っていなかった** (既存の潜在バグ — 1 の修正で UAF として顕在化)。 `build_i64_array` は raw slot を retain せずコピーするのに、 resolve stub が slot retain を構築後に release しており、 要素の所有者が不在。 旧実装では leak した upstream の Resolved state が偶然値を支えて読めていたが、 入力配列の消費開始で upstream が正しく死ぬと **heap 要素の `vs[i]` が解放済みメモリ** (`vs[0].n` が非決定な garbage)。 修正: slot retain をそのまま結果配列の要素所有分として移譲 (release ループを削除)。 reject 経路の「部分 slot の解放」はそのまま正しい。
3. **fresh promise を引数で渡すと leak — `needs_post_release` の 5 サイト全部に `Promise` が無かった** (既存)。 named-fn / closure / 暗黙 this / object method / `new` init のリストがそれぞれ手書きコピーで、 全部 Promise を欠いていた (init サイトには「promise field は retain しない」という**古い誤コメント**まで付いていた — 実際は `is_arc_slot` が Promise を含み field assign は retain する)。 `new Holder(Promise.resolve(new Box(k)))` で promise + settled 値が 1 個/call leak。 修正: 共通述語 `BodyCx::fresh_arg_needs_post_release` に一本化して 5 サイトを置き換え (Promise 込み)。

**probe 中に判明した独立の既存制限**: 対話 REPL は `use` 文を受け付けない (`<repl> mir: unexpected Item::Use post-loader`)。 std モジュール (time 等) が REPL から使えない。 async とは独立の機能ギャップとして未対応のまま (対応するか要ユーザー判断)。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 0/800。 回帰 fixture 3 件: `04_modules/promise_all_race_input_arc.il` (入力 churn 5 変種)、 `04_modules/promise_all_object_values.il` (heap 要素の値正しさ + churn — UAF の pin)、 `04_modules/promise_fresh_arg_release.il` (init / fn 引数の fresh promise churn + deinit 数)。

**probe で問題なしを確認した周辺** (再調査不要): enum payload (heap 側) の promise churn、 Map / 配列に promise を格納して捨てる churn、 promise class field + deinit (修正後 delta=0・deinit 200/200)、 await × deferred 400 周 (定数 56 bytes の一回きり初期化のみ、 線形成分なし)、 REPL のチャンク跨ぎ promise drain (`then` が後続チャンク前に発火)。

### [解決済み記録] 第 19 弾: 固定長配列 `T[N]` × heap 要素を正式サポート — 選択肢 (a) を実装 (2026-06-12)

第 18 弾の判断待ちに対しユーザーが **(a) フルサポート** を選択。実装した所有権モデル (ポインタ転送 + 値セマンティクス):

- **レイアウトの確定事実**: 固定長配列はヘッダ無しの `len*8` バッファ。非 CRepr class の field は **8 バイトのポインタスロット** (インライン展開は CRepr struct のみ — 当初インライン前提で書きかけて codegen 読解で訂正)。注釈なしリテラルは checker 上 `T[3]` に推論されても **lowering では動的配列に decay** する (`len_hint` は注釈起点でのみ立つ) — fixed の経路に入るのは注釈・field 型・param 型経由のみ。
- **所有権**: fresh リテラルを束縛した binding がバッファ + 要素 share を所有 ([stmt.rs](../crates/ilang-mir/src/lower/stmt.rs) が `fixed_owned_locals` に登録、 `crepr_owned_locals` と同型)。`let v = p.items` や param 渡しは**借用** — scope-exit / 早期脱出 sweep は owned のみ release ([body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs) / [control.rs](../crates/ilang-mir/src/lower/control.rs) のゲート)。
- **Release(fixed-of-arc)** は `$array.releaseFixed(ptr, len, elem_kind)` (要素 release + バッファ free) に lower ([arc.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/arc.rs))。primitive / CRepr 要素 (`u16[128]`, `Vertex[3]`) は従来どおり no-op (stride も 8 でないため)。
- **field 代入** ([expr.rs](../crates/ilang-mir/src/lower/expr.rs)): fresh リテラルは**ポインタ転送** (rc 操作ゼロ)、 非 fresh ソースは `$array.copyFixed` で**値コピー** (新バッファ + 要素 retain — ポインタを直接書くと 2 owner が 1 バッファを共有し二重 free)。旧 region は Release で要素 + バッファ解放 (init 書きはスキップ)。
- **object drop cascade**: field kind 表に**合成タグ** `KIND_FIXED_BASE(=1000) + len*16 + elem_kind` を導入 ([kind.rs](../crates/ilang-runtime/src/kind.rs) / [cascade.rs](../crates/ilang-runtime/src/cascade.rs) で decode、 JIT [print_kind.rs](../crates/ilang-mir-codegen/src/compile/print_kind.rs) `kind_tag_of` / AOT [helpers.rs](../crates/ilang-mir-codegen/src/aot/helpers.rs) `field_kind_tag` — AOT 側は classes 引数を追加し CRepr/handle 要素を 0 に揃えた)。第 18 弾の「per-slot 展開が要る」という見立てはポインタ格納の判明で不要になった。
- **置き場所の制限** (checker): 固定長 heap 配列は **class field / ローカル束縛 / fn param のみ**。戻り値型・他コンテナの構成要素 (`validate_type` の各 composite arm + tuple/some/配列/Map リテラルの式レベル検査 — **配列リテラル要素は decay するので除外**、これを忘れて既存 fixture 2 本を誤って落とした)・closure capture ([fn_expr.rs](../crates/ilang-mir/src/lower/fn_expr.rs) の lowering ガード)・束縛全体の再代入は型エラー。
- **JIT シンボル**: `$array.releaseFixed` / `$array.copyFixed` を [jit_symbols.rs](../crates/ilang-mir-codegen/src/compile/jit_symbols.rs) に明示登録 (releaseFixed は Rust 側参照で偶然リンクされていたが copyFixed は dead-strip され「can't resolve symbol」になった)。

**probe 結果 (全て正確な deinit 数 / delta=0)**: ローカル scope exit 400/400、 要素上書き 3/call、 field drop cascade 400/400 (deinit 連鎖 1002 = 1000+2 まで正確)、 field 上書き 2、 値コピーの独立性 (`p` 不変)、 借用 (alias read / param) は deinit 0、 早期 return 100/100、 `.length` / for-in / `console.log` 表示も正常。probe の「leak に見えた定数 56 bytes」は計測後に確保した `acc: i64[]` 自身だった (計測順の罠、 第 9 弾の教訓と同種)。

**後続で踏んだ二重解放 (修正済み)**: トップレベルの `let m = teamA.members` (別名) が**プロセス終了時に SIGABRT**。`__main` のエピローグは in-fn の scope sweep (`release_binding_for_scope_exit`) と**別の独自 release ループ** ([decl/bodies.rs](../crates/ilang-mir/src/lower/decl/bodies.rs) の top_scope 走査) を持っており、そちらが未ゲートだった — 別名 release がバッファと要素を先に解放し、直後の object cascade が解放済みバッファを再走査して abort。owner 判定 (`fixed_owned_locals`) のゲートをエピローグの Local/Ssa arm にも追加、slot 一掃にも同型のゲート (`fixed_owned_slots`) を追加。説明用のドキュメント例を実行して発覚 (probe は fn 内ローカルしか張っていなかった)。fixture: `05_edge_cases/fixed_array_top_level_alias.il`。**教訓: 解放経路は 1 本ではない — `release_binding_for_scope_exit` / `__main` エピローグ / slot 一掃 / break sweep の 4 箇所すべてに同じ所有ゲートが要る。**

**内部表現を rc 付きへ切替 + 全セル対応 (ユーザー承認)**: tuple のセル kind が 4bit 詰めで合成タグが入らない問題を機に、**fixed-of-arc の内部表現を動的配列と同一 (ヘッダ + rc) に変更**。見える挙動は案 A のまま (セル格納時の値コピーは `$array.copyShallow` をコンパイラが挿入)。primitive / CRepr 要素 (`u16[128]`, `Vertex[3]`) は従来のヘッダ無し inline を維持 — 分岐は全箇所 `kind_tag_of(elem) == 0`。これにより: (1) **tuple / 動的配列要素 / Map 値 / closure capture / Promise 値が全部対応** (ランタイムのセル機構は無変更 — 普通の KIND_ARRAY として扱われる)、(2) 合成タグ (`KIND_FIXED_BASE`)・`$array.releaseFixed` / `copyFixed`・所有者追跡 (`fixed_owned_locals` / `fixed_owned_slots`)・4 箇所の解放ゲートを**全削除** (固定長は通常 heap let の retain/release 均衡に乗る — エイリアスが所有者より長生きしても安全)、(3) print / fmt / builtin ラッパ (`fixed_to_dyn`) は arc 要素ならヘッダ経由に分岐。コピー挿入サイト: field 代入・some×3・enum ctor×2・tuple literal×2・配列 literal×3 系統・AssignIndex・push / unshift・map literal / set・Promise.resolve・capture 収集。**残る型エラー**: 後続セッションで全て解除済み — 戻り値と再代入は読み出し側の共有セマンティクス (generic の return 機構と Assign の heap 経路にそのまま乗る)、`Array.fill` は `$array.fillCopy` (スロットごとに shallow copy)。解除に伴い `copy_fixed_for_cell` に freshness を導入 (fresh な call 結果はコピーせず transfer — コピーすると元の +1 が宙に浮く)。**ついでに既存 leak を 1 件検出・修正**: `let o: Box? = makeBox()` の T→T? auto-coerce が fresh 元値の +1 を誰も解放しておらず 1 個/call leak (fixed と無関係の一般 heap で再現・修正、stmt.rs の coerce 後に Release)。fixture: `fixed_array_heap_elem_return_reassign_fill.il` (placement_error を改名・全面書き換え)。**既知の意味論エッジ (記録のみ)**: `.then` / `array.map` の**コールバックが固定長配列をそのまま返す**形は結果セルが共有になる (rc で健全、値セマンティクスのみ破れ — callback 結果の格納が runtime 内のため。対応するなら runtime の結果格納にコピーを足す)。fixture: `fixed_array_heap_elem_cells.il` (全セル + churn 100 周 delta=0)、placement_error は「戻り値」ケースに書き換え。検証: 既存 fixed fixture 全行が**表現切替前と同一出力** (意味論保存)、nextest 539/539、AOT PASS、nested_generic 100 並列 0 fail。

**generic 対応 (案 A、ユーザー決定)**: `fn hold<T>(v: T): T? { some(v) }` に `Box[2]` を渡せるようにした。規約は「**コンテナのセルは格納時に値コピー**」(field 代入と同じ): `copy_fixed_for_cell` ([body_cx.rs](../crates/ilang-mir/src/lower/body_cx.rs)) を Optional 構築 3 箇所 (Some / hint付き / T→T? coerce) + enum ctor payload 2 分岐に挿入。取り出し (match / unwrap) はコピーを指す。owner 判定は「リテラル右辺の let」に限定 (call 結果の fixed は常にエイリアス — generic 恒等関数が呼び出し元のバッファを返すため、 fresh 判定で owner 扱いすると二重 free)。型引数の一律拒否 (`reject_fixed_heap_type_arg`) と Optional/enum payload の注釈拒否を撤去。未対応のセル (tuple slot / 動的配列要素 / Map / Promise 値 / 再代入) は **lowering ガード** `forbid_fixed_in_cell` で明示診断 — checker は generic body を T 不透明で検査するため、mono 後に届く形はここで止める。probe: hold の独立性 (a を書いても o に見えない)・deinit 数 (3/2/2 で正確)・200 周 churn delta=0 deinits=400。fixture: `fixed_array_heap_elem_generic_arg.il` (旧 expect-error を正値テストに書き換え)。tuple の対応には cell kind の 4bit 詰めの拡張が要る (保留)。

**既知の残り**: REPL slot は固定長配列を正しく扱えない (`xs.length` がバッファ先頭を length と誤読 — **HEAD でも同じ挙動の既存ギャップ**、stash して確認済み)。

**generic 型引数のすり抜けは後続コミットで封鎖済み**: `fn hold<T>(v: T): T? { some(v) }` に `Box[2]` を渡すと T が fixed に推論され、 mono 後の Optional cell が借用バッファを own して **SIGABRT (二重 free) を実機再現**。 generic body は T を不透明として検査済みのため per-instantiation の再検査はできず、 「型パラメータは固定長 heap 配列に解決できない」を一律ルールに。 検査点は推論の合流点 4 箇所: generic fn 呼び出し ([calls.rs](../crates/ilang-types/src/checker/expr/calls.rs))・generic メソッド ([method.rs](../crates/ilang-types/src/checker/method.rs))・enum ctor ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs))・`new C<Box[2]>` の明示型引数 (check_new)。 ヘルパーは `reject_fixed_heap_type_arg`。 fixture: `05_edge_cases/fixed_array_heap_elem_generic_arg_error.il`。 直接 `Box[2]` と書く param は従来どおり可 (借用)。

**検証**: workspace nextest 539/539 (placement エラー fixture +1 込み)、 AOT arm 全 fixture PASS、 nested_generic 儀式 JIT 400 + AOT 400 で 0 fail。fixture: `05_edge_cases/fixed_array_heap_elem_store.il` を包括版に書き換え (10 系統を 1 本で pin)、 `05_edge_cases/fixed_array_heap_elem_placement_error.il` 新設。`docs/syntax.md` / `syntax_ja.md` の配列節に heap 要素の所有権と置き場所制限を追記。

### [解決済み記録] 第 18 弾: 固定長配列 `T[N]` × heap 要素 — ARC 未モデルの自認ギャップが実在 (2026-06-12、 第 19 弾で解決)

codegen の既存コメント ([arc.rs](../crates/ilang-mir-codegen/src/compile/lower_inst/arc.rs): 「fixed-length arrays skip rc bookkeeping entirely」、 [helpers.rs](../crates/ilang-mir-codegen/src/aot/helpers.rs): 「Heap-element fixed arrays would still need per-element release, which isn't modeled here yet」) が**実害として確認**できた回。 probe で確定した表面マップ:

**正しく動く** (fixture `05_edge_cases/fixed_array_heap_elem_store.il` で pin 済み): `arr[i] = x` の旧要素 release / 新要素 retain (deinit が上書きごとにちょうど 1 回)、 要素を束縛へ読み出して配列より長生きさせる形。

**漏れる** (すべて deinit 0 / 64 B/call 級): ローカル `Box[2]` の scope exit、 早期 return の sweep、 `class Pack { items: Box[2] }` の drop cascade (field kind が KIND_NONE 登録)、 field 上書き (`p.items = [..]` で旧要素が漏れる)。 string 要素 (`string[2]`) も同様に 2 個/call。

**修正が単純でない理由 — inline 配列の所有権モデルが未定義**: 固定長配列は rc ヘッダの無い inline 値データで、 (a) `let copy = arr` は**ポインタのエイリアス** (要素 release を sweep に足すだけだと二重解放)、 (b) `return arr` の所有権 (borrowed-retain は no-op に落ちる)、 (c) コンテナ内 (dyn 配列の要素 / enum payload / tuple / Map 値 / Optional) は kind テーブルがスカラーで「N 要素 × kind-K の inline 配列」を表現できない。 修正するなら: scope-exit/early-exit sweep の per-element release + 非 fresh let の借用 (PatternBinding) 化 + AssignField の旧要素 release / 非 fresh 代入の per-element retain + class field 登録の per-slot kind 展開 (JIT/AOT 両方) + return の per-element retain、 を一貫して入れる必要がある。

**選択肢 (ユーザー判断待ち)**: (a) 上記フルセットで heap 要素を正式サポート (所有権モデルの設計込み)、 (b) **型検査で「固定長配列の要素は primitive のみ」と制限** — 1 診断で健全性を回復する小さな修正 (heap が要るなら動的配列 `T[]` を使う、 という案内)、 (c) 現状を文書化して放置 (leak は残る)。 (b) が最小で健全、 (a) は機能価値あり。

### [確認済み記録] 第 17 弾: weak × sweep・property 本体・fs/path/Unicode・interface 配列 — 全 probe 問題なし (2026-06-12)

probe 対象: weak field を含む fn の早期 return (strong count 不変・deinit 0)、 property getter/setter **本体**の早期 return + heap ローカル (accessor も fn body — sweep 適用、 strings=0)、 string-repr enum / @flags enum、 fs のエラー経路 churn (FsError payload の解放 ✓ delta 定数・strings=0)、 fs の Unicode 内容 + Unicode ファイル名ラウンドトリップ、 path の端入力 (空文字 basename / ルート dirname / `..`・`.` 正規化 / join の空要素 / dotfile extname / relative)、 string の多バイト操作 (絵文字の length / charAt / slice / indexOf / split)、 **interface 型 fresh 配列を for-in + return で貫通** (vtable 分発 + 要素 deinit ちょうど 2/call)、 generic class が interface 値を保持する churn、 tuple を Map 鍵にした時の診断 (正規メッセージ ✓)。 **新規バグなし** — 検出は全て probe 側のミス (interface は nominal で `class X: Iface` 宣言が必須、 という言語仕様どおりの拒否を誤検出しかけた)。

fixture: `05_edge_cases/early_exit_weak_property_iface.il` (weak / property / interface×for-in×return / repr・flags enum を 1 本で pin)。 コンパイラ変更なし — workspace nextest 539/539 + AOT arm PASS、 儀式は省略。

### [解決済み記録] 第 16 弾: REPL の再定義セマンティクス — 型違い re-let の健全性穴を封鎖 (2026-06-12)

REPL の再定義を probe した回。 1 件修正、 2 件は設計判断待ち:

1. **型違い re-let が無言の型穴だった (修正済み)**。 `let x = 41` → `let x = "str"` → `console.log(x)` が **string ポインタの生値を印字** — slot_table が旧型 (i64) のまま新値の bit が slot に書かれ、 次の読みが再解釈していた。 同型 re-let は正常な上書き (REPL の通常ワークフロー) なので維持し、 **型が変わる re-let は明示エラー** (「use a new name」) に。 repl.rs +3 件 (同型上書き / 型違い拒否+旧値生存 / @derive 動作確認)。
2. **fn / class の再定義は設計判断待ち**。 fn 再定義は「duplicate overload」拒否で旧定義が残る (再定義不可)。 class 再定義は cranelift の「incompatible signature」という生エラー (旧 class が残る)。 enum 再定義は**偶然** last-wins で動く。 一般的な REPL は last-wins 置換だが、 class 置換は「旧 layout で作られた既存インスタンス × 新 layout のメソッド」という未定義動作を作るため、 置換するなら旧型 slot の扱い (無効化 or 警告) を決める必要がある — ユーザーと相談。
3. **chunk 内の runtime panic は REPL セッションごと落ちる** (限界として記録)。 panic 後の slot 状態は信頼できないため、 復帰させるのも危うい。 file モードと同じ「panic = プロセス終了」。

@derive(Eq, Hash) は REPL で機能することを確認 (第 15 弾の normalize 乗せ替えの恩恵)。 promise の `.then` は後続 chunk の drain で発火し、 エラー chunk を挟んでも復帰する。

**検証**: repl 14/14、 workspace nextest 539/539。 CLI のみの変更 (lowering 不変) のため AOT arm / nested_generic 儀式は省略。

### [解決済み記録] 第 15 弾: REPL を loader 相当の normalize 経路に乗せ替え — enum / async / const / generic slot / use を修復 (2026-06-12)

「別領域 (REPL・capability・未実装機能の境界)」を狙った回。 まず境界 probe の結果: `@requires(net, file.read)` は意図どおり「パースされ未 enforce」 (ilang-types/lib.rs に明記あり)。 **未知の属性 (`@bogus`) は無言で素通り** — typo した属性が静かに無効になる footgun として記録 (**後続セッション 2026-06-12 で解決**: `parse_attributes` に正規セット 17 種の whitelist を入れ、未知名は既知一覧つきパースエラー。コンパイラ合成の内部属性 `sel` / `byValue` / `variadic` はパースを通らないため対象外。fixture: `05_edge_cases/unknown_attribute_error.il`)。 `?` 演算子は **HANDOFF の「未実装」リストが古く、 Result<T, E> 用に実装済み** — ok/err 伝播・heap payload・err 経路で heap let を跨ぐ形 (第 11 弾 sweep が遡って直していた)・deinit 数まで全て正確なことを probe で確認。 `x?` を Optional に使うと「Optional has no variant "ok"」という分かりにくい診断になる点は記録 (**後続セッション 2026-06-12 で対応**: ユーザー決定により `?` を Optional に対応 — none で `return none`、some で展開。パーサの `?` 脱糖は型を知らず Result 形 (`ok/err` arm) を吐くため、checker ([match_ctrl.rs](../crates/ilang-types/src/checker/expr/match_ctrl.rs)) と lowering ([match_.rs](../crates/ilang-mir/src/lower/match_.rs)) が合成名 `__try_*` の scrutinee で形を識別して some/none に差し替える。囲み fn は Optional 返しの同期 fn 限定 — async 内は normalize が先に err arm を settle に書き換えるため明示診断で拒否。**この作業で Result `?` の既存 use-after-free を発見・修正**: 脱糖ブロックの `__try` 一時束縛が scope exit で cell を解放するのが、外側 `let v = e?` の retain より先 — heap payload を `?` の後で読むと解放済みメモリだった (第 15 弾の probe は payload を脱糖ブロックの外に持ち出していなかった)。裸 Var の arm 本体はブロック末尾の retain 対と組まないため、arm 内で明示 Retain を発行 (`force_tail_retain`)。併せて is_fresh_object_expr の Match 判定で diverge する arm (return/break/continue) を除外 — `?` の伝播 arm が「全 arm fresh」判定を壊して二重 retain leak になっていた。fixture: `05_edge_cases/question_op_optional.il`)。 throw / try / labeled break は正規の parse error。 generic fn は推論呼び出し (`id(5)`) が動作、 turbofish (`id<i64>(5)`) は比較式に誤パースされ「undefined variable "i64"」になる点を記録。

**REPL の本丸 — 根本原因は「loader も normalize も通らない」パイプライン**。 検出した実バグ: (1) enum が chunk を跨いで使えない (`E.a` の Field→EnumCtor 書き換えが normalize 内のため)、 (2) async fn が旧診断で全滅、 (3) `const` が Item::Const のまま MIR に到達、 (4) generic 型 slot (`Result<i64,string>` / `Box<i64>`) が**無言で**永続化失敗 → 次 chunk で素の "unbound variable"、 (5) `use` が "unexpected Item::Use post-loader"。 **修復** ([main.rs::run_chunk](../crates/ilang-cli/src/main.rs) 再構成):

- `loader::normalize_repl_chunk` 新設 — load_program_full 末尾と同じ連鎖 (enum-ref renormalize → @derive 展開 → const inline → async desugar) を in-memory プログラムに適用 (auto_lift_objc と dup_pub は対象外と明記)
- **chunk ごとに fresh TypeChecker** で「accumulated 生 items + chunk」の正規化済み全体を検査 (永続 checker での再検査は duplicate-overload ガードに当たる)。 過去 chunk の let は slot_table から `TypeChecker::define_global` (新設) で seed
- **slot 型を monomorphize の追加要求として注入** — `monomorphize_with_requests` / `monomorphize_enums_with_requests` (新設、 既存入口は空要求で委譲)。 読みしかない chunk には instantiation site が無く、 特殊化 class/enum が合成されないため。 lower の slot 解決にも Generic→mangled 名のフォールバックを追加 ([lower/mod.rs](../crates/ilang-mir/src/lower/mod.rs) 3b、 `monomorphize::mangle_generic_name` 公開)
- slot 化できない let は **stderr に「persist しない」注記** (旧: 無言)
- `use` は「REPL 未対応、 `ilang run` を使え」の明示診断 (file loader のモジュール解決が必要なため未実装のまま — 対応するなら loader の overlay 機構で chunk を仮想 entry 化する方向)

**成果**: REPL で enum / async fn / const / `Result<i64,string>`・`Box<i64>` 型 let の chunk 跨ぎがすべて動作。 repl.rs に回帰テスト 6 件追加 (計 11)。 REPL は行単位入力のため複数行構文が書けない点は別件の使い勝手課題として残る。

**検証**: workspace nextest 536/536、 AOT arm 全 fixture PASS、 nested_generic.il 0/800 (lower の slot 解決に触れたため実施)。

### [確認済み記録] 第 14 弾: 早期脱出 × 値の持ち出し — 全 probe 問題なし (2026-06-12)

第 13 弾の教訓 (leak probe は過剰解放を見落とす) を踏まえ、 **deinit カウントを過剰解放検出器**として「sweep が掃除する出口を値が通り抜ける」形を網羅した回。 **新規バグなし** — 以下すべて値・leak・deinit 数まで正確:

- `return b` (fresh scrutinee の payload を返す — sweep の scrutinee 解放より先に borrowed-return retain が立つ)
- `break b` (payload を loop の外へ — lower_break の borrowed retain が sweep に先行)
- mutating-capture closure の早期 return (scope の cell share だけが落ち、 escape した closure は自分の share で生存)
- 入れ子 fresh-iterable for-in を 1 つの return で貫通 (両 iterable 解放、 全要素 deinit ちょうど 1 回)
- async 本体内の match arm / while / for-in からの return (await 前後どちらも settle 正常)
- async-match churn / cell churn / payload churn すべて delta=0

**適切な診断付きの既知制限を 1 件確認**: match の**ある arm に await・別の arm に return** がある形は state-machine lowering 未対応で、 「Refactor to a supported shape」の正規診断が出る (誤動作ではない。 await を match の前に出せば全 arm で return 可)。

fixture: `05_edge_cases/early_exit_payload_escape.il` (上記全部を 1 本で pin)。 lowering 変更なしのため nested_generic 儀式は省略、 workspace nextest 530/530 + AOT arm PASS。

### [解決済み記録] 第 13 弾: for-in × 早期 return の両方向 — fresh iterable の leak と要素借用の過剰解放 (第 11 弾 sweep の退行) (2026-06-12)

probe 対象を for-in over Map/Set (`entries()`/`values()`) の途中 break/return、 if-let + 早期 return、 tuple 分解 + 早期 return へ広げた。 2 件 (1 は既存、 1 は**第 11 弾 sweep が main に入れた退行**):

1. **fresh iterable が `return` 経路で leak** (既存)。 for-in の fresh iterable (`for e in m.entries()`) の Release は loop の exit block にのみあり、 `break` は exit を通るが `return` は通らない — entries 配列一式 (~144 B/call) が leak。 `live_fresh_scrutinees` の機構に iterable も登録 (loop frame の**外側の depth** で push するので break/continue の sweep からは除外され、 exit block の既存 Release と二重にならない。 ilang にラベル付き break が無いため break が for-in 境界を跨ぐことはない)。
2. **for-in の要素束縛を return sweep が過剰解放** (第 11 弾の退行、 **2026-06-11 の `4abb94d8` から main に入っていた**)。 要素束縛は ArrayLoad の借用 (retain なし) なのに素の `Ssa` で env 登録されており、 第 11 弾の return sweep が「owned binding」として release — 長寿命配列の要素が配列より先に死ぬ UAF (`for e in m.entries() { ... return }` の SIGABRT で発覚。 非 fresh 配列でも同型で、 **昨日の probe は leak だけ見て過剰解放を見落としていた**)。 要素束縛を `PatternBinding` (match payload と同じ「容器が解放責任を持つ借用」、 sweep 除外) に変更。 `env.bind` の他の呼び出し箇所は監査済み — LetTuple / LetStruct の束縛は retain 済みの owned、 receiver temp も owned、 range の loop var は int で、 誤分類はこの 1 箇所だけ。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 0/800。 fixture: `05_edge_cases/forin_early_return_arc.il` (fresh iterable の回収 + 非 fresh 配列の要素が 200 回の早期 return 走査後も生存・deinit がちょうど要素数、 の両方向を pin)。

**probe で問題なしを確認した周辺** (再調査不要): Set `values()` + break、 if-let (fresh scrutinee) + 早期 return、 tuple 分解 (`let (a, b) = makePair()`) + 早期 return。

### [解決済み記録] 第 12 弾: continue の sweep・fresh scrutinee の早期脱出・string match の scrutinee・async の早期 return (2026-06-11)

第 11 弾の `return` sweep の同族を狙った回。 4 系統 (3 は既存バグ、 1 は未対応構文の実装):

1. **`continue` も sweep 無し** (既存)。 loop body の heap `let` を跨ぐ `continue` が 1 個/skip leak。 `lower_continue` に break と同じ「loop frame 以降の scope sweep」を追加 ([control.rs](../crates/ilang-mir/src/lower/control.rs)、 per-binding 規則は return と共通の `release_binding_for_scope_exit`)。
2. **fresh scrutinee の match で arm が diverge すると scrutinee が leak** (既存)。 arm 末尾の `Release(scrutinee)` は `!diverges` ガード付きで、 return/break で抜けると誰も解放しない (enum 64 B/call 等)。 `BodyCx::live_fresh_scrutinees` (ValueId + 登録時 env depth のスタック) を追加し、 enum / optional / if-let / string match の arm 本体 lowering 中に push、 3 つの早期脱出 sweep が「跨いで出る分」 (return = 全部、 loop jump = loop frame depth 以深) を release。
3. **string match は fresh scrutinee を一切 release していなかった** (既存・diverge 無関係)。 `match "k" + s { ... }` が素通りでも 1 string/評価 leak。 `lower_match_str` に enum と同じ freshness 規則を実装。
4. **async fn 本体の早期 `return` を実装** (旧: poll fn の `()` 戻りと衝突する誤解を招く型エラーで不支持)。 desugar が poll fn へコピーする `return e` を「`settleResolve(state_ref.__async_promise, e)` + 素の `return`」 に書き換える walker (`rewrite_returns_in_*`、 FnExpr には降りない — closure の return は closure のもの)。 tail がそのまま `return e` の場合の二重 settle は `emit_settle` の root-Return 特例で回避。 await 前 / await 間 (取る側・取らない側) / churn を確認。

**第 11 弾 sweep の取りこぼしも 1 件修正**: `return_sweep_base` — 引数なし fn は param scope が存在せず**本体が scope 0** になるため、 固定 `skip(1)` が全束縛を取りこぼしていた (deinit が早期 return で発火しない形で発見)。 `lower_block_for_fn_body` が本体直前の `env.scopes.len()` を記録し sweep の基点にする。 `__main` は usize::MAX (top-level return がもし通っても slot 管理と二重解放しない)。 deinit は sweep で内側束縛から順に発火することを pin。

**probe で問題なしを確認した周辺** (再調査不要): for-in 内の continue / break / return (heap local 跨ぎ)、 closure 本体の早期 return (lower_block_for_fn_body 経由で base 設定済み)、 break の既存 sweep、 async 早期 return の resolve 経路回帰。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 0/800。 回帰 fixture 2 件: `05_edge_cases/early_exit_sweep_continue_scrutinee.il`、 `04_modules/async_early_return.il`。

### [解決済み記録] 第 11 弾: await の rejection propagate (新意味論) と早期 return の scope release 欠落 (2026-06-11)

probe 対象を await × rejected promise、 `Promise.reject` factory churn、 fire 中の再入登録、 継続内からの連鎖 settle、 1000 段 then 連鎖、 async method churn、 入れ子 promise / tuple payload / `Promise<()>` / `Optional<Promise>` field / enum-holds-promise へ広げた。 成果は新意味論 1 件 + 一般バグ 1 件:

1. **await が rejected promise に当たったら async fn の結果 promise を同じ msg で reject する (JS 意味論、 ユーザー決定)**。 旧挙動は rejection が無言で消えて結果 promise が永久 pending (`.catch` 不発)。 実装: runtime に `$promise.rejectFollows(upstream, target)` (upstream の reject を target へ転送する forwarder — `promise_reject_stub` cell を再利用、 resolve は無視)、 desugar の suspend が `__awaited` を一度束縛して `.then(resume)` と `rejectFollows(__awaited, state_ref.__async_promise)` の両方を登録 ([gen_items.rs](../crates/ilang-parser/src/normalize/state_machine/gen_items.rs))。 checker の内部 static (`$promise.rejectFollows<T, U>`) と codegen 配線を追加。 docs (syntax.md / syntax_ja.md の async 節) に明記。 fixture: `04_modules/async_await_rejection_propagates.il` (同期 reject / timer deferred / 2 段 async 入れ子 / ループ内 / churn、 400 並列で出力 1 パターン確認済み)。
2. **早期 `return` が生きている heap 束縛を release しない** (既存・一般)。 `lower_break` には早期脱出 sweep があるのに `lower_return` には**何も無く**、 fn 直下 / match arm / loop body どこでも「`return` を跨ぐ heap `let`」が 1 個/call leak していた (tail 形は正常)。 async では poll fn の suspend が生成する `return` がこれを踏み、 awaited promise + state に乗る全ローカルが leak (rejection 経路で顕在化したが resolve 経路でも同じ — ManagedPromise が untracked なため不可視だった)。 修正: [control.rs::lower_return](../crates/ilang-mir/src/lower/control.rs) に「param scope (借用) を除く全 scope の sweep」を追加し、 per-binding 規則 (CRepr 所有 / COM 除外 / PatternBinding 除外 / Cell) は `release_top_scope_objects` と共通の helper に括り出し。 借用値の `return` は sweep 前に Retain (coerce が新しい値を mint した場合は除く — `T → T?` wrap は coerce 側が所有を作る)。 fixture: `05_edge_cases/early_return_scope_release.il` (3 形の churn + 借用 return の UAF ガード)。

**自分の fixture でタイミング依存を踏んだ記録**: 初版の fixture が「5ms timer の発火」と「churn 区間」の前後関係に依存し、 nested_generic 800 連発との並走負荷で suite を 1 回落とした (`target/fixture-failures.log` に記録)。 規約の再確認 — timer 発火位置を固定するには **`time.sleep(期限+余裕)` で期限を確実に跨いでから `time.tick()`** (due は時刻基準なので負荷は「より due」方向にしか働かない)。 churn の定数残差 (state machine 初回確保) は exact 値でなく `expectTrue(|delta| < 1024)` で判定する。

**probe で問題なしを確認した周辺** (再調査不要): `Promise.reject` factory churn (catch あり)、 fire 中の同一 upstream への再入 `.then` 登録、 継続内からの別 deferred resolve (drain 中の連鎖 settle)、 1000 段 then 連鎖、 async method (heap field 越し await) churn、 `Promise<Promise<T>>`、 tuple payload churn、 `Promise<()>` 印字、 `Optional<Promise>` field churn、 enum variant (tuple / struct) が promise を保持する形。 REPL は `use` 自体が未対応 (`unexpected Item::Use post-loader`) — async と独立の既存制限として記録。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 0/800、 新 fixture 2 件とも 400 並列で出力 1 パターン。

### [解決済み記録] fixture 増殖ラウンド第 9 弾: float executor の ABI 不一致・fresh 引数 release の残り 3 経路・TLS 破棄順 abort (2026-06-11)

probe 対象を float 型 promise、 rejection 経路の ARC churn、 同一 promise への複数 waiter、 heap 各種別の promise 値 churn、 暗黙 `this.method(...)`、 timer の相互 clear / executor 内 tick 再入 / float の Promise.all/race、 armed timer を残した panic 終了へ広げた。 4 系統のバグ (1 は新モデル起因、 3 は移行前から — 移行前バイナリで再現確認済み):

1. **float 型 promise の executor 経路が ABI 不一致で garbage / SIGSEGV** (既存)。 executor の `resolve` は ilang 側で `fn(f64)` として呼ばれ値が float レジスタに乗るが、 resolve_cb の fn_addr (Rust stub) は `(i64, i64)` で読む — 値に env が、 env に garbage が入り、 inline は garbage 値、 deferred は capture 読みで SIGSEGV。 float-ABI の stub 変種 (`promise_resolve_stub_f32/_f64`、 to_bits で `State::Resolved` の bits 規約に合流) を追加し、 `$promise.withExecutor` に `value_fk` 引数を追加 (lowering は `MirTy::F32/F64` から算出、 [program_decl.rs](../crates/ilang-mir-codegen/src/compile/program_decl.rs) は ternary 宣言へ)。 fixture: `04_modules/promise_float_executor.il`。
2. **fresh 引数の post-release が「クロージャ (間接) 呼び出し」に無い** (既存)。 named-fn 経路 ([call_fn.rs](../crates/ilang-mir/src/lower/call_fn.rs)) にはあった「fresh heap 引数の +1 を呼び出し後に release」が CallIndirect / CallRawIndirect 経路に無く、 closure へ渡した fresh string/object が 1 個/call leak。 promise の `resolve(new Box(..))` / `rej("...")` の値 leak (複数 waiter で 24 bytes/iter、 reject 文字列 1 個/iter など) は全部この現れ。 fixture: `10_closures_arc/closure_fresh_arg_release.il`。
3. **同じ release が「暗黙 `this.method(...)`」arm にも無い** (既存)。 fresh string は 1 個/call leak (fresh object は escape 解析の stack 化で隠れがち)。 さらに **promise `.then`/`.catch` の fresh receiver release も無く**、 チェーン `p.then(f).catch(g)` の中間 promise が settled 値ごと leak (ManagedPromise 自体は Box 確保で `liveAllocBytes` に映らず、 保持された rejection string が 1 個/iter で見えた)。 waiter は downstream を +1 保持するので fresh receiver の release でチェーンは切れない。 fixture: `02_classes/method_implicit_this_fresh_arg.il`、 `04_modules/promise_chain_intermediate_release.il`。
4. **armed timer を残した panic 終了で TLS 破棄順 abort** (新モデル起因)。 thread-local の `EventLoop` 破棄が残存 timer entry の closure release cascade を走らせ、 既に破棄された別 thread-local (cascade worklist / string registry) に触れて二重 panic → abort、 本来の panic メッセージが大量のノイズに埋まる。 `EventLoop` の `Drop` で残存タスクを `mem::forget` (プロセス終了中なので OS が回収)。 panic 終了は exit 1 + 本来のメッセージのみになった (fixture 化は困難 — expect-error はノイズの「不在」を表明できないため手動確認、 再発時はこの記録を参照)。

**probe 計測の罠 (保管用)**: `console.log("a=" + x + " b=" + (test.liveStringCount() - base))` のように **count 読みを concat 式の途中に置くと、 左半分の concat 中間文字列が生きたまま count が走って幻の +1** が出る。 churn probe では必ず値をローカルへ読み切ってから印字する。 closure 内での `test.expectTrue` 初回呼び出しも定数 +1 を作る — 検証は計測ループの外で行う。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 0/800。 (初回の workspace 実行で 1 件 leaky 表示 — テスト自体は PASS、 nested_generic 800 連発と並走させた負荷下のみで観測され、 単独再実行では 0 leaky。)

**probe で問題なしを確認した周辺** (再調査不要): rejection の then 素通り→catch 連鎖と uncaught reject の msg 収支 (修正後 delta=0)、 同一 promise 3 waiter / heap 各種別 (string・配列・Optional) の executor→then churn、 then が heap 値を返して downstream を捨てる形、 timer A→B 相互 clear と callback 内再スケジュール、 executor 本体からの `time.tick()` 再入、 float の Promise.all / race (bits 規約で整合)、 巨大 ms。

### [解決済み記録] fixture 増殖ラウンド第 8 弾: executor 3 セル leak と ICF 重複登録の二重解放 (2026-06-11)

probe 対象を新実行モデルの周辺 — timer の ARC churn (発火済み / cancel 済み / interval 自己 clear)、 deferred パターン (executor から resolve/reject を持ち出す JS `Promise.withResolvers` 相当)、 発火順序 (同一期限 FIFO / microtask vs macrotask / 入れ子 0ms 連鎖)、 pump/drain の再入 (callback 内 `time.tick()` / callback 内 `liveAllocBytes`)、 timer 解決の promise と await / Promise.all / catch の統合、 巨大 ms / 二重 clear / 未知 id — へ広げた。 2 系統のバグ (どちらも**移行前から存在**、 移行前バイナリで再現確認済み):

1. **`new Promise<T>(executor)` が 3 セルとも leak** (空 executor で正確に 16+32+24=72 bytes/回 + capture)。 lowering ([crates/ilang-mir/src/lower/expr.rs](crates/ilang-mir/src/lower/expr.rs) の Promise arm) は executor の +1 を runtime へ譲渡する規約 (fresh はそのまま、 非 fresh は Retain) だが、 `__promise_with_executor` が譲渡分を release していなかった。 runtime が rc=1 で mint する resolve_cb / reject_cb も「executor 本体が param を release する」という旧コメントが現行 MIR では成り立っておらず未解放。 修正: 同期 executor 呼び出しの直後に 3 つとも release。 escape された callback は容器 store の retain で生存する (deferred パターンは正常動作)。
2. **release ビルドの関数マージ (ICF) で capture 登録が二重化 → promise の二重解放** (`promise_stub_merged_registration.il` の形で `.catch` が無言で不発 / 配置次第で SIGABRT)。 `promise_resolve_stub` と `promise_race_resolve_stub` (reject 側も) は本体が同一で release ビルドでは同一アドレスに統合される。 capture 登録テーブル ([crates/ilang-runtime/src/closures.rs](crates/ilang-runtime/src/closures.rs)) は fn_addr キーに blind push なので、 `Promise.all`/`race` の stub 登録が executor stub と同じキーへ `(+16, KIND_PROMISE)` を重複追加 → 以後の callback cell 解放で captured promise が 2 回 release → 早期 FINAL-DROP で waiter ごと消える。 **バグ 1 の修正で cell が実際に解放されるようになって顕在化した潜在バグ** (leak していた間は cascade が走らなかった)。 修正: `__register_closure_capture` を (offset, kind) で冪等に。 マージされる関数はコードが同一 = capture 配置も同一なので dedup で失うものはない。

**デバッグ補助を常設化**: `ILANG_DEBUG_PROMISE=1` (promise の retain/release/FINAL-DROP/settle)、 `ILANG_DEBUG_CLOSURE=1` (closure cell の rc 遷移)、 `ILANG_DEBUG_TIMER=1` (timer の schedule/fire/discard)。 env 判定は OnceLock キャッシュで hot path は atomic load 1 回。 今回の真因特定はこの 3 つの突き合わせで行った。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 0/800。 回帰 fixture 4 件: `04_modules/promise_executor_cell_arc.il` (churn 4 変種 delta=0)、 `04_modules/promise_deferred_resolvers.il` (deferred + await/all/catch の timer 解決)、 `04_modules/promise_stub_merged_registration.il` (all/race を先に走らせてから deferred catch — ICF 二重登録の pin)、 `04_modules/timer_microtask_order.il` (同一期限 FIFO / then-before-timer0 / 0ms 連鎖 / tick 再入)。

**probe で問題なしを確認した周辺** (再調査不要): 発火済み・cancel 済み setTimeout / 自己 clear interval の heap capture churn (delta=0)、 巨大 ms (`i64::MAX`) の schedule + clear (overflow せず即終了)、 二重 clear / 未知 id clear / 発火済み id clear、 callback 内 `time.tick()` 再入・callback 内 `liveAllocBytes` (pump) 再入、 同一期限 3 本の FIFO、 queue 済み継続が due timer より先 (JS microtask 規約)、 入れ子 0ms timer 連鎖が単一 tick 内で 5 段、 timer 解決 promise の await / Promise.all (逆順解決) / race / escape された reject の catch 復帰。

### [解決済み記録] Promise/async 実行モデルを JS 型 (run-to-completion・シングルスレッド) へ移行した (2026-06-11)

ユーザー決定 (2026-06-11) を実装。 動機: 旧モデルは executor / `.then` 継続 / await 再開が work-stealing worker pool 上で**メインスレッドと並行に**走り、 (a) 同期コードからの promise 状態観測が非決定的 (`promise_print_state.il` の race を高負荷 1/400 で実測 — 下の記録)、 (b) callback がメインと同じ容器を触るとデータ競合になり得た (同期プリミティブが無いため防げない)。 実装計画は `/Users/iwao/.claude/plans/tingly-puzzling-dusk.md` (保管用)。

**新意味論** (JS / Node と同じ):

- ユーザーコード (main・executor・`.then` callback・async fn の再開・タイマー callback) は**すべて単一スレッド**で run-to-completion。 同期コード実行中に継続が割り込むことはない
- executor は `new Promise(...)` の**その場で同期実行**。 resolve/reject は状態遷移 + 継続の queue 投入のみ (inline では走らせない)
- 継続はメインスレッドの FIFO queue。 実行は drain ポイントのみ: プログラム終了時 (`run_main` / AOT main wrapper の `$promise.drain`) と、 新設の **`time.tick()`** (非ブロッキング pump — 期限到来済みタイマー + queue を空にして即 return。 自前メインループを持つアプリがフレームごとに呼ぶ)
- タイマーは [crates/ilang-runtime/src/pool.rs](crates/ilang-runtime/src/pool.rs) の期限順 BinaryHeap。 終了時 drain は期限まで sleep して発火させる (未発火タイマーはプロセスを生かす — Node と同じ)。 cancel 済み entry は先頭に来た時点で破棄するので、 cancel された長期タイマーが終了を遅らせることはない

**実装**: `pool.rs` を worker スレッド機構ごと書き換え (`submit`/`drain` の API 維持、 `schedule_timer`/`cancel_timer`/`pump` 追加、 thread-local の queue + timer heap + live map。 crossbeam-deque 依存を撤去)。 `promises.rs::__promise_with_executor` を同期呼び出し化 (worker 用の retain/release を撤去 — caller の borrow が同期呼び出しを跨いで生きるため不要)。 `timers.rs` は「pool に sleep 入りタスクを submit」から timer heap 登録へ (Mutex/Arc/AtomicBool 撤去)。 `$time.tick` export + jit_symbols 配線 + `libs/std/time.il` の `pub fn tick()`。 将来の CPU 並列は「共有メモリなしの worker API」(別設計) で提供する方針。

**ハマりどころ (保管用)**: timer callback の +1 を Drop guard (`ClosureGuard`) で持たせる実装で、 `move ||` クロージャ内の利用が `guard.0` (Copy な i64 フィールド) だけだと **Rust 2021 の disjoint capture がフィールドだけを capture して guard 本体を即 drop** → callback cell が発火前に解放され、 `fn_addr=0` の無言スキップになる (クラッシュせず「タイマーが発火しない」だけなので気づきにくい)。 メソッド呼び出し (`guard.ptr()`) にして全体 capture を強制した。

**fixture の書き方が変わる点**: run-to-completion では main 末尾の検証コードがタイマー/継続より**先に**走る。 タイマー発火を検証する fixture は「callback 自身が検証して印字する」JS 流に書く (`timer_set_interval_clear.il` を 3 tick で自己 clear する形に書き換え。 旧形は `time.sleep(300)` 中に worker が発火する前提だった)。

**後続修正 (同 2026-06-11)**: `test.liveAllocBytes` / `liveAllocCount` 内の flush を `drain` から **`pump` (非ブロッキング)** に変更。 drain はタイマー heap が空になるまで待つため、 interval が armed なまま probe を呼ぶと永久ブロックしていた (旧 worker pool モデルでも同形でハングする既存の角 — interval は終わらない pool タスクだった)。 probe の目的は「queue 済み継続を流しきって callback 待ちの確保分を leak と誤計上しない」ことなので pump で足りる。 fixture: `05_edge_cases/live_alloc_probe_nonblocking.il` (armed interval + probe が即返る + 自己 clear で終了も pin)。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 `promise_print_state.il` / `promise_race_array_order_tiebreak.il` を 16 並列 × 25 batch (各 400 回) で出力 1 パターン (決定性の実証)、 nested_generic.il 16×25×2 = **0/800**。 `examples/libs/gui/window_state_demo` はビルド確認済み (GUI 起動はしていない)。 docs: syntax.md / syntax_ja.md の Promise 節を新実行モデルに書き換え。

**持ち越し (任意のクリーンアップ)**: 同期プリミティブの簡素化 — `ManagedPromise` の `rc: AtomicI64` → i64、 `inner: Mutex<Inner>` → RefCell、 `PromiseAllState.remaining` の Atomic 外し、 `closures.rs` / `strings.rs` 等の atomic rc はシングルスレッド化後は過剰。 機能とは独立の commit で行うこと (今回は未着手)。

### [解決済み記録] AOT arm の確率的失敗は `promise_print_state.il` のタイミング依存期待だった (2026-06-11、 `df797334`)

**真因 (再現により確定)**: 第 4 弾で追加した [crates/ilang-cli/tests/programs/04_modules/promise_print_state.il](crates/ilang-cli/tests/programs/04_modules/promise_print_state.il) が `Promise.all([Promise.resolve(1)])` を直後に印字して `<promise pending>` を期待していたが、 `Promise.all` の解決継続は worker pool (実 OS スレッド) 上で**メインスレッドと並行に**走る。 アイドル機ではメインの print が常に勝つが、 高負荷時は pool が勝って `<promise resolved>` が印字され expect 不一致 → fixture FAIL → suite FAIL。 **16 並列 × 25 batch で 1/400 を実測再現**。 全観測と整合: 自然発生 2 回はどちらも「リビルド直後でマシンが高負荷」/ この fixture の追加後 / 記録罠の設置前。 アイドル機での意図的再現 27 回が全 PASS だったのは負荷条件を満たしていなかったため。

**修正**: pending の印字確認は「executor が resolve を呼ばない永続 pending の promise」に差し替え (同じ 400 並列で 400/400 決定的)。 `Promise.all` は `.then` での値検証に変更。

**二次容疑は白**: abort 系 expect-error fixture (`repr_c_enum_field_unknown_aborts.il`) の「abort 前 stderr 喪失」は同じ並列負荷 208 回で 0 回 — 現行の abort 経路では起きない。

**維持するもの**: 第 5 弾の harness hardening (`39d87ad7` — spawn 失敗 / リンク直後 SIGKILL(9) の 1 回 retry + `target/fixture-failures.log` への自動記録、 fixture 失敗詳細の同ファイル記録) は環境起因の防御としてそのまま残す。 タイミング依存の期待を持つ fixture を書かないこと (`<promise pending>` のような並行状態の印字は「決して解決しない promise」で固定する)。

### [解決済み記録] fixture 増殖ラウンド第 7 弾: for-in の live 化・分解束縛の ARC・テンプレート part 入力 (2026-06-11、 `ced57791`〜`52c6bb8f`)

probe 対象を for-in 中の mutation、 tuple / struct 分解束縛、 Result の heap 両側、 float キー Map、 入れ子テンプレート、 events モジュール churn へ拡張した。 3 系統のバグ:

1. **for-in 中の pop で「index out of bounds」panic** (`ced57791`)。 配列 arm が `ArrayLen` をループ前に巻き上げており、 本体内の pop で stale な長さのまま境界検査に当たっていた。 長さをループ header で毎周読み直す live 意味論 (JS の for..of と同じ) に変更: pop は途中終了、 push は追加要素も巡回。 ArrayLoad は data pointer を毎アクセス読み直すので push の realloc はもともと安全。 fixture: `05_edge_cases/forin_live_mutation.il`。
2. **分解束縛の ARC 2 点** (`8185589c`)。 `let (a, b) = makePair()` が fresh tuple を release せず 32 bytes/回 leak。 さらに束縛が要素を retain しないため、 **借用 tuple の分解**では束縛の scope-exit release が tuple の slot share を横取りし、 tuple の後続 cascade が解放済みセルを歩いていた (解放済みセルの rc がちょうど 1 に読めることが稀なため显在化していなかっただけ)。 束縛ごとに retain + fresh source は抽出後に release。 `let Class { f } = obj` (LetStruct) も同型で同修正。 fixture: `05_edge_cases/destructure_arc.il`。
3. **テンプレート `${}` 内の fresh 値が 1 個/評価 leak** (`52c6bb8f`)。 第 4 弾の修正は fmt 結果と concat 中間値だけで、 **part の入力** (`${`inner${x}`}` の内側テンプレートや `${"a" + b}` の concat) の transient +1 が残っていた。 fmt_value (Str 入力はコピー) の後に release。 `leak_template_literal_loop.il` に追記。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 400 並列 0/400。

**probe で問題なしを確認した周辺** (再調査不要): Result の heap 両側 (ok/err どちらも class) churn、 events モジュール (on/off/emit/removeAllListeners) churn、 float キー Map / Set (型検査で適切に拒否 — `Map<f64, _>` は診断あり)、 入れ子テンプレートの値の正しさ。

### [解決済み記録] fixture 増殖ラウンド第 6 弾: is/as? の ARC・reflection の kind tag・iteration snapshot (2026-06-11、 `e83ca1fc`〜`3fbf7667`)

probe 対象を `is` / `as?`、 reflection (`typeof`)、 regex、 iteration 中の mutation、 generic fn + heap、 Promise.all/race、 const 畳み込み、 string メソッドの端へ拡張した。 3 系統のバグ:

1. **`as?` の Optional セルが不正な 8 byte 形** (`e83ca1fc`)。 `DowncastOrNone` codegen が rc も kind tag も無い `[value]` 8 byte セルを確保 — 誰も release できず 8 bytes/hit leak、 release されたら範囲外読み、 inner の retain も無し。 WeakUpgrade と同じ 24 byte `[value | rc | kind=Object]` + inner retain に変更し、 `as?` を fresh 分類。 `is` / `as?` の fresh operand (`makeB() is B`) の transient release も追加 (旧: オブジェクト丸ごと leak/call)。
2. **reflection の ARC 3 点** (`3437012f`)。 (a) `new_string_array` の header kind tag が PK_STR(11) — release cascade の KIND_* 系では **11 = WEAK** で、 `.fields` / `.methods` の要素文字列の release が weak-table no-op になり 1 個/読み leak。 KIND_STR(7) に修正。 (b) `__type_parent` の Optional セルが中身 = TypeHandle id (整数) なのに kind=Object — release されたら整数に `__release_object` して暴走する形。 KIND_NONE に修正。 (c) reflection member 読み (`t.name` / `t.fields` 等、 全部 owned を返す) が借用分類で leak — `field_is_property_access` が TypeHandle member 読みを fresh と認識するよう拡張 (`.kind` の interned enum box は rc=-1 で release no-op なので一律 fresh で安全)。
3. **callback 駆動 iteration の use-after-free** (`3fbf7667`)。 配列の forEach / map / filter / find / findIndex / every / some が `(len, data)` を 1 回だけ読んで生バッファを歩く — callback 内の push (realloc) / pop で**解放済みメモリを走査** (`arr.forEach` 内の `arr.push` で walk が途中で止まる)。 共通 `CellSnapshot` (セルを先にコピー + heap 要素は iteration 中 +1 保持、 `__map_for_each` と同じ規約) に統一。 Map の forEach も key しか retain しておらず、 callback 内 delete で snapshot の **value** が解放されて読みが実行ごとに揺れていた — value も retain。 さらに `lower_index` の fresh receiver 経路が Object 要素限定で release していたため、 `freshStrArray()[0]` で配列丸ごと leak — 全要素種で release + `is_arc_slot` 要素は retain に一般化。

**検証**: workspace nextest 530/530、 AOT arm 全 fixture PASS、 nested_generic.il 400 並列 0/400。 回帰 fixture 3 件: `05_edge_cases/downcast_arc.il`、 `02_classes/typeof_reflection_arc.il`、 `05_edge_cases/iter_mutation_snapshot.il`。

**probe で問題なしを確認した周辺** (再調査不要): regex (compile/test/find/findAll/split/replace churn)、 generic fn + heap 引数/戻り churn、 Promise.all / race (型注釈付き — 注釈なしは既知の desugar 制限で適切な診断あり)、 const 畳み込み (連鎖参照 ✓、 0 除算は実行時 panic に遅延)、 string split/indexOf/lastIndexOf/slice/charAt の端、 Set forEach (element retain 実装済み)、 `i64::MIN / -1` 等の数値の罠 (第 5 弾で確認済み)。

### [解決済み記録] fn 本体内の自己再帰 closure を `ClosureSelf` で対応 (2026-06-11、 `70b6dde8`)

**旧状態**: `let fib: fn(i64): i64 = fn(n) { ... fib(...) }` はトップレベル (slot 経由の遅延解決) のみ対応。 fn 本体内では型注釈があっても `undefined function` で型検査落ち。 capture では原理的に解決できない (構築前の値の snapshot になるか、 cell 経由だと closure ↔ cell の retain 循環になる)。

**実装**: 新 MIR 命令 `Inst::ClosureSelf` = 隠し末尾 env パラメータ (= 実行中の closure 自身) を実体化する。 closure 本体内の自分の束縛名への参照 (呼び出し・値利用) はこれに解決される — capture なし・循環なし・escape しても「参照」が closure 自身に同行するので安全。 入れ子 closure が外側 closure の名前を参照する場合は ClosureSelf の値 snapshot を retain 付きで capture (外側は内側を保持しないので循環でない)。 tail で自分自身を返す場合は borrow 扱いで retain。 型検査は注釈付き fn 型を RHS 検査用 env に事前束縛 ([crates/ilang-types/src/checker/stmt.rs](crates/ilang-types/src/checker/stmt.rs))。 注釈は必須 (再帰型は推論不能) で、 注釈なしは `SelfRecursiveClosureNeedsAnnotation` の専用診断 (旧: 裸の undefined function)。 トップレベルの slot 遅延解決は従来どおり。

**検証**: workspace nextest 530/530、 AOT arm PASS、 nested_generic.il 400 並列 0/400。 fixture: `10_closures_arc/closure_self_recursion_fn_body.il` (素 / capture 併用 / escape / 入れ子 / 200 周 churn)、 `closure_self_recursion_unannotated_error.il`。 docs: syntax.md / syntax_ja.md のクロージャ節に追記。 ilang-lsp release ビルド済み。

### [解決済み記録] fixture 増殖ラウンド第 5 弾: 深い解放 cascade の stack overflow と REPL の chunk 跨ぎ解放 (2026-06-11、 `6c5437dd` + `80736da5`)

probe 対象を数値演算の罠 (`i64::MIN / -1`・shift 64・float→int の NaN/inf)、 深い連結構造の解放、 自己再帰 closure、 range の端、 対話 REPL へ拡張した。 2 件の重大バグ:

1. **~10 万リンクの連結構造を解放すると native stack overflow** (`6c5437dd`)。 [crates/ilang-runtime/src/cascade.rs::release_field_by_kind](crates/ilang-runtime/src/cascade.rs) が object → optional → object と 1 リンクごとに再帰していた。 入れ子の release を thread-local の worklist に積んで最外フレームだけが drain する反復方式に変更 — 呼び出し深さはグラフ深さに依らず O(1)。 解放順は深さ優先から幅優先寄りに変わるが、 ARC は兄弟間の解放順を保証しないし、 deinit から届く値は未解放のまま (保守方向)。 deinit は 1 ノード 1 回 (fixture の 5 万連鎖カウンタで pin)。 fixture: `05_edge_cases/deep_release_iterative.il` (Optional 連結 10 万 / enum cons 連鎖 10 万 / deinit カウント 5 万)。
2. **対話 REPL で heap 値の slot が chunk 終了ごとに解放され、 次の行が解放済みメモリを読む** (`80736da5`)。 __main のエピローグ (slot 一掃 release — ファイル実行では「終了前に deinit を発火」として正しい) が REPL の chunk ごとに走っていた。 `let arr = [1,2,3]` の次の行で `arr[1]` が無言の SIGSEGV、 `arr.length` が 0、 object の field 読みが garbage、 Map 読みが死ぬ。 `lower_program_with_slots_opts(.., release_slots_at_exit)` を追加し REPL は false。 同エピローグの Cell arm が「cell の中身だけ release」する旧規約のままだった点も第 2 弾のモデルに統一。 回帰テスト: `crates/ilang-cli/tests/repl.rs` (セッションをパイプで流して primitive / array / object / map / string の chunk 跨ぎを pin、 nextest 対象に +5 件)。

**問題なしを確認した周辺** (再調査不要): `i64::MIN / -1` (= MIN、 trap せず) と `% -1` (= 0)、 整数 0 除算の panic 診断、 `<< 63/64`、 float→int の飽和 (`1e300 as i64` = MAX、 NaN = 0)、 range の端 (`5..2` / `3..3` 空、 `0..=0` 1 回、 負範囲)、 配列リテラルの固定長型と `push` の型エラー (run / REPL 一致)。

### [解決済み記録] fixture 増殖ラウンド第 4 弾: async desugar の await 位置と Promise 印字 (2026-06-10 後続セッション、 `87a8ea2a` + `65ff1e4c`)

probe 対象を AOT 一斉実行 (`ILANG_TEST_AOT=1` 全 fixture)、 実行中 closure の自己差し替え、 interface dispatch churn、 set 演算の object 要素、 async の深掘り、 match/if-let payload の escape、 overload + heap 引数へ拡張した。 async だけが 2 系統で崩れていた:

1. **代入文の RHS / 入れ子ブロック内の await でコンパイラ panic または誤診断** (`87a8ea2a`)。 await-lift 前処理 ([crates/ilang-parser/src/normalize/async_desugar/await_lift.rs](crates/ilang-parser/src/normalize/async_desugar/await_lift.rs)) に Assign / AssignField / AssignIndex の arm がなく、 while / loop / for-in 本体・if 枝・match 腕・素の block へも降りていなかった。 `total = total + await p` は文位置だと `unreachable!("NoAwait after body_contains_await=true")` の **コンパイラ panic**、 ループ内だと「await は async fn 内のみ」という誤った診断になっていた。 lift を代入系 + 入れ子ブロックへ再帰させ (持ち上げた `let __await_tN` は見つけた block 内に留めて反復/条件性を保存)、 持ち上げ不能な位置 (`&&`/`||` の右辺等) は unreachable ではなく正規の診断にした。
2. **Promise 値の print / format が生ポインタ** (`65ff1e4c`)。 `console.log(p)` と `${p}` が raw-int fallback に落ちていた。 `$print.promise` / `$fmt.promise` を追加して `<promise pending|resolved|rejected>` を出すようにした。

**検証**: workspace nextest 525/525、 AOT arm 全 fixture PASS (1342 runs)、 nested_generic.il 16×25×2 = 0/800。 fixture 3 件: `04_modules/async_await_in_assignment.il`、 `04_modules/async_await_logical_rhs_error.il`、 `04_modules/promise_print_state.il`。

**probe で問題なしを確認した周辺** (再調査不要): AOT arm の全 fixture (今日の ARC 修正一式を含む)、 実行中 closure が自分を保持する field を差し替える形 (旧 closure は呼び出し終了まで生存 ✓)、 interface (vtable) dispatch + interface 型配列の churn、 object 要素の set union / intersection / difference churn、 async class method + await チェーン (P4 検証値一致)、 match / if-let の payload を容器へ escape、 heap 引数の overload dispatch。

### [解決済み記録] fixture 増殖ラウンド第 3 弾: heap 値 property の ARC を owned 規約に統一 (2026-06-10 後続セッション、 `df0e3f16`)

probe 対象を deinit 連鎖の周辺 (deinit 中の field 読み・メソッド呼び・コンテナ経由の連鎖発火)、 async churn、 generic class + heap field、 static field、 Unicode 文字列、 heap 値 property へ拡張した。 property だけが 3 点で崩れていた:

1. **setter への fresh 値が 1 個/call leak** — instance / static とも setter 呼び出し経路 ([crates/ilang-mir/src/lower/expr.rs](crates/ilang-mir/src/lower/expr.rs) の AssignField property 分岐) が transient +1 を release していなかった (setter 本体の store は AssignField 経由で自前 retain を取る)。
2. **`{ this.inner }` (Field tail) の getter が +1 を返すのに消費側は借用扱い** — 旧 `is_property_getter` の除外は bare-var tail だけに効いており、 Field/Index tail の getter は tail_is_borrow の無条件 retain で +1 を返していた。 entry が上書きされた時点で 1 share が永久に残る (`__map_get` と同型)。
3. **fresh tail の computed getter (`get fresh(): Box { new Box(..) }`) は読みごとに 1 個 leak** — 借用規約では mint された値の所有者が存在しないため、 構造的に直せない。

**修正 (owned 規約へ統一)**: getter は常に +1 を返す (bare-var tail の `is_property_getter` 除外を撤去 — フィールド/メソッドと同じ tail retain 規則に一本化し、 フラグ自体を削除)。 消費側は `is_fresh_object_expr` の `Field` arm で receiver の静的型が構文的に解決できる場合 (`Var` 束縛 / repl slot / `this` / クラス名 = static property) に `obj.prop` を fresh と分類 ([body_cx.rs::field_is_property_access](crates/ilang-mir/src/lower/body_cx.rs)、 lowering の dispatch と同じ property_getter map を参照)。 setter は instance / static とも fresh 値を呼び出し後に release。

**既知の制限**: receiver の静的型を構文的に解決できない形 (`call().prop`、 `arr[0].prop` 等) は借用 fallback のままなので、 その読みは getter の +1 が leak する (use-after-free にはならない)。 is_fresh が lowering 後の型を見られるようになったら解消できる。

**検証**: workspace nextest 525/525、 nested_generic.il 16×25×2 = 0/800。 回帰 fixture: `08_properties/property_heap_value_arc.il`。 **probe で問題なしを確認した周辺** (再調査不要): deinit 連鎖中の自 field 読み / メソッド呼び / tally 書き込み、 Map overwrite / array pop によるコンテナ経由の連鎖 deinit 発火回数、 async fn churn (`await` ループ 200 周 delta=32 定数)、 generic class (string / array / nested Box) churn、 static field 増分 churn、 Unicode 文字列メソッド (length / charAt / slice / split / indexOf)。

### [解決済み記録] 継承時の `deinit` を Swift と同様の自動連鎖に変更 (2026-06-10 後続セッション、 `37a4e8b9`)

**旧挙動**: `class Derived: Base` で両方に `deinit` がある場合、 派生側の deinit が親側を**置き換え**、 親の後始末は走らなかった (明示の `super.deinit()` が連鎖手段だった — 旧 `inheritance_deinit_chain.il` がその挙動を pin していた)。

**新仕様 (ユーザー判断)**: Swift と同様の自動連鎖。 破棄時に最派生クラスの deinit → 各祖先の deinit の順で自動実行。 deinit を持たないクラスは連鎖を切らずにスキップ。 `super.deinit()` は自動連鎖と二重実行になるため、 他の明示 deinit 呼び出しと同じく型検査で拒否 (`CannotCallDeinit`)。

**実装**: [crates/ilang-mir/src/lower/decl/class.rs](crates/ilang-mir/src/lower/decl/class.rs) の class decl で、 自前 deinit + 祖先 deinit の両方があるクラスに `Class.deinit$chain` wrapper (自前 deinit を呼んでから直近祖先の drop_fn を呼ぶ合成 MIR fn) を生成して `drop_fn` に据える。 祖先側の drop_fn は処理順 (親が先) により既に連鎖済みなので再帰的に根まで届く。 deinit 本体内の早期 `return` でも連鎖が切れないよう、 tail への呼び出し追記ではなく wrapper 方式を採用。 `super.deinit()` の拒否は [crates/ilang-types/src/checker/expr/mod.rs](crates/ilang-types/src/checker/expr/mod.rs) の SuperCall arm。

**検証**: workspace nextest 525/525、 nested_generic.il 16×25×2 = 0/800。 fixture: `02_classes/inheritance_deinit_chain.il` (新仕様に書き換え)、 `02_classes/inheritance_super_deinit_error.il` (拒否の pin)、 `09_subtyping/deinit_chain_to_parent.il` (Base 型束縛経由・deinit なし中間クラス・早期 return・200 周 churn)。 docs: syntax.md / syntax_ja.md のクラス基本節と継承節に連鎖規則を明記。 ilang-lsp も release ビルド済み。

### [解決済み記録] fixture 増殖ラウンド第 2 弾で検出した 3 系統 (2026-06-10 後続セッション、 `7ba1d28f`〜`9912a69c`)

probe 対象を可変ローカル / top-level slot / field / enum / Optional の上書き churn、 配列の fill / concat / slice / reverse / removeAt / unshift / sort、 cell capture (閉包が捕獲変数へ代入)、 break 値、 再帰、 vtable、 継承解放へ拡張した。 検出と修正:

1. **cell capture (mutating capture) が cell + 中身を毎構築 leak** (`7ba1d28f`)。 capture 登録が `is_cell` を「leak for now」でスキップしており、 `__release_closure` の cascade が cell を解放できなかった (1 閉包あたり 56〜80 bytes)。 所有権モデルを「scope が生成 +1 を持ち scope exit で release / 各閉包が MakeClosure で +1 retain し cascade で release」に統一。 落とし穴 2 つ: (a) 外側閉包の env から転送する cell ポインタが `MirTy::I64` で型付けされていて retain が無効化 → cascade だけ効いて過剰解放 (`closure_nested_shared_cell.il` の index OOB)。 cell 配列型で型付けして解決。 (b) その場で mint する private snapshot cell は閉包が生成 +1 を引き取る (retain しない)。 break の早期脱出 sweep も「中身を release」から「cell を release」に変更 (旧挙動は生存閉包に対して slot を dangle させていた)。
2. **`arr.fill(v)` の rc 2 点** (`f7bce86c`)。 lowering が fresh な fill 値の transient +1 を release せず 1 個/call leak。 runtime が「旧値 release → 新値 retain」の順だったため、 自己 fill (`arr.fill(arr[0])`) で自分の slot の値を retain 前に解放しうる。 retain-first に変更。
3. **`loop { break v }` の評価値が borrowed 分類で 1 個/評価 leak** (`7ba1d28f` に同梱、 fixture は `9912a69c`)。 `lower_break` は borrowed break 値に Retain を積むので loop の評価値は常に +1 所有 — `ExprKind::Loop` を fresh whitelist に追加。

**検証**: workspace nextest 525/525 (新 fixture 3 件込み)、 nested_generic.il 16×25×2 = 0/800。 probe: cell capture 全形 (未呼び出し / escape / 兄弟 2 閉包共有 / 入れ子転送) delta=0、 fill churn + 自己 fill ✓、 break 値 churn delta=0。

**回帰 fixture 3 件**: `10_closures_arc/cell_capture_release.il`、 `03_collections/array_fill_arc.il`、 `05_edge_cases/loop_break_value_ownership.il`

**probe で問題なしを確認した周辺** (再調査不要): 可変ローカル / top-level slot / field / enum / Optional / weak field の上書き churn、 concat / slice / reverse / removeAt / unshift / sort (object 要素)、 string メソッド群 + `+=` churn、 Result churn、 for-in churn、 閉包配列 churn、 再帰 (heap 引数/戻り)、 vtable churn、 派生クラスを Base 型束縛で解放 (動的 class_id で extra field も cascade ✓)、 Set values / Map entries-keys-forEach churn。

### [解決済み記録] fixture 増殖ラウンドで検出した 5 系統 (2026-06-10 後続セッション)

「さらに fixture を追加してバグをあぶり出す」ラウンド。 leak は `test.liveAllocBytes()` / `test.liveStringCount()` を 200 周ループで挟む probe、 別名健全性は「束縛 → 上書き / delete 後に読む」probe で網羅した。 検出と修正:

1. **`arc_peephole` の不健全な対消去** (`480ed47a`、 正しさバグ)。 `is_safe_to_cross` が「他値の Retain/Release は跨いで安全」としていたが、 コンテナの Release は cascade で候補値の指す先を解放しうる。 `retain v; release map; load_field v; release v` の対が消され、 `makeMap()["k"].n` / `makeArr()[1].n` の inline member 読みが解放済みメモリを返していた (束縛経由だと正常なので気づきにくい)。 他値の Release / WeakRelease を barrier に変更。 Retain は増やすだけなので跨ぎ可のまま。
2. **weak back-ref の cascade 中二重解放** (`d3b1d2cf`、 既存の潜在バグ)。 `__release_object` は strong rc を 0 にしてから field cascade を歩くが、 cascade が「親への `.weak` を持つ子」 を解放すると、 子の field release → `__release_weak(親)` が strong==0 を見て親本体を free → cascade 終了後に親自身の release 末尾がもう一度 free。 親→子配列 + 子→親 weak の定番形で解放順依存の SIGSEGV / malloc abort。 cascade 前に guard weak を 1 本取り、 cascade 後に返す形に変更 (free は `__release_weak` に一本化)。 `weak_parent_back_reference.il` が確率的にこの形を踏んでいた。
3. **配列 `indexOf` / `includes` / `remove` の string ポインタ比較** (`ebb95b4a`、 正しさバグ)。 生セル比較のため intern された literal 同士しか一致せず、 `["aa","bb"].indexOf("b" + "b")` が -1。 string kind の要素は内容比較に変更 (`==` の構造的等値に一致)。 object は参照等値のまま (`==` と一致)。 `remove` は needle ではなく**格納セル**の share を release するよう修正 (内容等値では別ポインタ)。
4. **fresh transient の release 漏れ 3 兄弟** (`8b9f8c31`)。 (a) `m.get/has/delete(new Key(1))` / `s.has/delete(...)` の fresh needle 引数 — lowering が `set`/`add` しか release していなかった。 24 bytes/call (object) または registry string 1 個/call の leak。 (b) fresh receiver のメソッド (`("v"+s).includes(..)` 等) — string/array/Optional は結果が receiver の格納物を share なしで借りることがない (pop/shift は移転、 find/unwrap は retain 済み) ので呼び出し後に blanket release。 (c) `.length` / `.isSome` / `.isNone` は field 経路なので別途 lower_field 側にも release を追加。
5. **template literal の評価ごと leak** (`aace25c7`)。 `lower_template` が `fmt_value` 結果と中間 `str_concat` 結果を release せず、 最終結果も borrowed 分類で誰も release しない — `${i}` 1 個につき registry string 2 個が永久に残っていた (console.log のテンプレも同様)。 中間値は消費した concat の直後に release、 `Template` を `is_fresh_object_expr` の whitelist に追加して消費側が結果を release。 空テンプレートは intern された "" を返してしまうので fresh copy を mint して契約を無条件化。

**検証**: workspace nextest 525/525 (新 fixture 8 件込み)、 nested_generic.il 16×25×2 = 0/800。 probe で fresh needle / fresh receiver / template churn / weak 解放順 すべて delta=0。

**回帰 fixture 8 件**: `05_edge_cases/fresh_receiver_inline_member_read.il`、 `05_edge_cases/weak_backref_cascade_release_order.il`、 `03_collections/array_indexof_string_content_eq.il`、 `03_collections/map_set_fresh_needle_arg_release.il`、 `05_edge_cases/leak_fresh_receiver_members.il`、 `05_edge_cases/leak_template_literal_loop.il`、 `03_collections/map_index_borrow_aliasing.il`、 `03_collections/map_value_kinds_churn.il`

**probe で「漏れない」ことを確認済みの周辺** (再調査不要): `m[new Key(1)]` index 読みの fresh key (escape 解析で stack 化)、 set の union/intersection/difference の fresh set 引数、 property getter の fresh receiver、 `s.add` の fresh 引数、 console.log の fresh string 引数、 文字列 literal 同士の concat。

### [解決済み記録] `Map<K, Object>` の overwrite で 24 bytes/iter leak は `__map_get` の読み出し retain と borrow 前提の消費側の二重 retain だった

**再現スクリプト** (修正済み、保管用):

```il
use std.test as test
class Box { n: i64; init(x: i64) { this.n = x } }
let m = new Map<string, Box>()
let base = test.liveAllocBytes()
let i = 0
while i < 200 {
    m["a"] = new Box(i + 1)         // 同じ key "a" に毎周上書き
    test.expect(m["a"].n, i + 1)
    i = i + 1
}
let after = test.liveAllocBytes()
console.log(`delta=${after - base}`)  // 修正前 4800 (= 200 × 24)、 修正後 24 (= map 内に生存中の最後の 1 個)
```

**真因 (2026-06-10 後続セッションで確定 + 修正、 `f3d0a899`)**:

当初仮説 (「`__map_set` の旧値 release 漏れ」) は外れ — overwrite release は [crates/ilang-runtime/src/maps.rs::__map_set](crates/ilang-runtime/src/maps.rs) に正しく入っていた (write-only ループでは leak しない)。 真因は読み出し側の規約矛盾:

- `__map_get` が読み出しごとに heap 値を retain して +1 で返していた (`d8c7f548` で導入)
- 一方 lowering の `is_fresh_object_expr` は `Index` の freshness を receiver に委譲するため、 `m[k]` (m が local) は「借用」と分類される
- その結果 `let x = m["a"]` は束縛時にもう 1 つ Retain を積み (stmt.rs の non-fresh 束縛規約)、 fn-body tail の `m[k]` も borrow retain を積む — ランタイムが付けた +1 を誰も release せず、 entry が上書き / delete された時点で +1 が永久に残る
- `arc_peephole::is_safe_to_cross` も「`MapGet` は自前で rc を増やさない」前提で whitelist しており、 ランタイムの retain は optimizer の前提とも矛盾していた

**修正**: `__map_get` の retain-on-read を撤去し、 Map の index 読みを `ArrayLoad` と同じ borrow 規約に統一。 `__map_get_optional` は fresh な Optional cell が +1 を所有するので retain を残す。 fresh receiver (`make_map()["k"]`) の経路だけ [crates/ilang-mir/src/lower/literals.rs::lower_index](crates/ilang-mir/src/lower/literals.rs) で「値 Retain → 孤児 map Release」 を発行して map release の value cascade による解放を防ぐ。

**検証結果**:

- 上記再現: delta=24 (合格条件 `delta < 1024` 達成)。 overwrite + alias + delete + clear + map drop を混ぜた churn では delta=0
- 別名健全性 (UAF なし): 「束縛 → 上書き / delete 後も旧値が読める」「fn tail で `m[k]` を返す」「`m.get(k)` Optional 経路」「string / array 値の overwrite + alias」「他コンテナへの移送」「fresh receiver index」 全 PASS
- `04_modules/events_basic.il` (retain-on-read 導入の動機だった EventEmitter) PASS
- workspace nextest: **525 / 525 PASS** (新 fixture 込み)
- nested_generic.il 16 並列 × 25 batch × 2 連続: **0/800**
- 回帰 fixture: `03_collections/map_overwrite_releases_prev_object.il` を追加

---

### [解決済み記録] `nested_generic.il` 系の SIGABRT は class method body の bare-var tail に retain が抜けていた

**真因 (2026-06-10 本セッションで確定 + 修正)**:

`class Box<T> { x: T; get(): T { x } }` のように **class method body の tail expression が bare var (= 暗黙の `this.field`)** のとき、 [crates/ilang-mir/src/lower/body_cx.rs:695](crates/ilang-mir/src/lower/body_cx.rs:695) の `tail_is_borrow` 判定が `ExprKind::Index { .. } | ExprKind::Field { .. }` の 2 形だけを borrow として扱い、 **`ExprKind::Var(name)` だが env / capture 解決できない (= class field) ケースを取りこぼしていた**。 その結果、 lower_block_hinted の scope-exit retain (L698-) が走らず、 戻り値が aliased のまま caller に返り、 caller は `lower_block` の scope-exit release で `release v7` を発行 → b1 の rc が 1 不足 → 後続の `release(b2)` の drop_fn による field release で b1 が freed → 直後の `release(b1)` (= let scope exit) が use-after-free。 malloc が同 slot の reuse 順序を ASLR で踏むかどうかで ~15% の確率 race として観測されていた。

`Box<i64>.get` (戻り値型 i64) は heap でないので発火条件を満たさず、 1 段 Box (= `Box<i64>` のみ) では race 0/400、 2 段 Box (= `Box<Box<i64>>` の field load が heap pointer) で初めて発火していた。

**修正**:

[crates/ilang-mir/src/lower/body_cx.rs::lower_block_hinted](crates/ilang-mir/src/lower/body_cx.rs:695) の `tail_is_borrow` に `ExprKind::Var(name)` arm を追加: `self.this_class.is_some() && env.lookup_binding(name).is_none() && capture にも居ない` ときは borrow とみなして scope-exit retain を発行する。 callee 側で正しい fresh +1 を caller に渡せて ARC が均衡する。

**検証結果**:

- nested_generic.il 最小再現 16 並列 × 25 batch × 2 連続: **0/800** (修正前 ~6% 発火、 dd40bc49 の `forget(compiled)` 撤去状態で再計測)
- workspace nextest: **525 / 525 PASS**
- 同時に dd40bc49 の `crates/ilang-cli/src/main.rs::run_file` の `std::mem::forget(compiled)` を撤去 (race 真因が消えたので drop も無罪)
- 周辺切り分けで得た事実 (本セッション保管用):
  - cranelift 0.131.1 → 0.132.1 への上げは race 確率不変で経路 A / B どちらにも効かず (= cranelift_module 自体は無罪)。 fix とは独立に依存上げは温存
  - Program / Function::value_tys / EnumLayout の MirTy は run_main 前後で完全一致 → ilang MIR data は intact、 corruption は ARC ref count 側
  - `process::exit(0)` で std rt cleanup を skip しても race が残った点が「真因は run_main 中」 を示唆していた

### [参考記録] 旧仮説と切り分け過程 (解決後の保管用)

`cranelift_module::ModuleDeclarations` の drop が真因方向 — という当初仮説の根拠と、 そこから真因へ辿った試行は次の通り。 同じパターンの race を将来踏んだときの足場として残す。

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

つまり cranelift_module / hashbrown のクライアント API の呼び方が壊している(declare_function / declare_data の重複登録、 ライフタイム違反、 ASLR-aware な内部状態を踏むサイズ依存)もしくは cranelift_module 自身の bug。

**cranelift バージョン上げは無関係と確定 (2026-06-10 本セッション)**:

`crates/ilang-mir-codegen/Cargo.toml` の cranelift 系 7 行を `0.131` → `0.132` に上げ (Cargo.lock 上は 0.132.1 で解決、 API breaking なし) て同じ最小再現を 16 並列 × 25 batch を 2 連続流したところ、 forget(compiled) を撤去した状態で 65/400 (16.25%) と 64/400 (16.00%) で発火、 0.131.1 時代の ~15% と統計差なし。 crash stack も `hashbrown::raw::RawTable::drop → drop_in_place<ModuleDeclarations> → drop_in_place<Compiled>` で 0.131.1 と完全に同じ経路。 → **cranelift_module 側の修正は 0.132.1 までに入っていない**ことが確認できたので、 依存上げは温存したまま forget(compiled) を戻して dd40bc49 状態を維持。

**次にやるべきこと**:

- 残る `Program::drop` race の真因究明: heap guard が捕まえない以上、 ilang コンパイラ side の Vec の中身が壊れている = `__mir_alloc` 由来でない別の overrun(JIT generated code が cranelift JIT page 経由で書き出すアドレス計算の bug が一番濃い)。 一段切り分けには次の手:
  - `Program` 自身も `std::mem::forget` してみて race が消えるかで「真因が drop の中か別か」を判別(本セッションでは Plan を守って forget(compiled) のみ commit)
  - 残る方なら `process::exit(0)` で全 drop を skip し、 fixture suite の flake を完全に消す(JIT child は短命なので無害)
- 「2 段 .get() chain → test.expect」 path で mono 化された method の declare_function を全部 dump して、 重複 declare がないかを確認:
  - `Box_i64.get`, `Box_Box_i64.get` のような mangled name が unique か
  - declaration 順序、 linkage、 signature の一貫性
- 次に試すなら cranelift 0.133 以降 (本セッションでは最新が 0.132.1 だったが無効)。 ただし 0.131 → 0.132 で何も改善しなかった事実から、 期待値は低い
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
