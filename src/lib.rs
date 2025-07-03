use dashmap::DashMap;
use libloading::{Library, Symbol};
use serde::Deserialize;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::{Language, Parser, Query, QueryCursor, Tree, StreamingIterator, Node};

const LEGEND_TYPES: &[SemanticTokenType] = &[
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
    SemanticTokenType::DECORATOR,
];


#[derive(Debug, Clone, Deserialize)]
struct HighlightItem {
    #[serde(flatten)]
    source: HighlightSource,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum HighlightSource {
    Path { path: String },
    Query { query: String },
}

#[derive(Debug, Clone, Deserialize)]
struct LanguageConfig {
    library: String,
    highlight: Vec<HighlightItem>,
}

#[derive(Debug, Deserialize)]
struct TreeSitterSettings {
    treesitter: std::collections::HashMap<String, LanguageConfig>,
    filetypes: std::collections::HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
struct SymbolDefinition {
    name: String,
    uri: Url,
    range: Range,
    kind: SymbolKind,
}

#[derive(Debug, Clone)]
struct SymbolReference {
    name: String,
    uri: Url,
    range: Range,
}

pub struct TreeSitterLs {
    client: Client,
    languages: std::sync::Mutex<std::collections::HashMap<String, Language>>,
    queries: std::sync::Mutex<std::collections::HashMap<String, Query>>,
    language_configs: std::sync::Mutex<std::collections::HashMap<String, LanguageConfig>>,
    filetype_map: std::sync::Mutex<std::collections::HashMap<String, String>>,
    libraries: std::sync::Mutex<std::collections::HashMap<String, Library>>,
    document_map: DashMap<Url, (String, Option<Tree>)>,
    symbol_definitions: DashMap<String, Vec<SymbolDefinition>>,
    symbol_references: DashMap<String, Vec<SymbolReference>>,
}

impl std::fmt::Debug for TreeSitterLs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeSitterLs")
            .field("client", &self.client)
            .field("languages", &"Mutex<HashMap<String, Language>>")
            .field("queries", &"Mutex<HashMap<String, Query>>")
            .field("language_configs", &"Mutex<HashMap<String, LanguageConfig>>")
            .field("filetype_map", &"Mutex<HashMap<String, String>>")
            .field("libraries", &"Mutex<HashMap<String, Library>>")
            .field("document_map", &self.document_map)
            .field("symbol_definitions", &self.symbol_definitions)
            .field("symbol_references", &self.symbol_references)
            .finish()
    }
}

