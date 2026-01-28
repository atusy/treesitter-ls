//! Semantic token legend and capture mapping utilities.
//!
//! This module defines the semantic token types and modifiers supported by the LSP,
//! and provides functions to map tree-sitter capture names to LSP token types.

use crate::config::{CaptureMappings, WILDCARD_KEY};
use tower_lsp_server::ls_types::{SemanticTokenModifier, SemanticTokenType};

/// Semantic token types supported by the LSP legend.
pub const LEGEND_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::COMMENT,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::REGEXP,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::TYPE,
    SemanticTokenType::STRUCT,
    SemanticTokenType::CLASS,
    SemanticTokenType::INTERFACE,
    SemanticTokenType::ENUM,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::TYPE_PARAMETER,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::METHOD,
    SemanticTokenType::MACRO,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::EVENT,
    SemanticTokenType::MODIFIER,
    SemanticTokenType::DECORATOR,
];

/// Semantic token modifiers supported by the LSP legend.
pub const LEGEND_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::READONLY,
    SemanticTokenModifier::STATIC,
    SemanticTokenModifier::DEPRECATED,
    SemanticTokenModifier::ABSTRACT,
    SemanticTokenModifier::ASYNC,
    SemanticTokenModifier::MODIFICATION,
    SemanticTokenModifier::DOCUMENTATION,
    SemanticTokenModifier::DEFAULT_LIBRARY,
];

/// Apply capture mappings to transform a capture name
///
/// Looks up the capture name in the provided mappings and returns the mapped value if found,
/// or the original capture name if no mapping exists.
///
/// # Arguments
/// * `capture_name` - The original capture name from the tree-sitter query
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The full capture mappings configuration
///
/// # Returns
/// `Some(mapped_name)` for known token types, `None` for unknown types.
/// Unknown types (not in LEGEND_TYPES) should not produce semantic tokens.
pub(crate) fn apply_capture_mapping(
    capture_name: &str,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
) -> Option<String> {
    if let Some(mappings) = capture_mappings {
        // Try filetype-specific mapping first
        if let Some(ft) = filetype
            && let Some(lang_mappings) = mappings.get(ft)
            && let Some(mapped) = lang_mappings.highlights.get(capture_name)
        {
            // Explicit mapping to empty string means "filter this capture"
            return (!mapped.is_empty()).then(|| mapped.clone());
        }

        // Try wildcard mapping
        if let Some(wildcard_mappings) = mappings.get(WILDCARD_KEY)
            && let Some(mapped) = wildcard_mappings.highlights.get(capture_name)
        {
            // Explicit mapping to empty string means "filter this capture"
            return (!mapped.is_empty()).then(|| mapped.clone());
        }
    }

    // No mapping found - check if the base type is in SemanticTokensLegend.
    // If not, return None to skip adding to all_tokens.
    // This prevents unknown captures (e.g., @spell) from blocking meaningful
    // tokens at the same position during deduplication.
    let base_type = capture_name.split('.').next().unwrap_or("");
    if LEGEND_TYPES.iter().any(|t| t.as_str() == base_type) {
        Some(capture_name.to_string())
    } else {
        None
    }
}

/// Map capture names from tree-sitter queries to LSP semantic token types and modifiers
///
/// Capture names can be in the format "type.modifier1.modifier2" where:
/// - The first part is the token type (e.g., "variable", "function")
/// - Following parts are modifiers (e.g., "readonly", "defaultLibrary")
///
/// Returns `None` for unknown token types (not in LEGEND_TYPES).
/// Unknown modifiers are ignored.
pub(crate) fn map_capture_to_token_type_and_modifiers(capture_name: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = capture_name.split('.').collect();
    let token_type_name = parts.first().copied().filter(|s| !s.is_empty())?;

    let token_type_index = LEGEND_TYPES
        .iter()
        .position(|t| t.as_str() == token_type_name)? as u32;

    let mut modifiers_bitset = 0u32;
    for modifier_name in &parts[1..] {
        if let Some(index) = LEGEND_MODIFIERS
            .iter()
            .position(|m| m.as_str() == *modifier_name)
        {
            modifiers_bitset |= 1 << index;
        }
    }

    Some((token_type_index, modifiers_bitset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legend_types_includes_keyword() {
        assert!(LEGEND_TYPES.iter().any(|t| t.as_str() == "keyword"));
    }

    #[test]
    fn test_legend_modifiers_includes_readonly() {
        assert!(LEGEND_MODIFIERS.iter().any(|m| m.as_str() == "readonly"));
    }

    #[test]
    fn test_map_capture_to_token_type_and_modifiers() {
        // Test basic token types without modifiers
        assert_eq!(
            map_capture_to_token_type_and_modifiers("comment"),
            Some((0, 0))
        );
        assert_eq!(
            map_capture_to_token_type_and_modifiers("keyword"),
            Some((1, 0))
        );
        assert_eq!(
            map_capture_to_token_type_and_modifiers("function"),
            Some((14, 0))
        );
        assert_eq!(
            map_capture_to_token_type_and_modifiers("variable"),
            Some((17, 0))
        );

        // Unknown types return None - they should not produce semantic tokens
        assert_eq!(
            map_capture_to_token_type_and_modifiers("unknown"),
            None,
            "'unknown' is not in LEGEND_TYPES"
        );
        assert_eq!(
            map_capture_to_token_type_and_modifiers("spell"),
            None,
            "'spell' is a tree-sitter hint, not a semantic token type"
        );
        assert_eq!(
            map_capture_to_token_type_and_modifiers("markup"),
            None,
            "'markup' is not in LEGEND_TYPES"
        );
        assert_eq!(
            map_capture_to_token_type_and_modifiers(""),
            None,
            "empty string should return None"
        );

        // Test with single modifier
        let (_, modifiers) = map_capture_to_token_type_and_modifiers("variable.readonly").unwrap();
        assert_eq!(modifiers & (1 << 2), 1 << 2); // readonly is at index 2

        let (_, modifiers) = map_capture_to_token_type_and_modifiers("function.async").unwrap();
        assert_eq!(modifiers & (1 << 6), 1 << 6); // async is at index 6

        // Test with multiple modifiers
        let (token_type, modifiers) =
            map_capture_to_token_type_and_modifiers("variable.readonly.defaultLibrary").unwrap();
        assert_eq!(token_type, 17); // variable
        assert_eq!(modifiers & (1 << 2), 1 << 2); // readonly
        assert_eq!(modifiers & (1 << 9), 1 << 9); // defaultLibrary

        // Test unknown modifiers are ignored
        let (token_type, modifiers) =
            map_capture_to_token_type_and_modifiers("function.unknownModifier.async").unwrap();
        assert_eq!(token_type, 14); // function
        assert_eq!(modifiers & (1 << 6), 1 << 6); // async should still be set
    }
}
