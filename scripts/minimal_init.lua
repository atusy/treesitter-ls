local cwd = vim.uv.cwd()
vim.lsp.config.treesitter_ls = {
	cmd = { cwd .. "/target/debug/treesitter-ls" },
	init_options = {
		-- searchPaths = { "/Users/atusy/Library/Application Support/treesitter-ls" },
		autoInstall = true,
	},
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

_G.helper = {}

---@return boolean true if callback returns true before timeout, false otherwise
function _G.helper.wait(timeout_ms, callback, interval_ms)
	timeout_ms = timeout_ms or 5000
	interval_ms = interval_ms or 10

	local start_time = vim.uv.hrtime()
	local timeout_ns = timeout_ms * 1000000

	while true do
		local result = callback()
		if result then
			return true
		end

		if vim.uv.hrtime() - start_time > timeout_ns then
			return false
		end

		vim.uv.sleep(interval_ms)
	end
end

if #vim.api.nvim_list_uis() == 0 then
	vim.cmd("set rtp+=deps/nvim/mini.nvim")
	vim.cmd("set rtp+=deps/nvim/nvim-treesitter")
	vim.cmd("set rtp+=deps/treesitter")

	require("mini.test").setup()
else
	vim.cmd("set rtp+=deps/nvim/catppuccin")
	require("catppuccin").setup()
	vim.cmd("colorscheme catppuccin")
end
