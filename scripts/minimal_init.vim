" do make deps/vim first

set nocompatible
filetype plugin indent on
syntax off


" Setup kakehashi
let s:cwd = getcwd()
set rtp+=deps/vim/prabirshrestha/vim-lsp
augroup lsp_setup
  autocmd!
  autocmd User lsp_setup call lsp#register_server({
        \ 'name': 'kakehashi',
        \ 'cmd': {server_info->[s:cwd . '/target/debug/kakehashi']},
        \ 'allowlist': ['*'],
        \ })
augroup END

" Enable logging
" let g:lsp_log_verbose = 1
" let g:lsp_log_file = expand('__ignored/vimlsp.log')

" Enable semantic highlight
let g:lsp_semantic_enabled = 1
let g:lsp_semantic_delay = 50
highlight LspSemanticComment guifg=#8b949e ctermfg=245
highlight LspSemanticKeyword guifg=#ff7b72 ctermfg=167 gui=bold cterm=bold
highlight LspSemanticString guifg=#a5d6ff ctermfg=117
highlight LspSemanticNumber guifg=#79c0ff ctermfg=111
highlight LspSemanticRegexp guifg=#a5d6ff ctermfg=117
highlight LspSemanticOperator guifg=#ff7b72 ctermfg=167
highlight LspSemanticNamespace guifg=#d2a8ff ctermfg=183
highlight LspSemanticType guifg=#d2a8ff ctermfg=183
highlight LspSemanticStruct guifg=#d2a8ff ctermfg=183
highlight LspSemanticClass guifg=#d2a8ff ctermfg=183
highlight LspSemanticInterface guifg=#d2a8ff ctermfg=183
highlight LspSemanticEnum guifg=#d2a8ff ctermfg=183
highlight LspSemanticEnumMember guifg=#79c0ff ctermfg=111
highlight LspSemanticTypeParameter guifg=#d2a8ff ctermfg=183
highlight LspSemanticFunction guifg=#d2a8ff ctermfg=183 gui=bold cterm=bold
highlight LspSemanticMethod guifg=#d2a8ff ctermfg=183
highlight LspSemanticMacro guifg=#79c0ff ctermfg=111
highlight LspSemanticVariable guifg=#ffa657 ctermfg=215
highlight LspSemanticParameter guifg=#ffa657 ctermfg=215
highlight LspSemanticProperty guifg=#79c0ff ctermfg=111
highlight LspSemanticEvent guifg=#ffa657 ctermfg=215
highlight LspSemanticModifier guifg=#ff7b72 ctermfg=167
highlight LspSemanticDecorator guifg=#d2a8ff ctermfg=183


if has('nvim')
  augroup my_disable_treesitter
    autocmd!
    autocmd FileType * lua vim.treesitter.stop(0)
  augroup END
endif
