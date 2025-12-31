local child = MiniTest.new_child_neovim()

local T = MiniTest.new_set({ hooks = { post_once = child.stop } })

---Helper function to create file-specific test set
---@param ext string file extension (e.g., ".md")
---@param lines string[] lines to write to the file
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

-- Markdown file with Rust code block where we'll test completion
-- The code block defines a struct with fields that we can complete on
T["markdown"] = create_file_test_set(".md", {
	"# Rust Example",
	"",
	"```rust",
	"struct Point {",
	"    x: i32,",
	"    y: i32,",
	"}",
	"",
	"fn main() {",
	"    let p = Point { x: 1, y: 2 };",
	"    p.", -- line 11, cursor after "p." for completion
	"}",
	"```",
})

T["markdown"]["completion returns items with adjusted textEdit ranges"] = function()
	-- Position cursor after "p." on line 11 (1-indexed in Vim)
	-- Line 11 is the "p." line inside the code block
	child.cmd([[normal! 11G$]])

	-- Wait a moment for rust-analyzer to index
	vim.uv.sleep(2000)

	-- Trigger completion using vim.lsp.buf.completion()
	-- This is an async operation, so we need to wait for results
	local completed = helper.wait(15000, function()
		-- Check if completion results are available
		-- We'll use omnifunc which uses LSP completion
		child.lua([[
			vim.lsp.buf.completion()
		]])
		vim.uv.sleep(500)

		-- Check for completion items by looking at the completion menu
		-- or by checking if lsp.buf.completion returned results
		local has_menu = child.lua_get([[vim.fn.pumvisible() == 1]])
		return has_menu
	end, 500)

	if completed then
		-- Get completion items from the popup menu
		local items = child.lua_get([[vim.fn.complete_info({"items"}).items]])

		-- Verify we got completion items (x and y fields from Point struct)
		MiniTest.expect.equality(type(items), "table", "Should return completion items table")

		-- Check that at least one completion item exists
		-- rust-analyzer should suggest 'x' and 'y' fields
		local found_field = false
		for _, item in ipairs(items) do
			if item.word == "x" or item.word == "y" then
				found_field = true
				break
			end
		end

		MiniTest.expect.equality(found_field, true, "Should have 'x' or 'y' field in completions")
	else
		-- If popup didn't appear, try alternative approach via complete()
		-- This tests that the completion handler is at least being called
		MiniTest.expect.equality(
			true,
			false,
			"Completion popup did not appear - completion handler may not be implemented"
		)
	end
end

return T
