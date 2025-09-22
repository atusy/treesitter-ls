; Fenced code blocks with language
(fenced_code_block
  (info_string
    (language) @injection.language)
  (code_fence_content) @injection.content)

; Fenced code blocks without language
(fenced_code_block
  (code_fence_content) @injection.content)

; YAML frontmatter (between --- markers)
((minus_metadata) @injection.content
  (#set! injection.language "yaml")
  (#offset! @injection.content 1 0 -1 0)
  (#set! injection.include-children))

; TOML frontmatter (between +++ markers)
((plus_metadata) @injection.content
  (#set! injection.language "toml")
  (#offset! @injection.content 1 0 -1 0)
  (#set! injection.include-children))

; HTML blocks
(html_block) @injection.content
(#set! injection.language "html")