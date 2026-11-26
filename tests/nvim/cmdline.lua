-- コマンドラインモードでの入力を、nvim_input で別の Neovim に打鍵を送って確かめる
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
vim.o.incsearch = true
local rinze = require('rinze')
rinze.setup({ cmd = cmd, socket = socket })
vim.keymap.set('i', '<C-j>', rinze.enable, { desc = 'rinze を有効にする' })
vim.keymap.set('c', '<C-j>', rinze.enable, { desc = 'rinze を有効にする' })

_G.T = { done = false, events = {}, snaps = {} }

local function floats()
  local found = {}
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    if vim.api.nvim_win_get_config(win).relative ~= '' then
      table.insert(found, table.concat(vim.api.nvim_buf_get_lines(vim.api.nvim_win_get_buf(win), 0, -1, false), '\n'))
    end
  end
  table.sort(found)
  return found
end
T.floats = floats

function T.cmaps(buf)
  local maps = {}
  for _, m in ipairs(buf and vim.api.nvim_buf_get_keymap(buf, 'c') or vim.api.nvim_get_keymap('c')) do
    maps[m.lhs] = m.desc or m.rhs or ''
  end
  return maps
end

function T.rinze_maps()
  local found = 0
  for _, scope in ipairs({ vim.api.nvim_get_keymap('c'), vim.api.nvim_buf_get_keymap(0, 'c') }) do
    for _, m in ipairs(scope) do
      if m.desc == 'rinze' then
        found = found + 1
      end
    end
  end
  return found
end

function T.snap()
  table.insert(T.snaps, {
    cmdline = vim.fn.getcmdline(),
    preedit = rinze.preedit(),
    mode = rinze.mode(),
    floats = floats(),
    rinze_maps = T.rinze_maps(),
  })
end

function T.fresh(lines)
  vim.cmd('enew!')
  vim.bo.bufhidden = 'wipe'
  vim.api.nvim_buf_set_lines(0, 0, -1, false, lines or { '' })
  vim.api.nvim_win_set_cursor(0, { 1, 0 })
  T.snaps, T.events = {}, {}
end

function T.text()
  return table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), '\n')
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

local function preedit()
  return lua('return require("rinze").preedit()')
end

local function nvim_mode()
  return lua('return vim.api.nvim_get_mode().mode')
end

local function cmdline()
  return lua('return vim.fn.getcmdline()')
end

local function send_space_until(word)
  for _ = 1, 30 do
    if preedit() == '▼' .. word then
      return true
    end
    type_keys('<Space>')
  end
  return false
end

local function snap(n)
  return lua('return T.snaps[...]', n)
end

local function in_history(prefix)
  for _, line in ipairs(vim.fn.filereadable(history) == 1 and vim.fn.readfile(history) or {}) do
    if vim.startswith(line, prefix) then
      return true
    end
  end
  return false
end

local S = '<Cmd>lua T.snap()<CR>'

