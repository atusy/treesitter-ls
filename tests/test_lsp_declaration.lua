local child = MiniTest.new_child_neovim()

local T = MiniTest.new_set({ hooks = { post_once = child.stop } })

---Helper function to create file-specific test set
---@param file_path string path to file to open in tests
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

T["markdown"] = create_file_test_set(".md", {
	"Here is a declaration example:",
	"",
	"```rust",
	"fn hello() -> i32 {", -- line 4 - function declaration
	"    42",
	"}",
	"",
	"fn main() {",
	"    let x = hello();", -- line 9 - function call
	"}",
	"```",
})
T["markdown"]["declaration"] = function()
	-- Position cursor on "hello" function call on line 9 (on the 'h' of 'hello')
	-- The pattern is "    let x = hello();" so 'h' is at column 13 (1-indexed)
	child.cmd([[normal! 9G13|]])

	-- Verify cursor is on line 9 before declaration jump
	local before = child.api.nvim_win_get_cursor(0)
	MiniTest.expect.equality(before[1], 9, "Cursor should start on line 9")

	-- Wait for rust-analyzer to index (this can take a while on first run)
	vim.uv.sleep(2000)

	-- Call declaration in child vim
	child.lua([[vim.lsp.buf.declaration()]])

	-- Poll child's cursor position until it moves to line 4 (function declaration) or timeout
	-- Note: In Rust, declaration typically goes to the same place as definition
	-- since Rust doesn't have separate declaration/definition like C/C++
	local jumped = helper.wait(10000, function()
		local line = child.api.nvim_win_get_cursor(0)[1]
		return line == 4
	end, 100)

	-- Get final cursor position for error message
	local after = child.api.nvim_win_get_cursor(0)

	-- Assert the jump occurred
	MiniTest.expect.equality(
		after[1],
		4,
		("Declaration jump failed: cursor at line %d, expected line 4"):format(after[1])
	)
end

return T
