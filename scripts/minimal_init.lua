local cwd = vim.uv.cwd()
vim.lsp.config.treesitter_ls = {
	cmd = { cwd .. "/target/debug/treesitter-ls" },
	init_options = { searchPaths = { cwd .. "/deps/treesitter" } },
}
vim.lsp.enable("treesitter_ls")
vim.lsp.log.set_level(vim.lsp.log_levels.DEBUG)

vim.keymap.set("n", " d", function()
	vim.lsp.buf.definition()
end)
vim.keymap.set("n", " a", function()
	vim.lsp.buf.code_action()
end)
vim.keymap.set({ "n", "v" }, " s", function()
	vim.lsp.buf.selection_range(vim.v.count1)
end)

-- Disable builtin highlights
vim.cmd("syntax off")
vim.api.nvim_create_autocmd("FileType", {
	callback = function()
		vim.treesitter.stop()
	end,
})

if #vim.api.nvim_list_uis() == 0 then
	vim.cmd("set rtp+=deps/nvim/mini.nvim")
	vim.cmd("set rtp+=deps/nvim/nvim-treesitter")
	vim.cmd("set rtp+=deps/treesitter")

	require("mini.test").setup()
end
