-- 打鍵を nvim_input で別の Neovim に送る。feedkeys の 'x' は先読みが尽きるとキーマップの続きを待たないので、
-- timeoutlen の待ちは実際に打ったときと同じ入力の経路でしか再現しない
local rinzed, model, dict, socket, user_dict, history = unpack(arg)
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
local root, cmd, socket = ...
vim.opt.runtimepath:prepend(root .. '/nvim')
vim.cmd.runtime('plugin/rinze.lua')
vim.o.swapfile = false
vim.o.hidden = true
vim.o.timeoutlen = 1000
require('rinze').setup({ cmd = cmd, socket = socket })

_G.T = { done = false, events = {} }

local function between_pair()
  local col = vim.api.nvim_win_get_cursor(0)[2]
  return vim.api.nvim_get_current_line():sub(col, col + 1) == '()'
end
vim.keymap.set('i', '(', function()
  return '()<Left>'
end, { expr = true, desc = 'pairs (' })
vim.keymap.set('i', '<BS>', function()
  return between_pair() and '<BS><Del>' or '<BS>'
end, { expr = true, desc = 'pairs <BS>' })
vim.keymap.set('i', '<CR>', function()
  return '<CR>'
end, { expr = true, desc = 'pairs <CR>' })
vim.keymap.set('i', '<C-j>', function()
  require('rinze').enable()
end, { desc = 'rinze を有効にする' })

vim.api.nvim_create_autocmd('FileType', {
  pattern = 'markdown',
  callback = function(ev)
    vim.keymap.set('i', '>', function()
      local row, col = unpack(vim.api.nvim_win_get_cursor(0))
      vim.api.nvim_buf_set_text(ev.buf, row - 1, col, row - 1, col, { '>' })
      vim.api.nvim_win_set_cursor(0, { row, col + 1 })
      table.insert(T.events, 'autotag >')
    end, { buffer = ev.buf, desc = 'autotag >' })
  end,
})
vim.api.nvim_create_autocmd('InsertEnter', {
  callback = function()
    if vim.b.llama then
      vim.keymap.set('i', '<Space>llf', '<Cmd>lua table.insert(T.events, "llama")<CR>', { buffer = true, desc = 'llama' })
    end
  end,
})

function T.imaps(buf)
  local maps = {}
  for _, m in ipairs(buf and vim.api.nvim_buf_get_keymap(buf, 'i') or vim.api.nvim_get_keymap('i')) do
    maps[m.lhs] = m.desc or m.rhs or ''
  end
  return maps
end

function T.rinze_maps()
  local found = {}
  for _, buf in ipairs(vim.api.nvim_list_bufs()) do
    for _, m in ipairs(vim.api.nvim_buf_get_keymap(buf, 'i')) do
      if m.desc == 'rinze' then
        found[buf] = (found[buf] or 0) + 1
      end
    end
  end
  for _, m in ipairs(vim.api.nvim_get_keymap('i')) do
    if m.desc == 'rinze' then
      found.global = (found.global or 0) + 1
    end
  end
  return found
end

function T.text()
  return table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), '\n')
end

function T.snap()
  table.insert(T.snaps, { preedit = require('rinze').preedit(), text = T.text(), mode = require('rinze').mode() })
end

function T.fresh(lines, filetype)
  vim.cmd('enew!')
  vim.bo.bufhidden = 'wipe'
  vim.api.nvim_buf_set_lines(0, 0, -1, false, lines or { '' })
  if filetype then
    vim.bo.filetype = filetype
  end
  T.snaps, T.events = {}, {}
  return vim.api.nvim_get_current_buf()
end
]],
  root,
  cmd,
  socket
)

local function type_keys(keys)
  lua('T.done = false')
  vim.rpcrequest(child, 'nvim_input', keys .. '<Cmd>lua T.done = true<CR>')
  local done = vim.wait(10000, function()
    return lua('return T.done')
  end, 5)
  assert(done, '打鍵の処理が終わらない: ' .. keys)
end

local function fresh(lines, filetype)
  return lua('return T.fresh(...)', lines, filetype)
end

local function text()
  return lua('return T.text()')
end

local snap = '<Cmd>lua T.snap()<CR>'

local global_before = lua('return T.imaps()')

fresh()
type_keys('i<C-j>ka' .. snap .. '<Esc>')
check('利用者の <C-j> で有効になり、かなを打てる', lua('return T.snaps[1]'), { preedit = '', text = 'か', mode = 'hiragana' })
check('挿入モードを抜けると、rinze のキーマップはどこにも残らない', lua('return T.rinze_maps()'), {})
check('有効な間も global な割り当ては有効にする前と同じ', lua('return T.imaps()'), global_before)

lua('vim.cmd("RinzeDisable")')
check(':RinzeDisable のあと global な割り当ては有効にする前と同じ', lua('return T.imaps()'), global_before)
fresh()
type_keys('i(<Esc>')
check(':RinzeDisable のあと、ほかのプラグインの global な expr マップが働く', text(), '()')

fresh()
type_keys('i<Cmd>RinzeEnable<CR>l' .. snap .. '(<Esc>')
check('ASCII モードに入る', lua('return T.snaps[1].mode'), 'ascii')
check('ASCII モードの間は、ほかのプラグインの global な expr マップが働く', text(), '()')
type_keys('A<C-j><Esc>')
lua('vim.cmd("RinzeDisable")')

fresh({ '()' })
type_keys('a<Cmd>RinzeEnable<CR><BS><Esc>')
check('ひらがなモードで rinze が処理しない BackSpace は、元の割り当てに渡す', text(), '')

