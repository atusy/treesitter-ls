-- E2E test for hover indexing state feedback (PBI-149)
-- Verifies hover shows informative indexing message during rust-analyzer initialization

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

-- Test markdown file with Rust code block for indexing message
T["markdown"] = create_file_test_set(".md", {
	"# Example",
	"",
	"```rust",
	"fn main() {", -- line 4 (1-indexed)
	'    println!("Hello, world!");',
	"}",
	"```",
})

T["markdown"]["hover_shows_indexing_feedback"] = function()
	-- Position cursor on "main" on line 4, column 4
	-- Use type_keys for reliable cursor positioning
	child.type_keys("4G4|")

	-- Verify cursor is on line 4
	local before = child.api.nvim_win_get_cursor(0)
	MiniTest.expect.equality(before[1], 4, "Cursor should be on line 4")

	-- Trigger hover immediately after LSP attach (during indexing)
	-- PBI-149: Should show informative indexing message
	local hover_content = nil
	local found_hover = helper.wait(5000, function()
		child.lua([[vim.lsp.buf.hover()]])
		child.lua([[vim.wait(500)]])

		local wins = child.api.nvim_list_wins()
		for _, win in ipairs(wins) do
			local config = child.api.nvim_win_get_config(win)
			if config.relative ~= "" then
				local buf = child.api.nvim_win_get_buf(win)
				local lines = child.api.nvim_buf_get_lines(buf, 0, -1, false)
				hover_content = table.concat(lines, "\n")
				return true
			end
		end
		return false
	end, 500)

	MiniTest.expect.equality(found_hover, true, "Hover should show a floating window")

	-- Verify the indexing message format (PBI-149 acceptance criteria)
	-- Should show hourglass emoji and mention rust-analyzer
	local has_indexing_format = hover_content:find("indexing") ~= nil
		and hover_content:find("rust%-analyzer") ~= nil
	MiniTest.expect.equality(
		has_indexing_format,
		true,
		"Indexing message should mention 'indexing' and 'rust-analyzer', got: " .. hover_content
	)
end

return T
