use tree_sitter::{Parser, Query, QueryCursor};

fn main() {
    let code = r#"local x = 1

print(x)
--    ^-- testing definition jump here"#;

    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_lua::language()).unwrap();
    
    let tree = parser.parse(code, None).unwrap();
    let root = tree.root_node();
    
    println!("Parse tree:");
    print_tree(root, code, 0);
    
    // Test the queries
    let query = Query::new(
        &tree_sitter_lua::language(),
        include_str!("../queries/lua/locals.scm")
    ).unwrap();
    
    let mut cursor = QueryCursor::new();
    let matches = cursor.matches(&query, root, code.as_bytes());
    
    println!("\n\nQuery matches:");
    for m in matches {
        for capture in m.captures {
            let node = capture.node;
            let text = &code[node.byte_range()];
            let capture_name = query.capture_names()[capture.index as usize];
            println!("  {} @ {}:{} = '{}'", 
                capture_name, 
                node.start_position().row + 1,
                node.start_position().column,
                text
            );
        }
    }
}

fn print_tree(node: tree_sitter::Node, source: &str, indent: usize) {
    let indent_str = " ".repeat(indent);
    let text = node.utf8_text(source.as_bytes()).unwrap();
    
    if node.child_count() == 0 {
        println!("{}{} [{}:{}] '{}'", 
            indent_str, 
            node.kind(),
            node.start_position().row + 1,
            node.start_position().column,
            text
        );
    } else {
        println!("{}{} [{}:{}]", 
            indent_str, 
            node.kind(),
            node.start_position().row + 1,
            node.start_position().column
        );
        
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                print_tree(child, source, indent + 2);
            }
        }
    }
}