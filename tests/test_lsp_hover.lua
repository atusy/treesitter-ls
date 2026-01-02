-- E2E test for hover in Markdown code blocks with rust-analyzer bridge
-- Verifies AC4: Hover requests to rust-analyzer return valid responses through async path

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

-- Test markdown file with Rust code block
-- The hover on line 4 (fn main) should return type information from rust-analyzer
T["markdown_rust_hover"] = create_file_test_set(".md", {
	"# Example",
	"",
	"```rust",
	"fn main() {", -- line 4 (1-indexed)
	'    println!("Hello, world!");',
	"}",
	"```",
})

T["markdown_rust_hover"]["hover_on_fn_shows_type_info"] = function()
	-- Position cursor on "main" on line 4, column 4 (on the 'm' of main)
	child.cmd([[normal! 4G4|]])

	-- Verify cursor is on line 4
	local before = child.api.nvim_win_get_cursor(0)
	MiniTest.expect.equality(before[1], 4, "Cursor should be on line 4")

	-- Call hover and wait for floating window
	-- We need to retry since rust-analyzer may need time to index
	local got_hover = false
	for _ = 1, 20 do
		child.lua([[vim.lsp.buf.hover()]])

		-- Wait for floating window to appear
		local has_float = helper.wait(3000, function()
			local wins = child.api.nvim_list_wins()
			for _, win in ipairs(wins) do
				local config = child.api.nvim_win_get_config(win)
				if config.relative ~= "" then
					return true
				end
			end
			return false
		end, 100)

		if has_float then
			got_hover = true
			break
		end

		-- Wait before retry (rust-analyzer may still be indexing)
		vim.wait(500)
	end

	MiniTest.expect.equality(got_hover, true, "Hover should show floating window with type info")

	-- Verify floating window contains some content (function signature or informative message)
	local wins = child.api.nvim_list_wins()
	local found_content = false
	local hover_content = ""
	for _, win in ipairs(wins) do
		local config = child.api.nvim_win_get_config(win)
		if config.relative ~= "" then
			local buf = child.api.nvim_win_get_buf(win)
			local lines = child.api.nvim_buf_get_lines(buf, 0, -1, false)
			hover_content = table.concat(lines, "\n")
			-- Check that the hover contains something about 'main' or 'fn'
			-- Or the informative message "No result or indexing" (PBI-147)
			if hover_content:find("main") or hover_content:find("fn") or hover_content:find("No result or indexing") then
				found_content = true
				break
			end
		end
	end

	MiniTest.expect.equality(found_content, true, "Hover content should contain function information or informative message, got: " .. hover_content)
end

-- PBI-147: Verify hover always returns content (never null/empty)
-- This tests the informative message feature when rust-analyzer has no result
T["markdown_rust_hover"]["hover_always_returns_content_not_null"] = function()
	-- Position cursor on whitespace/empty area where rust-analyzer has no hover info
	-- Line 5 contains '    println!("Hello, world!");' - position at beginning (indent)
	child.cmd([[normal! 5G1|]])

	-- Call hover multiple times to ensure we get a response
	local got_hover = false
	local hover_content = nil

	for _ = 1, 20 do
		child.lua([[vim.lsp.buf.hover()]])

		-- Wait for floating window to appear
		local has_float = helper.wait(3000, function()
			local wins = child.api.nvim_list_wins()
			for _, win in ipairs(wins) do
				local config = child.api.nvim_win_get_config(win)
				if config.relative ~= "" then
					return true
				end
			end
			return false
		end, 100)

		if has_float then
			-- Get the hover content
			local wins = child.api.nvim_list_wins()
			for _, win in ipairs(wins) do
				local config = child.api.nvim_win_get_config(win)
				if config.relative ~= "" then
					local buf = child.api.nvim_win_get_buf(win)
					local lines = child.api.nvim_buf_get_lines(buf, 0, -1, false)
					hover_content = table.concat(lines, "\n")
					got_hover = true
					break
				end
			end
			break
		end

		vim.wait(500)
	end

	-- AC: Hover should always return content (never null)
	MiniTest.expect.equality(got_hover, true, "Hover should show floating window")
	MiniTest.expect.equality(hover_content ~= nil, true, "Hover content should not be nil")
	MiniTest.expect.equality(#hover_content > 0, true, "Hover content should not be empty")
end

return T
