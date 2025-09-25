local child = MiniTest.new_child_neovim()

local T = MiniTest.new_set({
	hooks = {
		pre_case = function()
			child.restart({ "-u", "scripts/minimal_init.lua" })
			child.lua([[vim.cmd.edit("tests/assets/example.lua")]])
			local attached = helper.wait(5000, function()
				local clients = child.lua_get(
					[[#vim.lsp.get_clients({ bufnr = vim.api.nvim_get_current_buf(), name = "treesitter_ls" })]]
				)
				return clients > 0
			end, 10)
			if not attached then
				error("Failed to attach treesitter_ls")
			end
		end,
		post_once = child.stop,
	},
})

T["selectionRange"] = MiniTest.new_set({})
T["selectionRange"]["no injection"] = MiniTest.new_set({})

T["selectionRange"]["no injection"]["direction"] = MiniTest.new_set({
	parametrize = { { 1, "local" }, { 2, "local M = {}" } },
})
T["selectionRange"]["no injection"]["direction"]["works"] = function(direction, expected)
	child.cmd(([[lua vim.lsp.buf.selection_range(%d)]]):format(direction))
	if not helper.wait(5000, function()
		return child.api.nvim_get_mode().mode == "v"
	end, 10) then
		error("selection_range timed out")
	end
	child.cmd([[normal! y]])
	local reg = child.fn.getreg()
	MiniTest.expect.equality(reg, expected)
end

return T
