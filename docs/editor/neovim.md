# Neovim setup

Neovim 0.11 or newer can use its built-in LSP client and completion menu for
`.langmeld` files. Add this to your Lua configuration, replacing the command
with an absolute executable path if `langmeld` is not on Neovim's `PATH`:

```lua
vim.filetype.add({ extension = { langmeld = "langmeld" } })

vim.lsp.config("langmeld-definitions", {
  cmd = { "langmeld", "lsp", "--definitions" },
  filetypes = { "langmeld" },
  root_markers = { "langmeld.toml" },
  on_attach = function(client, bufnr)
    vim.lsp.completion.enable(true, client.id, bufnr)
    vim.keymap.set("i", "<C-Space>", function()
      vim.lsp.completion.get()
    end, { buffer = bufnr, desc = "Langmeld completion" })
  end,
})

vim.lsp.enable("langmeld-definitions")
```

Start Neovim from the language project's root (for example, `nvim .`) so the
server finds `langmeld.toml` and enables project validation and imported-name
completion. Opening a `.langmeld` file without a project manifest still gives
the built-in syntax diagnostics, completion, and formatting. Use `<C-Space>`
to open the completion menu and `<C-y>` to accept a selected item. See the
[Neovim LSP guide](https://neovim.io/doc/user/lsp.html) for client setup and
completion details.
