use tree_sitter::{Parser, Query, QueryCursor};

fn main() {
    // Initialize parser
    let mut parser = Parser::new();
    
    // Load the Rust language (assuming it's built)
    let lib_path = "./tree-sitter-rust/target/release/libtree_sitter_rust.so";
    let lib = unsafe { libloading::Library::new(lib_path).expect("Failed to load library") };
    let language_fn: libloading::Symbol<unsafe extern "C" fn() -> tree_sitter::Language> = 
        unsafe { lib.get(b"tree_sitter_rust").expect("Failed to get function") };
    let language = unsafe { language_fn() };
    
    parser.set_language(&language).expect("Failed to set language");
    
    // Read the test file
    let source_code = std::fs::read_to_string("test_rust_analysis.rs")
        .expect("Failed to read test file");
    
    // Parse the code
    let tree = parser.parse(&source_code, None).expect("Failed to parse");
    let root_node = tree.root_node();
    
    // Helper function to print node info
    fn print_node_info(node: tree_sitter::Node, source: &str, indent: usize) {
        let indent_str = " ".repeat(indent);
        let node_text = node.utf8_text(source.as_bytes()).unwrap_or("<invalid>");
        println!("{}Node: {} [{}]", indent_str, node.kind(), 
                 if node_text.len() > 50 { 
                     format!("{}...", &node_text[..50]) 
                 } else { 
                     node_text.to_string() 
                 });
        
        // Special handling for interesting nodes
        if node.kind() == "call_expression" || node.kind() == "identifier" {
            println!("{}  Parent: {}", indent_str, node.parent().map(|p| p.kind()).unwrap_or("<none>"));
            println!("{}  Range: {:?}", indent_str, node.range());
        }
    }
    
    // Find all instances of "stdin" in the AST
    println!("=== Finding all 'stdin' nodes ===\n");
    
    let mut cursor = root_node.walk();
    let mut visit_node = |node: tree_sitter::Node| {
        if let Ok(text) = node.utf8_text(source_code.as_bytes()) {
            if text == "stdin" {
                println!("\nFound 'stdin' at line {}, column {}:", 
                         node.start_position().row + 1, 
                         node.start_position().column);
                
                // Print the node and its ancestors
                let mut current = Some(node);
                let mut depth = 0;
                while let Some(n) = current {
                    print_node_info(n, &source_code, depth * 2);
                    current = n.parent();
                    depth += 1;
                    if depth > 5 { break; } // Limit depth
                }
            }
        }
        
        // Also look for call_expression nodes
        if node.kind() == "call_expression" {
            println!("\n=== Call Expression Found ===");
            print_node_info(node, &source_code, 0);
            
            // Print children
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    print_node_info(child, &source_code, 2);
                }
            }
        }
    };
    
    // Walk the tree
    fn walk_tree(cursor: &mut tree_sitter::TreeCursor, visit: &mut dyn FnMut(tree_sitter::Node)) {
        visit(cursor.node());
        
        if cursor.goto_first_child() {
            loop {
                walk_tree(cursor, visit);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }
    
    walk_tree(&mut cursor, &mut visit_node);
    
    println!("\n=== Full AST Structure ===");
    print_tree(root_node, &source_code, 0);
}

fn print_tree(node: tree_sitter::Node, source: &str, indent: usize) {
    let indent_str = " ".repeat(indent);
    let node_text = node.utf8_text(source.as_bytes()).unwrap_or("<invalid>");
    
    // Truncate long text
    let display_text = if node_text.contains('\n') || node_text.len() > 30 {
        format!("'{}'...", node_text.lines().next().unwrap_or("").chars().take(30).collect::<String>())
    } else {
        format!("'{}'", node_text)
    };
    
    println!("{}{} {}", indent_str, node.kind(), display_text);
    
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            print_tree(child, source, indent + 2);
        }
    }
}