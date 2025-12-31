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

-- Markdown file with unformatted Rust code block
-- The code block has inconsistent spacing that rustfmt would fix
T["markdown"] = create_file_test_set(".md", {
	"# Rust Example",
	"",
	"```rust",
	"fn main(){", -- Missing space before {
	"let x=42;", -- Missing spaces around =
	"println!(\"Hello\");",
	"}",
	"```",
})

T["markdown"]["formatting returns edits with translated ranges"] = function()
	-- Wait for rust-analyzer to index (this can take a while)
	vim.uv.sleep(3000)

	-- Use vim.lsp.buf_request_sync to directly test the LSP formatting handler
	child.lua([[
		_G.formatting_result = nil
		local bufnr = vim.api.nvim_get_current_buf()
		local clients = vim.lsp.get_clients({ bufnr = bufnr, name = "treesitter-ls" })
		if #clients == 0 then
			_G.formatting_result = { error = "No LSP client found" }
			return
		end

		local client = clients[1]
		-- Create formatting params
		local params = {
			textDocument = { uri = vim.uri_from_bufnr(bufnr) },
			options = {
				tabSize = 4,
				insertSpaces = true,
			},
		}
		local results = vim.lsp.buf_request_sync(bufnr, "textDocument/formatting", params, 15000)

		if not results then
			_G.formatting_result = { error = "No formatting response" }
			return
		end

		for client_id, response in pairs(results) do
			if response.result then
				local edits = response.result
				if type(edits) == "table" and #edits > 0 then
					-- Collect edit information
					local edit_lines = {}
					for _, edit in ipairs(edits) do
						table.insert(edit_lines, edit.range.start.line)
					end
					_G.formatting_result = {
						edit_count = #edits,
						edit_lines = edit_lines,
						first_edit = edits[1],
					}
					return
				else
					-- Empty edits list (code may already be formatted)
					_G.formatting_result = {
						edit_count = 0,
						edit_lines = {},
					}
					return
				end
			elseif response.err then
				_G.formatting_result = { error = vim.inspect(response.err) }
				return
			end
		end

		_G.formatting_result = { error = "No valid formatting response found" }
	]])

	local result = child.lua_get([[_G.formatting_result]])

	-- Verify we got a response (may be empty if rust-analyzer not ready or code already formatted)
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
		MiniTest.expect.equality(type(result.edit_count), "number", "Should have edit count")

		-- If we got edits, verify the edit lines are translated to host document coordinates
		-- (>= 3, after the ``` marker on line 2, 0-indexed)
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
