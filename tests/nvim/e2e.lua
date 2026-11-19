local rinzed, model, dict, socket, user_dict = unpack(arg)
local root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h:h')
vim.opt.runtimepath:prepend(root .. '/nvim')
vim.cmd.runtime('plugin/rinze.lua')

local rinze = require('rinze')
local cmd = { rinzed, '--model', model, '--dict', dict, '--user-dict', user_dict }

local failures = 0
local function check(label, got, want)
  local ok = vim.deep_equal(got, want)
  print(string.format('%s %s: %s', ok and 'ok  ' or 'FAIL', label, vim.inspect(got)))
  if not ok then
    print('     期待: ' .. vim.inspect(want))
    failures = failures + 1
  end
end

local function buffer_text()
  return table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), '\n')
end

local function float_text()
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    if vim.api.nvim_win_get_config(win).relative ~= '' then
      return table.concat(vim.api.nvim_buf_get_lines(vim.api.nvim_win_get_buf(win), 0, -1, false), '\n')
    end
  end
end

local snapshot = '<F12>'
local snapshots
vim.keymap.set('i', snapshot, function()
  table.insert(snapshots, { preedit = rinze.preedit(), buffer = buffer_text(), list = float_text() })
end)

local mode_snapshot = '<F10>'
vim.keymap.set('i', mode_snapshot, function()
  table.insert(snapshots, {
    buffer = buffer_text(),
    status = rinze.status(),
    a_reaches_rinze = vim.fn.maparg('a', 'i') ~= '',
  })
end)

local function type_in_new_buffer(keys)
  snapshots = {}
  vim.cmd('enew!')
  vim.api.nvim_feedkeys(vim.keycode('i' .. keys .. '<Esc>'), 'x', false)
  return buffer_text(), snapshots
end

local space_until_registration = '<F11>'
-- 打つ Space は先読み中のキーより前に差し込むので、space_until_registration のあとに書いたキーは登録に入ってから処理される
local seen
vim.keymap.set('i', space_until_registration, function()
  if (float_text() or ''):find('辞書登録', 1, true) or #seen > 30 then
    return
  end
  table.insert(seen, vim.trim(rinze.preedit() .. ' ' .. (float_text() or '')))
  vim.api.nvim_feedkeys(vim.keycode('<Space>' .. space_until_registration), 'i', false)
end)

local long_socket = vim.fn.tempname() .. string.rep('x', 120) .. '.sock'
rinze.setup({ cmd = cmd, socket = long_socket })
local ok, err = pcall(rinze.enable)
print('  長すぎるパスでのエラー: ' .. tostring(err))
check('長すぎるソケットのパスは原因を示して失敗する', not ok and tostring(err):find('上限', 1, true) ~= nil, true)

rinze.setup({ cmd = cmd, socket = socket })
check('開始時に常駐プロセスが動いていない', vim.uv.fs_stat(socket), nil)
vim.cmd('RinzeEnable')
check('RinzeEnable で常駐プロセスが起動する', vim.uv.fs_stat(socket) ~= nil, true)

local text, snaps = type_in_new_buffer('Asita<Space>' .. snapshot .. 'AEmasuka')
check('Asita<Space> の途中', snaps[1], { preedit = '▼明日', buffer = '' })
check('Asita<Space>AEmasuka のバッファ', text, '明日会えますか')

text, snaps = type_in_new_buffer('Mayone-zu<Space>' .. snapshot .. 'woAE' .. snapshot .. 'masu')
check('辞書に無い Mayone-zu<Space> は生成した表記が最初の候補', snaps[1].preedit, '▼マヨネーズ')
check('Mayone-zu<Space>woAE の途中', snaps[2], { preedit = '▼和え', buffer = 'マヨネーズを' })
check('Mayone-zu<Space>woAEmasu のバッファ', text, 'マヨネーズを和えます')

text, snaps = type_in_new_buffer('koredeAT' .. snapshot .. 't' .. snapshot .. '<Space>' .. snapshot .. 'eimasuka?')
check('koredeAT の途中', snaps[1], { preedit = '▽あ*t', buffer = 'これで' })
check('koredeATt の途中', snaps[2], { preedit = '▼会っt', buffer = 'これで' })
check('<Space> で 2 番目の候補', snaps[3], { preedit = '▼合っt', buffer = 'これで' })
check('koredeATteimasuka? のバッファ', text, 'これで合っていますか？')

