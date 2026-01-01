# Review

## Findings

1. **Blocking – Bridged document never updates after the first hover**
   - In the new async path we always call `ensure_document_open` which unconditionally sends `textDocument/didOpen` with `version: 1` for every request and never follows up with `textDocument/didChange` (`src/lsp/bridge/tokio_async_pool.rs:238-259`). The comment even says “TODO: Track document versions per connection”.
   - The previous implementation (`LanguageServerConnection::did_open_with_notifications`, `src/lsp/bridge/connection.rs:400-495`) persisted a `document_version` counter, rewrote the virtual file, and used `didChange` so the embedded language server always analysed the latest snippet.
   - As written now the first didOpen succeeds, but every subsequent hover tries to re-open the already-open URI with `version: 1`, which violates the LSP spec and causes rust-analyzer to ignore the new content. Users will see stale hover results (or `contentModified` errors) once they edit the Markdown block, because the async bridge never pushes the updated code.

2. **Blocking – Hover requests are issued before rust-analyzer finishes indexing**
   - `TokioAsyncLanguageServerPool::hover` calls `ensure_document_open` and then immediately sends `textDocument/hover` (`src/lsp/bridge/tokio_async_pool.rs:211-235`). There is no equivalent to the old `wait_for_indexing_with_notifications` call that ensured the first request waited for rust-analyzer to finish loading the virtual workspace (`src/lsp/bridge/connection.rs:482-495`).
   - The new end-to-end test had to loop up to 20 times calling `vim.lsp.buf.hover()` until a floating window appears because the first several requests return nothing while rust-analyzer is still indexing (`tests/test_lsp_hover.lua:56-83`). Real clients only send one hover request, so they will just get `null` responses whenever the bridge has to spin up or refresh rust-analyzer.
   - We need to restore the “wait for diagnostics/progress before serving the first hover” behaviour (and probably surface `$ /progress` notifications again) so that a single hover call reliably returns data.

3. **Blocking – Tokio connections leak rust-analyzer processes and temporary workspaces**
   - `TokioAsyncBridgeConnection` drops the spawned `tokio::process::Child` handle as soon as `spawn` returns and the struct keeps only stdin/stdout plus a reader task (`src/lsp/bridge/tokio_connection.rs:35-118`). Its `Drop` impl merely aborts the reader (`src/lsp/bridge/tokio_connection.rs:349-365`); it never sends `shutdown`/`exit`, never waits for the child, and has no reference to the `temp_dir` created in `spawn_and_initialize`.
   - `TokioAsyncLanguageServerPool::spawn_and_initialize` still creates a per-connection temp workspace (`src/lsp/bridge/tokio_async_pool.rs:91-161`), but nothing ever removes those directories or terminates the language server processes. By contrast, `LanguageServerConnection::shutdown` (`src/lsp/bridge/connection.rs:1513-1543`) explicitly sends `shutdown`, waits on the child, and removes the temp dir.
   - The async pool therefore leaks a rust-analyzer process and a `treesitter-ls-*` directory for every connection for the lifetime of the daemon. After a few Markdown hovers you end up with several orphaned servers consuming CPU/memory and cluttering `/tmp`. We need to store the `Child` and temp path in `TokioAsyncBridgeConnection` (or alongside it) and perform the same cleanup on drop.

4. **Major – `$ /progress` notifications are no longer forwarded**
   - The old hover implementation captured and forwarded progress notifications so clients saw “indexing…” messages (loop in `src/lsp/lsp_impl/text_document/hover.rs` before this change). The new async path simply returns whatever `tokio_async_pool.hover` yields and never forwards notifications (`src/lsp/lsp_impl/text_document/hover.rs:112-158`), and the `tokio_notification_rx` added to `TreeSitterLs` is never polled.
   - `TokioAsyncLanguageServerPool` even stores a `notification_sender` (`src/lsp/bridge/tokio_async_pool.rs:29-44`) but nothing in `TokioAsyncBridgeConnection` publishes into it, so the user now loses all progress feedback while rust-analyzer indexes. This regression makes hover feel “stuck” whenever the async bridge has to warm up. Please wire `$/progress` through the new connection and drain the receiver on the LSP side as before.
