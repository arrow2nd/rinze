local rinzed, model, dict, work, rounds, history_lines = unpack(arg)
rounds, history_lines = tonumber(rounds), tonumber(history_lines)

local function say(fmt, ...)
  io.stdout:write(string.format(fmt, ...), '\n')
end

local large_history = work .. '/large.tsv'
local lines = {}
for i = 1, history_lines do
  table.insert(lines, string.format('よみ%d\t語%d\t%d', i, i, 1700000000000 + i))
end
vim.fn.writefile(lines, large_history)

local instances = {
  { name = '学習なし', args = { '--no-learning' } },
  { name = '空の履歴', args = { '--history', work .. '/empty.tsv' } },
  { name = string.format('%d 行の履歴', history_lines), args = { '--history', large_history } },
}

for i, instance in ipairs(instances) do
  local socket = string.format('%s/%d.sock', work, i)
  local cmd = { rinzed, '--model', model, '--dict', dict, '--user-dict', string.format('%s/%d.jisyo', work, i), '--socket', socket }
  vim.list_extend(cmd, instance.args)
  local exited, stderr = false, {}
  instance.job = vim.fn.jobstart(cmd, {
    detach = true,
    stderr_buffered = true,
    on_stderr = function(_, data)
      stderr = data
    end,
    on_exit = function()
      exited = true
    end,
  })
  vim.wait(60000, function()
    local ok, chan = pcall(vim.fn.sockconnect, 'pipe', socket, { rpc = true })
    instance.chan = ok and chan > 0 and chan or nil
    return instance.chan ~= nil or exited
  end, 100)
  if not instance.chan then
    say('%s の rinzed に接続できない: %s', instance.name, table.concat(stderr, '\n'))
    os.exit(1)
  end
end

local left = 'これで'

local function key(instance, name)
  return vim.rpcrequest(instance.chan, 'key', name, left)
end

local function time_commit(instance, send)
  vim.rpcrequest(instance.chan, 'reset')
  for _, k in ipairs({ 'A', 'T', 't' }) do
    key(instance, k)
  end
  if send then
    key(instance, 'space')
  end
  local started = vim.uv.hrtime()
  local res = key(instance, 'e')
  local elapsed = (vim.uv.hrtime() - started) / 1e6
  assert(res.commit ~= '', '確定していない: ' .. vim.inspect(res))
  return elapsed, res.commit
end

local samples = {}
for _, instance in ipairs(instances) do
  samples[instance.name] = { sent = {}, first = {} }
end
local committed = {}
for _ = 1, rounds do
  for _, instance in ipairs(instances) do
    for _, case in ipairs({ { 'sent', true }, { 'first', false } }) do
      local ms, commit = time_commit(instance, case[2])
      table.insert(samples[instance.name][case[1]], ms)
      committed[commit] = (committed[commit] or 0) + 1
    end
  end
end

local function stats(v)
  table.sort(v)
  return string.format('中央値 %.2f ms (最小 %.2f、最大 %.2f、%d 回)', v[math.ceil(#v / 2)], v[1], v[#v], #v)
end

for _, instance in ipairs(instances) do
  say('%s: 選び直した候補の確定 %s', instance.name, stats(samples[instance.name].sent))
  say('%s: 最初の候補の確定 %s', instance.name, stats(samples[instance.name].first))
  vim.fn.jobstop(instance.job)
end
say('確定した文字列: %s', vim.inspect(committed))
local recorded = vim.fn.readfile(work .. '/empty.tsv')
say('空の履歴に書いた行: %s', vim.inspect(vim.list_slice(recorded, 2)))
os.exit(0)
