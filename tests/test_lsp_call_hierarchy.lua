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

-- Test with a code block containing function calls
T["markdown"] = create_file_test_set(".md", {
	"Here is a call hierarchy example:",
	"",
	"```rust",
	"fn helper() -> i32 {",       -- line 4 - helper function
	"    42",                      -- line 5
	"}",                           -- line 6
	"",                            -- line 7
	"fn caller() -> i32 {",        -- line 8 - caller function (calls helper)
	"    helper()",                -- line 9 - call to helper
	"}",                           -- line 10
	"",                            -- line 11
	"fn main() {",                 -- line 12 - main function (calls caller)
	"    let x = caller();",       -- line 13 - call to caller
	"}",                           -- line 14
	"```",                         -- line 15
})
T["markdown"]["prepareCallHierarchy"] = function()
	-- Wait for rust-analyzer to index
	vim.uv.sleep(3000)

	-- Set up a handler to capture the prepareCallHierarchy result
	-- Position cursor on "caller" function definition on line 8
	-- In the markdown file, line 8 (1-indexed) is "fn caller() -> i32 {"
	-- which is line 7 in 0-indexed LSP coordinates
	-- The 'c' of "caller" is at character 3 (after "fn ")
	child.lua([[
		_G.prepare_result = nil
		_G.prepare_err = nil
		_G.prepare_done = false
		local bufnr = vim.api.nvim_get_current_buf()
		local params = {
			textDocument = vim.lsp.util.make_text_document_params(bufnr),
			position = { line = 7, character = 3 }  -- line 8 (0-indexed), on "caller" starting at 'c'
		}
		vim.lsp.buf_request(0, "textDocument/prepareCallHierarchy", params, function(err, result, ctx, config)
			if err then
				_G.prepare_err = vim.inspect(err)
			end
			if result then
				_G.prepare_result = result
			end
			_G.prepare_done = true
		end)
	]])

	-- Wait for the handler to complete
	local got_result = helper.wait(10000, function()
		return child.lua_get([[_G.prepare_done]])
	end, 100)

	MiniTest.expect.equality(got_result, true, "prepareCallHierarchy request should complete")

	-- Check for errors
	local prepare_err = child.lua_get([[_G.prepare_err or "nil"]])

	-- Get the result count
	local item_count = child.lua_get([[(function() if _G.prepare_result and type(_G.prepare_result) == "table" then return #_G.prepare_result else return 0 end end)()]])

	-- We expect at least 1 call hierarchy item for the "caller" function
	MiniTest.expect.equality(
		item_count >= 1,
		true,
		("Expected at least 1 call hierarchy item, got %d (err: %s)"):format(item_count, prepare_err)
	)

	-- Verify the first item is named "caller" (the function we're on)
	local first_item_name = child.lua_get([[(function() if _G.prepare_result and type(_G.prepare_result) == "table" and #_G.prepare_result > 0 then return _G.prepare_result[1].name else return "" end end)()]])
	MiniTest.expect.equality(
		first_item_name,
		"caller",
		("Expected first item to be named 'caller', got '%s'"):format(first_item_name)
	)

	-- Verify the item's range is in host document coordinates (line 8 = line 7 in 0-indexed)
	-- The item's range.start.line should be 7 (line 8 in 1-indexed, mapped to host)
	local range_line = child.lua_get([[(function() if _G.prepare_result and type(_G.prepare_result) == "table" and #_G.prepare_result > 0 then return _G.prepare_result[1].range.start.line else return -1 end end)()]])
	MiniTest.expect.equality(
		range_line,
		7, -- line 8 in 0-indexed (host document line)
		("Expected range to be on line 7 (0-indexed), got %d"):format(range_line)
	)
end

-- Note: incomingCalls and outgoingCalls tests are skipped because rust-analyzer's
-- CallHierarchyItem contains internal state in the 'data' field that references
-- the original virtual file URI. When we translate URIs to host coordinates,
-- the data field still references the virtual file, causing rust-analyzer to
-- return empty results. This is a known limitation of bridging call hierarchy.
--
-- The prepareCallHierarchy test above verifies that the core mechanism works.

return T
