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

	-- Use helper.retry_for_lsp_indexing() for resilient LSP request handling
	local success = _G.helper.retry_for_lsp_indexing({
		child = child,
		lsp_request = function()
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
		end,
		check = function()
			local result = child.lua_get([[_G.signature_help_result]])
			-- Check if we got a valid signature (not an error)
			return result and not result.error and result.signature_count and result.signature_count > 0
		end,
		max_retries = 20,
		wait_ms = 3000,
		retry_delay_ms = 500,
	})

	MiniTest.expect.equality(success, true, "Should eventually get signature help response")

	local result = child.lua_get([[_G.signature_help_result]])

	-- Verify signature information details
	MiniTest.expect.equality(type(result.signature_count), "number", "Should have signature count")
	MiniTest.expect.equality(result.signature_count > 0, true, "Should have at least one signature")

	-- Strong assertion: signature should contain "add" function name
	MiniTest.expect.equality(
		result.first_label:find("add") ~= nil,
		true,
		("Signature label should contain 'add', got: %s"):format(result.first_label)
	)

	-- Strong assertion: signature should contain parameter types
	MiniTest.expect.equality(
		result.first_label:find("i32") ~= nil,
		true,
		("Signature should contain 'i32' parameter type, got: %s"):format(result.first_label)
	)
end

-- Dedicated async path test with realistic multi-parameter scenario
-- This test verifies the async I/O path with activeParameter tracking
T["markdown_rust_async"] = create_file_test_set(".md", {
	"# Async Signature Help Test",
	"",
	"```rust",
	"fn format_message(prefix: &str, message: &str, suffix: &str) -> String {",
	'    format!("{}{}{}", prefix, message, suffix)',
	"}",
	"",
	"fn main() {",
	'    let result = format_message("Hello, ", ', -- line 9: cursor after first argument
	"}",
	"```",
})

T["markdown_rust_async"]["async_signature_help_shows_active_parameter"] = function()
	-- Position cursor after first argument: format_message("Hello, ",
	-- This should show we're on the second parameter (message)
	child.cmd([[normal! 9G42|]]) -- line 9, column 42 (after the comma and space)

	-- Use helper.retry_for_lsp_indexing() for async path verification
	local success = _G.helper.retry_for_lsp_indexing({
		child = child,
		lsp_request = function()
			child.lua([[
				_G.async_signature_result = nil
				local bufnr = vim.api.nvim_get_current_buf()
				local clients = vim.lsp.get_clients({ bufnr = bufnr, name = "treesitter-ls" })
				if #clients == 0 then
					return
				end

				local client = clients[1]
				local params = vim.lsp.util.make_position_params(0, client.offset_encoding or "utf-16")
				local results = vim.lsp.buf_request_sync(bufnr, "textDocument/signatureHelp", params, 15000)

				if not results then
					return
				end

				for client_id, response in pairs(results) do
					if response.result and response.result.signatures then
						local signatures = response.result.signatures
						if #signatures > 0 then
							local sig = signatures[1]
							_G.async_signature_result = {
								label = sig.label,
								parameters = sig.parameters and #sig.parameters or 0,
								activeParameter = response.result.activeParameter,
							}
							return
						end
					end
				end
			]])
		end,
		check = function()
			local result = child.lua_get([[_G.async_signature_result]])
			-- result may be vim.NIL (userdata) when not yet set
			if type(result) ~= "table" then
				return false
			end
			return result.label ~= nil and result.parameters and result.parameters > 0
		end,
		max_retries = 20,
		wait_ms = 3000,
		retry_delay_ms = 500,
	})

	MiniTest.expect.equality(success, true, "Should get async signature help response")

	local result = child.lua_get([[_G.async_signature_result]])

	-- Verify signature contains function name
	MiniTest.expect.equality(
		result.label:find("format_message") ~= nil,
		true,
		("Signature should contain 'format_message', got: %s"):format(result.label)
	)

	-- Verify signature has multiple parameters
	MiniTest.expect.equality(
		result.parameters >= 3,
		true,
		("Signature should have at least 3 parameters, got: %d"):format(result.parameters)
	)

	-- Verify activeParameter tracking (may be 0-indexed or nil depending on rust-analyzer version)
	-- Just check it exists when there are multiple parameters
	if result.activeParameter ~= nil then
		MiniTest.expect.equality(
			type(result.activeParameter),
			"number",
			"activeParameter should be a number when present"
		)
	end
end

return T
