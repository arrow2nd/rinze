-- setup に何も渡さず、PATH の rinzed (flake の既定値を持つラッパー) が起動して変換できることを確かめる。
-- XDG_RUNTIME_DIR、XDG_DATA_HOME、XDG_STATE_HOME を一時ディレクトリに向けて呼ぶ (flake の checks.default-cmd)
local root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h:h')
vim.opt.runtimepath:prepend(root .. '/nvim')
vim.cmd.runtime('plugin/rinze.lua')

local failures = 0
local function check(label, got, want)
  local ok = vim.deep_equal(got, want)
  print(string.format('%s %s: %s', ok and 'ok  ' or 'FAIL', label, vim.inspect(got)))
  if not ok then
    print('     期待: ' .. vim.inspect(want))
    failures = failures + 1
  end
end

local rinze = require('rinze')
rinze.setup({})
local ok, err = pcall(rinze.enable)
check('cmd を省いても PATH の rinzed を起動して有効になる', { ok, err }, { true, nil })
check('有効にした直後はひらがなモード', rinze.mode(), 'hiragana')

local chan = vim.fn.sockconnect('pipe', require('rinze.socket').default(), { rpc = true })
local words = vim.rpcrequest(chan, 'lookup', 'たなか', '')
check('既定の辞書の L の語が出る', vim.tbl_contains(words, '田仲'), true)
check('既定の辞書の geo の語も出る', vim.tbl_contains(words, '田中町'), true)

if failures > 0 then
  print(failures .. ' 件失敗')
  os.exit(1)
end
print('すべて通過')
os.exit(0)
