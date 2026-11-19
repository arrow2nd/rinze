-- nvim --headless で rinzed を自動起動させ、実際の打鍵でバッファに入力した結果を確かめる。
-- 使い方: nvim --headless --clean -l tests/nvim/e2e.lua <rinzed> <model> <dict> <socket>

local rinzed, model, dict, socket = unpack(arg)
local root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h:h')
vim.opt.runtimepath:prepend(root .. '/nvim')
vim.cmd.runtime('plugin/rinze.lua')

local rinze = require('rinze')
rinze.setup({ cmd = { rinzed, '--model', model, '--dict', dict }, socket = socket })

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

-- 打鍵の途中の様子は、挿入モードの <F12> でその時点の未確定文字列・バッファ・候補一覧を記録して確かめる
local snapshots
vim.keymap.set('i', '<F12>', function()
  local list
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    if vim.api.nvim_win_get_config(win).relative ~= '' then
      list = vim.api.nvim_buf_get_lines(vim.api.nvim_win_get_buf(win), 0, -1, false)[1]
    end
  end
  table.insert(snapshots, { preedit = rinze.preedit(), buffer = buffer_text(), list = list })
end)

local function type_in_new_buffer(keys)
  snapshots = {}
  vim.cmd('enew!')
  vim.api.nvim_feedkeys(vim.keycode('i' .. keys .. '<Esc>'), 'x', false)
  return buffer_text(), snapshots
end

check('開始時に常駐プロセスが動いていない', vim.uv.fs_stat(socket), nil)
vim.cmd('RinzeEnable')
check('RinzeEnable で常駐プロセスが起動する', vim.uv.fs_stat(socket) ~= nil, true)

local text, snaps = type_in_new_buffer('Asita<Space><F12>AEmasuka')
check('Asita<Space> の途中', snaps[1], { preedit = '▼明日', buffer = '' })
check('Asita<Space>AEmasuka のバッファ', text, '明日会えますか')

text, snaps = type_in_new_buffer('Mayone-zu<Space>woAE<F12>masu')
check('Mayone-zu<Space>woAE の途中', snaps[1], { preedit = '▼和え', buffer = 'マヨネーズを' })
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

if failures > 0 then
  print(failures .. ' 件失敗')
  os.exit(1)
end
print('すべて通過')
os.exit(0)
