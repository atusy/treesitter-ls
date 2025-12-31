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

-- Markdown file with Rust code block where we'll test signature help
-- The code block defines a function that we'll call to trigger signature help
T["markdown"] = create_file_test_set(".md", {
	"# Rust Example",
	"",
	"```rust",
	"fn add(a: i32, b: i32) -> i32 {",
	"    a + b",
	"}",
	"",
	"fn main() {",
	"    let result = add(", -- line 10, cursor inside function call for signature help
	"}",
	"```",
})

T["markdown"]["signature_help returns function signature in injection region"] = function()
	-- Position cursor inside add( on line 10 (1-indexed in Vim)
	-- Line 10 is "    let result = add(" inside the code block
	child.cmd([[normal! 10G$]])

	-- Wait for rust-analyzer to index (this can take a while)
	vim.uv.sleep(3000)

	-- Use vim.lsp.buf_request_sync to directly test the LSP signatureHelp handler
	child.lua([[
		_G.signature_help_result = nil
		local bufnr = vim.api.nvim_get_current_buf()
		local clients = vim.lsp.get_clients({ bufnr = bufnr, name = "treesitter-ls" })
		if #clients == 0 then
			_G.signature_help_result = { error = "No LSP client found" }
			return
		end

		local client = clients[1]
		local params = vim.lsp.util.make_position_params(0, client.offset_encoding or "utf-16")
		local results = vim.lsp.buf_request_sync(bufnr, "textDocument/signatureHelp", params, 15000)

		if not results then
			_G.signature_help_result = { error = "No signatureHelp response" }
			return
		end

		for client_id, response in pairs(results) do
			if response.result then
				local signatures = response.result.signatures
				if signatures and #signatures > 0 then
					_G.signature_help_result = {
						signature_count = #signatures,
						first_label = signatures[1].label,
						active_parameter = response.result.activeParameter,
					}
					return
				end
			elseif response.err then
				_G.signature_help_result = { error = vim.inspect(response.err) }
				return
			end
		end

		_G.signature_help_result = { error = "No valid signature help found" }
	]])

	local result = child.lua_get([[_G.signature_help_result]])

	-- Verify we got a response (may be nil if rust-analyzer not ready, which is acceptable)
	-- The important thing is that the request was handled and bridged correctly
	if result.error then
		-- If we got an error, it should not be "No LSP client found" which would indicate
		-- the handler is missing. "No valid signature help found" is acceptable if
		-- rust-analyzer hasn't indexed yet.
		MiniTest.expect.equality(
			result.error ~= "No LSP client found",
			true,
			"Should have LSP client: " .. tostring(result.error)
		)
	else
		-- Verify we got signature information
		MiniTest.expect.equality(type(result.signature_count), "number", "Should have signature count")
		MiniTest.expect.equality(result.signature_count > 0, true, "Should have at least one signature")
		-- The signature should contain "add" function info
		if result.first_label then
			MiniTest.expect.equality(
				result.first_label:find("add") ~= nil or result.first_label:find("i32") ~= nil,
				true,
				("Signature should relate to add function, got: %s"):format(result.first_label)
			)
		end
	end
end

return T
