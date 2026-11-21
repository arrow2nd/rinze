-- rinzed との片方向の msgpack-rpc 通信。応答 (kind == 1) だけを読み、相手からの要求は実行しない。
local socket = require('rinze.socket')

local M = {}

M.MAX_MESSAGE_BYTES = 1024 * 1024

local CONNECT_TIMEOUT_MS = 2000

-- 届いたチャンクを先頭から読み直さずに msgpack の応答を取り出す。
function M.new_decoder(on_message, on_error)
  local unpacker = vim.mpack.Unpacker()
  local msg_bytes = 0
  return function(data)
    local pos = 1
    while pos <= #data do
      local ok, message, next_pos = pcall(unpacker, data, pos)
      if not ok then
        on_error('rinzed の応答が壊れている')
        return
      end
      msg_bytes = msg_bytes + (next_pos or (#data + 1)) - pos
      if msg_bytes > M.MAX_MESSAGE_BYTES then
        on_error('rinzed の応答が大きすぎる')
        return
      end
      if message == nil then
        break
      end
      msg_bytes = 0
      pos = next_pos
      if type(message) ~= 'table' or message[1] ~= 1 then
        on_error('rinzed から応答以外のメッセージが届いた')
        return
      end
      on_message(message)
    end
  end
end

local function close_pipe(pipe)
  if pipe and not pipe:is_closing() then
    pipe:close()
  end
end

local Client = {}
Client.__index = Client

function Client:is_open()
  return self.pipe ~= nil and not self.pipe:is_closing() and self.closed_err == nil
end

function Client:close(reason)
  close_pipe(self.pipe)
  self.pipe = nil
  if reason and self.closed_err == nil then
    self.closed_err = reason
  end
end

function Client:request(method, params, timeout_ms)
  if not self:is_open() then
    error(self.closed_err or 'rinzed との接続が閉じた')
  end
  self.next_id = self.next_id + 1
  local id = self.next_id
  local pending = { id = id, done = false, err = nil, result = nil }
  self.pending = pending
  self.pipe:write(vim.mpack.encode({ 0, id, method, params }))
  vim.wait(timeout_ms, function()
    return pending.done or self.closed_err ~= nil
  end, 10, true)
  self.pending = nil
  if not pending.done then
    local reason = self.closed_err or ('rinzed が ' .. timeout_ms .. 'ms 以内に応答しなかった')
    self:close(reason)
    error(reason)
  end
  if pending.err ~= nil and pending.err ~= vim.NIL then
    error(tostring(pending.err))
  end
  return pending.result
end

-- ソケットの所有者を確かめて接続する。所有者が違うなどの拒否は第 3 戻り値を true で返す。
function M.connect(path)
  local ok_owner, owner_err = socket.verify_owner(path)
  if not ok_owner then
    return nil, owner_err, owner_err ~= 'not_found'
  end
  local pipe = assert(vim.uv.new_pipe(false))
  local done, connect_err = false, nil
  pipe:connect(path, function(err)
    connect_err = err
    done = true
  end)
  vim.wait(CONNECT_TIMEOUT_MS, function()
    return done
  end, 10, true)
  if not done or connect_err then
    close_pipe(pipe)
    return nil, connect_err or '接続が時間切れになった', false
  end
  local client = setmetatable({
    pipe = pipe,
    next_id = 0,
    pending = nil,
    closed_err = nil,
  }, Client)
  local decode = M.new_decoder(function(message)
    local pending = client.pending
    if pending and message[2] == pending.id then
      pending.err = message[3]
      pending.result = message[4]
      pending.done = true
    end
  end, function(reason)
    client:close(reason)
  end)
  pipe:read_start(function(read_err, data)
    if read_err or not data then
      client:close(read_err or 'rinzed との接続が閉じた')
      return
    end
    decode(data)
  end)
  return client
end

return M
