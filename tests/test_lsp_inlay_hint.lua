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
	"Here is an inlay hint example:",
	"",
	"```rust",
	"fn main() {",           -- line 4
	"    let x = 1;",        -- line 5 - should show `: i32` hint
	"    let y = 2.5;",      -- line 6 - should show `: f64` hint
	"    let z = \"hello\";",-- line 7 - should show `: &str` hint
	"}",                     -- line 8
	"```",                   -- line 9
})
T["markdown"]["inlay_hint"] = function()
	-- Wait for rust-analyzer to index (this can take a while on first run)
	vim.uv.sleep(2000)

	-- Set up a handler to capture the inlay hint result
	-- Request inlay hints for the code block range (lines 4-8)
	child.lua([[
		_G.inlay_result = nil
		_G.inlay_done = false
		local bufnr = vim.api.nvim_get_current_buf()
		local params = {
			textDocument = vim.lsp.util.make_text_document_params(bufnr),
			range = {
				start = { line = 3, character = 0 },  -- line 4 (0-indexed)
				["end"] = { line = 8, character = 0 } -- line 9 (0-indexed)
			}
		}
		vim.lsp.buf_request(0, "textDocument/inlayHint", params, function(err, result, ctx, config)
			if result then
				_G.inlay_result = result
			end
			_G.inlay_done = true
		end)
	]])

	-- Wait for the handler to complete
	local got_result = helper.wait(10000, function()
		return child.lua_get([[_G.inlay_done]])
	end, 100)

	MiniTest.expect.equality(got_result, true, "Inlay hint request should complete")

	-- Get the inlay hint result
	-- Use type check to handle vim.NIL (userdata) case
	local hint_count = child.lua_get([[(function() if _G.inlay_result and type(_G.inlay_result) == "table" then return #_G.inlay_result else return 0 end end)()]])

	-- We expect at least 1 inlay hint (type hints for the let bindings)
	-- Note: rust-analyzer may return more or fewer depending on configuration
	MiniTest.expect.equality(
		hint_count >= 1,
		true,
		("Expected at least 1 inlay hint, got %d"):format(hint_count)
	)

	-- Verify that all hints are on expected lines (lines 4-7 in LSP 0-indexed)
	-- - line 4 (let x = 1;) in the code block = line 4 in host document (0-indexed)
	-- - line 5 (let y = 2.5;) = line 5 in host
	-- - line 6 (let z = "hello";) = line 6 in host
	local valid_lines_result = child.lua_get([[(function() local valid = { [4] = true, [5] = true, [6] = true }; local all_valid = true; local invalid_lines = {}; if _G.inlay_result and type(_G.inlay_result) == "table" then for _, hint in ipairs(_G.inlay_result) do local line = hint.position.line; if not valid[line] then all_valid = false; table.insert(invalid_lines, line) end end end; return { all_valid = all_valid, invalid_lines = invalid_lines } end)()]])

	MiniTest.expect.equality(
		valid_lines_result.all_valid,
		true,
		("Inlay hint on unexpected lines: %s"):format(vim.inspect(valid_lines_result.invalid_lines))
	)
end

return T
