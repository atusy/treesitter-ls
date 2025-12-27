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

T["markdown"] = create_file_test_set(".md", {
	"Here is a function definition:",
	"",
	"```rust",
	"fn example() {", -- line 4
	'    println!("Hello, world!");',
	"}",
	"",
	"fn main() {",
	"    example();", -- line 9
	"}",
	"```",
})
T["markdown"]["definition"] = function()
	-- Position cursor on "example" call on line 9, column 5 (on the 'e' of example)
	child.cmd([[normal! 9G5|]])
	child.lua([[vim.lsp.buf.definition()]])

	-- Wait for async LSP request/response (rust-analyzer spawn + initialize + query)
	vim.wait(2000)

	local line = 0
	helper.wait(5000, function()
		line = child.api.nvim_win_get_cursor(0)[1]
		return line == 4
	end, 10)

	MiniTest.expect.equality(line, 4)
end

return T
