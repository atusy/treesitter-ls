local cwd = vim.uv.cwd()
-- Get default treesitter-ls data directory
vim.lsp.config.treesitter_ls = {
	cmd = { cwd .. "/target/debug/treesitter-ls" },
	init_options = {
		bridge = {
			servers = {
				["rust-analyzer"] = {
					command = "rust-analyzer",
					languages = { "rust" },
					workspace_type = "cargo",
				},
			},
		},
	},
	on_init = function(client)
		-- to use semanticTokens/full/delta
		client.server_capabilities.semanticTokensProvider.range = false
	end,
}
vim.lsp.enable("treesitter_ls")
vim.lsp.log.set_level(vim.lsp.log_levels.DEBUG)

vim.keymap.set("n", "gd", function()
	vim.lsp.buf.definition()
end)

-- Disable builtin highlights
vim.cmd("syntax off")
vim.api.nvim_create_autocmd("FileType", {
	callback = function()
		vim.treesitter.stop()
	end,
})

_G.progress_messages = {}

vim.api.nvim_create_autocmd("LspProgress", {
	callback = function(ev)
		local msg = ev.data.params.value.kind .. ": " .. ev.data.params.value.title
		table.insert(_G.progress_messages, msg)
		vim.notify(msg)
	end,
})

vim.api.nvim_create_user_command("LspProgress", function()
	for _, msg in ipairs(_G.progress_messages) do
		vim.print(msg)
	end
end, {})

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
