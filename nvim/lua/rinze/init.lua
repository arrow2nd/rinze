local M = {}

local config = {
  -- 接続できなかったときに起動するコマンド。`--socket` は自動で足す。
  cmd = { 'rinzed' },
  socket = nil,
  startup_timeout = 30000,
  -- true なら見出しの入力中に変換候補を blink.cmp のメニューに出す
  blink = false,
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
  anchor = nil,
  mode = nil,
  passthrough = false,
  held = nil,
  float_buf = nil,
  float_win = nil,
  blink = nil,
  held_commit = nil,
}

local special_keys = {
  ['<Space>'] = 'space',
  ['<CR>'] = 'enter',
  ['<BS>'] = 'backspace',
  ['<C-g>'] = 'ctrl-g',
  ['<C-q>'] = 'ctrl-q',
}

local function try_connect(path)
  local ok, chan = pcall(vim.fn.sockconnect, 'pipe', path, { rpc = true })
  if ok and chan > 0 then
    return chan
  end
end

local EXIT_ANOTHER_RINZED_SERVES = 0

local function socket_path()
  return config.socket or require('rinze.socket').default()
end

local function connect()
  -- 常駐プロセスが落ちて閉じた接続は情報が空になるので、つなぎ直す
  if state.chan and next(vim.api.nvim_get_chan_info(state.chan)) then
    return state.chan
  end
  local path = socket_path()
  local chan = try_connect(path)
  if not chan then
    if vim.fn.executable(config.cmd[1]) ~= 1 then
      error('rinze: ' .. path .. ' に接続できず、起動コマンド ' .. config.cmd[1] .. ' も見つからない (setup の cmd で指定する)')
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

local function show_float(minibuffer, candidates)
  local lines = {}
  if present(minibuffer) then
    table.insert(lines, minibuffer.prompt .. minibuffer.text .. minibuffer.preedit)
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