impl TreeSitterLs {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            languages: std::sync::Mutex::new(std::collections::HashMap::new()),
            queries: std::sync::Mutex::new(std::collections::HashMap::new()),
            language_configs: std::sync::Mutex::new(std::collections::HashMap::new()),
            filetype_map: std::sync::Mutex::new(std::collections::HashMap::new()),
            libraries: std::sync::Mutex::new(std::collections::HashMap::new()),
            document_map: DashMap::new(),
            symbol_definitions: DashMap::new(),
            symbol_references: DashMap::new(),
        }
    }

    async fn parse_document(&self, uri: Url, text: String) {
        // Detect file extension
        let extension = uri.path().split('.').last().unwrap_or("");
        
        // Find the language for this file extension
        let filetype_map = self.filetype_map.lock().unwrap();
        let language_name = filetype_map.get(extension);
        
        if let Some(language_name) = language_name {
            let languages = self.languages.lock().unwrap();
            if let Some(language) = languages.get(language_name) {
                let mut parser = Parser::new();
                if parser.set_language(language).is_ok() {
                    if let Some(tree) = parser.parse(&text, None) {
                        // Index symbols for definition jumping
                        self.index_symbols(&uri, &text, &tree);
                        
                        self.document_map.insert(uri, (text, Some(tree)));
                        return;
                    }
                }
            }
        }
        
        self.document_map.insert(uri, (text, None));
    }

    fn load_language(&self, path: &str, func_name: &str, lang_name: &str) -> std::result::Result<Language, String> {
        unsafe {
            let lib = Library::new(path).map_err(|e| format!("Failed to load library {}: {}", path, e))?;
            let lang_func: Symbol<unsafe extern "C" fn() -> Language> =
                lib.get(func_name.as_bytes()).map_err(|e| format!("Failed to find function {}: {}", func_name, e))?;
            let language = lang_func();
            
            // Store the library to keep it loaded
            self.libraries.lock().unwrap().insert(lang_name.to_string(), lib);
            
            Ok(language)
        }
    }

    fn get_language_for_document(&self, uri: &Url) -> Option<String> {
        let extension = uri.path().split('.').last().unwrap_or("");
        let filetype_map = self.filetype_map.lock().unwrap();
        filetype_map.get(extension).cloned()
    }

    fn index_symbols(&self, uri: &Url, text: &str, tree: &Tree) {
        let mut cursor = tree.walk();
        self.visit_node(&mut cursor, uri, text, tree.root_node());
    }

    fn visit_node(&self, cursor: &mut tree_sitter::TreeCursor, uri: &Url, text: &str, node: Node) {
        // Check if this node represents a symbol definition
        match node.kind() {
            // Function definitions
            "function_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = text[name_node.byte_range()].to_string();
                    let range = self.node_to_range(&name_node, text);
                    
                    let definition = SymbolDefinition {
                        name: name.clone(),
                        uri: uri.clone(),
                        range,
                        kind: SymbolKind::FUNCTION,
                    };
                    
                    self.symbol_definitions.entry(name).or_insert_with(Vec::new).push(definition);
                }
            }
            
            // Struct definitions
            "struct_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = text[name_node.byte_range()].to_string();
                    let range = self.node_to_range(&name_node, text);
                    
                    let definition = SymbolDefinition {
                        name: name.clone(),
                        uri: uri.clone(),
                        range,
                        kind: SymbolKind::STRUCT,
                    };
                    
                    self.symbol_definitions.entry(name).or_insert_with(Vec::new).push(definition);
                }
            }
            
            // Enum definitions
            "enum_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = text[name_node.byte_range()].to_string();
                    let range = self.node_to_range(&name_node, text);
                    
                    let definition = SymbolDefinition {
                        name: name.clone(),
                        uri: uri.clone(),
                        range,
                        kind: SymbolKind::ENUM,
                    };
                    
                    self.symbol_definitions.entry(name).or_insert_with(Vec::new).push(definition);
                }
            }
            
            // Trait definitions
            "trait_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = text[name_node.byte_range()].to_string();
                    let range = self.node_to_range(&name_node, text);
                    
                    let definition = SymbolDefinition {
                        name: name.clone(),
                        uri: uri.clone(),
                        range,
                        kind: SymbolKind::INTERFACE,
                    };
                    
                    self.symbol_definitions.entry(name).or_insert_with(Vec::new).push(definition);
                }
            }
            
            // Module definitions
            "mod_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = text[name_node.byte_range()].to_string();
                    let range = self.node_to_range(&name_node, text);
                    
                    let definition = SymbolDefinition {
                        name: name.clone(),
                        uri: uri.clone(),
                        range,
                        kind: SymbolKind::MODULE,
                    };
                    
                    self.symbol_definitions.entry(name).or_insert_with(Vec::new).push(definition);
                }
            }
            
            // Constant definitions
            "const_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = text[name_node.byte_range()].to_string();
                    let range = self.node_to_range(&name_node, text);
                    
                    let definition = SymbolDefinition {
                        name: name.clone(),
                        uri: uri.clone(),
                        range,
                        kind: SymbolKind::CONSTANT,
                    };
                    
                    self.symbol_definitions.entry(name).or_insert_with(Vec::new).push(definition);
                }
            }
            
            // Static definitions
            "static_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = text[name_node.byte_range()].to_string();
                    let range = self.node_to_range(&name_node, text);
                    
                    let definition = SymbolDefinition {
                        name: name.clone(),
                        uri: uri.clone(),
                        range,
                        kind: SymbolKind::VARIABLE,
                    };
                    
                    self.symbol_definitions.entry(name).or_insert_with(Vec::new).push(definition);
                }
            }
            
            // Type alias definitions
            "type_item" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = text[name_node.byte_range()].to_string();
                    let range = self.node_to_range(&name_node, text);
                    
                    let definition = SymbolDefinition {
                        name: name.clone(),
                        uri: uri.clone(),
                        range,
                        kind: SymbolKind::TYPE_PARAMETER,
                    };
                    
                    self.symbol_definitions.entry(name).or_insert_with(Vec::new).push(definition);
                }
            }
            
            // Local variable definitions (let statements)
            "let_declaration" => {
                if let Some(pattern_node) = node.child_by_field_name("pattern") {
                    self.extract_identifiers_from_pattern(pattern_node, uri, text);
                }
            }
            
            // Also try let_statement
            "let_statement" => {
                if let Some(pattern_node) = node.child_by_field_name("pattern") {
                    self.extract_identifiers_from_pattern(pattern_node, uri, text);
                }
            }
            
            // Generic pattern for any node that might contain variable bindings
            _ => {
                // Check if this is any kind of let/variable binding node
                if node.kind().contains("let") || node.kind() == "variable_declaration" {
                    // Look for pattern field
                    if let Some(pattern_node) = node.child_by_field_name("pattern") {
                        self.extract_identifiers_from_pattern(pattern_node, uri, text);
                    } else {
                        // Look for identifier children directly
                        for i in 0..node.child_count() {
                            if let Some(child) = node.child(i) {
                                if child.kind() == "identifier" {
                                    let name = text[child.byte_range()].to_string();
                                    let range = self.node_to_range(&child, text);
                                    
                                    let definition = SymbolDefinition {
                                        name: name.clone(),
                                        uri: uri.clone(),
                                        range,
                                        kind: SymbolKind::VARIABLE,
                                    };
                                    
                                    self.symbol_definitions.entry(name).or_insert_with(Vec::new).push(definition);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Recursively visit children
        if cursor.goto_first_child() {
            loop {
                self.visit_node(cursor, uri, text, cursor.node());
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }

    fn extract_identifiers_from_pattern(&self, pattern_node: Node, uri: &Url, text: &str) {
        match pattern_node.kind() {
            "identifier" => {
                let name = text[pattern_node.byte_range()].to_string();
                let range = self.node_to_range(&pattern_node, text);
                
                let definition = SymbolDefinition {
                    name: name.clone(),
                    uri: uri.clone(),
                    range,
                    kind: SymbolKind::VARIABLE,
                };
                
                self.symbol_definitions.entry(name).or_insert_with(Vec::new).push(definition);
            }
            "mut_pattern" => {
                if let Some(child) = pattern_node.child(0) {
                    self.extract_identifiers_from_pattern(child, uri, text);
                }
            }
            "ref_pattern" => {
                if let Some(child) = pattern_node.child(0) {
                    self.extract_identifiers_from_pattern(child, uri, text);
                }
            }
            "tuple_pattern" => {
                for i in 0..pattern_node.child_count() {
                    if let Some(child) = pattern_node.child(i) {
                        if child.kind() != "," && child.kind() != "(" && child.kind() != ")" {
                            self.extract_identifiers_from_pattern(child, uri, text);
                        }
                    }
                }
            }
            _ => {
                // For other pattern types, recursively check children
                for i in 0..pattern_node.child_count() {
                    if let Some(child) = pattern_node.child(i) {
                        self.extract_identifiers_from_pattern(child, uri, text);
                    }
                }
            }
        }
    }

    fn node_to_range(&self, node: &Node, _text: &str) -> Range {
        let start_pos = node.start_position();
        let end_pos = node.end_position();
        
        Range {
            start: Position {
                line: start_pos.row as u32,
                character: start_pos.column as u32,
            },
            end: Position {
                line: end_pos.row as u32,
                character: end_pos.column as u32,
            },
        }
    }

    fn get_symbol_at_position(&self, text: &str, tree: &Tree, position: Position) -> Option<String> {
        let point = tree_sitter::Point {
            row: position.line as usize,
            column: position.character as usize,
        };

        let node = tree.root_node().descendant_for_point_range(point, point)?;
        
        // Look for an identifier node at this position
        if node.kind() == "identifier" {
            let symbol_name = text[node.byte_range()].to_string();
            return Some(symbol_name);
        }
        
        // If not directly on an identifier, check parent nodes
        let mut current_node = node;
        while let Some(parent) = current_node.parent() {
            for child in 0..parent.child_count() {
                if let Some(child_node) = parent.child(child) {
                    if child_node.kind() == "identifier" {
                        let child_start = child_node.start_position();
                        let child_end = child_node.end_position();
                        
                        // Check if position is within this identifier
                        if point.row >= child_start.row 
                            && point.row <= child_end.row
                            && (point.row > child_start.row || point.column >= child_start.column)
                            && (point.row < child_end.row || point.column <= child_end.column) {
                            let symbol_name = text[child_node.byte_range()].to_string();
                            return Some(symbol_name);
                        }
                    }
                }
            }
            current_node = parent;
        }

        None
    }

    fn load_query_from_highlight(&self, highlight_items: &[HighlightItem]) -> std::result::Result<String, String> {
        let mut combined_query = String::new();
        
        for item in highlight_items {
            match &item.source {
                HighlightSource::Path { path } => {
                    match std::fs::read_to_string(path) {
                        Ok(content) => {
                            combined_query.push_str(&content);
                            combined_query.push('\n');
                        }
                        Err(e) => {
                            return Err(format!("Failed to read query file {}: {}", path, e));
                        }
                    }
                }
                HighlightSource::Query { query } => {
                    combined_query.push_str(query);
                    combined_query.push('\n');
                }
            }
        }
        
        Ok(combined_query)
    }

    async fn load_settings(&self, settings: TreeSitterSettings) {
        // Store language configs
        {
            *self.language_configs.lock().unwrap() = settings.treesitter.clone();
        }

        // Build filetype map
        {
            let mut filetype_map = self.filetype_map.lock().unwrap();
            for (language, extensions) in settings.filetypes {
                for ext in extensions {
                    filetype_map.insert(ext, language.clone());
                }
            }
        }

        // Load languages and queries
        for (lang_name, config) in &settings.treesitter {
            // For now, assume the function name is tree_sitter_<language>
            let func_name = format!("tree_sitter_{}", lang_name);
            
            match self.load_language(&config.library, &func_name, lang_name) {
                Ok(language) => {
                    match self.load_query_from_highlight(&config.highlight) {
                        Ok(combined_query) => {
                            match Query::new(&language, &combined_query) {
                                Ok(query) => {
                                    {
                                        self.queries.lock().unwrap().insert(lang_name.clone(), query);
                                    }
                                    self.client
                                        .log_message(MessageType::INFO, format!("Query loaded for {}.", lang_name))
                                        .await;
                                }
                                Err(err) => {
                                    self.client
                                        .log_message(
                                            MessageType::ERROR,
                                            format!("Failed to parse query for {}: {}", lang_name, err),
                                        )
                                        .await;
                                }
                            }
                        }
                        Err(err) => {
                            self.client
                                .log_message(
                                    MessageType::ERROR,
                                    format!("Failed to load highlight queries for {}: {}", lang_name, err),
                                )
                                .await;
                        }
                    }
                    {
                        self.languages.lock().unwrap().insert(lang_name.clone(), language);
                    }
                    self.client
                        .log_message(MessageType::INFO, format!("Language {} loaded.", lang_name))
                        .await;
                }
                Err(err) => {
                    self.client
                        .log_message(MessageType::ERROR, format!("Failed to load language {}: {}", lang_name, err))
                        .await;
                }
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for TreeSitterLs {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Debug: Log initialization
        self.client
            .log_message(MessageType::INFO, "Received initialization request")
            .await;

        // Parse configuration from initialization_options
        if let Some(options) = params.initialization_options {
            if let Ok(settings) = serde_json::from_value::<TreeSitterSettings>(options) {
                self.client
                    .log_message(MessageType::INFO, "Parsed as TreeSitterSettings")
                    .await;
                self.load_settings(settings).await;
            } else {
                self.client
                    .log_message(MessageType::ERROR, "Failed to parse initialization options")
                    .await;
            }
        }

        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "treesitter-ls".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                        legend: SemanticTokensLegend {
                            token_types: LEGEND_TYPES.to_vec(),
                            token_modifiers: vec![],
                        },
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                        ..Default::default()
                    }),
                ),
                definition_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server is ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.parse_document(params.text_document.uri, params.text_document.text)
            .await;
        self.client
            .log_message(MessageType::INFO, "file opened!")
            .await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.parse_document(
            params.text_document.uri,
            params.content_changes.remove(0).text,
        )
        .await;
        self.client
            .log_message(MessageType::INFO, "file changed!")
            .await;
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        if let Ok(settings) = serde_json::from_value::<TreeSitterSettings>(params.settings) {
            self.load_settings(settings).await;
            self.client
                .log_message(MessageType::INFO, "Configuration updated!")
                .await;
        }
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        // Find the language for this document
        let language_name = self.get_language_for_document(&uri).unwrap_or("default".to_string());
        
        // Get the query for this language
        let queries = self.queries.lock().unwrap();
        let query = if let Some(query) = queries.get(&language_name) {
            query
        } else {
            return Ok(None);
        };

        let doc = if let Some(doc) = self.document_map.get(&uri) {
            doc
        } else {
            return Ok(None);
        };

        let (text, tree) = &*doc;
        let tree = if let Some(tree) = tree {
            tree
        } else {
            return Ok(None);
        };

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());

        let mut tokens = vec![];
        while let Some(m) = matches.next() {
            for c in m.captures {
                let node = c.node;
                let start_pos = node.start_position();
                let end_pos = node.end_position();
                if start_pos.row == end_pos.row {
                    tokens.push((
                        start_pos.row,
                        start_pos.column,
                        end_pos.column - start_pos.column,
                        c.index,
                    ));
                }
            }
        }
        tokens.sort();

        let mut last_line = 0;
        let mut last_start = 0;
        let mut data = Vec::new();

        for (line, start, length, capture_index) in tokens {
            let delta_line = line - last_line;
            let delta_start = if delta_line == 0 {
                start - last_start
            } else {
                start
            };

            let token_type_name = &query.capture_names()[capture_index as usize];
            let token_type = LEGEND_TYPES
                .iter()
                .position(|t| t.as_str() == *token_type_name)
                .unwrap_or(0);

            data.push(SemanticToken {
                delta_line: delta_line as u32,
                delta_start: delta_start as u32,
                length: length as u32,
                token_type: token_type as u32,
                token_modifiers_bitset: 0,
            });

            last_line = line;
            last_start = start;
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!("Definition request for position {}:{}", position.line, position.character),
            )
            .await;

        // Get the word at the cursor position
        if let Some(doc_entry) = self.document_map.get(&uri) {
            let (text, tree) = doc_entry.value();
            if let Some(tree) = tree {
                // Find the node at the cursor position
                if let Some(symbol_name) = self.get_symbol_at_position(text, tree, position) {
                    self.client
                        .log_message(
                            MessageType::INFO,
                            format!("Found symbol '{}' at position", symbol_name),
                        )
                        .await;
                    // Look up the symbol definition
                    if let Some(definitions) = self.symbol_definitions.get(&symbol_name) {
                        let locations: Vec<Location> = definitions
                            .iter()
                            .map(|def| Location {
                                uri: def.uri.clone(),
                                range: def.range,
                            })
                            .collect();

                        if !locations.is_empty() {
                            self.client
                                .log_message(
                                    MessageType::INFO,
                                    format!("Found {} definition(s) for '{}'", locations.len(), symbol_name),
                                )
                                .await;
                            
                            return Ok(Some(GotoDefinitionResponse::Array(locations)));
                        }
                    }
                    
                    self.client
                        .log_message(
                            MessageType::INFO,
                            format!("No definition found for '{}'", symbol_name),
                        )
                        .await;
                } else {
                    self.client
                        .log_message(
                            MessageType::INFO,
                            format!("No symbol found at position {}:{}", position.line, position.character),
                        )
                        .await;
                }
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod simple_tests;