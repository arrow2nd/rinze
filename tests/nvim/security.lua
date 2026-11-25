-- セキュリティ監査の回帰テスト。偽の rinzed ソケットを使い、モデル無しで Neovim 側の防御を確かめる。
local root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h:h')
vim.opt.runtimepath:prepend(root .. '/nvim')
vim.cmd.runtime('plugin/rinze.lua')

local rinze = require('rinze')

local work = vim.fn.tempname()
vim.fn.mkdir(work, 'p')

local failures = 0
local function check(label, got, want)
  local ok = vim.deep_equal(got, want)
  print(string.format('%s %s: %s', ok and 'ok  ' or 'FAIL', label, vim.inspect(got)))
  if not ok then
    print('     期待: ' .. vim.inspect(want))
    failures = failures + 1
  end
end

local function start_server(path, on_request)
  os.remove(path)
  local server = assert(vim.uv.new_pipe(false))
  assert(server:bind(path))
  local clients = {}
  assert(server:listen(16, function(err)
    if err then
      return
    end
    local conn = assert(vim.uv.new_pipe(false))
    table.insert(clients, conn)
    server:accept(conn)
    local unpacker = vim.mpack.Unpacker()
    conn:read_start(function(read_err, data)
      if read_err or not data then
        if not conn:is_closing() then
          conn:close()
        end
        return
      end
      local pos = 1
      while pos <= #data do
        local ok, msg, next_pos = pcall(unpacker, data, pos)
        if not ok or msg == nil then
          break
        end
        pos = next_pos
        on_request(conn, msg)
      end
    end)
  end))
  return function()
    for _, conn in ipairs(clients) do
      if not conn:is_closing() then
        conn:close()
      end
    end
    if not server:is_closing() then
      server:close()
    end
    os.remove(path)
  end
end

local function response(id, overrides)
  local res = vim.tbl_extend('force', {
    handled = true,
    commit = '',
    preedit = '',
    candidates = vim.NIL,
    minibuffer = vim.NIL,
    mode = 'hiragana',
    passthrough = false,
    reading = vim.NIL,
    version = id,
  }, overrides or {})
  return vim.mpack.encode({ 1, id, vim.NIL, res })
end

-- 1. 確定文字列に制御文字 (ESC や CR) が含まれても挿入モードを抜けず Ex コマンドを実行しない
do
  local marker = work .. '/marker.txt'
  local payload = 'V\27:call writefile(["injected"], "' .. marker .. '")\rq'
  local sock = work .. '/ctrl.sock'
  local stop = start_server(sock, function(conn, msg)
    local id, method = msg[2], msg[3]
    if method == 'key' then
      conn:write(response(id, { commit = payload }))
    else
      conn:write(response(id))
    end
  end)
  rinze.setup({ cmd = { '/nonexistent' }, socket = sock, request_timeout = 2000 })
  vim.cmd('enew!')
  local mode_after = nil
  vim.keymap.set('i', '<F12>', function()
    mode_after = vim.api.nvim_get_mode().mode
  end, { buffer = 0 })
  rinze.enable()
  vim.api.nvim_feedkeys(vim.keycode('ia<F12><Esc>'), 'x', false)
  local line = vim.api.nvim_buf_get_lines(0, 0, -1, false)[1]
  rinze.disable()
  stop()
  check('制御文字を含む確定文字列で挿入モードを抜けない', mode_after, 'i')
  check('制御文字を含む確定文字列で Ex コマンドが実行されない', vim.fn.filereadable(marker), 0)
  check('制御文字が打鍵でなく文字として入る', line, payload)
end

-- 1b. 不正な UTF-8 (生の 0x80 K_SPECIAL <Cmd>) や NUL を含む確定文字列でコマンドが実行されない
do
  T = { kspecial = false }
  local payload = 'X\128\253hlua T.kspecial = true --\rY\0Z'
  local sock = work .. '/kspecial.sock'
  local stop = start_server(sock, function(conn, msg)
    local id, method = msg[2], msg[3]
    if method == 'key' then
      conn:write(response(id, { commit = payload }))
    else
      conn:write(response(id))
    end
  end)
  rinze.setup({ cmd = { '/nonexistent' }, socket = sock, request_timeout = 2000 })
  vim.cmd('enew!')
  local mode_after = nil
  vim.keymap.set('i', '<F12>', function()
    mode_after = vim.api.nvim_get_mode().mode
  end, { buffer = 0 })
  rinze.enable()
  vim.api.nvim_feedkeys(vim.keycode('ia<F12><Esc>'), 'x', false)
  local line = vim.api.nvim_buf_get_lines(0, 0, -1, false)[1]
  rinze.disable()
  stop()
  check('不正な UTF-8 K_SPECIAL で挿入モードを抜けない', mode_after, 'i')
  check('不正な UTF-8 K_SPECIAL で Lua が実行されない', T.kspecial, false)
  check('不正な UTF-8 と NUL を除いた文字が入る', line, 'Xhlua T.kspecial = true --\rYZ')
