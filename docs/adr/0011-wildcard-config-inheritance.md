# ADR-0011: Wildcard Config Inheritance

## Status

Proposed

## Context

ADR-0010 defines how configuration merges across layers (user → project → init_options). However, there's another dimension of merging: **within a single config**, the `_` (wildcard) key serves as defaults that should be inherited by specific entries.

Currently in `captureMappings`:
- `captureMappings._` defines default capture-to-token mappings for all languages
- `captureMappings.rust` can define rust-specific overrides

The expected behavior is that `rust` inherits all mappings from `_` and only overrides what it explicitly specifies. This is similar to how CSS cascading works with wildcards.

This pattern could also apply to other HashMaps:
- `languages._` could define default language settings (e.g., default `bridge` filter)
- `bridge.servers._` could define default server settings

## Decision

**The `_` key in HashMaps serves as a wildcard that provides defaults inherited by all specific keys.**

### Inheritance Rules

When resolving configuration for a specific key (e.g., `rust`):

```
effective_config[rust] = merge(config["_"], config["rust"])
```

1. Start with `_` (wildcard) values as the base
2. Override with specific key values
3. Missing specific key → use `_` entirely
4. Missing `_` → use specific key entirely
5. Both missing → no config for that key

### Application Order

Wildcard inheritance happens **after** cross-layer merging (ADR-0010):

```
1. Merge across layers:    final_config = merge_all([user, project, init_options])
2. Resolve wildcards:      effective[rust] = merge(final_config["_"], final_config["rust"])
```

This means:
- Layer merging produces a single `captureMappings` with both `_` and language-specific entries
- At resolution time, `_` is applied as defaults for each language

### Affected HashMaps

| HashMap | Wildcard Key | Use Case |
|---------|--------------|----------|
| `captureMappings` | `_` | Default capture-to-token mappings for all languages |
| `languages` | `_` (future) | Default language settings (e.g., default `bridge` filter) |
| `bridge.servers` | `_` (future) | Default server settings |

### Example: captureMappings

```toml
[captureMappings._.highlights]
"variable" = "variable"
"variable.builtin" = "variable.defaultLibrary"
"function" = "function"
"function.builtin" = "function.defaultLibrary"

[captureMappings.rust.highlights]
"type.builtin" = "type.defaultLibrary"  # rust-specific addition

# Effective config for rust:
# "variable" = "variable"                    # inherited from _
# "variable.builtin" = "variable.defaultLibrary"  # inherited from _
# "function" = "function"                    # inherited from _
# "function.builtin" = "function.defaultLibrary"  # inherited from _
# "type.builtin" = "type.defaultLibrary"    # rust-specific
```

### Example: Overriding a wildcard value

```toml
[captureMappings._.highlights]
"comment" = "comment"

[captureMappings.lua.highlights]
"comment" = ""  # suppress comments for lua specifically

# Effective config for lua:
# "comment" = ""  # overridden (empty string suppresses the token)
```

## Consequences

### Positive

- **DRY configuration**: Define common mappings once in `_`, override only exceptions
- **Intuitive**: Matches user expectation of "wildcard = default"
- **Extensible**: Pattern can be applied to other HashMaps as needed
- **Consistent with CSS/TOML conventions**: `_` as wildcard is familiar

### Negative

- **Two-phase resolution**: Must merge layers first, then resolve wildcards
- **Complexity**: Users must understand both cross-layer and wildcard merging
- **Order matters**: `_` must be processed before specific keys during resolution

### Neutral

- **`_` is reserved**: Cannot use `_` as an actual language/server name
- **Explicit over implicit**: Language must be listed to get non-wildcard config
- **Lazy resolution**: Wildcards resolved at access time, not at load time

## Implementation Notes

```rust
fn resolve_with_wildcard<V: Merge>(
    map: &HashMap<String, V>,
    key: &str,
) -> Option<V> {
    let wildcard = map.get("_");
    let specific = map.get(key);

    match (wildcard, specific) {
        (Some(w), Some(s)) => Some(w.merge(s)),
        (Some(w), None) => Some(w.clone()),
        (None, Some(s)) => Some(s.clone()),
        (None, None) => None,
    }
}
```

## Implementation Phases

### Phase 1: captureMappings Wildcard (Not started)
- [ ] Implement `resolve_with_wildcard()` function
- [ ] Apply wildcard resolution to `captureMappings` lookup
- [ ] Unit tests for wildcard resolution with various combinations

### Phase 2: languages Wildcard (Future)
- [ ] Apply wildcard resolution to `languages._` for default language settings
- [ ] Integration test: Define `_` in user config, override in project

### Phase 3: languageServers Wildcard (Future)
- [ ] Apply wildcard resolution to `languageServers._` for default server settings
- [ ] E2E test: Verify semantic tokens use wildcard mappings correctly

## Related Decisions

- [ADR-0010](0010-configuration-merging-strategy.md): Cross-layer configuration merging strategy
