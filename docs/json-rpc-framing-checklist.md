# JSON-RPC Framing Checklist for LSP Bridge Development

## Context

This checklist was created during Sprint 134 Retrospective after successfully implementing JSON-RPC framing for LSP stdio communication. JSON-RPC framing is a common error pattern in LSP implementations that requires careful attention to protocol details.

## Reference Implementation

See Sprint 134 commits:
- `707496d` - feat(bridge): implement JSON-RPC message framing for LSP

## Checklist

Use this checklist when implementing or reviewing JSON-RPC message framing code:

### Writing Messages (to Language Server stdin)

- [ ] Content-Length header included
  - [ ] Header format: `Content-Length: {byte_count}\r\n`
  - [ ] Byte count is UTF-8 byte length, NOT character count
  - [ ] Example: "hello" = 5 bytes, "你好" = 6 bytes (UTF-8)

- [ ] Header/body separator is `\r\n\r\n` (CRLF CRLF)
  - [ ] NOT `\n\n` (LF LF)
  - [ ] NOT `\r\n` (single CRLF)

- [ ] JSON body is valid UTF-8
  - [ ] Use `serde_json::to_vec()` to serialize to UTF-8 bytes
  - [ ] Do not use `to_string()` + `as_bytes()` (same result but less direct)

- [ ] Async I/O patterns
  - [ ] Use `AsyncWrite` trait (tokio::io::AsyncWriteExt)
  - [ ] Write header and body in single write operation or flush between
  - [ ] Handle write errors (broken pipe, process exit)

### Reading Messages (from Language Server stdout)

- [ ] Parse Content-Length header
  - [ ] Read until `\r\n\r\n` separator
  - [ ] Extract numeric value from `Content-Length: {value}\r\n`
  - [ ] Handle missing or malformed header gracefully

- [ ] Read exactly Content-Length bytes for body
  - [ ] Use `AsyncRead::read_exact()` with buffer of correct size
  - [ ] Do NOT read until EOF (stdout may have multiple messages)
  - [ ] Handle EOF or short read as error

- [ ] Parse JSON body
  - [ ] Use `serde_json::from_slice()` on exact byte buffer
  - [ ] Handle JSON parse errors (invalid JSON, encoding issues)

- [ ] Async I/O patterns
  - [ ] Use `BufReader` for efficient header parsing
  - [ ] Use `AsyncRead` trait (tokio::io::AsyncReadExt)
  - [ ] Handle read errors (process exit, broken pipe)

### Common Pitfalls to Avoid

- [ ] Character count vs. byte count
  - Problem: Using `str.len()` for Content-Length with non-ASCII characters
  - Solution: Use `as_bytes().len()` or `to_vec().len()`

- [ ] Separator confusion
  - Problem: Using `\n\n` instead of `\r\n\r\n`
  - Solution: Always use CRLF line endings per HTTP-style headers

- [ ] Partial reads
  - Problem: Reading until newline instead of exact byte count
  - Solution: Use `read_exact()` with Content-Length as buffer size

- [ ] Flushing issues
  - Problem: Header written but body buffered, causing timeout
  - Solution: Flush after header or write header+body atomically

- [ ] Process lifecycle
  - Problem: Not handling process exit during read/write
  - Solution: Check process health, handle broken pipe gracefully

## Testing Recommendations

### Unit Tests
- Test with ASCII-only messages (simple case)
- Test with Unicode messages (UTF-8 byte length vs. character count)
- Test with large messages (buffer handling)
- Test with malformed headers (error handling)
- Test with truncated messages (EOF handling)

### E2E Tests
- Verify real language server can initialize (full handshake)
- Verify request/response cycle works (completion, hover, etc.)
- Verify process cleanup (no zombie processes)

## Related Documentation

- LSP Specification: Base Protocol (JSON-RPC over stdio)
- ADR-0012: Multi-LS Async Bridge Architecture (Section 5.1: Initialization Protocol)
- Sprint 134 implementation: `src/lsp/bridge/connection.rs` (read_message, write_message functions)