text, snaps = type_in_new_buffer('Kanji<Space><Space><Space><Space>' .. snapshot .. '<Space>' .. snapshot .. 'a' .. snapshot)
print('一覧の表示: ' .. vim.inspect(snaps))
check('4 回目の Space まではその場に出す', snaps[1].list, nil)
check('5 回目の Space で一覧を出す', snaps[2].list and snaps[2].list:match('^a:') ~= nil, true)
check('一覧の a で確定し一覧を閉じる', { snaps[3].list, snaps[3].preedit, snaps[3].buffer == snaps[2].list:match('^a:(%S+)') }, { nil, '', true })

text = type_in_new_buffer('a<Space>i<CR>u<BS>e')
check('素通しした空白・改行・後退', text, 'あ い\nえ')

text, snaps = type_in_new_buffer(
  'koreha' .. mode_snapshot .. 'l' .. mode_snapshot .. 'Neovim' .. mode_snapshot
    .. '<C-j>' .. mode_snapshot .. 'noq' .. mode_snapshot .. 'puraguinq' .. mode_snapshot .. 'deKaKimasu.'
)
print('モードを切り替えながら打った途中: ' .. vim.inspect(snaps))
check('始めはひらがな', snaps[1], { buffer = 'これは', status = 'かな', a_reaches_rinze = true })
check('l で ASCII に入り、キーを rinze に送らない', snaps[2], { buffer = 'これは', status = 'aA', a_reaches_rinze = false })
check('ASCII では打ったとおりに入る', snaps[3].buffer, 'これはNeovim')
check('C-j でひらがなに戻る', snaps[4], { buffer = 'これはNeovim', status = 'かな', a_reaches_rinze = true })
check('q でカタカナに入る', { snaps[5].buffer, snaps[5].status }, { 'これはNeovimの', 'カナ' })
check('もう一度 q でひらがなに戻る', { snaps[6].buffer, snaps[6].status }, { 'これはNeovimのプラグイン', 'かな' })
check('モードを切り替えながら打った文', text, 'これはNeovimのプラグインで書きます。')

text = type_in_new_buffer('<C-q>pa-n<C-q>Lab<C-j>a')
check('半角カナと全角英字', text, 'ﾊﾟｰﾝａｂあ')

type_in_new_buffer('l')
text, snaps = type_in_new_buffer('abc' .. mode_snapshot .. '<C-j>')
check('挿入モードを抜けて戻ってもモードを保つ', { text, snaps[1].status }, { 'abc', 'aA' })
type_in_new_buffer('q')
vim.cmd('RinzeDisable')
check('無効にするとモードは nil', { rinze.mode(), rinze.status() }, { nil, '' })
vim.cmd('RinzeEnable')
check('有効にするとひらがなで始まる', rinze.mode(), 'hiragana')

seen = {}
text, snaps = type_in_new_buffer('Rinze<Space>' .. space_until_registration .. snapshot .. 'Rin<Space>' .. snapshot .. '<CR>ze' .. snapshot .. '<CR>' .. snapshot)
print('りんぜ で送った候補: ' .. vim.inspect(seen))
print('登録の途中: ' .. vim.inspect(snaps))
check('辞書に無い読みは生成した表記から始まる', seen[1] ~= '▼りんぜ' and seen[1]:match('^▼') ~= nil, true)
check('生成した表記を送り切ると登録の入力欄を出す', { snaps[1].preedit, snaps[1].list }, { '▼りんぜ', '[辞書登録] りんぜ: ' })
check('入力欄でも変換できる', snaps[2].list:match('^%[辞書登録%] りんぜ: ▼') ~= nil, true)
local word = snaps[3].list:match('^%[辞書登録%] りんぜ: (.*)$')
check('登録する語は送った候補に無かった', table.concat(seen, '\n'):find(word, 1, true), nil)
check('Enter で登録した語を元の位置に確定する', { snaps[4].buffer, snaps[4].list, text }, { word, nil, word })
check('ユーザー辞書に書く', vim.fn.readfile(user_dict)[4], 'りんぜ /' .. word .. '/')

text, snaps = type_in_new_buffer('Rinze<Space>' .. snapshot)
check('登録した語が次の変換で候補に出る', snaps[1].preedit, '▼' .. word)

vim.fn.system({ 'pkill', '-f', '--socket ' .. socket })
vim.wait(5000, function()
  return vim.fn.system({ 'pgrep', '-f', '--socket ' .. socket }) == ''
end, 50)
check('止めた rinzed のソケットが残っている', vim.uv.fs_stat(socket) ~= nil, true)
text, snaps = type_in_new_buffer('Rinze<Space>' .. snapshot)
check('残ったソケットを片付けて起動し直した rinzed でも登録した語が出る', snaps[1].preedit, '▼' .. word)

if failures > 0 then
  print(failures .. ' 件失敗')
  os.exit(1)
end
print('すべて通過')
os.exit(0)
