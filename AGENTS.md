# Repository Guidelines

## Project Structure & Module Organization
- `src/` contains the Rust crates for the language server. Key submodules:
  - `lsp/` hosts the Tower LSP entry point and protocol adapters.
  - `workspace/` orchestrates documents, languages, and runtime services.
  - `analysis/`, `runtime/`, `document/`, `config/`, and `text/` encapsulate parsing, evaluation, state storage, and utilities.
- `tests/` holds integration tests. Each file exercises cross-module scenarios (e.g., parser pooling, workspace recovery).
- `__ignored/` stores internal plans and notes; files here are ignored by default.
- `src/bin/main.rs` provides the executable entry point used by the LSP client wrappers.

## Build, Test, and Development Commands
- `make test` — runs `cargo test` for unit and integration coverage across `src/` and `tests/`.
- `make format` — formats the codebase via `cargo fmt`.
- `make lint` — executes `cargo clippy -- -D warnings` to enforce lint cleanliness.
- `make test format lint` — preferred pre-commit pipeline combining the three steps.

## Coding Style & Naming Conventions
- Rust code follows default `rustfmt` rules (4-space indentation, trailing commas) enforced by `cargo fmt`.
- Module boundaries mirror directory names; prefer `mod.rs` only for small re-export hubs.
- Public APIs return domain types rather than `tower_lsp::lsp_types::*`; conversions live in `lsp/`.
- Keep file names snake_case (`workspace_service.rs`, `analysis_runtime.rs`).

## Testing Guidelines
- Unit tests reside alongside the modules they cover; integration tests live in `tests/` and use `tokio-test` and `tree-sitter-rust` fixtures.
- Name tests with intent (`test_language_layer_should_be_in_document_module`).
- Run `make test` before opening a PR; new features should add targetted tests.

## Commit & Pull Request Guidelines
- Use concise, imperative commit messages (`docs: expand refactoring plan`, `refactor: avoid excessive use of Arc`).
- Each commit must pass `make test format lint` and keep the tree buildable.
- Pull requests should: describe the change scope, link issues when applicable, summarize test evidence, and flag any follow-up work.
- Rebase before merging to maintain a linear history.

## Architecture Notes
- The LSP layer should remain thin; most logic belongs in `workspace/` and domain modules.
- Runtime components (`runtime/`) supply tree-sitter parsing and query loading; inject them via explicit traits to aid testing.
- Capture mappings and configuration parsing live in `config/`; ensure new settings are documented in `README.md`.