lua('T.fresh(...)', { 'abc', 'ここに漢字がある' })
type_keys('/' .. S .. '<C-j>' .. S .. 'Kanji' .. S)
check('/ に入った時点では rinze は何も受け取らない', snap(1).rinze_maps, 0)
check(
  '<C-j> で有効になりひらがなで始まる',
  { snap(2).mode, snap(2).rinze_maps > 0 },
  { 'hiragana', true }
)
check(
  '見出しはコマンドラインに入れず、カーソルの位置に重ねて出す',
  { snap(3).cmdline, snap(3).preedit, snap(3).floats },
  { '', '▽かんじ', { '▽かんじ' } }
)
check('Space で漢字まで送れる', send_space_until('漢字'), true)
type_keys('<CR>' .. S)
check(
  '変換中の Enter は確定だけを行い、検索しない',
  { snap(4).cmdline, snap(4).preedit, snap(4).floats, nvim_mode() },
  { '漢字', '', {}, 'c' }
)
type_keys('<CR>')
check(
  '未確定のものが無い Enter で検索し、漢字にカーソルが移る',
  { nvim_mode(), lua('return vim.api.nvim_win_get_cursor(0)') },
  { 'n', { 2, #'ここに' } }
)
check('検索の履歴には確定した文字列だけが残る', lua('return vim.fn.histget("/", -1)'), '漢字')
check('コマンドラインを抜けると rinze の割り当ては残らない', lua('return T.rinze_maps()'), 0)

lua('T.fresh()')
type_keys(':let g:typed = "kanji"' .. S .. '<CR>')
check(
  ': で <C-j> を打たなければ英字のコマンドが打て、rinze は何も受け取らない',
  { lua('return vim.g.typed'), snap(1).rinze_maps, snap(1).preedit, snap(1).mode },
  { 'kanji', 0, '', nil }
)
type_keys('/' .. '<C-j>ka<Esc>:ka' .. S .. '<Esc>')
check(
  'コマンドラインに入り直すと無効から始まる',
  { snap(2).cmdline, snap(2).rinze_maps },
  { 'ka', 0 }
)

lua('T.fresh(...)', { 'ここに漢字がある' })
type_keys(':s/<C-j>Kanji')
check(':s/ の中で見出しを打てる', { cmdline(), preedit() }, { 's/', '▽かんじ' })
send_space_until('漢字')
type_keys('<CR>l/<C-j>Moji')
send_space_until('文字')
type_keys('<CR>' .. S .. '<CR>')
check(
  ':s/ の置換のパターンと置換後の文字列に日本語を打てる',
  { snap(1).cmdline, lua('return T.text()') },
  { 's/漢字/文字', 'ここに文字がある' }
)
type_keys(':<C-j>ka<C-v>/ki' .. S .. '<Esc>')
check(
  'かなのモードでも C-v / で区切りの / を打てる',
  { snap(2).cmdline, snap(2).mode },
  { 'か/き', 'hiragana' }
)

lua('T.fresh(...)', { 'あいう' })
type_keys(':<C-j>Asita' .. S .. '<Esc>')
check(
  '見出しの入力中に Esc でコマンドラインを抜ける',
  { snap(1).preedit, nvim_mode() },
  { '▽あした', 'n' }
)
check(
  'Esc のあと、履歴にもバッファにも未確定文字列が残らず、フロートも閉じる',
  { lua('return vim.fn.histget(":", -1)'):find('▽', 1, true), lua('return T.text()'), lua('return T.floats()') },
  { nil, 'あいう', {} }
)
type_keys(':let g:x = 1<Left><Left><Left><Left><C-j>Asita')
-- C-c は先読みを捨てるので、終わりを知らせる <Cmd> を続けて送れない
vim.rpcrequest(child, 'nvim_input', '<C-c>')
vim.wait(5000, function()
  return nvim_mode() == 'n'
end, 5)
check(
  'C-c でも未確定文字列を捨てて抜け、履歴に残らない',
  { lua('return vim.fn.histget(":", -1)'), nvim_mode(), lua('return T.floats()') },
  { 'let g:x = 1', 'n', {} }
)
type_keys('/<C-j>Kanji<Space><C-g>' .. S .. '<C-g>' .. S .. '<Esc>')
check(
  '未確定のものがある C-g は rinze の取り消し',
  { snap(2).preedit, snap(2).cmdline, snap(3).preedit, snap(3).floats },
  { '▽かんじ', '', '', {} }
)
lua('T.fresh(...)', { 'ab', 'ab', 'ab' })
-- 'incsearch' は先読みが残っている間は検索しないので、1 つずつ送って処理を待つ
for _, key in ipairs({ '/ab', '<C-j>', '<C-g>', '<CR>' }) do
  vim.rpcrequest(child, 'nvim_input', key)
  vim.wait(200)
end
check(
  '未確定のものが無い C-g は検索中の次の一致へ移る',
  lua('return vim.api.nvim_win_get_cursor(0)'),
  { 3, 0 }
)

lua('T.fresh(...)', { 'xy' })
type_keys(':abc<Left><Left><C-j>Kan<Left>' .. S .. '<Esc>')
check(
  'コマンドラインのカーソルを動かすと、未確定文字列を元の位置に確定する',
  { snap(1).cmdline, snap(1).preedit, snap(1).floats },
  { 'aかんbc', '', {} }
)

lua('T.fresh()')
type_keys('i<C-j>qAsita' .. S .. '<C-o>:' .. S .. '<C-j>' .. S .. 'Kanji' .. S .. '<Esc>' .. S)
check(
  '挿入モードは有効でカタカナモード、見出しの入力中',
  { snap(1).preedit, snap(1).mode },
  { '▽アシタ', 'katakana' }
)
check(
  'コマンドラインに入った時点では rinze は無効',
  { snap(2).mode, snap(2).rinze_maps },
  { 'katakana', 0 }
)
check(
  'コマンドラインはひらがなで始まり、挿入モードの見出しと混ざらない',
  { snap(3).mode, snap(4).preedit, snap(4).floats },
  { 'hiragana', '▽かんじ', { '▽かんじ' } }
)
check(
  'コマンドラインから戻ると、挿入モードの見出しとモードはそのまま',
  { snap(5).preedit, snap(5).mode },
  { '▽アシタ', 'katakana' }
)
type_keys('<Space>' .. S .. '<CR><Esc>')
check(
  '戻ったあとも挿入モードの見出しを変換して確定できる',
  lua('return T.text()'),
  (snap(6).preedit:gsub('^▼', ''))
)
lua('vim.cmd("RinzeDisable")')

lua('T.fresh()')
type_keys(':<C-j>Kanji<Esc>ika' .. S .. '<Esc>')
check(
  '挿入モードで無効のままコマンドラインで使っても、挿入モードは無効のまま',
  { lua('return T.text()'), snap(1).mode, snap(1).preedit },
  { 'ka', nil, '' }
)

lua('T.fresh()')
type_keys(':<C-j>Kou<Space><Space><Space><Space>' .. S .. '<Space>' .. S)
local list = snap(2).floats[1] or ''
check('4 回目の Space まではその場に出す', #snap(1).floats, 1)
check('5 回目の Space でコマンドラインの近くに一覧を出す', list:match('^a:') ~= nil, true)
type_keys('a' .. S .. '<Esc>')
check(
  '一覧の a で選んだ候補をコマンドラインに確定する',
  { snap(3).cmdline, snap(3).floats },
  { list:match('^a:%s*(%S+)'), {} }
)

lua('T.fresh()')
type_keys(':<C-j>Rinzetesuto<Space>')
local registering = false
for _ = 1, 30 do
  local fl = lua('return T.floats()')
  if table.concat(fl, '\n'):find('辞書登録', 1, true) then
    registering = true
    break
  end
  type_keys('<Space>')
end
type_keys(S .. 'lrz' .. S .. '<CR>' .. S .. '<Esc>')
check(
  '辞書に無い読みで辞書登録に入り、入力欄をフロートに出す',
  { registering, vim.tbl_contains(snap(1).floats, '[辞書登録] りんぜてすと: ') },
  { true, true }
)
check(
  '入力欄に打った語が出る',
  vim.tbl_contains(snap(2).floats, '[辞書登録] りんぜてすと: rz'),
  true
)
check('Enter で登録してコマンドラインに確定する', { snap(3).cmdline, snap(3).floats }, { 'rz', {} })
check(
  '登録した語をユーザー辞書に書く',
  vim.tbl_contains(vim.fn.readfile(user_dict), 'りんぜてすと /rz/'),
  true
)

lua('T.fresh()')
type_keys(':<C-j>Kanji<Space>' .. S .. '<Space>' .. S .. '<CR><Esc>')
local second = snap(2).preedit:gsub('^▼', '')
check('コマンドラインで 2 番目の候補を選べる', snap(1).preedit ~= snap(2).preedit, true)
check(
  'コマンドラインで選び直した候補を学習の履歴に書く',
  in_history('かんじ\t' .. second .. '\t'),
  true
)

lua('T.fresh()')
type_keys(':<C-j>kal' .. S .. 'abc' .. S .. '<C-j>' .. S .. 'ka' .. S .. '<Esc>')
check(
  'l で ASCII モードに入り、C-j 以外を rinze が受け取らない',
  { snap(1).mode, snap(1).rinze_maps },
  { 'ascii', 1 }
)
check('ASCII モードでは英字をそのまま打てる', snap(2).cmdline, 'かabc')
check('C-j でひらがなに戻る', { snap(3).mode, snap(4).cmdline }, { 'hiragana', 'かabcか' })

lua('T.fresh()')
type_keys(':<C-j>Asita<Cmd>doautocmd FocusLost<CR>' .. S .. 'abc' .. S .. '<Esc>')
check(
  'コマンドライン中の FocusLost で未確定文字列を確定して ASCII モードに戻る',
  { snap(1).cmdline, snap(1).mode, snap(2).cmdline },
  { 'あした', 'ascii', 'あしたabc' }
)

lua([[
vim.keymap.set('c', '<CR>', function()
  table.insert(T.events, 'global <CR>')
  return '<CR>'
end, { expr = true, desc = 'plugin <CR>' })
vim.keymap.set('c', 'x', function()
  table.insert(T.events, 'global x')
  return 'x'
end, { expr = true, desc = 'plugin x' })
]])
lua('T.fresh()')
lua([[vim.keymap.set('c', ';', function()
  table.insert(T.events, 'buffer ;')
  return ';'
end, { expr = true, buffer = true, desc = 'plugin ;' })]])
local global_before, buffer_before = lua('return T.cmaps()'), lua('return T.cmaps(0)')
type_keys(':let g:y = 1<C-j><CR>')
check(
  'rinze が処理しない Enter はほかのプラグインの割り当てに渡す',
  { lua('return T.events'), nvim_mode() },
  { { 'global <CR>' }, 'n' }
)
check(
  'rinze を使ったあと、コマンドラインの割り当ては使う前と同じ',
  { lua('return T.cmaps()'), lua('return T.cmaps(0)') },
  { global_before, buffer_before }
)
lua('T.events = {}')
type_keys(':let g:z = "x;"<CR>')
check(
  'rinze を使ったあと、global とバッファローカルの割り当てが働く',
  { lua('return T.events'), lua('return vim.g.z') },
  { { 'global x', 'buffer ;', 'global <CR>' }, 'x;' }
)

vim.fn.jobstop(child)
if failures > 0 then
  print(failures .. ' 件失敗')
  os.exit(1)
end
print('すべて通過')
os.exit(0)
