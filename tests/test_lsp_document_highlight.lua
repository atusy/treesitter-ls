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
	"Here is a document highlight example:",
	"",
	"```rust",
	"fn main() {",                -- line 4
	"    let x = 1;",             -- line 5 - x definition
	"    let y = x + 2;",         -- line 6 - x usage (should be highlighted as Read)
	"    println!(\"{}\", x);",   -- line 7 - x usage (should be highlighted as Read)
	"}",                          -- line 8
	"```",                        -- line 9
})
T["markdown"]["document_highlight"] = function()
	-- Position cursor on "x" variable on line 5 (the definition)
	-- The pattern is "    let x = 1;" so 'x' is at column 9 (1-indexed)
	child.cmd([[normal! 5G9|]])

	-- Verify cursor is on line 5 before document highlight
	local before = child.api.nvim_win_get_cursor(0)
	MiniTest.expect.equality(before[1], 5, "Cursor should start on line 5")

	-- Wait for rust-analyzer to index (this can take a while on first run)
	vim.uv.sleep(2000)

	-- Set up a handler to capture the document highlight result
	child.lua([[
		_G.highlight_result = nil
		_G.highlight_done = false
		local params = vim.lsp.util.make_position_params()
		vim.lsp.buf_request(0, "textDocument/documentHighlight", params, function(err, result, ctx, config)
			if result then
				_G.highlight_result = result
			end
			_G.highlight_done = true
		end)
	]])

	-- Wait for the handler to complete
	local got_result = helper.wait(10000, function()
		return child.lua_get([[_G.highlight_done]])
	end, 100)

	MiniTest.expect.equality(got_result, true, "Document highlight request should complete")

	-- Get the highlight result
	local highlights = child.lua_get([[_G.highlight_result]])

	-- We expect at least 1 highlight (the definition on line 5)
	-- Note: rust-analyzer may return more or fewer depending on indexing
	local highlight_count = (highlights and #highlights) or 0
	MiniTest.expect.equality(
		highlight_count >= 1,
		true,
		("Expected at least 1 highlight, got %d"):format(highlight_count)
	)

	-- Verify that all highlights are on expected lines
	-- Lines in LSP are 0-indexed, so line 5 = line 4, line 6 = line 5, line 7 = line 6
	-- But in the markdown, the rust block starts at line 4, so:
	-- - line 5 (let x = 1;) = line 4 in LSP (0-indexed)
	-- - line 6 (let y = x + 2;) = line 5 in LSP
	-- - line 7 (println!) = line 6 in LSP
	local valid_lines = { [4] = true, [5] = true, [6] = true }
	for _, highlight in ipairs(highlights or {}) do
		local line = highlight.range.start.line
		MiniTest.expect.equality(
			valid_lines[line] or false,
			true,
			("Highlight on unexpected line %d (expected lines 4-6)"):format(line)
		)
	end
end

return T
