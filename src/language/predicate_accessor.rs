use tree_sitter::{Query, QueryMatch, QueryPredicate, QueryProperty};

/// Unified interface for accessing query predicates and properties
pub struct PredicateAccessor;

impl PredicateAccessor {
    /// Get all predicates for a pattern, including both general predicates and property settings
    pub fn get_all_predicates(query: &Query, pattern_index: usize) -> PredicateIterator<'_> {
        PredicateIterator {
            query,
            pattern_index,
            general_predicates: query.general_predicates(pattern_index),
            property_settings: query.property_settings(pattern_index),
            general_index: 0,
            property_index: 0,
        }
    }

    /// Get general predicates for a pattern
    pub fn get_general_predicates(query: &Query, pattern_index: usize) -> &[QueryPredicate] {
        query.general_predicates(pattern_index)
    }

    /// Get property settings for a pattern
    pub fn get_property_settings(query: &Query, pattern_index: usize) -> &[QueryProperty] {
        query.property_settings(pattern_index)
    }

    /// Get all predicates for a match
    pub fn get_match_predicates<'a>(query: &'a Query, match_: &QueryMatch) -> PredicateIterator<'a> {
        Self::get_all_predicates(query, match_.pattern_index)
    }
}

/// Iterator over all predicates (both general and property-based)
pub struct PredicateIterator<'a> {
    query: &'a Query,
    pattern_index: usize,
    general_predicates: &'a [QueryPredicate],
    property_settings: &'a [QueryProperty],
    general_index: usize,
    property_index: usize,
}

/// Unified predicate type that can represent both general predicates and property settings
#[derive(Debug, Clone)]
pub enum UnifiedPredicate<'a> {
    General(&'a QueryPredicate),
    Property(&'a QueryProperty),
}

impl<'a> UnifiedPredicate<'a> {
    /// Get the operator/key of the predicate
    pub fn operator(&self) -> &str {
        match self {
            UnifiedPredicate::General(p) => p.operator.as_ref(),
            UnifiedPredicate::Property(p) => p.key.as_ref(),
        }
    }

    /// Check if this is a property setting (like #set!)
    pub fn is_property(&self) -> bool {
        matches!(self, UnifiedPredicate::Property(_))
    }

    /// Check if this is a general predicate
    pub fn is_general(&self) -> bool {
        matches!(self, UnifiedPredicate::General(_))
    }
}

impl<'a> Iterator for PredicateIterator<'a> {
    type Item = UnifiedPredicate<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        // First yield all general predicates
        if self.general_index < self.general_predicates.len() {
            let predicate = &self.general_predicates[self.general_index];
            self.general_index += 1;
            return Some(UnifiedPredicate::General(predicate));
        }

        // Then yield all property settings
        if self.property_index < self.property_settings.len() {
            let property = &self.property_settings[self.property_index];
            self.property_index += 1;
            return Some(UnifiedPredicate::Property(property));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_predicate_accessor_basic() {
        // This is a basic test structure - actual testing would require
        // creating a Query with predicates, which requires a language
        let _accessor = PredicateAccessor;
        // Test would go here with actual query objects
        assert!(true); // Placeholder
    }

    #[test]
    fn test_unified_predicate_operator() {
        // Would test with actual predicate objects
        assert!(true); // Placeholder
    }
}