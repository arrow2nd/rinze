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

local snapshots
vim.keymap.set('i', '<F12>', function()
  table.insert(snapshots, { preedit = rinze.preedit(), buffer = buffer_text(), list = float_text() })
end)

local function type_in_new_buffer(keys)
  snapshots = {}
  vim.cmd('enew!')
  vim.api.nvim_feedkeys(vim.keycode('i' .. keys .. '<Esc>'), 'x', false)
  return buffer_text(), snapshots
end

-- 打つ Space は先読み中のキーより前に差し込むので、<F11> のあとに書いたキーは登録に入ってから処理される
local seen
vim.keymap.set('i', '<F11>', function()
  if (float_text() or ''):find('辞書登録', 1, true) or #seen > 30 then
    return
  end
  table.insert(seen, vim.trim(rinze.preedit() .. ' ' .. (float_text() or '')))
  vim.api.nvim_feedkeys(vim.keycode('<Space><F11>'), 'i', false)
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

local text, snaps = type_in_new_buffer('Asita<Space><F12>AEmasuka')
check('Asita<Space> の途中', snaps[1], { preedit = '▼明日', buffer = '' })
check('Asita<Space>AEmasuka のバッファ', text, '明日会えますか')

text, snaps = type_in_new_buffer('Mayone-zu<Space><F12>woAE<F12>masu')
check('辞書に無い Mayone-zu<Space> は生成した表記が最初の候補', snaps[1].preedit, '▼マヨネーズ')
check('Mayone-zu<Space>woAE の途中', snaps[2], { preedit = '▼和え', buffer = 'マヨネーズを' })
check('Mayone-zu<Space>woAEmasu のバッファ', text, 'マヨネーズを和えます')

text, snaps = type_in_new_buffer('koredeAT<F12>t<F12><Space><F12>eimasuka?')
check('koredeAT の途中', snaps[1], { preedit = '▽あ*t', buffer = 'これで' })
check('koredeATt の途中', snaps[2], { preedit = '▼会っt', buffer = 'これで' })
check('<Space> で 2 番目の候補', snaps[3], { preedit = '▼合っt', buffer = 'これで' })
check('koredeATteimasuka? のバッファ', text, 'これで合っていますか？')

text, snaps = type_in_new_buffer('Kanji<Space><Space><Space><Space><F12><Space><F12>a<F12>')
print('一覧の表示: ' .. vim.inspect(snaps))
check('4 回目の Space まではその場に出す', snaps[1].list, nil)
check('5 回目の Space で一覧を出す', snaps[2].list and snaps[2].list:match('^a:') ~= nil, true)
check('一覧の a で確定し一覧を閉じる', { snaps[3].list, snaps[3].preedit, snaps[3].buffer == snaps[2].list:match('^a:(%S+)') }, { nil, '', true })

text = type_in_new_buffer('a<Space>i<CR>u<BS>e')
check('素通しした空白・改行・後退', text, 'あ い\nえ')

seen = {}
text, snaps = type_in_new_buffer('Rinze<Space><F11><F12>Rin<Space><F12><CR>ze<F12><CR><F12>')
print('りんぜ で送った候補: ' .. vim.inspect(seen))
print('登録の途中: ' .. vim.inspect(snaps))
check('辞書に無い読みは生成した表記から始まる', seen[1] ~= '▼りんぜ' and seen[1]:match('^▼') ~= nil, true)
check('生成した表記を送り切ると登録の入力欄を出す', { snaps[1].preedit, snaps[1].list }, { '▼りんぜ', '[辞書登録] りんぜ: ' })
check('入力欄でも変換できる', snaps[2].list:match('^%[辞書登録%] りんぜ: ▼') ~= nil, true)
local word = snaps[3].list:match('^%[辞書登録%] りんぜ: (.*)$')
check('登録する語は送った候補に無かった', table.concat(seen, '\n'):find(word, 1, true), nil)
check('Enter で登録した語を元の位置に確定する', { snaps[4].buffer, snaps[4].list, text }, { word, nil, word })
check('ユーザー辞書に書く', vim.fn.readfile(user_dict)[4], 'りんぜ /' .. word .. '/')

text, snaps = type_in_new_buffer('Rinze<Space><F12>')
check('登録した語が次の変換で候補に出る', snaps[1].preedit, '▼' .. word)

vim.fn.system({ 'pkill', '-f', '--socket ' .. socket })
vim.wait(5000, function()
  return vim.fn.system({ 'pgrep', '-f', '--socket ' .. socket }) == ''
end, 50)
check('止めた rinzed のソケットが残っている', vim.uv.fs_stat(socket) ~= nil, true)
text, snaps = type_in_new_buffer('Rinze<Space><F12>')
check('残ったソケットを片付けて起動し直した rinzed でも登録した語が出る', snaps[1].preedit, '▼' .. word)

if failures > 0 then
  print(failures .. ' 件失敗')
  os.exit(1)
end
print('すべて通過')
os.exit(0)
