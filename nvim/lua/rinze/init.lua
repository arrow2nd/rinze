local M = {}

local config = {
  -- 接続できなかったときに起動するコマンド。`--socket` は自動で足す
  cmd = nil,
  socket = nil,
  startup_timeout = 30000,
  -- status() が返す、モードごとの表示
  mode_labels = {
    hiragana = 'かな',
    katakana = 'カナ',
    ['halfwidth-katakana'] = 'ｶﾅ',
    ascii = 'aA',
    ['fullwidth-latin'] = '全英',
  },
}

local ns = vim.api.nvim_create_namespace('rinze')
local state = {
  chan = nil,
  enabled = false,
  preedit = '',
  mode = nil,
  keys_mapped = false,
  float_buf = nil,
  float_win = nil,
}

local special_keys = {
  ['<Space>'] = 'space',
  ['<CR>'] = 'enter',
  ['<BS>'] = 'backspace',
  ['<C-g>'] = 'ctrl-g',
  ['<C-q>'] = 'ctrl-q',
}

local kakutei_key = '<C-j>'

local function try_connect(path)
  local ok, chan = pcall(vim.fn.sockconnect, 'pipe', path, { rpc = true })
  if ok and chan > 0 then
    return chan
  end
end

local EXIT_ANOTHER_RINZED_SERVES = 0

local function connect()
  -- 常駐プロセスが落ちて閉じた接続は情報が空になるので、つなぎ直す
  if state.chan and next(vim.api.nvim_get_chan_info(state.chan)) then
    return state.chan
  end
  local path = config.socket or require('rinze.socket').default()
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
    local function failed_to_start()
      return exit_code ~= nil and exit_code ~= EXIT_ANOTHER_RINZED_SERVES
    end
    vim.wait(config.startup_timeout, function()
      chan = try_connect(path)
      return chan ~= nil or failed_to_start()
    end, 50)
    if not chan then
      local reason = failed_to_start()
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

local function close_float()
  if state.float_win and vim.api.nvim_win_is_valid(state.float_win) then
    vim.api.nvim_win_close(state.float_win, true)
  end
  state.float_win = nil
end

local function present(v)
  return v ~= nil and v ~= vim.NIL
end

local function show_float(registration, candidates)
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
    close_float()
    return
  end
  if not (state.float_buf and vim.api.nvim_buf_is_valid(state.float_buf)) then
    state.float_buf = vim.api.nvim_create_buf(false, true)
  end
  vim.api.nvim_buf_set_lines(state.float_buf, 0, -1, false, lines)
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
  if state.float_win and vim.api.nvim_win_is_valid(state.float_win) then
    vim.api.nvim_win_set_config(state.float_win, opts)
  else
    opts.noautocmd = true
    state.float_win = vim.api.nvim_open_win(state.float_buf, false, opts)
  end
end

local map_input_keys, unmap_input_keys

local function follow_mode(res)
  if res.passthrough == state.keys_mapped then
    if res.passthrough then
      unmap_input_keys()
    else
      map_input_keys()
    end
  end
  if res.mode ~= state.mode then
    state.mode = res.mode
    vim.cmd.redrawstatus()
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
  show_float(res.registration, res.candidates)
  follow_mode(res)
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

local function each_input_key(f)
  for lhs, name in pairs(special_keys) do
    f(lhs, name)
  end
  for byte = 33, 126 do
    local c = string.char(byte)
    local lhs = ({ ['<'] = '<lt>', ['|'] = '<Bar>', ['\\'] = '<Bslash>' })[c] or c
    f(lhs, c)
  end
end

local function map_key(lhs, name)
  vim.keymap.set('i', lhs, function()
    on_key(name, lhs)
  end, { nowait = true, silent = true, desc = 'rinze' })
end

function map_input_keys()
  each_input_key(map_key)
  vim.keymap.set('i', '<Esc>', function()
    flush()
    vim.api.nvim_feedkeys(vim.keycode('<Esc>'), 'ni', false)
  end, { nowait = true, silent = true, desc = 'rinze' })
  state.keys_mapped = true
end

function unmap_input_keys()
  each_input_key(function(lhs)
    pcall(vim.keymap.del, 'i', lhs)
  end)
  pcall(vim.keymap.del, 'i', '<Esc>')
  state.keys_mapped = false
end

function M.setup(opts)
  config = vim.tbl_extend('force', config, opts or {})
  vim.api.nvim_set_hl(0, 'RinzePreedit', { default = true, link = 'Underlined' })
end

function M.enable()
  if state.enabled then
    return
  end
  local res = request('reset')
  map_key(kakutei_key, 'ctrl-j')
  vim.api.nvim_create_autocmd('InsertLeave', {
    group = vim.api.nvim_create_augroup('rinze', { clear = true }),
    callback = flush,
  })
  state.enabled = true
  follow_mode(res)
end

function M.disable()
  if not state.enabled then
    return
  end
  flush()
  unmap_input_keys()
  pcall(vim.keymap.del, 'i', kakutei_key)
  vim.api.nvim_del_augroup_by_name('rinze')
  state.enabled = false
  state.mode = nil
  vim.cmd.redrawstatus()
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

-- 有効な間は `hiragana` `katakana` `halfwidth-katakana` `ascii` `fullwidth-latin` のどれか、無効なら nil
function M.mode()
  return state.mode
end

-- ステータスラインに出す文字列。無効なら空
function M.status()
  return state.mode and config.mode_labels[state.mode] or ''
end

return M
