" do make deps/vim first

set nocompatible
filetype plugin indent on
syntax off


" Setup tree-sitter-ls
let s:cwd = getcwd()
set rtp+=deps/vim/prabirshrestha/vim-lsp
augroup lsp_setup
  autocmd!
  autocmd User lsp_setup call lsp#register_server({
        \ 'name': 'tree-sitter-ls',
        \ 'cmd': {server_info->[s:cwd . '/target/debug/tree-sitter-ls']},
        \ 'allowlist': ['*'],
        \ })
augroup END

" Enable logging
" let g:lsp_log_verbose = 1
" let g:lsp_log_file = expand('__ignored/vimlsp.log')

" Enable semantic highlight
let g:lsp_semantic_enabled = 1
let g:lsp_semantic_delay = 500
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
