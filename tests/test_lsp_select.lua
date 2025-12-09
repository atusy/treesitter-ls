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

T["assets/example.lua"] = create_file_test_set("tests/assets/example.md")

T["assets/example.lua"]["selectionRange"] = MiniTest.new_set({})
T["assets/example.lua"]["selectionRange"]["with injection"] = MiniTest.new_set({})
T["assets/example.lua"]["selectionRange"]["with injection"]["direction"] = MiniTest.new_set({
	parametrize = {
		-- Direction 1 should select innermost (just "title" key)
		{ 1, "title" },
		-- Direction 5 should select the full YAML content (no trailing newline in visual yank)
		{
			5,
			table.concat({
				[==[title: "awesome"]==],
				[==[array: ["xxxx"]]==],
			}, "\n"),
		},
	},
})
T["assets/example.lua"]["selectionRange"]["with injection"]["direction"]["works"] = function(direction, expected)
	child.cmd([[normal! j]])
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

-- Test selection outside injection region (User Story 4 criterion 4)
T["assets/example.lua"]["selectionRange"]["outside injection"] = MiniTest.new_set({})
T["assets/example.lua"]["selectionRange"]["outside injection"]["direction"] = MiniTest.new_set({
	parametrize = {
		-- Cursor on line 18 "# section" - direction 1 should select "section"
		{ 18, 3, 1, "section" },
		-- Cursor on line 20 "paragraph" - direction 1 should select "paragraph"
		{ 20, 1, 1, "paragraph" },
	},
})
T["assets/example.lua"]["selectionRange"]["outside injection"]["direction"]["works"] =
	function(line, col, direction, expected)
		-- Move to specified line and column
		child.cmd(([[normal! %dG%d|]]):format(line, col))
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

-- Test nested injection (User Story 5: Markdown → Markdown → Lua)
-- Line 14 contains "local injection = true" which is Lua inside inner Markdown inside outer Markdown
T["assets/example.lua"]["selectionRange"]["nested injection"] = MiniTest.new_set({})
T["assets/example.lua"]["selectionRange"]["nested injection"]["direction"] = MiniTest.new_set({
	parametrize = {
		-- Cursor on line 14 inside "injection" - direction 1 should select the identifier
		{ 14, 7, 1, "injection" },
		-- Cursor on line 14 inside "injection" - direction 2 should select larger expression
		{ 14, 7, 2, "injection = true" },
		-- Cursor on line 14 inside "injection" - direction 3 should select the full statement
		{ 14, 7, 3, "local injection = true" },
	},
})
T["assets/example.lua"]["selectionRange"]["nested injection"]["direction"]["works"] =
	function(line, col, direction, expected)
		-- Move to specified line and column
		child.cmd(([[normal! %dG%d|]]):format(line, col))
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