fresh()
type_keys('ia<CR>i<Esc>.')
check('元の割り当てに渡した Enter を含む挿入も . で繰り返せる', text(), 'あ\nあ\nいい')
type_keys('u')
check('元の割り当てに渡した Enter を含む挿入も u で 1 回にまとめて戻す', text(), 'あ\nい')

fresh()
type_keys('iKanji<Space>' .. snap .. '><Esc>')
local plain = text()
local kanji = lua('return T.snaps[1].preedit'):gsub('^▼', '')
check('素のバッファでの Kanji<Space>>', plain, kanji .. '>')
local markdown = fresh({ '' }, 'markdown')
lua('vim.bo.bufhidden = "hide"')
local markdown_maps = lua('return T.imaps(...)', markdown)
type_keys('iKanji<Space>><Esc>')
check('バッファローカルの > があっても、Kanji<Space>> は素のバッファと同じになる', text(), plain)
check('有効な間に挿入モードを抜けると、バッファローカルの割り当ては元に戻る', lua('return T.imaps(...)', markdown), markdown_maps)

lua('vim.cmd("RinzeDisable")')
lua('T.events = {}')
type_keys('o><Esc>')
check(':RinzeDisable のあと、バッファローカルの > が働く', lua('return T.events'), { 'autotag >' })
check(':RinzeDisable のあと、バッファローカルの割り当ては有効にする前と同じ', lua('return T.imaps(...)', markdown), markdown_maps)

fresh()
lua('vim.b.llama = true')
type_keys('i<Cmd>RinzeEnable<CR><Esc>')
lua('T.started = nil')
local started = vim.uv.hrtime()
vim.rpcrequest(child, 'nvim_input', 'iKanji<Space>')
local converted = vim.wait(3000, function()
  return lua('return require("rinze").preedit()'):match('^▼') ~= nil
end, 5)
local space_ms = (vim.uv.hrtime() - started) / 1e6
type_keys('<Esc>')
print(string.format('  バッファローカルの <Space>llf があるときの Kanji<Space> から変換が出るまで: %.0fms', space_ms))
check('同じ文字で始まる長い割り当てがあっても、Space は timeoutlen (1000ms) を待たずに変換する', converted and space_ms < 500, true)

local first = fresh({ '' }, 'markdown')
lua('vim.bo.bufhidden = "hide"')
type_keys('i' .. snap)
local second = lua('local b = vim.api.nvim_create_buf(true, false) vim.bo[b].bufhidden = "wipe" return b')
type_keys('ka<Cmd>buffer ' .. second .. '<CR>sa' .. snap .. '<Cmd>lua T.first_maps = T.imaps(' .. first .. ')<CR><Esc>')
check('挿入モードのまま別のバッファへ移っても rinze に届く', lua('return { vim.api.nvim_get_current_line(), T.snaps[2].mode }'), { 'さ', 'hiragana' })
check('移る前のバッファに打った分はそのまま残る', lua('return vim.api.nvim_buf_get_lines(..., 0, -1, false)', first), { 'か' })
check('挿入モードのまま移ると、移る前のバッファのバッファローカルの割り当ては元に戻る', lua('return T.first_maps'), { ['>'] = 'autotag >' })
type_keys('i<Cmd>bwipeout!<CR>ta<Esc>')
check('挿入モードのままバッファを閉じても、戻ったバッファで rinze に届く', lua('return { vim.api.nvim_get_current_buf(), T.text() }'), { first, 'かた' })
check('バッファを渡り歩いたあと、rinze のキーマップはどこにも残らない', lua('return T.rinze_maps()'), {})

local prompt = fresh()
lua([[
local buf = ...
vim.keymap.set('i', '<CR>', '<Cmd>lua table.insert(T.events, "picker CR")<CR>', { buffer = buf, desc = 'picker <CR>' })
vim.keymap.set('i', '<Esc>', '<Cmd>lua table.insert(T.events, "picker Esc")<CR>', { buffer = buf, desc = 'picker <Esc>' })
]], prompt)
type_keys('iAsita<CR>' .. snap .. '<CR>')
check('見出しの入力中の Enter は rinze が読みのまま確定し、バッファローカルの <CR> に渡さない', { lua('return T.snaps[1].text'), lua('return T.events') }, { 'あした', { 'picker CR' } })
type_keys('<Esc>')
check('rinze が処理しない Enter と Esc はバッファローカルの割り当てに渡す', lua('return T.events'), { 'picker CR', 'picker Esc' })
type_keys('<C-\\><C-n>')

fresh()
lua('vim.bo.buftype = "prompt" T.submitted = {} vim.fn.prompt_setcallback(vim.api.nvim_get_current_buf(), function(line) table.insert(T.submitted, line) end)')
type_keys('aka<CR><Esc>')
check('prompt バッファでも打てて、Enter で送れる', lua('return T.submitted'), { 'か' })

local before_toggle = { lua('return T.imaps()'), lua('return T.imaps(...)', markdown) }
for _, toggle in ipairs({ 'vim.cmd("RinzeToggle")', 'require("rinze").toggle()' }) do
  lua(toggle)
  type_keys('i<Esc>')
  lua(toggle)
  check(toggle .. ' を 2 回呼んだあと、割り当ては呼ぶ前と同じ', { lua('return T.imaps()'), lua('return T.imaps(...)', markdown) }, before_toggle)
end
check('最後に有効にする前と global な割り当てが同じ', lua('return T.imaps()'), global_before)

vim.fn.jobstop(child)
if failures > 0 then
  print(failures .. ' 件失敗')
  os.exit(1)
end
print('すべて通過')
os.exit(0)