local input_keys = {}
local function add_key(lhs, name)
  local key = { lhs = lhs, lhsraw = vim.keycode(lhs), name = name, index = #input_keys + 1 }
  -- 右辺は Lua 関数でなく <Cmd> にする。blink.cmp がバッファローカルに割り当てたキーの fallback から呼ばれたとき、
  -- Lua 関数の右辺は vim.schedule で後回しにされ、続けて打ったキーと順序が入れ替わる
  key.rhs = string.format("<Cmd>lua require('rinze')._press(%d)<CR>", key.index)
  table.insert(input_keys, key)
  return key
end
for lhs, name in pairs(special_keys) do
  add_key(lhs, name)
end
for byte = 33, 126 do
  local c = string.char(byte)
  add_key(({ ['<'] = '<lt>', ['|'] = '<Bar>', ['\\'] = '<Bslash>' })[c] or c, c)
end
local kakutei = add_key('<C-j>', 'ctrl-j')
local escape = add_key('<Esc>', 'escape')

-- blink.cmp は自分のキーを挿入モードに入るときにバッファローカルに置き、fallback ではそのとき無かった
-- バッファローカルの割り当ての代わりに global な割り当てを探す。blink.cmp のキーは奪わず、rinze を global に置いて
-- fallback から rinze に回るようにする
local BLINK_DESC_PREFIX = 'blink.cmp: '
local OWN_DESC = 'rinze'

local function keymap_lookup()
  local by_scope = {}
  return function(buf, key)
    local scope = buf or 'global'
    if not by_scope[scope] then
      local by_lhs = {}
      for _, map in ipairs(buf and vim.api.nvim_buf_get_keymap(buf, 'i') or vim.api.nvim_get_keymap('i')) do
        -- <C-j> などは lhsraw が修飾キーつきの形になり、lhsrawalt が 1 バイトの形になる
        by_lhs[map.lhsraw] = map
        if map.lhsrawalt then
          by_lhs[map.lhsrawalt] = map
        end
      end
      by_scope[scope] = by_lhs
    end
    return by_scope[scope][key.lhsraw]
  end
end

local function take(buf, key, find)
  local scope = buf
  local original = find(buf, key)
  if original and vim.startswith(original.desc or '', BLINK_DESC_PREFIX) then
    scope = nil
    original = find(nil, key)
  end
  local opts = { noremap = true, nowait = true, silent = true, desc = OWN_DESC }
  if scope then
    vim.api.nvim_buf_set_keymap(scope, 'i', key.lhs, key.rhs, opts)
  else
    vim.api.nvim_set_keymap('i', key.lhs, key.rhs, opts)
  end
  return { buf = scope, original = original, aside = false }
end

local function restore(key, taken, find)
  if taken.buf and not vim.api.nvim_buf_is_valid(taken.buf) then
    return
  end
  local current = find(taken.buf, key)
  if not (current and current.desc == OWN_DESC) then
    return
  end
  if taken.buf then
    vim.api.nvim_buf_del_keymap(taken.buf, 'i', key.lhs)
  else
    vim.api.nvim_del_keymap('i', key.lhs)
  end
  if taken.original then
    -- mapset はバッファローカルの割り当てをカレントバッファに置く
    vim.api.nvim_buf_call(taken.buf or 0, function()
      vim.fn.mapset(taken.original)
    end)
  end
end

local function release(held, index, find)
  local taken = held.keys[index]
  held.keys[index] = nil
  if not taken.aside then
    restore(input_keys[index], taken, find)
  end
end

local function sync()
  local buf = vim.api.nvim_get_current_buf()
  local inserting = state.enabled and vim.api.nvim_get_mode().mode:match('^[iR]') ~= nil
  local held = state.held
  if held and (held.buf ~= buf or not inserting) then
    local find = keymap_lookup()
    for index in pairs(held.keys) do
      release(held, index, find)
    end
    held = nil
  end
  if inserting then
    held = held or { buf = buf, keys = {} }
    local find = keymap_lookup()
    for _, key in ipairs(input_keys) do
      local wanted = key == kakutei or not state.passthrough
      if wanted and not held.keys[key.index] then
        held.keys[key.index] = take(buf, key, find)
      elseif not wanted and held.keys[key.index] then
        release(held, key.index, find)
      end
    end
  end
  state.held = held
end

local function follow_mode(res)
  if res.passthrough ~= state.passthrough then
    state.passthrough = res.passthrough
    sync()
  end
  if res.mode ~= state.mode then
    state.mode = res.mode
    vim.cmd.redrawstatus()
  end
end

local function preedit_position()
  local anchor = state.anchor
  if not (anchor and vim.api.nvim_buf_is_valid(anchor.buf)) then
    return nil
  end
  local pos = vim.api.nvim_buf_get_extmark_by_id(anchor.buf, ns, anchor.mark, {})
  if #pos == 0 then
    return nil
  end
  return anchor.buf, pos[1], pos[2]
end

local function show(res, left_after_commit)
  if state.anchor and vim.api.nvim_buf_is_valid(state.anchor.buf) then
    vim.api.nvim_buf_clear_namespace(state.anchor.buf, ns, 0, -1)
  end
  state.anchor = nil
  state.preedit = res.preedit
  if res.preedit ~= '' then
    local row, col = unpack(vim.api.nvim_win_get_cursor(0))
    -- 右 gravity の印は、このあと先読みから入る確定文字列に押されて、その後ろへ動く
    local mark = vim.api.nvim_buf_set_extmark(0, ns, row - 1, col, {
      virt_text = { { res.preedit, 'RinzePreedit' } },
      virt_text_pos = 'inline',
      right_gravity = true,
    })
    state.anchor = { buf = vim.api.nvim_get_current_buf(), mark = mark }
  end
  show_float(res.minibuffer, res.candidates)
  follow_mode(res)
  if state.blink then
    state.blink.update(res, left_after_commit)
  end
end

-- 確定文字列は打鍵として先読みに入れる。API で書き込むと `.` で繰り返せず、undo も打鍵と別の単位になる。
-- 'i' で先読み中のキーより前に差し込むので、後に入れるものから先に送る
local function type_ahead(commit)
  if commit ~= '' then
    vim.api.nvim_feedkeys(commit, 'ni', true)
  end
end

local function pass_through(key, commit)
  local taken = state.held and state.held.keys[key.index]
  local find = keymap_lookup()
  local original = taken and not taken.aside and (taken.original or (taken.buf and find(nil, key)))
  if original then
    taken.aside = true
    restore(key, taken, find)
    vim.api.nvim_feedkeys(vim.keycode(string.format("<Cmd>lua require('rinze')._resume(%d)<CR>", key.index)), 'ni', false)
    vim.api.nvim_feedkeys(key.lhsraw, 'mi', false)
  else
    vim.api.nvim_feedkeys(key.lhsraw, 'ni', false)
  end
  type_ahead(commit)
end

local function commit_or_hold(commit)
  if state.held_commit then
    state.held_commit = state.held_commit .. commit
  else
    type_ahead(commit)
  end
end

local function left_of_cursor()
  local col = vim.api.nvim_win_get_cursor(0)[2]
  return vim.api.nvim_get_current_line():sub(1, col)
end

local function flush_in_place()
  if not state.chan then
    return
  end
  local ok, res = pcall(request, 'flush')
  if not ok then
    return
  end
  local buf, row, col = preedit_position()
  if res.commit ~= '' and buf then
    vim.api.nvim_buf_set_text(buf, row, col, row, col, { res.commit })
    local cursor = vim.api.nvim_win_get_cursor(0)
    if buf == vim.api.nvim_get_current_buf() and cursor[1] == row + 1 and cursor[2] >= col then
      vim.api.nvim_win_set_cursor(0, { cursor[1], cursor[2] + #res.commit })
    end
  end
  show(res)
end

local function flush_if_cursor_left_preedit()
  local buf, row, col = preedit_position()
  if not buf then
    return
  end
  local cursor = vim.api.nvim_win_get_cursor(0)
  if buf ~= vim.api.nvim_get_current_buf() or cursor[1] ~= row + 1 or cursor[2] ~= col then
    flush_in_place()
  end
end

local function choose(version, word)
  local left = left_of_cursor()
  local ok, res = pcall(request, 'choose', version, word, left)
  if not ok then
    vim.notify(tostring(res), vim.log.levels.ERROR)
    return
  end
  commit_or_hold(res.commit)
  show(res, left .. res.commit)
end

local function settle_menu(name)
  if not state.blink then
    return '', false
  end
  -- settle の vim.wait(0) は、blink.cmp が後回しにしたほかのキーの処理も走らせ、その中でもここを通る
  local outer = state.held_commit
  state.held_commit = ''
  local ok, consumed = pcall(state.blink.settle, name)
  local held = state.held_commit
  state.held_commit = outer
  if not ok then
    vim.notify(tostring(consumed), vim.log.levels.ERROR)
    consumed = false
  end
  return held, consumed
end

local function on_key(key)
  local held, consumed = settle_menu(key.name)
  if consumed then
    type_ahead(held)
    return
  end
  -- CursorMovedI は先読みが尽きるまで起きないので、続けて打たれたキーの前でも確かめる
  flush_if_cursor_left_preedit()
  local left = left_of_cursor() .. held
  local ok, res = pcall(request, 'key', key.name, left)
  if not ok then
    vim.notify(tostring(res), vim.log.levels.ERROR)
    pass_through(key, held)
    return
  end
  if res.handled then
    type_ahead(held .. res.commit)
  else
    pass_through(key, held .. res.commit)
  end
  show(res, left .. res.commit)
end

local function on_escape()
  local held = settle_menu('escape')
  local ok, res = pcall(request, 'flush')
  pass_through(escape, held .. (ok and res.commit or ''))
  if ok then
    show(res)
  end
end

function M._press(index)
  local key = input_keys[index]
  local taken = state.held and state.held.keys[index]
  -- blink.cmp の fallback は、挿入モードに入ったときにあった rinze の割り当てを覚えていて、rinze が手を離したあとも呼ぶ
  if not taken or taken.aside then
    vim.api.nvim_feedkeys(key.lhsraw, 'ni', false)
  elseif key == escape then
    on_escape()
  else
    on_key(key)
  end
end

function M._resume(index)
  local taken = state.held and state.held.keys[index]
  if taken and taken.aside then
    state.held.keys[index] = nil
  end
  sync()
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
  if config.blink then
    state.blink = require('rinze.blink')
    state.blink.attach(socket_path(), { choose = choose })
  end
  local group = vim.api.nvim_create_augroup('rinze', { clear = true })
  -- ModeChanged は InsertEnter のあとに起きるので、InsertEnter で割り当てを置くプラグインより後に置ける。
  -- 同じ文字で始まる長い割り当てより後に置いた <nowait> だけが、続きを待たずに働く
  vim.api.nvim_create_autocmd({ 'ModeChanged', 'BufEnter' }, {
    group = group,
    callback = sync,
  })
  -- 挿入モードのまま別のウィンドウへ移ったときは InsertLeave が起きない
  vim.api.nvim_create_autocmd({ 'InsertLeave', 'WinLeave', 'BufLeave' }, {
    group = group,
    callback = flush_in_place,
  })
  vim.api.nvim_create_autocmd('CursorMovedI', {
    group = group,
    callback = flush_if_cursor_left_preedit,
  })
  state.enabled = true
  follow_mode(res)
  sync()
end

function M.disable()
  if not state.enabled then
    return
  end
  flush_in_place()
  state.enabled = false
  sync()
  vim.api.nvim_del_augroup_by_name('rinze')
  if state.blink then
    state.blink.detach()
    state.blink = nil
  end
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
