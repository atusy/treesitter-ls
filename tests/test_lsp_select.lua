local child = MiniTest.new_child_neovim()

local T = MiniTest.new_set({
	hooks = {
		pre_case = function()
			child.restart({ "-u", "scripts/minimal_init.lua" })
			child.lua([[vim.cmd.edit("tests/assets/example.lua")]])
			local clients = 0
			for _ = 0, 10, 1 do
				vim.uv.sleep(10)
				clients = child.lua_get(
					[[#vim.lsp.get_clients({ bufnr = vim.api.nvim_get_current_buf(), name = "treesitter_ls" })]]
				)
				if clients > 0 then
					break
				end
			end
			if clients == 0 then
				error("Failed to attach treesitter_ls")
			end
		end,
		post_once = child.stop,
	},
})

T["selectionRange"] = MiniTest.new_set({})
T["selectionRange"]["no injection"] = MiniTest.new_set({})

T["selectionRange"]["no injection"]["1"] = function()
	child.cmd([[lua vim.lsp.buf.selection_range(1)]])
	for _ = 0, 10, 1 do
		vim.uv.sleep(10)
		if child.api.nvim_get_mode().mode == "v" then
			break
		end
	end
	child.cmd([[normal! y]])
	local reg = child.fn.getreg()
	MiniTest.expect.equality(reg, "local")
end

T["selectionRange"]["no injection"]["2"] = function()
	child.cmd([[lua vim.lsp.buf.selection_range(2)]])
	for _ = 0, 10, 1 do
		vim.uv.sleep(10)
		if child.api.nvim_get_mode().mode == "v" then
			break
		end
	end
	child.cmd([[normal! y]])
	local reg = child.fn.getreg()
	MiniTest.expect.equality(reg, "local M = {}")
end

return T