end

-- 2. なりすましプロセスが nvim_exec_lua を送り返しても実行されず接続を拒む
do
  T = { pwned = false }
  local sock = work .. '/impostor.sock'
  local stop = start_server(sock, function(conn)
    conn:write(vim.mpack.encode({ 0, 1, 'nvim_exec_lua', { 'T.pwned = true', {} } }))
  end)
  rinze.setup({ cmd = { '/nonexistent' }, socket = sock, request_timeout = 1000 })
  local ok, err = pcall(rinze.enable)
  stop()
  check('応答以外のメッセージを送る相手は拒む', ok, false)
  check('拒否の理由にメッセージ種別が出る', tostring(err):find('応答以外', 1, true) ~= nil, true)
  check('相手からの nvim_exec_lua は実行されない', T.pwned, false)
end

-- 2b. 通知メッセージ (kind == 2) も拒む
do
  local sock = work .. '/notify.sock'
  local stop = start_server(sock, function(conn)
    conn:write(vim.mpack.encode({ 2, 'notify', {} }))
  end)
  rinze.setup({ cmd = { '/nonexistent' }, socket = sock, request_timeout = 1000 })
  local ok, err = pcall(rinze.enable)
  stop()
  check('通知メッセージを送る相手は拒む', ok, false)
  check(
    '通知拒否の理由にメッセージ種別が出る',
    tostring(err):find('応答以外', 1, true) ~= nil,
    true
  )
end

-- 3. 応答しないプロセスには request_timeout で打ち切る
do
  local sock = work .. '/hang.sock'
  local stop = start_server(sock, function() end)
  rinze.setup({ cmd = { '/nonexistent' }, socket = sock, request_timeout = 150 })
  local started = vim.uv.hrtime()
  local ok, err = pcall(rinze.enable)
  local elapsed_ms = (vim.uv.hrtime() - started) / 1e6
  stop()
  check('応答しない相手には時間切れで失敗する', ok, false)
  check(
    '時間切れの理由と所要時間',
    { tostring(err):find('応答しなかった', 1, true) ~= nil, elapsed_ms < 1000 },
    { true, true }
  )
end

-- 4. 上限 (1 MiB) を超える応答は接続を閉じる
do
  local sock = work .. '/huge.sock'
  local big = string.rep('a', 64 * 1024)
  local stop = start_server(sock, function(conn)
    -- str32 ヘッダ (2 MiB) + 1.1 MiB の本体を送る
    conn:write('\xdb\x00\x20\x00\x00')
    for _ = 1, 18 do
      conn:write(big)
    end
  end)
  rinze.setup({ cmd = { '/nonexistent' }, socket = sock, request_timeout = 2000 })
  local ok, err = pcall(rinze.enable)
  stop()
  check('上限を超える応答は拒む', ok, false)
  check('大きすぎる応答の理由が出る', tostring(err):find('大きすぎる', 1, true) ~= nil, true)
end

-- 5. 通常ファイルやシンボリックリンクには接続せず自動起動もしない
do
  local plain = work .. '/plain.sock'
  vim.fn.writefile({ 'not a socket' }, plain)
  rinze.setup({ cmd = { '/nonexistent' }, socket = plain, request_timeout = 1000 })
  local ok, err = pcall(rinze.enable)
  check('通常ファイルをソケットとして信じない', ok, false)
  check('ソケットでない理由が出る', tostring(err):find('ソケットでない', 1, true) ~= nil, true)

  local real_sock = work .. '/real.sock'
  local link_sock = work .. '/link.sock'
  local stop = start_server(real_sock, function(conn, msg)
    conn:write(response(msg[2]))
  end)
  assert(vim.uv.fs_symlink(real_sock, link_sock))
  rinze.setup({ cmd = { '/nonexistent' }, socket = link_sock, request_timeout = 1000 })
  local link_ok, link_err = pcall(rinze.enable)
  stop()
  check('シンボリックリンクをソケットとして信じない', link_ok, false)
  check(
    'シンボリックリンク拒否の理由が出る',
    tostring(link_err):find('ソケットでない', 1, true) ~= nil,
    true
  )
end

vim.fn.delete(work, 'rf')
if failures > 0 then
  print(failures .. ' 件失敗')
  os.exit(1)
end
print('すべて通過')
os.exit(0)
