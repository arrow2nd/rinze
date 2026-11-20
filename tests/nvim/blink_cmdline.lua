-- blink.cmp のコマンドラインの補完は vim.on_key と vim.schedule で動くので、nvim_input で別の Neovim に打鍵を送って確かめる
local rinzed, model, dict, socket, user_dict, history, blink_path = unpack(arg)
local root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h:h')
local cmd = { rinzed, '--model', model, '--dict', dict, '--user-dict', user_dict, '--history', history }

local failures = 0
local function check(label, got, want)
  local ok = vim.deep_equal(got, want)
  print(string.format('%s %s: %s', ok and 'ok  ' or 'FAIL', label, vim.inspect(got, { newline = ' ', indent = '' })))
  if not ok then
    print('     期待: ' .. vim.inspect(want, { newline = ' ', indent = '' }))
    failures = failures + 1
  end
end

local child = vim.fn.jobstart({ 'nvim', '--embed', '--headless', '--clean' }, { rpc = true })
local function lua(code, ...)
  return vim.rpcrequest(child, 'nvim_exec_lua', code, { ... })
end

lua(
  [[
local root, blink_path, cmd, socket = ...
vim.opt.runtimepath:prepend(root .. '/nvim')
vim.opt.runtimepath:prepend(blink_path)
vim.o.swapfile = false
vim.cmd.runtime('plugin/rinze.lua')
require('blink.cmp').setup({
  fuzzy = { implementation = 'lua' },
  cmdline = {
    sources = function()
      return vim.fn.getcmdtype() == ':' and { 'cmdline', 'buffer' } or { 'buffer' }
    end,
    completion = { menu = { auto_show = true } },
  },
})
local rinze = require('rinze')
rinze.setup({ cmd = cmd, socket = socket, blink = true })
vim.keymap.set('c', '<C-j>', rinze.enable)
vim.keymap.set('c', '<C-l>', rinze.disable)
vim.api.nvim_buf_set_lines(0, 0, -1, false, { 'echoes' })
_G.T = {}
function T.menu()
  local cmp = require('blink.cmp')
  if not cmp.is_menu_visible() then
    return false
  end
  return vim.tbl_map(function(item)
    return item.label
  end, cmp.get_items())
end
]],
  root,
  blink_path,
  cmd,
  socket
)

-- blink.cmp は打鍵を vim.schedule で後回しに処理するので、キーごとに少し待つ
local function input(keys)
  vim.rpcrequest(child, 'nvim_input', keys)
  vim.wait(300)
end

local function menu_after(keys)
  for _, key in ipairs(keys) do
    input(key)
  end
  local menu = lua('return T.menu()')
  local preedit = lua('return require("rinze").preedit()')
  local mode = lua('return require("rinze").mode()')
  input('<Esc>')
  return menu, preedit, mode
end

local function blink_cmdline_enabled()
  return lua('return require("blink.cmp.config").cmdline.enabled')
end

check('rinze を使わない検索では blink.cmp のコマンドラインのメニューが出る', menu_after({ '/ec' }), { 'echoes' })
local menu, preedit = menu_after({ '/ec', '<C-j>', 'K', 'a' })
check('コマンドラインで rinze が有効な間は、打鍵に反応して blink.cmp のメニューを出さない', { preedit, menu }, { '▽か', false })
check(': でも同じ', ({ menu_after({ ':ec', '<C-j>', 'K', 'a' }) })[1], false)
check('コマンドラインを抜けると blink.cmp のメニューがまた出る', menu_after({ '/ec' }), { 'echoes' })
check('blink.cmp の設定の cmdline.enabled は元に戻る', blink_cmdline_enabled(), true)

local ascii_menu, _, ascii_mode = menu_after({ '/', '<C-j>', 'l', 'ec' })
check('ASCII モードの間は blink.cmp のメニューが出る', { ascii_mode, ascii_menu }, { 'ascii', { 'echoes' } })
local command_menu = menu_after({ ':', '<C-j>', 'l', 'ec' })
check(
  ': の ASCII モードでもコマンドの補完のメニューが出る',
  type(command_menu) == 'table' and vim.tbl_contains(command_menu, 'echo'),
  true
)
local back_menu, back_preedit, back_mode = menu_after({ '/', '<C-j>', 'l', 'ec', '<C-j>', 'K', 'a' })
check(
  'C-j でかなのモードに戻ると、blink.cmp のメニューがまた出なくなる',
  { back_mode, back_preedit, back_menu },
  { 'hiragana', '▽か', false }
)
check('かなのモードに戻ってから抜けても cmdline.enabled は元に戻る', blink_cmdline_enabled(), true)
check(
  'コマンドラインで rinze を無効にすると、抜ける前から blink.cmp のメニューが出る',
  menu_after({ '/', '<C-j>', '<C-l>', 'ec' }),
  { 'echoes' }
)
check('無効にしてから抜けても cmdline.enabled は元に戻る', blink_cmdline_enabled(), true)

vim.fn.jobstop(child)
if failures > 0 then
  print(failures .. ' 件失敗')
  os.exit(1)
end
print('すべて通過')
os.exit(0)
