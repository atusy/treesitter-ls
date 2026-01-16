local cwd = vim.uv.cwd()
-- Get default tree-sitter-ls data directory
vim.lsp.config["tree-sitter-ls"] = {
	cmd = { cwd .. "/target/debug/tree-sitter-ls" },
	init_options = {
		languages = {
			markdown = {
				bridge = {
					python = { enabled = true },
					rust = { enabled = true },
					lua = { enabled = true },
				},
			},
		},
		languageServers = {
			["rust-analyzer"] = {
				cmd = { "rust-analyzer" },
				languages = { "rust" },
				workspaceType = "cargo",
			},
			["pyright"] = {
				cmd = { "pyright-langserver", "--stdio" },
				languages = { "python" },
			},
			["lua-language-server"] = {
				cmd = { "lua-language-server" },
				languages = { "lua" },
			},
		},
	},
	on_init = function(client)
		-- to use semanticTokens/full/delta
		client.server_capabilities.semanticTokensProvider.range = false
	end,
}
vim.lsp.enable("tree-sitter-ls")
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

---Retry LSP operation with indexing delays
---Language servers need time to index after document open/change
---@param opts table Options: {child: child_neovim, lsp_request: function, check: function, max_retries: number?, wait_ms: number?, retry_delay_ms: number?}
---@return boolean success true if check passed, false if all retries exhausted
function _G.helper.retry_for_lsp_indexing(opts)
	local child = opts.child
	local lsp_request = opts.lsp_request
	local check = opts.check
	local max_retries = opts.max_retries or 20
	local wait_ms = opts.wait_ms or 3000
	local retry_delay_ms = opts.retry_delay_ms or 500

	for _ = 1, max_retries do
		-- Execute LSP request
		lsp_request()

		-- Wait for result to meet check condition
		local success = _G.helper.wait(wait_ms, check, 100)

		if success then
			return true
		end

		-- Wait before retry (language server may still be indexing)
		vim.wait(retry_delay_ms)
	end

	return false
end

if #vim.api.nvim_list_uis() == 0 then
	-- Use Nix-provided paths if available, otherwise fall back to deps/
	local mini_nvim = vim.env.MINI_NVIM or "deps/nvim/mini.nvim"
	local tree_sitter_grammars = vim.env.TREE_SITTER_GRAMMARS or "deps/tree-sitter"

	vim.cmd("set rtp+=" .. mini_nvim)
	vim.cmd("set rtp+=" .. tree_sitter_grammars)

	-- Only add nvim-treesitter if not using Nix (Nix grammars include queries)
	if not vim.env.TREE_SITTER_GRAMMARS then
		vim.cmd("set rtp+=deps/nvim/nvim-treesitter")
	end

	require("mini.test").setup()
else
	vim.cmd("set rtp+=deps/nvim/catppuccin")
	require("catppuccin").setup()
	vim.cmd("colorscheme catppuccin")
end
