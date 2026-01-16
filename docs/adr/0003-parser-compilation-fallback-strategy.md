# ADR-0003: Parser Compilation Fallback Strategy

| | |
|---|---|
| **Status** | superseded |
| **Date** | 2025-12-18 |
| **Decision-makers** | atusy |
| **Consulted** | Claude Code |
| **Informed** | tree-sitter-ls users |
| **Superseded by** | [ADR-0004](0004-keep-tree-sitter-cli-dependency.md) |

> **⚠️ SUPERSEDED**: This ADR has been superseded by [ADR-0004](0004-keep-tree-sitter-cli-dependency.md). See that document for the rationale.

## Context and Problem Statement

tree-sitter-ls needs to load Tree-sitter parser shared libraries (`.so`/`.dylib` files) at runtime. Currently, users must either find pre-compiled binaries or install `tree-sitter-cli` via `cargo install tree-sitter-cli` to compile parsers themselves. This creates a high barrier to entry, especially since `tree-sitter-cli` requires the Rust toolchain.

How can we minimize dependencies for parser compilation while maintaining compatibility with all parser repositories?

## Decision Drivers

* **Minimize user dependencies**: Most users should not need to install additional tools beyond a C compiler
* **Maximize compatibility**: Support parsers at various stages of development (pre-generated vs source-only)
* **Graceful degradation**: Provide clear guidance when compilation isn't possible
* **Cross-platform support**: Work on Linux, macOS, and Windows

## Considered Options

1. **Require tree-sitter-cli for all compilation**
2. **Direct C compilation only (require pre-generated parser.c)**
3. **Fallback chain: Direct C compilation → tree-sitter-cli generation**
4. **Ship pre-compiled parsers for common languages**

## Decision Outcome

**Chosen option**: "Fallback chain: Direct C compilation → tree-sitter-cli generation", because it minimizes dependencies for the majority of users (90%+ of parsers ship pre-generated `parser.c`) while maintaining full compatibility with development/bleeding-edge parsers.

### Compilation Flow

```
Parser source directory
         │
         ▼
┌─────────────────────┐
│ src/parser.c exists?│
└─────────┬───────────┘
     yes /  \ no
        /    \
       ▼      ▼
┌──────────┐  ┌─────────────────────┐
│ Compile  │  │ src/grammar.json    │
│ directly │  │ exists?             │
│ with cc  │  └──────────┬──────────┘
└──────────┘        yes /  \ no
                       /    \
                      ▼      ▼
       ┌────────────────┐   ┌─────────────────┐
       │ tree-sitter    │   │ grammar.js      │
       │ generate       │   │ exists?         │
       │ src/grammar.json│  └────────┬────────┘
       │ (no Node.js)   │       yes /  \ no
       └────────────────┘          /    \
                                  ▼      ▼
                   ┌────────────────┐  ┌─────────────┐
                   │ tree-sitter    │  │ Error with  │
                   │ generate       │  │ helpful     │
                   │ grammar.js     │  │ message     │
                   │ (needs Node.js)│  └─────────────┘
                   └────────────────┘
```

### Coverage by Approach

| Parser Has | Approach Used | Dependencies Needed |
|------------|---------------|---------------------|
| `src/parser.c` | Direct compile | C compiler only |
| `src/grammar.json` | CLI generate → compile | C compiler + tree-sitter-cli |
| `grammar.js` only | CLI generate → compile | C compiler + tree-sitter-cli + Node.js |
| Nothing | Error | N/A |

**Note**: Most mature parsers ship both `src/parser.c` and `src/grammar.json`, so typically only a C compiler is needed. Node.js is only required for bleeding-edge parsers that haven't generated their JSON grammar yet.

### Consequences

**Positive:**

* Most users only need a C compiler (cc/gcc/clang), which is commonly available
* Full compatibility with all Tree-sitter parser repositories
* Clear, actionable error messages when dependencies are missing
* No need to maintain pre-compiled binaries for multiple platforms

**Negative:**

* Users with source-only parsers still need tree-sitter-cli
* Compilation requires a C compiler, which may not be installed on all systems
* Different error scenarios require different user actions

**Neutral:**

* The fallback approach adds complexity to the compilation code path
* Users need to understand why different parsers have different requirements

### Confirmation

* Integration tests verify compilation works with:
  - Parsers containing pre-generated `parser.c`
  - Parsers requiring `tree-sitter generate`
* Error message tests verify helpful guidance is provided
* CI tests run on Linux, macOS, and Windows

## Pros and Cons of the Options

### Option 1: Require tree-sitter-cli for all compilation

Always use `tree-sitter build` or `tree-sitter generate` followed by compilation.

* Good, because consistent approach for all parsers
* Good, because tree-sitter-cli handles edge cases (scanner compilation, etc.)
* Bad, because requires Rust toolchain installation (`cargo install tree-sitter-cli`)
* Bad, because significantly higher barrier to entry for users

### Option 2: Direct C compilation only

Only support parsers that ship pre-generated `parser.c`.

* Good, because minimal dependencies (C compiler only)
* Good, because simple implementation
* Bad, because excludes parsers without pre-generated source
* Bad, because prevents using development versions of parsers

### Option 3: Fallback chain (Chosen)

Try direct C compilation first, fall back to tree-sitter-cli if needed.

* Good, because minimizes dependencies for most use cases
* Good, because maintains full compatibility
* Good, because provides clear guidance for each scenario
* Neutral, because adds complexity to handle multiple paths
* Bad, because error handling must cover multiple failure modes

### Option 4: Ship pre-compiled parsers

Distribute pre-compiled `.so`/`.dylib` files for common languages.

* Good, because zero compilation dependencies for common languages
* Good, because fastest setup experience
* Bad, because significant binary size increase
* Bad, because platform matrix explosion (OS × arch × language)
* Bad, because maintenance burden for keeping parsers updated

## More Information

* [Tree-sitter Parser List](https://github.com/tree-sitter/tree-sitter/wiki/List-of-parsers) - indicates which parsers have pre-generated files
* [Tree-sitter Using Parsers Documentation](https://tree-sitter.github.io/tree-sitter/using-parsers/)
* Related: ADR-0001 establishes `language` subcommand group where compilation command would live
