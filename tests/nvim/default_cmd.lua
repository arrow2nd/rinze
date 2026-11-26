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
local entries = vim.rpcrequest(chan, 'lookup', 'たなか', '')
local words = vim.tbl_map(function(entry)
  return type(entry) == 'table' and entry[1] or entry
end, entries)
check('既定の辞書の L の語が出る', vim.tbl_contains(words, '田仲'), true)
check('既定の辞書の geo の語も出る', vim.tbl_contains(words, '田中町'), true)

if failures > 0 then
  print(failures .. ' 件失敗')
  os.exit(1)
end
print('すべて通過')
os.exit(0)
