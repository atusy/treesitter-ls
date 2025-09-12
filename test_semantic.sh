#!/bin/bash
# Simple test to trigger semantic tokens request
(
echo 'Content-Length: 2104

{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"clientInfo":{"name":"test"},"rootUri":"file:///tmp","capabilities":{"textDocument":{"semanticTokens":{"tokenTypes":["namespace","type","class","enum","interface","struct","typeParameter","parameter","variable","property","enumMember","event","function","method","macro","keyword","modifier","comment","string","number","regexp","operator","decorator"],"tokenModifiers":["declaration","definition","readonly","static","deprecated","abstract","async","modification","documentation","defaultLibrary"],"formats":["relative"],"requests":{"range":false,"full":{"delta":true}},"multilineTokenSupport":false,"overlappingTokenSupport":false,"serverCancelSupport":false,"augmentsSyntaxTokens":true}}},"initializationOptions":{"searchPaths":["/home/atusy/.local/share/nvim/treesitter"]}}}' 

sleep 0.5

echo 'Content-Length: 238

{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/test.rs","languageId":"rust","version":1,"text":"fn main() {\n    let x = 42;\n    println!(\"Hello, world!\");\n}"}}}'

sleep 0.5

echo 'Content-Length: 103

{"jsonrpc":"2.0","id":2,"method":"textDocument/semanticTokens/full","params":{"textDocument":{"uri":"file:///tmp/test.rs"}}}'

sleep 1
) | /home/atusy/ghq/github.com/atusy/treesitter-ls/target/release/treesitter-ls 2>&1 | grep -E "(Processing|Fallback|tokens|has_tree|has_root_layer|Available queries)"
