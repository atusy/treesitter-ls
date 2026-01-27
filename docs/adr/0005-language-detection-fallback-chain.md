# ADR-0005: Language Detection Fallback Chain

## Status

Accepted (Supersedes [ADR-0002](0002-filetype-detection-via-extension-mapping.md))

## Context

ADR-0002 established extension-based document-level language detection as the primary method with LSP languageId as fallback. However, this approach has limitations:

1. **LSP clients are authoritative**: Modern LSP clients (VS Code, Neovim, etc.) already perform sophisticated language detection and send accurate `languageId` values
2. **Extension mapping is redundant**: Duplicating what clients already do creates maintenance burden and potential conflicts
3. **Missing heuristic layer**: Files without extensions (e.g., `Dockerfile`, scripts with shebangs) aren't handled well

Additionally, PBI-061 removed the `filetypes` configuration field entirely, eliminating the ability to configure extension mappings in the server. This forces a rethinking of the detection strategy.

The key insight is: **detection should find an *available* Tree-sitter parser, not just identify a language name**. If the detected language has no parser loaded, detection should continue to the next method.

This applies to both document-level language detection and injected language resolution (e.g., code blocks in Markdown).

## Decision

**Implement a fallback chain that continues until an available Tree-sitter parser is found.** This applies at two levels:

1. **Document-level**: Detecting the primary language when a file is opened
2. **Injection-level**: Resolving embedded languages within a parsed document

```
1. LSP languageId  →  Check if parser available  →  If yes: use it
                                                 →  If no: continue
2. Alias resolution →  Map to canonical name     →  Check if parser available  →  If yes: use it
                                                                               →  If no: continue
3. Heuristic       →  Check if parser available  →  If yes: use it
                                                 →  If no: continue
4. File extension  →  Check if parser available  →  If yes: use it
                                                 →  If no: return None
```

### Priority Order Rationale

1. **LSP languageId (highest priority)**
   - Client has full context: file path, content, user preferences, workspace settings
   - Already handles complex cases: `.tsx` vs `.ts`, polyglot files, user overrides
   - Trust the client—it knows best

2. **Alias resolution (second priority)**
   - Maps alternative languageId values to canonical parser names
   - Configured via `aliases` field in language config:
     ```toml
     [languages.markdown]
     aliases = ["rmd", "qmd"]
     ```
   - Handles cases where editors send non-standard languageIds that the user cannot control
   - Example: Editor sends `rmd` for R Markdown files → alias maps to `markdown` parser

4. **Heuristic analysis (middle priority)**
   - Shebang detection: `#!/usr/bin/env python` → python
   - Magic comments: `# -*- mode: ruby -*-` → ruby
   - File patterns: `Makefile` → make, `Dockerfile` → dockerfile
   - Useful when client sends generic languageId (e.g., "plaintext")
   - Candidate implementation: syntect's `find_syntax_for_file` (reads first line for shebang/magic)

5. **File extension (lowest priority)**
   - Strips the dot: `.rs` → `rs`, `.py` → `py`
   - No mapping — uses extension directly as parser name candidate
   - Fallback when above methods fail or return unavailable parsers

### Availability Check

Each detection method returns a candidate language. Before accepting it:

```rust
fn detect_language(&self, path: &str, language_id: Option<&str>, content: &str) -> Option<String> {
    // 1. Try languageId directly
    if let Some(lang_id) = language_id {
        if self.has_parser_available(lang_id) {
            return Some(lang_id.to_string());
        }

        // 2. Try alias resolution (e.g., "rmd" → "markdown")
        if let Some(canonical) = self.resolve_alias(lang_id) {
            if self.has_parser_available(&canonical) {
                return Some(canonical);
            }
        }
    }

    // 3. Try heuristics (shebang, etc.)
    if let Some(candidate) = self.detect_from_heuristics(path, content) {
        if self.has_parser_available(&candidate) {
            return Some(candidate);
        }
    }

    // 4. Try file extension
    if let Some(candidate) = self.detect_from_extension(path) {
        if self.has_parser_available(&candidate) {
            return Some(candidate);
        }
    }

    None
}
```

This means:
- If client sends `languageId: "rmd"` and alias maps `rmd` → `markdown`, use the markdown parser
- If client sends `languageId: "typescript"` but only JavaScript parser is loaded, fall through to check if extension `.ts` maps to an available parser
- If shebang says `python3` but Python parser isn't installed, continue to extension check

### Language Injection

The fallback chain also applies to **injected languages** (e.g., code blocks in Markdown, JavaScript inside HTML). Injection queries extract a language identifier, but this identifier needs resolution:

```
Document (markdown) ──parse──▶ AST ──injection query──▶ "py" ──detect──▶ python
                                                      ▶ "sh" ──detect──▶ bash
```

For example, a Markdown code fence with ` ```py ` provides the identifier `"py"`, which must be resolved to an available parser. This resolution follows a fallback pattern:

1. **Try the identifier directly**: Check if a parser named `"py"` is available
2. **Normalize and retry**: If not, map aliases (`py` → `python`, `js` → `javascript`, `sh` → `bash`) and check again
   - Candidate implementation: syntect's `find_syntax_by_extension` provides alias mappings
3. **Skip if unavailable**: If no parser matches, the region is skipped

This means:
- Injected languages benefit from the same graceful degradation
- A Markdown file can have some code blocks with semantic tokens and others without, depending on installed parsers
- Alias normalization is needed for both document-level `languageId` and injection identifiers

## Consequences

### Positive

- **Respects client authority**: LSP clients invest heavily in language detection
- **No configuration needed**: Works out of the box without `filetypes` mapping
- **Graceful degradation**: Missing parsers don't block detection entirely
- **Handles edge cases**: Shebangs, magic comments, extensionless files
- **Simpler configuration**: Removed redundant `filetypes` field (PBI-061)

### Negative

- **Heuristic overhead**: Reading file content for shebang detection adds I/O
- **Non-deterministic**: Same file might use different parsers on different systems (based on available parsers)
- **Heuristic maintenance**: Shebang patterns need ongoing updates
- **languageId naming variance**: Clients may send languageIds that differ from parser names (e.g., `shellscript` vs `bash`); normalization may be needed later

### Neutral

- **Extension mapping still exists**: But as last resort, not primary method
- **Parser availability matters**: Detection result depends on what's installed
- **Auto-install interaction**: Detection completes first (returning None if no parser found); auto-install runs asynchronously afterward, making the parser available for subsequent requests
- **Caching**: Detection result is stored per-document; cache invalidates on content change or `languageId` change from client

## Migration from ADR-0002

The `filetypes` configuration field has been removed (PBI-061). Users who relied on custom extension mappings should:

1. Configure their LSP client to send the correct `languageId`
2. Use file associations in their editor (e.g., VS Code's `files.associations`)
3. Add shebangs or magic comments to extensionless files

This aligns with the principle: **configure at the source (client), not the sink (server)**.
