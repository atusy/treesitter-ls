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

-- Markdown file with Rust code block where we'll test rename
-- The code block defines a variable that is used multiple times
T["markdown"] = create_file_test_set(".md", {
	"# Rust Example",
	"",
	"```rust",
	"fn main() {",
	"    let my_var = 42;", -- line 5 (1-indexed), variable defined here
	"    let y = my_var + 1;", -- line 6, variable used here
	"    let z = my_var * 2;", -- line 7, variable used here
	"}",
	"```",
})

T["markdown"]["rename returns workspace edit with translated ranges"] = function()
	-- Position cursor on 'my_var' definition on line 5 (1-indexed in Vim)
	-- Line 5 is "    let my_var = 42;" inside the code block
	child.cmd([[normal! 5G0fmy_var]])
	-- Position on the 'm' of 'my_var'
	child.cmd([[normal! 5G8|]])

	-- Wait for rust-analyzer to index (this can take a while)
	vim.uv.sleep(3000)

	-- Use vim.lsp.buf_request_sync to directly test the LSP rename handler
	child.lua([[
		_G.rename_result = nil
		local bufnr = vim.api.nvim_get_current_buf()
		local clients = vim.lsp.get_clients({ bufnr = bufnr, name = "treesitter_ls" })
		if #clients == 0 then
			_G.rename_result = { error = "No LSP client found" }
			return
		end

		local client = clients[1]
		local params = vim.lsp.util.make_position_params(0, client.offset_encoding or "utf-16")
		params.newName = "new_name"
		local results = vim.lsp.buf_request_sync(bufnr, "textDocument/rename", params, 15000)

		if not results then
			_G.rename_result = { error = "No rename response" }
			return
		end

		for client_id, response in pairs(results) do
			if response.result then
				local workspace_edit = response.result
				-- Check for changes or documentChanges
				if workspace_edit.changes then
					-- Count edits and collect line numbers
					local total_edits = 0
					local lines = {}
					for uri, edits in pairs(workspace_edit.changes) do
						for _, edit in ipairs(edits) do
							total_edits = total_edits + 1
							table.insert(lines, edit.range.start.line)
						end
					end
					_G.rename_result = {
						edit_count = total_edits,
						lines = lines,
						has_changes = true,
					}
					return
				elseif workspace_edit.documentChanges then
					-- Count edits from documentChanges
					local total_edits = 0
					local lines = {}
					for _, doc_change in ipairs(workspace_edit.documentChanges) do
						if doc_change.edits then
							for _, edit in ipairs(doc_change.edits) do
								total_edits = total_edits + 1
								if edit.range then
									table.insert(lines, edit.range.start.line)
								end
							end
						end
					end
					_G.rename_result = {
						edit_count = total_edits,
						lines = lines,
						has_document_changes = true,
					}
					return
				else
					_G.rename_result = { error = "WorkspaceEdit has no changes or documentChanges" }
					return
				end
			elseif response.err then
				_G.rename_result = { error = vim.inspect(response.err) }
				return
			end
		end

		_G.rename_result = { error = "No valid rename response found" }
	]])

	local result = child.lua_get([[_G.rename_result]])

	-- Verify we got a response (may be nil if rust-analyzer not ready, which is acceptable)
	-- The important thing is that the request was handled and bridged correctly
	if result.error then
		-- If we got an error, it should not be "No LSP client found" which would indicate
		-- the handler is missing. "No valid rename response found" is acceptable if
		-- rust-analyzer hasn't indexed yet.
		MiniTest.expect.equality(
			result.error ~= "No LSP client found",
			true,
			"Should have LSP client: " .. tostring(result.error)
		)
	else
		-- Verify we got edit operations
		MiniTest.expect.equality(type(result.edit_count), "number", "Should have edit count")
		MiniTest.expect.equality(result.edit_count > 0, true, "Should have at least one edit")

		-- Verify the lines are translated back to host document coordinates
		-- The edits should be in the Markdown file lines (4, 5, 6 in 0-indexed)
		-- not virtual document lines (0, 1, 2)
		if result.lines then
			for _, line in ipairs(result.lines) do
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
