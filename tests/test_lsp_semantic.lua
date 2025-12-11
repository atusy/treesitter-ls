local child = MiniTest.new_child_neovim()

local T = MiniTest.new_set({ hooks = { post_once = child.stop } })

---Helper function to create file-specific test set
---@param file_path string path to file to open in tests
local function create_file_test_set(file_path)
	return MiniTest.new_set({
		hooks = {
			pre_case = function()
				-- Restart Neovim with minimal init
				child.restart({ "-u", "scripts/minimal_init.lua" })
				child.lua(([[vim.cmd.edit(%q)]]):format(file_path))
				local attached = helper.wait(5000, function()
					local clients = child.lua_get(
						[[#vim.lsp.get_clients({ bufnr = vim.api.nvim_get_current_buf(), name = "treesitter_ls" })]]
					)
					return clients > 0
				end, 10)
				if not attached then
					error("Failed to attach treesitter_ls")
				end

				-- Force semantic token refresh and wait for extmarks to be applied
				child.lua([[vim.lsp.semantic_tokens.force_refresh(0)]])
				child.cmd("/.") -- ensure the cursor is on some character
				local has_tokens = helper.wait(3000, function()
					local tokens = child.lua_get([[vim.lsp.semantic_tokens.get_at_pos()]])
					return tokens and #tokens > 0
				end, 50)
				if not has_tokens then
					error("Failed to get semantic tokens")
				end
			end,
		},
	})
end

T["assets/example.lua"] = create_file_test_set("tests/assets/example.lua")
T["assets/example.lua"]["semanticToken"] = MiniTest.new_set({
	parametrize = {
		{ 0, 1, { { type = "keyword" } } },
	},
})
T["assets/example.lua"]["semanticToken"]["works"] = function(line, col, tokens)
	local given_tokens = child.lua_get(string.format([[vim.lsp.semantic_tokens.get_at_pos(0, %d, %d)]], line, col))
	MiniTest.expect.equality(#given_tokens, #tokens)
	for i, token in ipairs(tokens) do
		for key, value in pairs(token) do
			MiniTest.expect.equality(given_tokens[i][key], value)
		end
	end
end

return T
