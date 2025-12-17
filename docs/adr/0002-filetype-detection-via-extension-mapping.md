# ADR-0002: Filetype Detection via Extension Mapping

## Status

Accepted

## Context

treesitter-lsは、開いたファイルに対して適切なTree-sitterパーサーを選択する必要がある。LSPプロトコルでは、クライアントが`textDocument/didOpen`でlanguage_idを送信するが、クライアントによって動作が異なり、一貫性がない場合がある。

言語解決の方法として以下の選択肢が考えられる:

1. **LSPクライアントのlanguage_idに完全依存** - クライアント任せ
2. **ファイル拡張子による設定ベースマッピング** - サーバー側で制御
3. **ファイル内容のヒューリスティック解析** - shebang、マジックコメント等

## Decision

**ファイル拡張子による設定ベースマッピングを優先し、LSPクライアントのlanguage_idをフォールバックとして使用する。**

具体的な実装:
- `FiletypeResolver`が拡張子→言語のマッピングを保持
- 設定ファイルで各言語の`filetypes`を定義
- ファイルオープン時: 拡張子マッピング → LSP language_id の優先順位で解決
- 一度決定された言語はドキュメントのライフタイム中保持

```rust
let language_name = self
    .language
    .get_language_for_path(uri.path())        // 優先: ファイル拡張子
    .or_else(|| language_id.map(|s| s.to_string())); // 代替: LSPクライアント
```

## Consequences

### Positive

- LSPクライアントの実装差異に影響されない予測可能な動作
- ユーザーが設定ファイルで完全に言語マッピングを制御可能
- 同じファイルを異なるエディタで開いても一貫した動作
- シンプルな実装で高速な言語解決

### Negative

- 拡張子のないファイル（Makefile等）は設定で明示的にマッピングが必要
- `file.tar.gz`のような複合拡張子は最後の部分（`gz`）のみ認識
- shebangによる言語検出（例: `#!/usr/bin/env python`）は未サポート

### Neutral

- LSPクライアントのlanguage_idはフォールバックとして機能し、未設定の拡張子に対応可能
- 編集中の言語変更は想定外（ファイルを閉じて再度開く必要あり）
