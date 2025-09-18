# 大規模リファクタリング計画

## 変更ルール
- [ ] step-by-stepで動作可能な範囲で`git commit`しながら進める
- [ ] `git commit`前には必ず `just fmt check`を実行し問題を解決
- [ ] README.md/CONTRIBUTING.mdも随時更新
- [ ] 進捗や気付きを逐次このファイルに記載

## 全体像
過剰な中間層を削除し、シンプルで理解しやすいアーキテクチャへ移行

### 現在のアーキテクチャ
```
lsp (TreeSitterLs)
  ↓
workspace (Workspace)
  ↓
analysis + language (LanguageCoordinator) + document (DocumentStore)
  ↓
runtime (ConfigStore) + text + config
```

### 目標アーキテクチャ
```
lsp (TreeSitterLs)
  ↓
language (LanguageCoordinator + ConfigStore) + document (DocumentStore) + analysis
  ↓
text + config
```

## フェーズ1: runtime モジュールの削除
**目的**: 不要な間接層を削除し、ConfigStoreを適切な場所へ移動

### ステップ
- [x] ConfigStoreをlanguageモジュールへ移動
- [x] runtime::configの参照をlanguage::config_storeへ更新
- [x] runtimeモジュールを削除
- [x] テストの修正と実行
- [x] `cargo fmt && cargo clippy`実行
- [ ] コミット: "refactor: remove runtime module and move ConfigStore to language"

**完了の定義**:
- runtimeモジュールが存在しない
- 全テストが通過
- ConfigStoreがlanguage::config_storeとしてアクセス可能

## フェーズ2: domain モジュールの解体
**目的**: 薄いラッパー層を削除し、型定義を適切な場所へ配置

### ステップ
- [ ] domain::semanticをanalysis::semanticへ統合
- [ ] domain::settingsをconfig::settingsへ移動
- [ ] LSP型の再エクスポートを削除（直接lsp_typesを使用）
- [ ] domainモジュールの参照を更新
- [ ] domainモジュールを削除
- [ ] テストの修正と実行
- [ ] `just fmt check`実行
- [ ] コミット: "refactor: dissolve domain module and relocate types"

**完了の定義**:
- domainモジュールが存在しない
- 全ての型定義が適切な場所に配置
- 全テストが通過

## フェーズ3: Workspace構造体の簡素化
**目的**: 過剰な委譲メソッドを削除し、責任を明確化

### ステップ
- [ ] TreeSitterLsが直接LanguageCoordinatorとDocumentStoreを保持
- [ ] Workspaceの単純な委譲メソッドを削除
- [ ] 必要に応じてWorkspace自体を削除
- [ ] lsp_implの更新
- [ ] テストの修正と実行
- [ ] `just fmt check`実行
- [ ] コミット: "refactor: simplify workspace abstraction"

**完了の定義**:
- 不要な委譲メソッドが削除
- TreeSitterLsが直接必要なコンポーネントを操作
- 全テストが通過

## フェーズ4: DocumentView traitの削除
**目的**: 過度な抽象化を削除し、直接的なアクセスへ

### ステップ
- [ ] 分析関数をDocumentまたはDocumentHandleを直接受け取るよう変更
- [ ] DocumentView traitの参照を削除
- [ ] document::viewモジュールを削除
- [ ] テストの修正と実行
- [ ] `just fmt check`実行
- [ ] コミット: "refactor: remove DocumentView trait abstraction"

**完了の定義**:
- DocumentView traitが存在しない
- 分析関数が直接Document型を扱う
- 全テストが通過

## 進捗状況
- 開始時刻: 2025-01-19
- 現在のフェーズ: 計画作成中
- 完了フェーズ: なし

## 変更中に得た気付き
（変更を進めながら記載）

## リスクと対策
- **リスク**: 大規模な変更により一時的にビルドが壊れる可能性
- **対策**: 各フェーズごとにテストを実行し、動作を確認してからコミット

## 成功指標
- コード行数の削減（目標: 10-20%削減）
- モジュール数の削減（11 → 8モジュール）
- 不要な抽象化の除去
- テストカバレッジの維持または向上