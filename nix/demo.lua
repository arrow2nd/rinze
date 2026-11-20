local work = assert(os.getenv('RINZE_DEMO_WORK'), 'rinze-demo から起動する')

vim.o.laststatus = 2
vim.o.statusline = "%f%=%{v:lua.require'rinze'.status()} "

require('blink.cmp').setup({
  keymap = { preset = 'default' },
  fuzzy = { implementation = 'prefer_rust' },
  sources = { default = { 'buffer' } },
})

require('rinze').setup({
  cmd = { 'rinzed', '--user-dict', work .. '/user.jisyo', '--history', work .. '/history.tsv' },
  socket = work .. '/rinze.sock',
  blink = true,
})
vim.keymap.set('i', '<C-l>', function()
  require('rinze').toggle()
end)

vim.api.nvim_buf_set_lines(0, 0, -1, false, {
  '-- rinze の試用。終了すると一時ディレクトリごと消える',
  '-- Asita でメニューに候補、Space で変換、C-y でメニューの候補を確定、C-l で rinze のオンオフ',
  '',
})
vim.api.nvim_win_set_cursor(0, { 3, 0 })
vim.cmd('RinzeEnable')
vim.cmd.startinsert()
