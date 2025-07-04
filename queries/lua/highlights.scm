; Lua highlights.scm for LSP semantic tokens
; Converted from Neovim treesitter builtin highlights to semantic token types

; ============================================================================
; KEYWORDS
; ============================================================================

"return" @keyword

[
  "goto"
  "in"
  "local"
] @keyword

(break_statement) @keyword

(do_statement
  [
    "do"
    "end"
  ] @keyword)

(while_statement
  [
    "while"
    "do"
    "end"
  ] @keyword)

(repeat_statement
  [
    "repeat"
    "until"
  ] @keyword)

(if_statement
  [
    "if"
    "elseif"
    "else"
    "then"
    "end"
  ] @keyword)

(elseif_statement
  [
    "elseif"
    "then"
    "end"
  ] @keyword)

(else_statement
  [
    "else"
    "end"
  ] @keyword)

(for_statement
  [
    "for"
    "do"
    "end"
  ] @keyword)

(function_declaration
  [
    "function"
    "end"
  ] @keyword)

(function_definition
  [
    "function"
    "end"
  ] @keyword)

; ============================================================================
; OPERATORS
; ============================================================================

[
  "and"
  "not"
  "or"
] @operator

[
  "+"
  "-"
  "*"
  "/"
  "%"
  "^"
  "#"
  "=="
  "~="
  "<="
  ">="
  "<"
  ">"
  "="
  "&"
  "~"
  "|"
  "<<"
  ">>"
  "//"
  ".."
] @operator

; ============================================================================
; PUNCTUATION (mapped to operator for semantic tokens)
; ============================================================================

[
  ";"
  ":"
  "::"
  ","
  "."
] @operator

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @operator

; ============================================================================
; VARIABLES & IDENTIFIERS
; ============================================================================

; Base identifier - will be overridden by more specific patterns
(identifier) @variable

; Built-in constants
((identifier) @variable
  (#eq? @variable "_VERSION"))

((identifier) @variable
  (#eq? @variable "self"))

; Built-in modules
((identifier) @namespace
  (#any-of? @namespace "_G" "debug" "io" "jit" "math" "os" "package" "string" "table" "utf8"))

((identifier) @namespace
  (#eq? @namespace "coroutine"))

; All-caps identifiers are constants
((identifier) @variable
  (#lua-match? @variable "^[A-Z][A-Z_0-9]*$"))

; ============================================================================
; CONSTANTS & LITERALS
; ============================================================================

(nil) @variable

[
  (false)
  (true)
] @variable

(number) @number

(string) @string

(escape_sequence) @string

; ============================================================================
; FUNCTIONS
; ============================================================================

; Function parameters
(parameters
  (identifier) @parameter)

(vararg_expression) @parameter

; Function declarations
(function_declaration
  name: [
    (identifier) @function
    (dot_index_expression
      field: (identifier) @function)
  ])

(function_declaration
  name: (method_index_expression
    method: (identifier) @method))

; Function assignments
(assignment_statement
  (variable_list
    .
    name: [
      (identifier) @function
      (dot_index_expression
        field: (identifier) @function)
    ])
  (expression_list
    .
    value: (function_definition)))

; Function in table constructors
(table_constructor
  (field
    name: (identifier) @function
    value: (function_definition)))

; Function calls
(function_call
  name: [
    (identifier) @function
    (dot_index_expression
      field: (identifier) @function)
    (method_index_expression
      method: (identifier) @method)
  ])

; Built-in functions
(function_call
  (identifier) @function
  (#any-of? @function
    ; built-in functions in Lua 5.1
    "assert" "collectgarbage" "dofile" "error" "getfenv" "getmetatable" "ipairs" "load" "loadfile"
    "loadstring" "module" "next" "pairs" "pcall" "print" "rawequal" "rawget" "rawlen" "rawset"
    "require" "select" "setfenv" "setmetatable" "tonumber" "tostring" "type" "unpack" "xpcall"
    "__add" "__band" "__bnot" "__bor" "__bxor" "__call" "__concat" "__div" "__eq" "__gc" "__idiv"
    "__index" "__le" "__len" "__lt" "__metatable" "__mod" "__mul" "__name" "__newindex" "__pairs"
    "__pow" "__shl" "__shr" "__sub" "__tostring" "__unm"))

; ============================================================================
; PROPERTIES & FIELDS
; ============================================================================

; Table fields
(field
  name: (identifier) @property)

(dot_index_expression
  field: (identifier) @property)

; Table constructor
(table_constructor
  [
    "{"
    "}"
  ] @operator)

; ============================================================================
; ATTRIBUTES & LABELS
; ============================================================================

; Variable attributes
(variable_list
  (attribute
    "<" @operator
    (identifier) @decorator
    ">" @operator))

; Labels
(label_statement
  (identifier) @variable)

(goto_statement
  (identifier) @variable)

; ============================================================================
; COMMENTS
; ============================================================================

(comment) @comment

; Documentation comments
((comment) @comment
  (#lua-match? @comment "^[-][-][-]"))

((comment) @comment
  (#lua-match? @comment "^[-][-](%s?)@"))

; Hash bang
(hash_bang_line) @comment

; ============================================================================
; REGULAR EXPRESSIONS
; ============================================================================

; String patterns in string methods
(function_call
  (dot_index_expression
    field: (identifier) @_method
    (#any-of? @_method "find" "match" "gmatch" "gsub"))
  arguments: (arguments
    .
    (_)
    .
    (string
      content: (string_content) @regexp)))

; String patterns in method calls
(function_call
  (method_index_expression
    method: (identifier) @_method
    (#any-of? @_method "find" "match" "gmatch" "gsub"))
  arguments: (arguments
    .
    (string
      content: (string_content) @regexp)))