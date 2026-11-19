local M = {}

local config = {
  -- 接続できなかったときに起動するコマンド。`--socket` は自動で足す
  cmd = nil,
  socket = nil,
  startup_timeout = 30000,
}

local ns = vim.api.nvim_create_namespace('rinze')
local state = {
  chan = nil,
  enabled = false,
  preedit = '',
  list_buf = nil,
  list_win = nil,
}

local special_keys = {
  ['<Space>'] = 'space',
  ['<CR>'] = 'enter',
  ['<BS>'] = 'backspace',
  ['<C-g>'] = 'ctrl-g',
}

local function default_socket()
  if vim.fn.has('mac') == 1 then
    return vim.env.HOME .. '/Library/Caches/rinze/rinze.sock'
  end
  local dir = vim.env.XDG_RUNTIME_DIR
  if dir and dir ~= '' then
    return dir .. '/rinze.sock'
  end
  return '/tmp/rinze-' .. (vim.env.USER or '') .. '.sock'
end

local function try_connect(path)
  local ok, chan = pcall(vim.fn.sockconnect, 'pipe', path, { rpc = true })
  if ok and chan > 0 then
    return chan
  end
end

local function connect()
  -- 常駐プロセスが落ちて閉じた接続は情報が空になるので、つなぎ直す
  if state.chan and next(vim.api.nvim_get_chan_info(state.chan)) then
    return state.chan
  end
  local path = config.socket or default_socket()
  local chan = try_connect(path)
  if not chan then
    if not config.cmd then
      error('rinze: ' .. path .. ' に接続できず、起動コマンド (cmd) も設定されていない')
    end
    local cmd = vim.list_extend(vim.deepcopy(config.cmd), { '--socket', path })
    local exit_code, stderr = nil, {}
    local job = vim.fn.jobstart(cmd, {
      detach = true,
      stderr_buffered = true,
      on_stderr = function(_, data)
        stderr = data
      end,
      on_exit = function(_, code)
        exit_code = code
      end,
    })
    if job <= 0 then
      error('rinze: 常駐プロセスを起動できない: ' .. table.concat(cmd, ' '))
    end
    vim.wait(config.startup_timeout, function()
      chan = try_connect(path)
      return chan ~= nil or (exit_code ~= nil and exit_code ~= 0)
    end, 50)
    if not chan then
      local reason = (exit_code ~= nil and exit_code ~= 0)
          and ('終了した: ' .. vim.trim(table.concat(stderr, '\n')))
        or (config.startup_timeout .. 'ms 以内に待ち受けを始めなかった')
      error('rinze: 常駐プロセスが' .. reason)
    end
  end
  state.chan = chan
  return chan
end

local function request(method, ...)
  local ok, res = pcall(vim.rpcrequest, connect(), method, ...)
  if not ok then
    state.chan = nil
    error(res)
  end
  return res
end

local function close_list()
  if state.list_win and vim.api.nvim_win_is_valid(state.list_win) then
    vim.api.nvim_win_close(state.list_win, true)
  end
  state.list_win = nil
end

local function present(v)
  return v ~= nil and v ~= vim.NIL
end

local function show_list(registration, candidates)
  local lines = {}
  if present(registration) then
    table.insert(lines, registration.prompt .. registration.text .. registration.preedit)
  end
  if present(candidates) then
    local parts = {}
    for _, item in ipairs(candidates.items) do
      table.insert(parts, item[1] .. ':' .. item[2])
    end
    table.insert(lines, table.concat(parts, '  ') .. string.format('  [%d/%d]', candidates.page + 1, candidates.pages))
  end
  if #lines == 0 then
    close_list()
    return
  end
  if not (state.list_buf and vim.api.nvim_buf_is_valid(state.list_buf)) then
    state.list_buf = vim.api.nvim_create_buf(false, true)
  end
  vim.api.nvim_buf_set_lines(state.list_buf, 0, -1, false, lines)
  local width = 1
  for _, line in ipairs(lines) do
    width = math.max(width, vim.fn.strdisplaywidth(line))
  end
  local opts = {
    relative = 'cursor',
    row = 1,
    col = 0,
    width = width,
    height = #lines,
    style = 'minimal',
    focusable = false,
  }
  if state.list_win and vim.api.nvim_win_is_valid(state.list_win) then
    vim.api.nvim_win_set_config(state.list_win, opts)
  else
    opts.noautocmd = true
    state.list_win = vim.api.nvim_open_win(state.list_buf, false, opts)
  end
end

local function render(res)
  local row, col = unpack(vim.api.nvim_win_get_cursor(0))
  if res.commit ~= '' then
    vim.api.nvim_buf_set_text(0, row - 1, col, row - 1, col, { res.commit })
    col = col + #res.commit
    vim.api.nvim_win_set_cursor(0, { row, col })
  end
  vim.api.nvim_buf_clear_namespace(0, ns, 0, -1)
  state.preedit = res.preedit
  if res.preedit ~= '' then
    vim.api.nvim_buf_set_extmark(0, ns, row - 1, col, {
      virt_text = { { res.preedit, 'RinzePreedit' } },
      virt_text_pos = 'inline',
    })
  end
  show_list(res.registration, res.candidates)
end

local function on_key(name, lhs)
  local col = vim.api.nvim_win_get_cursor(0)[2]
  local left = vim.api.nvim_get_current_line():sub(1, col)
  local ok, res = pcall(request, 'key', name, left)
  if not ok then
    vim.notify(tostring(res), vim.log.levels.ERROR)
    vim.api.nvim_feedkeys(vim.keycode(lhs), 'ni', false)
    return
  end
  render(res)
  if not res.handled then
    -- 'i' で先読み中のキーより前に差し込み、打った順を保つ
    vim.api.nvim_feedkeys(vim.keycode(lhs), 'ni', false)
  end
end

local function flush()
  if not state.chan then
    return
  end
  local ok, res = pcall(request, 'flush')
  if ok then
    render(res)
  end
end

local function each_key(f)
  for lhs, name in pairs(special_keys) do
    f(lhs, name)
  end
  for byte = 33, 126 do
    local c = string.char(byte)
    local lhs = ({ ['<'] = '<lt>', ['|'] = '<Bar>', ['\\'] = '<Bslash>' })[c] or c
    f(lhs, c)
  end
end

function M.setup(opts)
  config = vim.tbl_extend('force', config, opts or {})
  vim.api.nvim_set_hl(0, 'RinzePreedit', { default = true, link = 'Underlined' })
end

function M.enable()
  if state.enabled then
    return
  end
  connect()
  each_key(function(lhs, name)
    vim.keymap.set('i', lhs, function()
      on_key(name, lhs)
    end, { nowait = true, silent = true, desc = 'rinze' })
  end)
  vim.keymap.set('i', '<Esc>', function()
    flush()
    vim.api.nvim_feedkeys(vim.keycode('<Esc>'), 'ni', false)
  end, { nowait = true, silent = true, desc = 'rinze' })
  vim.api.nvim_create_autocmd('InsertLeave', {
    group = vim.api.nvim_create_augroup('rinze', { clear = true }),
    callback = flush,
  })
  state.enabled = true
end

function M.disable()
  if not state.enabled then
    return
  end
  flush()
  each_key(function(lhs)
    pcall(vim.keymap.del, 'i', lhs)
  end)
  pcall(vim.keymap.del, 'i', '<Esc>')
  vim.api.nvim_del_augroup_by_name('rinze')
  state.enabled = false
end

function M.toggle()
  if state.enabled then
    M.disable()
  else
    M.enable()
  end
end

function M.preedit()
  return state.preedit
end

return M
