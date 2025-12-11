local child = MiniTest.new_child_neovim()

local T = MiniTest.new_set({ hooks = { post_once = child.stop } })

---Helper function to create file-specific test set
---@param file_path string path to file to open in tests
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

---Helper function to test selection range at a given position and direction
---@param line number? 1-indexed line number (nil to use cursor position)
---@param col number? 1-indexed column number (nil to use cursor position)
---@param direction number selection expansion level (1 = smallest, higher = larger)
---@param expected string expected yanked text after selection
local function test_selection_range(line, col, direction, expected)
	if line and col then
		child.cmd(([[normal! %dG%d|]]):format(line, col))
	end
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

-- ============================================================================
-- Lua file tests (no injection)
-- ============================================================================
T["assets/example.lua"] = create_file_test_set("tests/assets/example.lua")
T["assets/example.lua"]["selectionRange"] = MiniTest.new_set({})
T["assets/example.lua"]["selectionRange"]["no injection"] = MiniTest.new_set({
	parametrize = { { 1, "local" }, { 2, "local M = {}" } },
})
T["assets/example.lua"]["selectionRange"]["no injection"]["works"] = function(direction, expected)
	test_selection_range(nil, nil, direction, expected)
end

-- ============================================================================
-- Markdown file tests (with injections)
-- ============================================================================
T["assets/example.md"] = create_file_test_set("tests/assets/example.md")
T["assets/example.md"]["selectionRange"] = MiniTest.new_set({})

-- Test selection no injection region (plain Markdown)
T["assets/example.md"]["selectionRange"]["no injection"] = MiniTest.new_set({
	parametrize = {
		{ 28, 1, 1, "paragraph" }, -- line 20 "paragraph"
		{ 26, 1, 3, "# section\n\nparagraph" }, -- line 18 "# section"
	},
})
T["assets/example.md"]["selectionRange"]["no injection"]["works"] = function(line, col, direction, expected)
	test_selection_range(line, col, direction, expected)
end

-- Test selection expansion through injection boundaries
-- Verifies that selection properly includes host document ranges (code block delimiters, frontmatter)
T["assets/example.md"]["selectionRange"]["injection"] = MiniTest.new_set({
	parametrize = {
		-- YAML frontmatter expansion
		{ 2, 1, 1, "title" },
		{ 2, 1, 2, 'title: "awesome"' },
		{ 2, 1, 3, table.concat({ 'title: "awesome"', 'array: ["xxxx"]' }, "\n") },
		{ 2, 1, 4, table.concat({ "---", 'title: "awesome"', 'array: ["xxxx"]', "---" }, "\n") },
		-- Lua code block expansion
		{ 7, 1, 1, "local" },
		{ 7, 1, 2, "local xyz = 12345" },
		{ 7, 1, 3, "local xyz = 12345" },
		{ 7, 1, 4, "```lua\nlocal xyz = 12345\n```" },
		-- nested injection markdown -> markdown -> lua
		{ 14, 7, 1, "injection" }, -- select identifier
		{ 14, 7, 2, "injection = true" }, -- select expression
		{ 14, 7, 3, "local injection = true" }, -- select full statement
		-- indented injection markdown -> lua
		{ 23, 22, 1, "true" }, -- select identifier
		{ 23, 22, 2, "indent = true" }, -- select expression
	},
})
T["assets/example.md"]["selectionRange"]["injection"]["works"] = function(line, col, direction, expected)
	test_selection_range(line, col, direction, expected)
end

return T
