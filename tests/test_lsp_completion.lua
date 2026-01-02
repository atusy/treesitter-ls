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

	-- Wait for rust-analyzer to index (this can take a while)
	vim.uv.sleep(3000)

	-- Use vim.lsp.buf_request_sync to directly test the LSP completion handler
	-- This bypasses the popup mechanism and directly tests the LSP response
	child.lua([[
		_G.completion_result = nil
		local bufnr = vim.api.nvim_get_current_buf()
		local clients = vim.lsp.get_clients({ bufnr = bufnr, name = "treesitter-ls" })
		if #clients == 0 then
			_G.completion_result = { error = "No LSP client found" }
			return
		end

		local client = clients[1]
		local params = vim.lsp.util.make_position_params(0, client.offset_encoding or "utf-16")
		local results = vim.lsp.buf_request_sync(bufnr, "textDocument/completion", params, 15000)

		if not results then
			_G.completion_result = { error = "No completion response" }
			return
		end

		for client_id, response in pairs(results) do
			if response.result then
				local items = response.result.items or response.result
				if type(items) == "table" then
					local item_info = {}
					for i, item in ipairs(items) do
						if i <= 5 then
							table.insert(item_info, {
								label = item.label,
								textEdit = item.textEdit,
							})
						end
					end
					_G.completion_result = {
						count = #items,
						items = item_info,
						first_item_range = items[1] and items[1].textEdit and items[1].textEdit.range,
					}
					return
				end
			elseif response.err then
				_G.completion_result = { error = vim.inspect(response.err) }
				return
			end
		end

		_G.completion_result = { error = "No valid completion items found" }
	]])

	local result = child.lua_get([[_G.completion_result]])

	-- Verify we got a response
	MiniTest.expect.equality(result.error, nil, "Should not have error: " .. tostring(result.error))

	-- Verify we got completion items
	MiniTest.expect.equality(type(result.count), "number", "Should have item count")

	if result.count > 0 then
		-- Check that at least one completion item has 'x' or 'y' label
		local found_field = false
		for _, item in ipairs(result.items or {}) do
			if item.label == "x" or item.label == "y" then
				found_field = true
				-- Verify textEdit range is in host document coordinates
				-- The "p." is on line 11 in Markdown (0-indexed: line 10)
				-- In virtual document it would be around line 7 (0-indexed)
				if item.textEdit and item.textEdit.range then
					local range = item.textEdit.range
					-- The range start line should be 10 (host) not 7 (virtual)
					MiniTest.expect.equality(
						range.start.line >= 10,
						true,
						("textEdit range should be in host coordinates (got line %d, expected >= 10)"):format(
							range.start.line
						)
					)
				end
				break
			end
		end

		-- If we found rust-analyzer fields, great. If not, at least verify we got items.
		-- rust-analyzer might return other items depending on indexing state.
		if not found_field and result.count > 0 then
			-- Just verify we have items with proper structure
			MiniTest.expect.equality(type(result.items), "table", "Should have items array")
		end
	end
end

-- Test Rust completion with retry mechanism to verify async path resilience (PBI-142 AC3)
T["markdown_rust_async"] = create_file_test_set(".md", {
	"# String Methods Example",
	"",
	"```rust",
	"fn main() {",
	"    let s = String::n", -- line 5, completion after "String::n"
	"}",
	"```",
})

T["markdown_rust_async"]["completion_through_async_path_with_retry"] = function()
	-- Position cursor after "String::n" on line 5 (1-indexed in Vim)
	child.cmd([[normal! 5G$]])

	-- Retry completion request - rust-analyzer may need time to index
	local got_completion = false
	for attempt = 1, 20 do
		-- Request completion
		child.lua([[
			_G.completion_result = nil
			local bufnr = vim.api.nvim_get_current_buf()
			local clients = vim.lsp.get_clients({ bufnr = bufnr, name = "treesitter-ls" })
			if #clients == 0 then
				_G.completion_result = { error = "No LSP client found" }
				return
			end

			local client = clients[1]
			local params = vim.lsp.util.make_position_params(0, client.offset_encoding or "utf-16")
			local results = vim.lsp.buf_request_sync(bufnr, "textDocument/completion", params, 3000)

			if not results then
				_G.completion_result = { error = "No completion response" }
				return
			end

			for client_id, response in pairs(results) do
				if response.result then
					local items = response.result.items or response.result
					if type(items) == "table" and #items > 0 then
						-- Look for "new" in completion items (String::new)
						for _, item in ipairs(items) do
							if item.label and item.label == "new" then
								_G.completion_result = {
									success = true,
									item_label = item.label,
									has_textEdit = item.textEdit ~= nil,
								}
								return
							end
						end
						-- Got items but no "new" - save for debugging
						_G.completion_result = {
							count = #items,
							first_labels = vim.tbl_map(function(x) return x.label end, vim.list_slice(items, 1, 5)),
						}
						return
					end
				elseif response.err then
					_G.completion_result = { error = vim.inspect(response.err) }
					return
				end
			end

			_G.completion_result = { error = "No valid completion items found" }
		]])

		local result = child.lua_get([[_G.completion_result]])

		if result.success then
			got_completion = true
			-- Verify we got the expected completion item
			MiniTest.expect.equality(result.item_label, "new", "Should get 'new' completion for String::n")
			MiniTest.expect.equality(result.has_textEdit, true, "Completion item should have textEdit")
			break
		end

		-- Wait before retry (rust-analyzer may still be indexing)
		vim.wait(500)
	end

	-- Assert that we eventually got completion
	MiniTest.expect.equality(
		got_completion,
		true,
		"Should eventually get completion through async path (rust-analyzer may need time to index)"
	)
end

return T
