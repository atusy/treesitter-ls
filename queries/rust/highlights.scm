; Sophisticated Rust highlights.scm for LSP semantic tokens
; Adapted from tree-sitter-rust highlights2.scm with intelligent pattern matching

; ============================================================================
; IDENTIFIER CONVENTIONS & SMART DETECTION
; ============================================================================

; Base identifier - will be overridden by more specific patterns
(identifier) @variable

; Uppercase identifiers are likely types
((identifier) @type
  (#match? @type "^[A-Z]"))

; All-caps identifiers are constants
((identifier) @variable
  (#match? @variable "^[A-Z][A-Z\\d_]*$"))

; ============================================================================
; TYPES
; ============================================================================

(type_identifier) @type
(primitive_type) @type

; Module names in scoped identifiers
(scoped_identifier
  path: (identifier) @namespace)

(scoped_identifier
  (scoped_identifier
    name: (identifier) @namespace))

(scoped_type_identifier
  path: (identifier) @namespace)

(scoped_type_identifier
  (scoped_identifier
    name: (identifier) @namespace))

; Smart type detection in scoped contexts
((scoped_identifier
  path: (identifier) @type)
  (#match? @type "^[A-Z]"))

((scoped_identifier
  name: (identifier) @type)
  (#match? @type "^[A-Z]"))

((scoped_type_identifier
  path: (identifier) @type)
  (#match? @type "^[A-Z]"))

; Constants in scoped contexts
((scoped_identifier
  name: (identifier) @variable)
  (#match? @variable "^[A-Z][A-Z\\d_]*$"))

; ============================================================================
; MODULES & NAMESPACES
; ============================================================================

(mod_item
  name: (identifier) @namespace)

[
  (crate)
  (super)
] @namespace

(scoped_use_list
  path: (identifier) @namespace)

(scoped_use_list
  path: (scoped_identifier
    (identifier) @namespace))

(use_list
  (scoped_identifier
    (identifier) @namespace))

; Types in use statements
(use_list
  (identifier) @type
  (#match? @type "^[A-Z]"))

(use_as_clause
  alias: (identifier) @type
  (#match? @type "^[A-Z]"))

; ============================================================================
; CONSTANTS & ENUM VARIANTS
; ============================================================================

(const_item
  name: (identifier) @variable)

(enum_variant
  name: (identifier) @enumMember)

; Uppercase field identifiers are likely enum variants/constants
((field_identifier) @enumMember
  (#match? @enumMember "^[A-Z]"))

; Built-in constants
((identifier) @variable
  (#any-of? @variable "Some" "None" "Ok" "Err"))

; Constants in match patterns
((match_arm
  pattern: (match_pattern
    (identifier) @variable))
  (#match? @variable "^[A-Z]"))

((match_arm
  pattern: (match_pattern
    (scoped_identifier
      name: (identifier) @variable)))
  (#match? @variable "^[A-Z]"))

; ============================================================================
; FUNCTIONS
; ============================================================================

; Function definitions
(function_item
  name: (identifier) @function)

(function_signature_item
  name: (identifier) @function)

; Function calls - direct
(call_expression
  function: (identifier) @function)

; Function calls - scoped
(call_expression
  function: (scoped_identifier
    name: (identifier) @function))

; Method calls
(call_expression
  function: (field_expression
    field: (field_identifier) @method))

; Generic function calls
(generic_function
  function: (identifier) @function)

(generic_function
  function: (scoped_identifier
    name: (identifier) @function))

(generic_function
  function: (field_expression
    field: (field_identifier) @method))

; Enum constructors in function calls
(call_expression
  function: (scoped_identifier
    "::"
    name: (identifier) @enumMember)
  (#match? @enumMember "^[A-Z]"))

; ============================================================================
; MACROS
; ============================================================================

; Macro invocations
(macro_invocation
  macro: (identifier) @macro)

(macro_invocation
  macro: (scoped_identifier
    name: (identifier) @macro))

; Macro definitions
(macro_definition
  "macro_rules!" @macro)

; Metavariables in macros
"$" @macro
(metavariable) @macro

; Attribute macros
(attribute_item
  (attribute
    (identifier) @macro))

(inner_attribute_item
  (attribute
    (identifier) @macro))

(attribute
  (scoped_identifier
    name: (identifier) @macro))

; Special macro highlighting
((macro_invocation
  macro: (identifier) @macro)
  (#any-of? @macro "println" "print" "eprintln" "eprint" "panic" "assert" "debug_assert" "dbg"))

; ============================================================================
; VARIABLES & PARAMETERS
; ============================================================================

; Function parameters
(parameter
  pattern: (identifier) @parameter)

(parameter
  pattern: (mut_pattern
    (identifier) @parameter))

(parameter
  pattern: (ref_pattern
    (identifier) @parameter))

(parameter
  pattern: (ref_pattern
    (mut_pattern
      (identifier) @parameter)))

; Closure parameters
(closure_parameters
  (identifier) @parameter)

; ============================================================================
; PROPERTIES & FIELDS
; ============================================================================

(field_identifier) @property
(shorthand_field_identifier) @property

(shorthand_field_initializer
  (identifier) @property)

; ============================================================================
; KEYWORDS
; ============================================================================

; Import keywords
[
  "use"
  "mod"
] @keyword

(use_as_clause
  "as" @keyword)

; Declaration keywords
[
  "let"
  "const"
  "static"
  "fn"
] @keyword

; Type keywords  
[
  "struct"
  "enum"
  "union"
  "trait"
  "type"
  "impl"
] @keyword

; Control flow
[
  "if"
  "else"
  "match"
  "loop"
  "while"
  "for"
  "in"
  "break"
  "continue"
] @keyword

; Function keywords
[
  "return"
  "yield"
] @keyword

; Async keywords
[
  "async"
  "await"
  "gen"
] @keyword

; Other keywords
[
  "move"
  "ref"
  "pub"
  "extern"
  "unsafe"
  "dyn"
  "where"
  "default"
  "try"
] @keyword

(mutable_specifier) @keyword

; Special identifiers
(self) @keyword

; ============================================================================
; LITERALS
; ============================================================================

(boolean_literal) @variable
(integer_literal) @number
(float_literal) @number

[
  (string_literal)
  (raw_string_literal)
  (char_literal)
] @string

(escape_sequence) @string

; ============================================================================
; OPERATORS
; ============================================================================

[
  "!"
  "!="
  "%"
  "%="
  "&"
  "&&"
  "&="
  "*"
  "*="
  "+"
  "+="
  "-"
  "-="
  ".."
  "..="
  "..."
  "/"
  "/="
  "<"
  "<<"
  "<<="
  "<="
  "="
  "=="
  ">"
  ">="
  ">>"
  ">>="
  "?"
  "@"
  "^"
  "^="
  "|"
  "|="
  "||"
] @operator

; Type cast operator
(type_cast_expression
  "as" @operator)

(qualified_type
  "as" @operator)

; ============================================================================
; COMMENTS
; ============================================================================

[
  (line_comment)
  (block_comment)
] @comment

; ============================================================================
; LIFETIMES & LABELS
; ============================================================================

(lifetime
  "'" @operator)

(lifetime
  (identifier) @operator)

(label
  "'" @operator)

(label
  (identifier) @operator)

; ============================================================================
; ATTRIBUTES & DECORATORS
; ============================================================================

(attribute_item) @decorator
(inner_attribute_item) @decorator

; ============================================================================
; SPECIAL SYMBOLS
; ============================================================================

"_" @variable

; Wildcard in use statements
(use_wildcard
  "*" @operator)

; Range patterns
(remaining_field_pattern
  ".." @operator)

(range_pattern
  [
    ".."
    "..="
    "..."
  ] @operator)

; ============================================================================
; CONTEXTUAL OVERRIDES
; ============================================================================

; Self in different contexts
(use_list
  (self) @namespace)

(scoped_use_list
  (self) @namespace)

(scoped_identifier
  (self) @namespace)

(visibility_modifier
  [
    (crate)
    (super)
    (self)
  ] @namespace)