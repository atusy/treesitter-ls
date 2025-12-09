local child = MiniTest.new_child_neovim()

local T = MiniTest.new_set({
	hooks = {
		post_once = child.stop,
	},
})

-- Helper function to create file-specific test set
local function create_file_test_set(file_path)
	return MiniTest.new_set({
		hooks = {
			pre_case = function()
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
			end,
		},
	})
end

T["assets/example.lua"] = create_file_test_set("tests/assets/example.lua")

T["assets/example.lua"]["selectionRange"] = MiniTest.new_set({})
T["assets/example.lua"]["selectionRange"]["no injection"] = MiniTest.new_set({})

T["assets/example.lua"]["selectionRange"]["no injection"]["direction"] = MiniTest.new_set({
	parametrize = { { 1, "local" }, { 2, "local M = {}" } },
})
T["assets/example.lua"]["selectionRange"]["no injection"]["direction"]["works"] = function(direction, expected)
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
