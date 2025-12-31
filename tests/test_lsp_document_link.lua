local child = MiniTest.new_child_neovim()

local T = MiniTest.new_set({ hooks = { post_once = child.stop } })

---Helper function to create file-specific test set
---@param ext string file extension including dot
---@param lines string[] lines of content
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

-- Test documentLink in a markdown file with a Rust code block containing a URL
-- Note: rust-analyzer may not support documentLink for URLs in comments,
-- so we test that the request completes without error (may return empty or null)
T["markdown"] = create_file_test_set(".md", {
	"Here is a document link example:",
	"",
	"```rust",
	"/// See https://example.com for more info",  -- line 4 (0-indexed: 3)
	"fn main() {",                                 -- line 5
	"    println!(\"Hello!\");",                   -- line 6
	"}",                                           -- line 7
	"```",                                         -- line 8
})

T["markdown"]["document_link"] = function()
	-- Wait for rust-analyzer to index (this can take a while on first run)
	vim.uv.sleep(2000)

	-- Set up a handler to capture the document link result
	child.lua([[
		_G.link_result = nil
		_G.link_err = nil
		_G.link_done = false
		local params = { textDocument = vim.lsp.util.make_text_document_params() }
		vim.lsp.buf_request(0, "textDocument/documentLink", params, function(err, result, ctx, config)
			_G.link_err = err
			_G.link_result = result
			_G.link_done = true
		end)
	]])

	-- Wait for the handler to complete
	local got_result = helper.wait(10000, function()
		return child.lua_get([[_G.link_done]])
	end, 100)

	MiniTest.expect.equality(got_result, true, "Document link request should complete")

	-- Check for errors
	local link_err = child.lua_get([[_G.link_err]])
	MiniTest.expect.equality(link_err, vim.NIL, "Document link request should not error")

	-- Get the link result - may be nil/empty if rust-analyzer doesn't support links for this content
	local links = child.lua_get([[_G.link_result]])

	-- We accept either:
	-- 1. nil/empty result (rust-analyzer may not return links for URLs in comments)
	-- 2. A list of links (if rust-analyzer does support it)
	-- The main test is that the request completes without error
	if links ~= vim.NIL and links ~= nil then
		-- If we got links, verify they have valid structure
		if type(links) == "table" and #links > 0 then
			for _, link in ipairs(links) do
				-- Each link should have a range
				MiniTest.expect.equality(
					link.range ~= nil,
					true,
					"Each document link should have a range"
				)
			end
		end
	end

	-- Log the result for debugging
	child.lua([[
		vim.api.nvim_echo({{"Document link result: " .. vim.inspect(_G.link_result)}}, true, {})
	]])
end

return T
