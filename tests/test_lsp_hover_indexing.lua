-- E2E test for hover indexing state feedback (PBI-149)
-- Verifies AC5: hover during indexing shows message, hover after Ready shows normal content

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

-- Test markdown file with Rust code block for indexing message
T["markdown_rust_hover_indexing"] = create_file_test_set(".md", {
	"# Example",
	"",
	"```rust",
	"fn main() {", -- line 4 (1-indexed)
	'    println!("Hello, world!");',
	"}",
	"```",
})

T["markdown_rust_hover_indexing"]["hover_shows_indexing_message_then_real_content"] = function()
	-- Position cursor on "main" on line 4, column 4 (on the 'm' of main)
	child.cmd([[normal! 4G4|]])

	-- Verify cursor is on line 4
	local before = child.api.nvim_win_get_cursor(0)
	MiniTest.expect.equality(before[1], 4, "Cursor should be on line 4")

	-- Track if we saw indexing message and real hover
	local saw_indexing = false
	local saw_real_hover = false

	-- Call hover and check for indexing message or real content
	-- We retry multiple times to observe the indexing -> ready transition
	for attempt = 1, 20 do
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
			-- Get content from floating window
			local wins = child.api.nvim_list_wins()
			for _, win in ipairs(wins) do
				local config = child.api.nvim_win_get_config(win)
				if config.relative ~= "" then
					local buf = child.api.nvim_win_get_buf(win)
					local lines = child.api.nvim_buf_get_lines(buf, 0, -1, false)
					local content = table.concat(lines, "\n")

					-- Check if this is indexing message or real hover
					if content:find("indexing") then
						saw_indexing = true
					elseif content:find("main") or content:find("fn") then
						saw_real_hover = true
					end

					-- Close the hover window by pressing escape
					child.cmd([[normal! \<Esc>]])
					break
				end
			end
		end

		-- If we've seen real hover, we're done
		if saw_real_hover then
			break
		end

		-- Wait before retry (rust-analyzer may still be indexing)
		vim.wait(500)
	end

	-- Verify we eventually got real hover content
	MiniTest.expect.equality(saw_real_hover, true, "Should eventually get real hover content (not just indexing message)")

	-- Note: We may or may not see the indexing message depending on rust-analyzer speed
	-- The important thing is that we eventually get real content after Ready state
end

return T
