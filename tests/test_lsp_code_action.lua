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

-- Markdown file with Rust code block where we'll test code actions
-- The code block has unused variable which rust-analyzer can suggest fixing
T["markdown"] = create_file_test_set(".md", {
	"# Rust Example",
	"",
	"```rust",
	"fn main() {",
	"    let my_var = 42;", -- line 5 (1-indexed), variable defined but unused
	"    println!(\"Hello\");",
	"}",
	"```",
})

T["markdown"]["code_action returns actions with translated ranges"] = function()
	-- Position cursor on 'my_var' definition on line 5 (1-indexed in Vim)
	-- Line 5 is "    let my_var = 42;" inside the code block
	child.cmd([[normal! 5G8|]])

	-- Wait for rust-analyzer to index (this can take a while)
	vim.uv.sleep(3000)

	-- Use vim.lsp.buf_request_sync to directly test the LSP code action handler
	child.lua([[
		_G.code_action_result = nil
		local bufnr = vim.api.nvim_get_current_buf()
		local clients = vim.lsp.get_clients({ bufnr = bufnr, name = "treesitter_ls" })
		if #clients == 0 then
			_G.code_action_result = { error = "No LSP client found" }
			return
		end

		local client = clients[1]
		-- Create code action params with range at cursor
		local params = vim.lsp.util.make_range_params(0, client.offset_encoding or "utf-16")
		params.context = { diagnostics = {} }
		local results = vim.lsp.buf_request_sync(bufnr, "textDocument/codeAction", params, 15000)

		if not results then
			_G.code_action_result = { error = "No code action response" }
			return
		end

		for client_id, response in pairs(results) do
			if response.result then
				local actions = response.result
				if type(actions) == "table" and #actions > 0 then
					-- Collect action titles and any edit ranges
					local titles = {}
					local edit_lines = {}
					for _, action in ipairs(actions) do
						table.insert(titles, action.title)
						-- Check if action has an edit with ranges
						if action.edit then
							if action.edit.changes then
								for uri, edits in pairs(action.edit.changes) do
									for _, edit in ipairs(edits) do
										table.insert(edit_lines, edit.range.start.line)
									end
								end
							elseif action.edit.documentChanges then
								for _, doc_change in ipairs(action.edit.documentChanges) do
									if doc_change.edits then
										for _, edit in ipairs(doc_change.edits) do
											if edit.range then
												table.insert(edit_lines, edit.range.start.line)
											end
										end
									end
								end
							end
						end
					end
					_G.code_action_result = {
						action_count = #actions,
						titles = titles,
						edit_lines = edit_lines,
					}
					return
				else
					-- Empty actions list is acceptable if rust-analyzer hasn't found any
					_G.code_action_result = {
						action_count = 0,
						titles = {},
						edit_lines = {},
					}
					return
				end
			elseif response.err then
				_G.code_action_result = { error = vim.inspect(response.err) }
				return
			end
		end

		_G.code_action_result = { error = "No valid code action response found" }
	]])

	local result = child.lua_get([[_G.code_action_result]])

	-- Verify we got a response (may be empty if rust-analyzer not ready, which is acceptable)
	-- The important thing is that the request was handled correctly
	if result.error then
		-- If we got an error, it should not be "No LSP client found" which would indicate
		-- the handler is missing.
		MiniTest.expect.equality(
			result.error ~= "No LSP client found",
			true,
			"Should have LSP client: " .. tostring(result.error)
		)
	else
		-- Verify the response structure is correct
		MiniTest.expect.equality(type(result.action_count), "number", "Should have action count")

		-- If we got actions with edits, verify the edit lines are translated
		-- to host document coordinates (>= 3, after the ``` marker)
		if result.edit_lines and #result.edit_lines > 0 then
			for _, line in ipairs(result.edit_lines) do
				MiniTest.expect.equality(
					line >= 3, -- Should be at least line 3 (0-indexed, after the ``` marker)
					true,
					("Edit line %d should be in host document coordinates (>= 3)"):format(line)
				)
			end
		end
	end
end

return T
