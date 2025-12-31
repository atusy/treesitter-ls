local child = MiniTest.new_child_neovim()
local T = MiniTest.new_set({
	hooks = {
		pre_case = function()
			child.restart({ "-u", "scripts/minimal_init.lua" })
			child.lua([[vim.cmd.edit("tests/assets/example.lua")]])
		end,
		post_once = child.stop,
	},
})

T["LSP starts"] = function()
	local clients = 0
	helper.wait(5000, function()
		clients =
			child.lua_get([[#vim.lsp.get_clients({ bufnr = vim.api.nvim_get_current_buf(), name = "treesitter-ls" })]])
		return clients > 0
	end, 10)
	MiniTest.expect.equality(clients, 1)
end

return T

