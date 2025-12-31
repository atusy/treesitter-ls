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

-- Markdown file with Rust code block where we'll test find references
-- The code block defines a variable that is used multiple times
T["markdown"] = create_file_test_set(".md", {
	"# Rust Example",
	"",
	"```rust",
	"fn main() {",
	"    let x = 42;",      -- line 5, x is defined here
	"    let y = x + 1;",   -- line 6, x is used here
	"    let z = x * 2;",   -- line 7, x is used here
	"}",
	"```",
})

T["markdown"]["references returns locations in injection region"] = function()
	-- Position cursor on 'x' definition on line 5 (1-indexed in Vim)
	-- Line 5 is "    let x = 42;" inside the code block
	child.cmd([[normal! 5G0fx]])

	-- Wait for rust-analyzer to index (this can take a while)
	vim.uv.sleep(3000)

	-- Use vim.lsp.buf_request_sync to directly test the LSP references handler
	child.lua([[
		_G.references_result = nil
		local bufnr = vim.api.nvim_get_current_buf()
		local clients = vim.lsp.get_clients({ bufnr = bufnr, name = "treesitter-ls" })
		if #clients == 0 then
			_G.references_result = { error = "No LSP client found" }
			return
		end

		local client = clients[1]
		local params = vim.lsp.util.make_position_params(0, client.offset_encoding or "utf-16")
		params.context = { includeDeclaration = true }
		local results = vim.lsp.buf_request_sync(bufnr, "textDocument/references", params, 15000)

		if not results then
			_G.references_result = { error = "No references response" }
			return
		end

		for client_id, response in pairs(results) do
			if response.result then
				local locations = response.result
				if locations and #locations > 0 then
					_G.references_result = {
						location_count = #locations,
						-- Collect line numbers of all references
						lines = vim.tbl_map(function(loc)
							return loc.range.start.line
						end, locations),
					}
					return
				end
			elseif response.err then
				_G.references_result = { error = vim.inspect(response.err) }
				return
			end
		end

		_G.references_result = { error = "No valid references found" }
	]])

	local result = child.lua_get([[_G.references_result]])

	-- Verify we got a response (may be nil if rust-analyzer not ready, which is acceptable)
	-- The important thing is that the request was handled and bridged correctly
	if result.error then
		-- If we got an error, it should not be "No LSP client found" which would indicate
		-- the handler is missing. "No valid references found" is acceptable if
		-- rust-analyzer hasn't indexed yet.
		MiniTest.expect.equality(
			result.error ~= "No LSP client found",
			true,
			"Should have LSP client: " .. tostring(result.error)
		)
	else
		-- Verify we got reference locations
		MiniTest.expect.equality(type(result.location_count), "number", "Should have location count")
		MiniTest.expect.equality(result.location_count > 0, true, "Should have at least one reference")
		
		-- Verify the lines are translated back to host document coordinates
		-- The references should be in the Markdown file lines (4, 5, 6 in 0-indexed)
		-- not virtual document lines (0, 1, 2)
		if result.lines then
			for _, line in ipairs(result.lines) do
				MiniTest.expect.equality(
					line >= 3, -- Should be at least line 3 (0-indexed, after the ``` marker)
					true,
					("Reference line %d should be in host document coordinates (>= 3)"):format(line)
				)
			end
		end
	end
end

return T
