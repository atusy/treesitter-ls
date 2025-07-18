// Definition jump resolution using tree-sitter queries
use std::collections::HashMap;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator, Tree};

#[derive(Debug, Clone)]
pub struct DefinitionCandidate {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_position: tree_sitter::Point,
    pub end_position: tree_sitter::Point,
    pub definition_type: String,
    pub scope_depth: usize,
    pub distance_to_reference: usize,
    pub scope_ids: Vec<usize>, // IDs of enclosing scopes from innermost to outermost
}

#[derive(Debug, Clone)]
pub struct ReferenceContext {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_position: tree_sitter::Point,
    pub end_position: tree_sitter::Point,
    pub reference_type: String,
    pub context_type: ContextType,
    pub scope_ids: Vec<usize>, // IDs of enclosing scopes from innermost to outermost
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContextType {
    FunctionCall,
    VariableReference,
    TypeAnnotation,
    FieldAccess,
    Unknown,
}

#[derive(Default)]
pub struct DefinitionResolver {
    pub context_patterns: HashMap<String, Vec<String>>,
}

impl DefinitionResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve definition jump using scope analysis
    pub fn resolve_definition<'a>(
        &self,
        text: &'a str,
        tree: &'a Tree,
        query: &'a Query,
        cursor_byte_offset: usize,
    ) -> Option<DefinitionCandidate> {
        // Step 1: Collect all definitions and references
        let (definitions, references) = self.collect_definitions_and_references(text, tree, query);

        // Step 2: Find the reference at cursor position
        let target_reference = self.find_reference_at_cursor(&references, cursor_byte_offset)?;

        // Step 3: Extract target text
        let target_text = &text[target_reference.start_byte..target_reference.end_byte];

        // Step 4: Find matching definitions with enhanced scope analysis
        let candidates =
            self.find_matching_definitions(&definitions, target_text, target_reference, text);

        // Step 5: Rank candidates using language-agnostic scoring
        self.rank_and_select_best_candidate(candidates, target_reference)
    }

    fn collect_definitions_and_references<'a>(
        &self,
        text: &'a str,
        tree: &'a Tree,
        query: &'a Query,
    ) -> (Vec<DefinitionCandidate>, Vec<ReferenceContext>) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());

        let mut definitions = Vec::new();
        let mut references = Vec::new();

        while let Some(match_) = matches.next() {
            for capture in match_.captures {
                let capture_name = &query.capture_names()[capture.index as usize];
                let node = capture.node;
                let start_byte = node.start_byte();
                let end_byte = node.end_byte();

                if capture_name.starts_with("local.definition") {
                    let definition_type = capture_name
                        .strip_prefix("local.definition.")
                        .unwrap_or("unknown")
                        .to_string();

                    definitions.push(DefinitionCandidate {
                        start_byte,
                        end_byte,
                        start_position: node.start_position(),
                        end_position: node.end_position(),
                        definition_type,
                        scope_depth: self.calculate_scope_depth(node),
                        distance_to_reference: 0, // Will be calculated later
                        scope_ids: self.get_scope_ids(node),
                    });
                } else if capture_name.starts_with("local.reference") {
                    let reference_type = capture_name
                        .strip_prefix("local.reference.")
                        .unwrap_or("reference")
                        .to_string();

                    let context_type =
                        if reference_type == "function_call" || reference_type == "method_call" {
                            ContextType::FunctionCall
                        } else if reference_type == "type" {
                            ContextType::TypeAnnotation
                        } else if reference_type == "field" {
                            ContextType::FieldAccess
                        } else if reference_type == "variable" {
                            ContextType::VariableReference
                        } else {
                            self.determine_context_type(node)
                        };

                    references.push(ReferenceContext {
                        start_byte,
                        end_byte,
                        start_position: node.start_position(),
                        end_position: node.end_position(),
                        reference_type,
                        context_type,
                        scope_ids: self.get_scope_ids(node),
                    });
                }
            }
        }

        (definitions, references)
    }

    fn find_reference_at_cursor<'a>(
        &self,
        references: &'a [ReferenceContext],
        cursor_byte_offset: usize,
    ) -> Option<&'a ReferenceContext> {
        references.iter().find(|ref_ctx| {
            cursor_byte_offset >= ref_ctx.start_byte && cursor_byte_offset <= ref_ctx.end_byte
        })
    }

    fn find_matching_definitions(
        &self,
        definitions: &[DefinitionCandidate],
        target_text: &str,
        target_reference: &ReferenceContext,
        text: &str,
    ) -> Vec<DefinitionCandidate> {
        definitions
            .iter()
            .filter_map(|def| {
                let def_text = &text[def.start_byte..def.end_byte];
                if def_text == target_text {
                    // Temporal ordering constraint: prefer definitions that come before the reference
                    // This prevents cases like `let stdin = stdin()` where the variable on the left
                    // should not be considered for the function call on the right
                    let def_pos = def.start_position;
                    let ref_pos = target_reference.start_position;

                    // Allow forward references only for certain definition types (functions, types)
                    let allows_forward_reference = matches!(
                        def.definition_type.as_str(),
                        "function" | "method" | "type" | "struct" | "enum" | "class"
                    );

                    if def_pos.row > ref_pos.row
                        || (def_pos.row == ref_pos.row && def_pos.column >= ref_pos.column)
                    {
                        // Definition comes after reference
                        if !allows_forward_reference {
                            return None; // Skip this definition
                        }
                    }

                    let mut candidate = def.clone();
                    candidate.distance_to_reference =
                        self.calculate_distance_by_position(def, target_reference);
                    Some(candidate)
                } else {
                    None
                }
            })
            .collect()
    }

    fn rank_and_select_best_candidate(
        &self,
        mut candidates: Vec<DefinitionCandidate>,
        target_reference: &ReferenceContext,
    ) -> Option<DefinitionCandidate> {
        if candidates.is_empty() {
            return None;
        }

        // Sort by multiple criteria for language-agnostic ranking
        candidates.sort_by(|a, b| {
            // 1. Prefer definitions that are in scope
            let a_in_scope = self.is_in_scope_by_ids(&a.scope_ids, &target_reference.scope_ids);
            let b_in_scope = self.is_in_scope_by_ids(&b.scope_ids, &target_reference.scope_ids);

            match (a_in_scope, b_in_scope) {
                (true, false) => return std::cmp::Ordering::Less,
                (false, true) => return std::cmp::Ordering::Greater,
                _ => {}
            }

            // 2. Prefer definitions that match the reference context
            let a_context_match =
                self.context_matches(&a.definition_type, &target_reference.context_type);
            let b_context_match =
                self.context_matches(&b.definition_type, &target_reference.context_type);

            match (a_context_match, b_context_match) {
                (true, false) => return std::cmp::Ordering::Less,
                (false, true) => return std::cmp::Ordering::Greater,
                _ => {}
            }

            // 3. Prefer definitions with greater scope depth (more local)
            let scope_cmp = b.scope_depth.cmp(&a.scope_depth);
            if scope_cmp != std::cmp::Ordering::Equal {
                return scope_cmp;
            }

            // 4. Prefer definitions closer to the reference
            a.distance_to_reference.cmp(&b.distance_to_reference)
        });

        candidates.into_iter().next()
    }

    fn calculate_scope_depth(&self, node: Node) -> usize {
        let mut depth = 0;
        let mut current = node.parent();

        while let Some(parent) = current {
            // Language-agnostic scope detection based on node types
            let node_type = parent.kind();
            if self.is_scope_node(node_type) {
                depth += 1;
            }
            current = parent.parent();
        }

        depth
    }

    fn is_scope_node(&self, node_type: &str) -> bool {
        // Language-agnostic scope patterns
        matches!(
            node_type,
            "block"
                | "function_item"
                | "function_declaration"
                | "function_definition"
                | "method_definition"
                | "if_statement"
                | "if_expression"
                | "while_statement"
                | "while_expression"
                | "for_statement"
                | "for_expression"
                | "loop_expression"
                | "match_expression"
                | "match_statement"
                | "try_statement"
                | "catch_clause"
                | "class_definition"
                | "class_declaration"
                | "struct_item"
                | "enum_item"
                | "impl_item"
                | "module"
                | "namespace"
                | "scope"
                | "chunk"
                | "do_statement"
                | "closure_expression"
                | "lambda"
                | "arrow_function"
        )
    }

    fn determine_context_type(&self, node: Node) -> ContextType {
        // Walk up the AST to determine context
        let mut current = node.parent();

        while let Some(parent) = current {
            match parent.kind() {
                // Function call patterns
                "call_expression" | "function_call" => return ContextType::FunctionCall,
                // Type annotation patterns
                "type_annotation" | "type_identifier" | "type_parameter" => {
                    return ContextType::TypeAnnotation;
                }
                // Field access patterns
                "field_expression" | "member_expression" | "field_access" => {
                    return ContextType::FieldAccess;
                }
                _ => {}
            }
            current = parent.parent();
        }

        ContextType::VariableReference
    }

    pub fn context_matches(&self, definition_type: &str, context_type: &ContextType) -> bool {
        match context_type {
            ContextType::FunctionCall => {
                matches!(definition_type, "function" | "method" | "macro" | "import")
            }
            ContextType::TypeAnnotation => {
                matches!(definition_type, "type" | "struct" | "enum" | "class")
            }
            ContextType::FieldAccess => {
                matches!(definition_type, "field" | "property" | "attribute")
            }
            ContextType::VariableReference => {
                matches!(
                    definition_type,
                    "var" | "variable" | "parameter" | "let" | "const"
                )
            }
            ContextType::Unknown => true, // Don't filter based on unknown context
        }
    }

    fn get_scope_ids(&self, node: Node) -> Vec<usize> {
        let mut scope_ids = Vec::new();
        let mut current = node.parent();

        while let Some(n) = current {
            if self.is_scope_node(n.kind()) {
                scope_ids.push(n.id());
            }
            current = n.parent();
        }

        scope_ids
    }

    fn calculate_distance_by_position(
        &self,
        def: &DefinitionCandidate,
        ref_ctx: &ReferenceContext,
    ) -> usize {
        let def_pos = def.start_position;
        let ref_pos = ref_ctx.start_position;

        // Enhanced distance calculation considering scope depth and lexical proximity
        let line_distance = if def_pos.row <= ref_pos.row {
            // Definition comes before reference (normal case)
            ref_pos.row - def_pos.row
        } else {
            // Definition comes after reference (forward reference)
            // Higher penalty for forward references
            (def_pos.row - ref_pos.row) * 100
        };

        // Add column distance for same-line definitions
        let column_distance = if def_pos.row == ref_pos.row {
            if def_pos.column <= ref_pos.column {
                ref_pos.column - def_pos.column
            } else {
                (def_pos.column - ref_pos.column) * 5
            }
        } else {
            0
        };

        // Calculate scope distance using scope IDs
        let scope_distance =
            self.calculate_scope_distance_by_ids(&def.scope_ids, &ref_ctx.scope_ids);

        // Weighted combination: prioritize scope proximity over line distance
        (scope_distance * 1000) + (line_distance * 10) + column_distance
    }

    fn is_in_scope_by_ids(&self, def_scope_ids: &[usize], ref_scope_ids: &[usize]) -> bool {
        // Check if any of the reference's scope IDs contain the definition
        // The definition is in scope if it shares a common ancestor scope
        for ref_scope_id in ref_scope_ids.iter() {
            if def_scope_ids.contains(ref_scope_id) {
                return true;
            }
        }
        false
    }

    fn calculate_scope_distance_by_ids(
        &self,
        def_scope_ids: &[usize],
        ref_scope_ids: &[usize],
    ) -> usize {
        // Find the common scope depth
        let mut common_depth = 0;
        for (i, def_id) in def_scope_ids.iter().rev().enumerate() {
            if let Some(j) = ref_scope_ids.iter().rev().position(|&id| id == *def_id) {
                common_depth = i.min(j) + 1;
                break;
            }
        }

        // Distance is the sum of steps from each to common ancestor
        let def_distance = def_scope_ids.len().saturating_sub(common_depth);
        let ref_distance = ref_scope_ids.len().saturating_sub(common_depth);
        def_distance + ref_distance
    }
}
