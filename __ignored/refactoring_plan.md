# Refactoring Plan

## 背景と目的
- LSPエントリポイント `TreeSitterLs` が言語・ドキュメント・設定処理を一手に抱えモジュール境界が崩壊している。
- `language`/`document`/`analysis`/`text` が互いの詳細実装や `tower_lsp` 型に強く依存し、再利用性と保守性が低い。
- モジュール間の責務を再定義し、テスタビリティと将来の機能拡張を容易にする大規模リファクタリングを進める。

## 変更方針
- プロジェクトは未公開であり、後方互換性を気にせず大胆に設計を見直す。
- 既存コードへの忖度は不要。読みやすさと責務分離を最優先に、不要なAPIや構造は遠慮なく破棄・改名する。

## 変更計画
1. `LanguageCoordinator` と関連ストアのAPIを閉じ、`language` モジュールを廃止して `runtime::{registry,config,query,loader}` など責務別に再編する。
2. LSPサーバ層と内部ロジックを分離するため、`TreeSitterLs` からドキュメント／言語操作を切り出した `workspace` 層（例: `workspace::{documents,languages}`）を新設する。
3. `DocumentStore` の公開APIを見直し、`DashMap` や `tower_lsp` 依存を隠蔽するハンドル型を導入する。
4. `analysis`/`text` モジュールを `tower_lsp` 型に依存しない純粋なロジックに再構成する。
   4.1. 共有ドメイン型を `domain` モジュールとして整備する。
   4.2. `text`/`analysis` の API をドメイン型に差し替える。
        - 4.2.1 `text/position` と `analysis/selection`
        - 4.2.2 `analysis/definition`
        - 4.2.3 `analysis/refactor`
        - 4.2.4 `analysis/semantic`
   4.3. LSP への変換レイヤーを `lsp` モジュール側に集約する。
5. 上記移行後にテスト・ドキュメント・サンプルを更新し、不要コードを削除する。

## 進捗状況
- [x] 1. `language` モジュール再編とAPI整備（2025-02-14 完了）
- [x] 2. `workspace` 層の新設と `TreeSitterLs` の依存整理（2025-02-14 完了）
- [x] 3. `DocumentStore` API刷新（2025-02-14 完了）
- [ ] 4. `analysis`/`text` のLSP依存排除
  - [x] 4.1 ドメイン型の定義と整備（2025-02-14 完了）
  - [x] 4.2 `text`/`analysis` のドメイン化（`text`/`analysis` 系のLSP依存解消完了）
        - [x] 4.2.1 `text/position` と `analysis/selection`
        - [x] 4.2.2 `analysis/definition`
        - [x] 4.2.3 `analysis/refactor`
        - [x] 4.2.4 `analysis/semantic`
  - [ ] 4.3 LSP変換レイヤの集約
- [ ] 5. テスト・ドキュメント更新と不要コード削除

## 変更中に得た気付き
- 2025-02-14: `language` 配下の型が広範囲に公開されているため、段階的に `runtime` へ移植しつつ互換レイヤを置く必要がある。
- 2025-02-14: `RuntimeCoordinator` に「既にロード済みか」を問い合わせるAPIが必要だったので `is_language_loaded` を追加。後続ステップでも API 単位の用途を洗い出す。
- 2025-02-14: `workspace::Workspace` を介してLSP層から `DocumentStore`/`RuntimeCoordinator` への直接アクセスを排除。今後の `DocumentStore` 改修時は `WorkspaceDocuments` のAPIを狭めていく方針にする。
- 2025-02-14: `DocumentStore` に `DocumentHandle` と `SemanticSnapshot` を導入し、DashMapガードと `tower_lsp` 型を外部APIから排除できた。Step4で解析層からも LSP 依存を取り除きやすくなった。
- 2025-02-14: Step4 はドメイン型導入→`text`/`analysis` の段階的移行→LSP 変換集約の3段階で進めるべきと判断。今後はこの順序で作業する。
- 2025-02-14: Step4 のサブタスクを 4.2.1〜4.2.4 に分割。まず `text/position`/`analysis::selection` を最初に置き換え、徐々に複雑なモジュールへ展開する。
- 2025-02-14: `analysis`/`text` から `tower_lsp` 依存を排除した結果、`lsp_impl` 側で Domain ↔ LSP の変換ヘルパーを実装。今後のStep4.3ではこのヘルパーを活用しながらエントリポイントを整理する。
- 2025-02-14: ドメイン型を追加するにあたり `url` を直接依存に追加した。LSP 層の変換を進める際はこのモジュールを噛ませる前提で設計する。

## 変更ルール
- ステップごとに動作可能な状態で小まめに `git commit` する。
- 各コミット前に必ず `make test format lint` を実行し、検出された問題を解消する。
- README.md と CONTRIBUTING.md を変更が生じ次第随時更新する。
- 進捗や気付きを都度この `__ignored/refactoring_plan.md` に追記・更新する。
