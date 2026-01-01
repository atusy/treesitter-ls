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

-- Test case: Markdown with Rust code block containing error
T["markdown_diagnostic"] = create_file_test_set(".md", {
	"# Test Document",
	"",
	"```rust",
	"fn main() {",       -- line 4 (0-indexed: 3)
	"    let x = 1",     -- line 5 - missing semicolon, error on this line
	"}",
	"```",
})

T["markdown_diagnostic"]["diagnostics appear for invalid rust code"] = function()
	-- Wait for diagnostics to appear
	-- Diagnostics are sent asynchronously from rust-analyzer through treesitter-ls
	local got_diagnostics = helper.wait(30000, function()
		local diags = child.lua_get([[vim.diagnostic.get(0)]])
		return #diags > 0
	end, 100)

	if not got_diagnostics then
		-- If no diagnostics after waiting, provide debug info
		local buf_name = child.api.nvim_buf_get_name(0)
		local clients = child.lua_get([[vim.lsp.get_clients({ bufnr = 0 })]])
		MiniTest.expect.equality(true, false, 
			"No diagnostics received. Buffer: " .. buf_name .. ", Clients: " .. vim.inspect(clients))
	end

	-- Get the diagnostics
	local diagnostics = child.lua_get([[vim.diagnostic.get(0)]])
	
	-- Should have at least one diagnostic
	MiniTest.expect.equality(
		#diagnostics > 0,
		true,
		"Expected at least one diagnostic, got " .. #diagnostics
	)

	-- The diagnostic should be on the correct line (line 5 in host = line 2 in virtual)
	-- After translation, it should appear at line 5 (0-indexed: 4)
	local first_diag = diagnostics[1]
	MiniTest.expect.equality(
		first_diag.lnum >= 3 and first_diag.lnum <= 5,
		true,
		("Diagnostic should be on lines 3-5 (got line %d)"):format(first_diag.lnum)
	)
end

return T
