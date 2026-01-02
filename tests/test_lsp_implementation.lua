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
	"Here is an implementation example:",
	"",
	"```rust",
	"trait Foo {",                              -- line 4
	"    fn bar(&self);",                       -- line 5
	"}",                                        -- line 6
	"",                                         -- line 7
	"struct S;",                                -- line 8
	"",                                         -- line 9
	"impl Foo for S {",                         -- line 10
	"    fn bar(&self) {}",                     -- line 11 - impl method
	"}",                                        -- line 12
	"",                                         -- line 13
	"fn main() {",                              -- line 14
	"    let s = S;",                           -- line 15
	"    s.bar();",                             -- line 16 - method call
	"}",                                        -- line 17
	"```",                                      -- line 18
})
T["markdown"]["implementation"] = function()
	-- Position cursor on "bar" method call on line 16 (on the 'b' of 'bar')
	-- The pattern is "    s.bar();" so 'b' is at column 7 (1-indexed)
	child.cmd([[normal! 16G7|]])

	-- Verify cursor is on line 16 before implementation jump
	local before = child.api.nvim_win_get_cursor(0)
	MiniTest.expect.equality(before[1], 16, "Cursor should start on line 16")

	-- Retry implementation request since rust-analyzer may still be indexing
	-- This is more resilient than a fixed sleep as indexing time varies
	local jumped = false
	for _ = 1, 20 do
		-- Reset cursor position before each attempt
		child.cmd([[normal! 16G7|]])

		-- Call implementation in child vim
		child.lua([[vim.lsp.buf.implementation()]])

		-- Poll child's cursor position until it moves to line 10 or 11 or timeout
		-- rust-analyzer may return the impl block line (10) or the method line (11)
		-- We accept either as they both point to the implementation
		local success = helper.wait(2000, function()
			local line = child.api.nvim_win_get_cursor(0)[1]
			return line == 11 or line == 10
		end, 100)

		if success then
			jumped = true
			break
		end

		-- Wait before retry (rust-analyzer may still be indexing)
		vim.wait(500)
	end

	-- Get final cursor position for error message
	local after = child.api.nvim_win_get_cursor(0)

	-- Assert the jump occurred to either the impl method or the impl block
	local valid_lines = { [10] = true, [11] = true }
	MiniTest.expect.equality(
		valid_lines[after[1]] or false,
		true,
		("Implementation jump failed: cursor at line %d, expected line 10 or 11"):format(after[1])
	)
end

return T
