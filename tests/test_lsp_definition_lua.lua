-- E2E test for async go-to-definition with Lua code blocks in Markdown
-- PBI-141 Subtask 3: Verify definition works through async bridge with lua-language-server

local child = MiniTest.new_child_neovim()

local T = MiniTest.new_set({ hooks = { post_once = child.stop } })

---Helper function to create file-specific test set
---@param ext string file extension
---@param lines string[] file content lines
local function create_file_test_set(ext, lines)
	return MiniTest.new_set({
		hooks = {
			pre_case = function()
				local tempname = vim.fn.tempname() .. ext
				vim.cmd.edit(tempname)
				vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
				-- Write the file to disk so child can read it
				vim.cmd.write()
				child.restart({ "-u", "scripts/minimal_init.lua" })
				child.lua(([[vim.cmd.edit(%q)]]):format(tempname))
				local attached = helper.wait(5000, function()
					local clients = child.lua_get(
						[[#vim.lsp.get_clients({ bufnr = vim.api.nvim_get_current_buf(), name = "treesitter-ls" })]]
					)
					return clients > 0
				end, 10)
				if not attached then
					error("Failed to attach treesitter-ls")
				end
			end,
		},
	})
end

-- Test Lua code block in Markdown
-- Line numbers (1-indexed in Neovim):
-- 1: "Here is a Lua function:"
-- 2: ""
-- 3: "```lua"
-- 4: "local function greet(name)"  <-- definition here
-- 5: "    return 'Hello, ' .. name"
-- 6: "end"
-- 7: ""
-- 8: "local message = greet('World')"  <-- reference here (greet at column 17)
-- 9: "print(message)"
-- 10: "```"
T["markdown_lua"] = create_file_test_set(".md", {
	"Here is a Lua function:",
	"",
	"```lua",
	"local function greet(name)",
	"    return 'Hello, ' .. name",
	"end",
	"",
	"local message = greet('World')",
	"print(message)",
	"```",
})

T["markdown_lua"]["definition"] = function()
	-- Position cursor on "greet" call on line 8, column 17 (on the 'g' of greet)
	child.cmd([[normal! 8G17|]])

	-- Verify cursor is on line 8 before definition jump
	local before = child.api.nvim_win_get_cursor(0)
	MiniTest.expect.equality(before[1], 8, "Cursor should start on line 8")

	-- Call definition in child vim
	child.lua([[vim.lsp.buf.definition()]])

	-- Poll child's cursor position until it moves to line 4 or timeout
	-- lua-language-server may need time to process the file, retry definition requests
	local jumped = false
	for attempt = 1, 30 do
		child.lua([[vim.lsp.buf.definition()]])
		jumped = helper.wait(500, function()
			local line = child.api.nvim_win_get_cursor(0)[1]
			return line == 4
		end, 50)
		if jumped then
			break
		end
		-- Wait between attempts for lua-language-server to be ready
		vim.uv.sleep(200)
	end

	-- Get final cursor position for error message
	local after = child.api.nvim_win_get_cursor(0)

	-- Assert the jump occurred
	MiniTest.expect.equality(
		after[1],
		4,
		("Definition jump failed: cursor at line %d, expected line 4"):format(after[1])
	)
end

return T
