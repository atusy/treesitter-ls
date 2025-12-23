set nocompatible
let s:cwd = getcwd()
set rtp+=deps/vim/vim-lsp

" Enable filetype detection
filetype plugin indent on
syntax off

" Register treesitter-ls server
augroup lsp_setup
  autocmd!
  autocmd User lsp_setup call lsp#register_server({
        \ 'name': 'treesitter-ls',
        \ 'cmd': {server_info->[s:cwd . '/target/debug/treesitter-ls']},
        \ 'allowlist': ['*'],
        \ })
augroup END

" Semantic highlight
let g:lsp_semantic_enabled = 1
let g:lsp_semantic_delay = 100
highlight LspSemanticComment guifg=#6a737d ctermfg=245
highlight LspSemanticKeyword guifg=#d73a49 ctermfg=167 gui=bold cterm=bold
highlight LspSemanticString guifg=#032f62 ctermfg=24
highlight LspSemanticNumber guifg=#005cc5 ctermfg=26
highlight LspSemanticRegexp guifg=#032f62 ctermfg=24
highlight LspSemanticOperator guifg=#d73a49 ctermfg=167
highlight LspSemanticNamespace guifg=#6f42c1 ctermfg=97
highlight LspSemanticType guifg=#6f42c1 ctermfg=97
highlight LspSemanticStruct guifg=#6f42c1 ctermfg=97
highlight LspSemanticClass guifg=#6f42c1 ctermfg=97
highlight LspSemanticInterface guifg=#6f42c1 ctermfg=97
highlight LspSemanticEnum guifg=#6f42c1 ctermfg=97
highlight LspSemanticEnumMember guifg=#005cc5 ctermfg=26
highlight LspSemanticTypeParameter guifg=#6f42c1 ctermfg=97
highlight LspSemanticFunction guifg=#6f42c1 ctermfg=97 gui=bold cterm=bold
highlight LspSemanticMethod guifg=#6f42c1 ctermfg=97
highlight LspSemanticMacro guifg=#005cc5 ctermfg=26
highlight LspSemanticVariable guifg=#e36209 ctermfg=166
highlight LspSemanticParameter guifg=#e36209 ctermfg=166
highlight LspSemanticProperty guifg=#005cc5 ctermfg=26
highlight LspSemanticEvent guifg=#e36209 ctermfg=166
highlight LspSemanticModifier guifg=#d73a49 ctermfg=167
highlight LspSemanticDecorator guifg=#6f42c1 ctermfg=97
