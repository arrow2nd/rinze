local root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h:h')
vim.opt.runtimepath:prepend(root .. '/nvim')

local socket = require('rinze.socket')
local table_path = root .. '/crates/rinze-socket/tests/fixtures/default_socket.json'
local cases = vim.json.decode(table.concat(vim.fn.readfile(table_path), '\n'), { luanil = { object = true } })

local function normalize(path)
  return path and vim.fs.normalize(path)
end

local failures = 0
for _, case in ipairs(cases) do
  local ok, got = pcall(socket.path, case.os, case.env)
  got = ok and normalize(got) or nil
  local want = normalize(case.socket)
  local pass = got == want
  print(string.format('%s %s: %s', pass and 'ok  ' or 'FAIL', case.name, vim.inspect(got)))
  if not pass then
    print('     期待: ' .. vim.inspect(want))
    failures = failures + 1
  end
end

if failures > 0 then
  print(failures .. ' 件失敗')
  os.exit(1)
end
print('すべて通過 (' .. #cases .. ' 件)')
os.exit(0)
