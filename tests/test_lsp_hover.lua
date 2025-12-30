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

T["markdown"]["hover on function call shows signature"] = function()
	-- Position cursor on "example" call on line 9, column 5 (on the 'e' of example)
	child.cmd([[normal! 9G5|]])

	-- Verify cursor is on line 9
	local before = child.api.nvim_win_get_cursor(0)
	MiniTest.expect.equality(before[1], 9, "Cursor should start on line 9")

	-- Call hover in child vim
	child.lua([[vim.lsp.buf.hover()]])

	-- Wait for hover window to appear (floating window with hover content)
	-- Use longer timeout since rust-analyzer needs time to index
	local hover_content = nil
	local hover_appeared = helper.wait(15000, function()
		-- Check for floating windows (hover creates a floating window)
		local wins = child.api.nvim_list_wins()
		for _, win in ipairs(wins) do
			local config = child.api.nvim_win_get_config(win)
			if config.relative ~= "" then
				-- Found a floating window, check its content
				local buf = child.api.nvim_win_get_buf(win)
				local lines = child.api.nvim_buf_get_lines(buf, 0, -1, false)
				hover_content = table.concat(lines, "\n")
				-- Hover should show "fn example" function signature
				if hover_content:find("fn example") or hover_content:find("example") then
					return true
				end
			end
		end
		return false
	end, 100)

	MiniTest.expect.equality(
		hover_appeared,
		true,
		("Hover window should appear with function signature. Got content: %s"):format(hover_content or "nil")
	)
end

return T
