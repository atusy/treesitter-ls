#!/usr/bin/env python3
import subprocess
import json
import time
import os

# Start the LSP server
server = subprocess.Popen(
    ["./target/release/treesitter-ls"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=None,  # Let stderr go to console
    text=False
)

def send_request(request):
    content = json.dumps(request).encode('utf-8')
    header = f"Content-Length: {len(content)}\r\n\r\n".encode('utf-8')
    server.stdin.write(header + content)
    server.stdin.flush()

def read_response():
    responses = []
    while True:
        headers = b""
        while True:
            line = server.stdout.readline()
            if not line:
                return responses
            headers += line
            if line == b"\r\n":
                break
        
        # Parse content length
        content_length = 0
        for line in headers.decode('utf-8').split('\r\n'):
            if line.startswith('Content-Length: '):
                content_length = int(line.split(': ')[1])
                break
        
        if content_length == 0:
            continue
            
        # Read content
        content = server.stdout.read(content_length)
        response = json.loads(content.decode('utf-8'))
        responses.append(response)
        
        # If this is a response to our request (has an id), return it
        if 'id' in response:
            return response

# Initialize
send_request({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
        "processId": os.getpid(),
        "rootUri": f"file://{os.getcwd()}",
        "capabilities": {}
    }
})

response = read_response()
print("Initialize response:", response)

# Open the test file
send_request({
    "jsonrpc": "2.0",
    "method": "textDocument/didOpen",
    "params": {
        "textDocument": {
            "uri": f"file://{os.getcwd()}/test_def_jump.lua",
            "languageId": "lua",
            "version": 1,
            "text": "local x = 1\n\nprint(x)\n--    ^-- testing definition jump here"
        }
    }
})

time.sleep(0.1)  # Give it time to process

# Request definition at position of 'x' in print(x)
send_request({
    "jsonrpc": "2.0",
    "id": 2,
    "method": "textDocument/definition",
    "params": {
        "textDocument": {
            "uri": f"file://{os.getcwd()}/test_def_jump.lua"
        },
        "position": {
            "line": 2,
            "character": 6
        }
    }
})

response = read_response()
print("\nDefinition response:", json.dumps(response, indent=2))

# Shutdown
send_request({
    "jsonrpc": "2.0",
    "id": 3,
    "method": "shutdown"
})

server.terminate()