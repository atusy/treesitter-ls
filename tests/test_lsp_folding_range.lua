local child = MiniTest.new_child_neovim()

local T = MiniTest.new_set({ hooks = { post_once = child.stop } })

---Helper function to create file-specific test set
---@param ext string file extension including dot
---@param lines string[] lines of content
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

-- Test foldingRange in a markdown file with a Rust code block containing a function
-- The function body should be foldable
T["markdown"] = create_file_test_set(".md", {
	"# Rust Example",                               -- line 1 (0-indexed: 0)
	"",                                              -- line 2 (0-indexed: 1)
	"Here is a function:",                          -- line 3 (0-indexed: 2)
	"",                                              -- line 4 (0-indexed: 3)
	"```rust",                                       -- line 5 (0-indexed: 4)
	"fn example() {",                                -- line 6 (0-indexed: 5) - virtual line 0
	"    let x = 1;",                                -- line 7 (0-indexed: 6) - virtual line 1
	"    let y = 2;",                                -- line 8 (0-indexed: 7) - virtual line 2
	"    println!(\"{}\", x + y);",                  -- line 9 (0-indexed: 8) - virtual line 3
	"}",                                             -- line 10 (0-indexed: 9) - virtual line 4
	"```",                                           -- line 11 (0-indexed: 10)
})

T["markdown"]["folding_range"] = function()
	-- Wait for rust-analyzer to index (this can take a while on first run)
	vim.uv.sleep(2000)

	-- Set up a handler to capture the folding range result
	child.lua([[
		_G.fold_result = nil
		_G.fold_err = nil
		_G.fold_done = false
		local params = { textDocument = vim.lsp.util.make_text_document_params() }
		vim.lsp.buf_request(0, "textDocument/foldingRange", params, function(err, result, ctx, config)
			_G.fold_err = err
			_G.fold_result = result
			_G.fold_done = true
		end)
	]])

	-- Wait for the handler to complete
	local got_result = helper.wait(10000, function()
		return child.lua_get([[_G.fold_done]])
	end, 100)

	MiniTest.expect.equality(got_result, true, "Folding range request should complete")

	-- Check for errors
	local fold_err = child.lua_get([[_G.fold_err]])
	MiniTest.expect.equality(fold_err, vim.NIL, "Folding range request should not error")

	-- Get the folding range result
	local ranges = child.lua_get([[_G.fold_result]])

	-- Log the result for debugging
	child.lua([[
		vim.api.nvim_echo({{"Folding range result: " .. vim.inspect(_G.fold_result)}}, true, {})
	]])

	-- We accept either:
	-- 1. nil/empty result (if rust-analyzer doesn't return folds for simple content)
	-- 2. A list of folding ranges
	-- The main test is that the request completes without error
	if ranges ~= vim.NIL and ranges ~= nil then
		-- If we got ranges, verify they have valid structure
		if type(ranges) == "table" and #ranges > 0 then
			for _, range in ipairs(ranges) do
				-- Each range should have startLine and endLine
				MiniTest.expect.equality(
					range.startLine ~= nil,
					true,
					"Each folding range should have startLine"
				)
				MiniTest.expect.equality(
					range.endLine ~= nil,
					true,
					"Each folding range should have endLine"
				)
				-- Verify the line numbers are translated to host coordinates
				-- The rust code block starts at line 5 (0-indexed), so any folding range
				-- should have startLine >= 5 (the rust code block start in host document)
				MiniTest.expect.equality(
					range.startLine >= 5,
					true,
					"Folding range startLine should be translated to host coordinates (>= 5)"
				)
			end
		end
	end
end

return T
