-- E2E test for hover in Markdown code blocks with rust-analyzer bridge
-- Verifies hover requests work through async bridge path

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

-- Test markdown file with Rust code block
T["markdown"] = create_file_test_set(".md", {
	"# Example",
	"",
	"```rust",
	"fn main() {", -- line 4 (1-indexed)
	'    println!("Hello, world!");',
	"}",
	"```",
})

T["markdown"]["hover_returns_content"] = function()
	-- Position cursor on "main" on line 4, column 4 (on the 'm' of main)
	-- Use type_keys for reliable cursor positioning
	child.type_keys("4G4|")

	-- Verify cursor is on line 4
	local before = child.api.nvim_win_get_cursor(0)
	MiniTest.expect.equality(before[1], 4, "Cursor should be on line 4")

	-- Trigger hover and wait for a floating window to appear
	-- During indexing, hover returns "indexing (rust-analyzer)" message (PBI-149)
	-- Either response proves the async bridge is working correctly
	local hover_content = nil
	local found_hover = helper.wait(10000, function()
		child.lua([[vim.lsp.buf.hover()]])
		child.lua([[vim.wait(500)]])

		-- Check all windows for a floating window
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

	-- Verify we got some content (either indexing message or real hover)
	-- Both prove the async bridge is working
	MiniTest.expect.equality(
		hover_content ~= nil and #hover_content > 0,
		true,
		"Hover content should not be empty"
	)

	-- Verify it's related to rust-analyzer (either real content or indexing message)
	local is_valid = hover_content:find("main") ~= nil
		or hover_content:find("fn") ~= nil
		or hover_content:find("rust%-analyzer") ~= nil
		or hover_content:find("indexing") ~= nil
	MiniTest.expect.equality(is_valid, true, "Hover should show function info or indexing status")
end

return T
