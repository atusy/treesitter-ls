use treesitter_ls::definition_resolution::{ContextType, LanguageAgnosticResolver};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_type_determination() {
        let resolver = LanguageAgnosticResolver::new();

        // Test context_matches for different context types
        assert!(resolver.context_matches("function", &ContextType::FunctionCall));
        assert!(resolver.context_matches("method", &ContextType::FunctionCall));
        assert!(!resolver.context_matches("variable", &ContextType::FunctionCall));

        assert!(resolver.context_matches("type", &ContextType::TypeAnnotation));
        assert!(resolver.context_matches("struct", &ContextType::TypeAnnotation));
        assert!(!resolver.context_matches("function", &ContextType::TypeAnnotation));

        assert!(resolver.context_matches("field", &ContextType::FieldAccess));
        assert!(resolver.context_matches("property", &ContextType::FieldAccess));
        assert!(!resolver.context_matches("function", &ContextType::FieldAccess));

        assert!(resolver.context_matches("var", &ContextType::VariableReference));
        assert!(resolver.context_matches("variable", &ContextType::VariableReference));
        assert!(resolver.context_matches("parameter", &ContextType::VariableReference));
        assert!(!resolver.context_matches("function", &ContextType::VariableReference));
    }
}
