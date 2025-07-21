; Imports
(extern_crate_declaration
  name: (identifier) @local.definition.import)

(use_declaration
  argument: (scoped_identifier
    name: (identifier) @local.definition.import))

(use_as_clause
  alias: (identifier) @local.definition.import)

(use_list
  (identifier) @local.definition.import) ; use std::process::{Child, Command, Stdio};

; Functions
(function_item
  name: (identifier) @local.definition.function)

(function_item
  name: (identifier) @local.definition.method
  parameters: (parameters
    (self_parameter)))

; Variables
(parameter
  pattern: (identifier) @local.definition.var)

(let_declaration
  pattern: (identifier) @local.definition.var)

(const_item
  name: (identifier) @local.definition.var)

(tuple_pattern
  (identifier) @local.definition.var)

(let_condition
  pattern: (_
    (identifier) @local.definition.var))

(tuple_struct_pattern
  (identifier) @local.definition.var)

(closure_parameters
  (identifier) @local.definition.var)

(self_parameter
  (self) @local.definition.var)

(for_expression
  pattern: (identifier) @local.definition.var)

; Types
(struct_item
  name: (type_identifier) @local.definition.type)

(enum_item
  name: (type_identifier) @local.definition.type)

; Fields
(field_declaration
  name: (field_identifier) @local.definition.field)

(enum_variant
  name: (identifier) @local.definition.field)

; References - Context-aware patterns for better resolution

; Function calls
(call_expression
  function: (identifier) @local.reference.function_call)

(call_expression
  function: (field_expression
    field: (field_identifier) @local.reference.method_call))

; Variable references (not in function calls)
(identifier) @local.reference.variable
  ; Exclude function calls, type annotations, etc.

; Type annotations and type references
(type_identifier) @local.reference.type

; Field access
(field_expression
  field: (field_identifier) @local.reference.field)

; Field references in patterns and assignments
(field_identifier) @local.reference.field

; Generic references (fallback)
(identifier) @local.reference

; Macros
(macro_definition
  name: (identifier) @local.definition.macro)

; Module
(mod_item
  name: (identifier) @local.definition.namespace)

; Scopes
[
  (block)
  (function_item)
  (closure_expression)
  (while_expression)
  (for_expression)
  (loop_expression)
  (if_expression)
  (match_expression)
  (match_arm)
  (struct_item)
  (enum_item)
  (impl_item)
] @local.scope
