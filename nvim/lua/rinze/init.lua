local M = {}

local rpc = require('rinze.rpc')

local config = {
  -- 接続できなかったときに起動するコマンド。`--socket` は自動で足す。
  cmd = { 'rinzed' },
  socket = nil,
  startup_timeout = 30000,
  request_timeout = 10000,
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

local EXIT_ANOTHER_RINZED_SERVES = 0
local STDERR_MAX_LINES = 4
local MAX_LEFT_CONTEXT_CHARS = 40
local MAX_ANNOTATION_WIDTH = 48

local function socket_path()
  return config.socket or require('rinze.socket').default()
end

local function append_stderr(state, data)
  if not state.active or not data then
    return
  end
  for i, chunk in ipairs(data) do
    if i == 1 and #state.lines > 0 then
      state.lines[#state.lines] = state.lines[#state.lines] .. chunk
    else
      table.insert(state.lines, chunk)
    end
  end
  while #state.lines > STDERR_MAX_LINES + 1 do
    table.remove(state.lines, 1)
  end
end

local function spawn_rinzed(path)
  if vim.fn.executable(config.cmd[1]) ~= 1 then
    error(
      'rinze: '
        .. path
        .. ' に接続できず、起動コマンド '
        .. config.cmd[1]
        .. ' も見つからない (setup の cmd で指定する)'
    )
  end
  local cmd = vim.list_extend(vim.deepcopy(config.cmd), { '--socket', path })
  local exit_code = nil
  local stderr = { active = true, lines = {} }
  local job = vim.fn.jobstart(cmd, {
    detach = true,
    on_stderr = function(_, data)
      append_stderr(stderr, data)
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
  local chan, fatal_err = nil, nil
  vim.wait(config.startup_timeout, function()
    local c, err, fatal = rpc.connect(path)
    if fatal then
      fatal_err = err
      return true
    end
    chan = c
    return chan ~= nil or failed_to_start()
  end, 50)
  stderr.active = false
  if fatal_err then
    error(fatal_err)
  end
  if not chan then
    local reason = failed_to_start() and ('終了した: ' .. vim.trim(table.concat(stderr.lines, '\n')))
      or (config.startup_timeout .. 'ms 以内に待ち受けを始めなかった')
    error('rinze: 常駐プロセスが' .. reason)
  end
  stderr.lines = {}
  return chan
end

-- rinzed は接続ごとに入力状態を持つので、入力の場所ごとに接続を分ける
local function connect(session)
  -- 常駐プロセスが落ちて閉じた接続はつなぎ直す
  if session.chan and session.chan:is_open() then
    return session.chan
  end
  local path = socket_path()
  local chan, err, fatal = rpc.connect(path)
  if fatal then
    error(err)
  end
  session.chan = chan or spawn_rinzed(path)
  return session.chan
end

local function disconnect(session)
  if session.chan then
    pcall(session.chan.close, session.chan)
    session.chan = nil
  end
end

local function request(session, method, ...)
  local chan = connect(session)
  local ok, res = pcall(chan.request, chan, method, { ... }, config.request_timeout)
  if not ok then
    disconnect(session)
    error(res)
  end
  return res
end

local function close_float(session)
  if session.float_win and vim.api.nvim_win_is_valid(session.float_win) then
    vim.api.nvim_win_close(session.float_win, true)
  end
  session.float_win = nil
end

local function present(v)
  return v ~= nil and v ~= vim.NIL
end

local function truncate_width(text, max_width)
  if vim.fn.strdisplaywidth(text) <= max_width then
    return text
  end
  local out = ''
  local width = 0
  local total = vim.fn.strchars(text)
  for i = 0, total - 1 do
    local ch = vim.fn.strcharpart(text, i, 1)
    local w = vim.fn.strdisplaywidth(ch)
    if width + w > max_width - 1 then
      break
    end
    out = out .. ch
    width = width + w
  end
  return out .. '…'
end

local function append_candidate_lines(lines, highlights, candidates)
  local max_word_width = 0
  for _, item in ipairs(candidates.items) do
    max_word_width = math.max(max_word_width, vim.fn.strdisplaywidth(item[2]))
  end
  for _, item in ipairs(candidates.items) do
    local prefix = item[1] .. ': ' .. item[2]
    local note = item[3]
    if present(note) and note ~= '' then
      local pad = string.rep(' ', max_word_width - vim.fn.strdisplaywidth(item[2]) + 2)
      local shown_note = truncate_width(note, MAX_ANNOTATION_WIDTH)
      local line = prefix .. pad .. shown_note
      table.insert(lines, line)
      table.insert(highlights, { #lines - 1, #prefix + #pad, #line })
    else
      table.insert(lines, prefix)
    end
  end
  local page_line = string.format('[%d/%d]', candidates.page + 1, candidates.pages)
  table.insert(lines, page_line)
  table.insert(highlights, { #lines - 1, 0, #page_line })
end

local function build_float_content(minibuffer, candidates, annotation)
  local lines = {}
  local highlights = {}
  if present(minibuffer) then
    table.insert(lines, minibuffer.prompt .. minibuffer.text .. minibuffer.preedit)
  end
  if present(candidates) then
    append_candidate_lines(lines, highlights, candidates)
  elseif present(annotation) and annotation ~= '' then
    local shown_note = truncate_width(annotation, MAX_ANNOTATION_WIDTH)
    table.insert(lines, shown_note)
    table.insert(highlights, { #lines - 1, 0, #shown_note })
  end
  return lines, highlights
end

local function show_float(session, minibuffer, candidates, annotation)
  local lines, highlights = build_float_content(minibuffer, candidates, annotation)
  if #lines == 0 then
    close_float(session)
    return
  end
  if not (session.float_buf and vim.api.nvim_buf_is_valid(session.float_buf)) then
    session.float_buf = vim.api.nvim_create_buf(false, true)
  end
  vim.api.nvim_buf_set_lines(session.float_buf, 0, -1, false, lines)
  vim.api.nvim_buf_clear_namespace(session.float_buf, ns, 0, -1)
  for _, hl in ipairs(highlights) do
    vim.api.nvim_buf_set_extmark(session.float_buf, ns, hl[1], hl[2], {
      end_col = hl[3],
      hl_group = 'RinzeAnnotation',
    })
  end
  local width = 1
  for _, line in ipairs(lines) do
    width = math.max(width, vim.fn.strdisplaywidth(line))
  end
  local opts = session.surface.float_config(width, #lines)
  if session.float_win and vim.api.nvim_win_is_valid(session.float_win) then
    vim.api.nvim_win_set_config(session.float_win, opts)
  else
    opts.noautocmd = true
    session.float_win = vim.api.nvim_open_win(session.float_buf, false, opts)
  end
end

local KEYS = {
  { '<Space>', 'space' },
  { '<CR>', 'enter' },
  { '<BS>', 'backspace' },
  { '<C-g>', 'ctrl-g' },
  { '<C-q>', 'ctrl-q' },
  { '<C-j>', 'ctrl-j' },
}
for byte = 33, 126 do
  local c = string.char(byte)
  table.insert(KEYS, { ({ ['<'] = '<lt>', ['|'] = '<Bar>', ['\\'] = '<Bslash>' })[c] or c, c })
end
local KAKUTEI = 'ctrl-j'
local ESCAPE = 'escape'

local function key_list(surface_id, extra)
  local keys = {}
  for _, spec in ipairs(vim.list_extend(vim.deepcopy(KEYS), extra)) do
    local index = #keys + 1
    keys[index] = {
      lhs = spec[1],
      lhsraw = vim.keycode(spec[1]),
      name = spec[2],
      index = index,
      -- 右辺は Lua 関数でなく <Cmd> にする。blink.cmp がバッファローカルに割り当てたキーの fallback から呼ばれたとき、
      -- Lua 関数の右辺は vim.schedule で後回しにされ、続けて打ったキーと順序が入れ替わる
      rhs = string.format("<Cmd>lua require('rinze')._press('%s', %d)<CR>", surface_id, index),
    }
  end
  return keys
end

-- blink.cmp は自分のキーを挿入モードに入るときにバッファローカルに置き、fallback ではそのとき無かった
-- バッファローカルの割り当ての代わりに global な割り当てを探す。blink.cmp のキーは奪わず、rinze を global に置いて
-- fallback から rinze に回るようにする
local BLINK_DESC_PREFIX = 'blink.cmp: '
local OWN_DESC = 'rinze'

local function keymap_lookup(mode)
  local by_scope = {}
  return function(buf, key)
    local scope = buf or 'global'
    if not by_scope[scope] then
      local by_lhs = {}
      for _, map in ipairs(buf and vim.api.nvim_buf_get_keymap(buf, mode) or vim.api.nvim_get_keymap(mode)) do
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

local function take(surface, buf, key, find)
  local mode = surface.map_mode
  local scope = buf
  local original = find(buf, key)
  if original and vim.startswith(original.desc or '', BLINK_DESC_PREFIX) then
    scope = nil
    original = find(nil, key)
  end
  local opts = { noremap = true, nowait = true, silent = surface.silent_keys, desc = OWN_DESC }
  if scope then
    vim.api.nvim_buf_set_keymap(scope, mode, key.lhs, key.rhs, opts)
  else
    vim.api.nvim_set_keymap(mode, key.lhs, key.rhs, opts)
  end
  return { buf = scope, original = original, aside = false }
end

local function restore(mode, key, taken, find)
  if taken.buf and not vim.api.nvim_buf_is_valid(taken.buf) then
    return
  end
  local current = find(taken.buf, key)
  if not (current and current.desc == OWN_DESC) then
    return
  end
  if taken.buf then
    vim.api.nvim_buf_del_keymap(taken.buf, mode, key.lhs)
  else
    vim.api.nvim_del_keymap(mode, key.lhs)
  end
  if taken.original then
    -- mapset はバッファローカルの割り当てをカレントバッファに置く
    vim.api.nvim_buf_call(taken.buf or 0, function()
      vim.fn.mapset(taken.original)
    end)
  end
end

local function release(session, index, find)
  local held = session.held
  local taken = held.keys[index]
  held.keys[index] = nil
  if not taken.aside then
    restore(session.surface.map_mode, session.keys[index], taken, find)
  end
end

local function sync(session) -- luacheck: ignore 561
  local mode = session.surface.map_mode
  local buf = vim.api.nvim_get_current_buf()
  local active = session.enabled and session.surface.accepts_keys()
  -- 割り当ての置き直しが途中で失敗しても blink.cmp の補完を止めたまま残さないよう、割り当てより先に知らせる
  session.surface.follow_keys_taken(session, active and not session.passthrough)
  if session.held and (session.held.buf ~= buf or not active) then
    local find = keymap_lookup(mode)
    for index in pairs(session.held.keys) do
      release(session, index, find)
    end
    session.held = nil
  end
  if active then
    session.held = session.held or { buf = buf, keys = {} }
    local held = session.held
    local find = keymap_lookup(mode)
    for _, key in ipairs(session.keys) do
      local wanted = key.name == KAKUTEI or not session.passthrough
      if wanted and not held.keys[key.index] then
        held.keys[key.index] = take(session.surface, buf, key, find)
      elseif not wanted and held.keys[key.index] then
        release(session, key.index, find)
      end
    end
  end
end

local function follow_mode(session, res)
  if res.passthrough ~= session.passthrough then
    session.passthrough = res.passthrough
    sync(session)
  end
  if res.mode ~= session.mode then
    session.mode = res.mode
    vim.cmd.redrawstatus()
  end
end

local function show(session, res, left_after_commit)
  session.preedit = res.preedit
  session.surface.show_preedit(session, res.preedit)
  show_float(session, res.minibuffer, res.candidates, res.annotation)
  session.surface.redraw()
  follow_mode(session, res)
  if session.blink then
    session.blink.update(res, left_after_commit)
  end
end

local function pass_through(session, key, commit)
  local mode = session.surface.map_mode
  local taken = session.held and session.held.keys[key.index]
  local find = keymap_lookup(mode)
  local original = taken and not taken.aside and (taken.original or (taken.buf and find(nil, key)))
  if original then
    taken.aside = true
    restore(mode, key, taken, find)
    vim.api.nvim_feedkeys(
      vim.keycode(string.format("<Cmd>lua require('rinze')._resume('%s', %d)<CR>", session.surface_id, key.index)),
      'ni',
      false
    )
    vim.api.nvim_feedkeys(key.lhsraw, 'mi', false)
  else
    vim.api.nvim_feedkeys(key.lhsraw, 'ni', false)
  end
  session.surface.insert(session, commit)
end

local function flush_in_place(session)
  if not session.chan then
    return
  end
  local ok, res = pcall(request, session, 'flush')
  if not ok then
    return
  end
  session.surface.insert_flushed(session, res.commit)
  show(session, res)
end

local function flush_if_cursor_left_preedit(session)
  if session.surface.cursor_left_preedit(session) then
    flush_in_place(session)
  end
end

-- 挿入モード。未確定文字列は extmark の inline の仮想テキストで出し、確定文字列は打鍵として入れる
local insert = { map_mode = 'i', silent_keys = true }

function insert.accepts_keys()
  return vim.api.nvim_get_mode().mode:match('^[iR]') ~= nil
end

function insert.left_context()
  local col = vim.api.nvim_win_get_cursor(0)[2]
  local before = vim.api.nvim_get_current_line():sub(1, col)
  local chars = vim.fn.strchars(before)
  if chars > MAX_LEFT_CONTEXT_CHARS then
    return vim.fn.strcharpart(before, chars - MAX_LEFT_CONTEXT_CHARS)
  end
  return before
end

local function in_range(b, lo, hi)
  return b ~= nil and b >= lo and b <= hi
end

local function utf8_3_len(b1, b2, b3)
  local lo = b1 == 0xe0 and 0xa0 or 0x80
  local hi = b1 == 0xed and 0x9f or 0xbf
  return in_range(b2, lo, hi) and in_range(b3, 0x80, 0xbf) and 3 or 0
end

local function utf8_4_len(b1, b2, b3, b4)
  local lo = b1 == 0xf0 and 0x90 or 0x80
  local hi = b1 == 0xf4 and 0x8f or 0xbf
  return in_range(b2, lo, hi) and in_range(b3, 0x80, 0xbf) and in_range(b4, 0x80, 0xbf) and 4 or 0
end

local function utf8_len(text, i)
  local b1, b2, b3, b4 = text:byte(i, i + 3)
  if in_range(b1, 0x01, 0x7f) then
    return 1
  elseif in_range(b1, 0xc2, 0xdf) then
    return in_range(b2, 0x80, 0xbf) and 2 or 0
  elseif in_range(b1, 0xe0, 0xef) then
    return utf8_3_len(b1, b2, b3)
  elseif in_range(b1, 0xf0, 0xf4) then
    return utf8_4_len(b1, b2, b3, b4)
  end
  return 0
end

-- 不正な UTF-8 バイト列 (生の 0x80 K_SPECIAL など) と NUL バイトを除く
local function sanitize_utf8(text)
  local out = {}
  local i = 1
  while i <= #text do
    local len = utf8_len(text, i)
    if len > 0 then
      out[#out + 1] = text:sub(i, i + len - 1)
      i = i + len
    else
      i = i + 1
    end
  end
  return table.concat(out)
end

-- 確定文字列は打鍵として先読みに入れる。API で書き込むと `.` で繰り返せず、undo も打鍵と別の単位になる。
-- 'i' で先読み中のキーより前に差し込むので、後に入れるものから先に送る
function insert.insert(_, commit)
  local safe = sanitize_utf8(commit)
  if safe ~= '' then
    -- 制御文字は CTRL-V を前に置き、打鍵として働かせず文字として入れる
    local literal = safe:gsub('%c', function(c)
      return '\22' .. c
    end)
    vim.api.nvim_feedkeys(literal, 'ni', true)
  end
end

local function preedit_position(session)
  local anchor = session.anchor
  if not (anchor and vim.api.nvim_buf_is_valid(anchor.buf)) then
    return nil
  end
  local pos = vim.api.nvim_buf_get_extmark_by_id(anchor.buf, ns, anchor.mark, {})
  if #pos == 0 then
    return nil
  end
  return anchor.buf, pos[1], pos[2]
end

function insert.insert_flushed(session, commit)
  local buf, row, col = preedit_position(session)
  if commit ~= '' and buf then
    local lines = vim.split(commit:gsub('\r\n', '\n'), '\n', { plain = true })
    vim.api.nvim_buf_set_text(buf, row, col, row, col, lines)
    local cursor = vim.api.nvim_win_get_cursor(0)
    if buf == vim.api.nvim_get_current_buf() and cursor[1] == row + 1 and cursor[2] >= col then
      if #lines == 1 then
        vim.api.nvim_win_set_cursor(0, { cursor[1], cursor[2] + #lines[1] })
      else
        vim.api.nvim_win_set_cursor(0, { cursor[1] + #lines - 1, #lines[#lines] + (cursor[2] - col) })
      end
    end
  end
end

function insert.cursor_left_preedit(session)
  local buf, row, col = preedit_position(session)
  if not buf then
    return false
  end
  local cursor = vim.api.nvim_win_get_cursor(0)
  return buf ~= vim.api.nvim_get_current_buf() or cursor[1] ~= row + 1 or cursor[2] ~= col
end

function insert.show_preedit(session, preedit)
  if session.anchor and vim.api.nvim_buf_is_valid(session.anchor.buf) then
    vim.api.nvim_buf_clear_namespace(session.anchor.buf, ns, 0, -1)
  end
  session.anchor = nil
  if preedit ~= '' then
    local row, col = unpack(vim.api.nvim_win_get_cursor(0))
    -- 右 gravity の印は、このあと先読みから入る確定文字列に押されて、その後ろへ動く。
    -- Neovim は同じ位置の overlay の仮想テキストを、priority が低い (同じなら先に置かれた) inline の仮想テキストの
    -- 後ろに描き、それ以外の inline の仮想テキストには上から重ねる。llama.vim などがカーソルの位置に先に置いた
    -- overlay に隠されないよう、priority を最低にする
    local mark = vim.api.nvim_buf_set_extmark(0, ns, row - 1, col, {
      virt_text = { { preedit, 'RinzePreedit' } },
      virt_text_pos = 'inline',
      right_gravity = true,
      priority = 0,
    })
    session.anchor = { buf = vim.api.nvim_get_current_buf(), mark = mark }
  end
end

function insert.redraw() end

function insert.follow_keys_taken() end

function insert.float_config(width, height)
  return {
    relative = 'cursor',
    row = 1,
    col = 0,
    width = width,
    height = height,
    style = 'minimal',
    focusable = false,
  }
end

local function new_session(surface_id, surface, extra_keys)
  return {
    surface_id = surface_id,
    surface = surface,
    keys = key_list(surface_id, extra_keys),
    chan = nil,
    enabled = false,
    preedit = '',
    mode = nil,
    passthrough = false,
    held = nil,
  }
end

local sessions = {
  -- Esc は挿入モードでは入力途中を確定してから抜け、コマンドラインでは割り当てずに Neovim が抜ける (入力途中は捨てる)
  i = new_session('i', insert, { { '<Esc>', ESCAPE } }),
  c = new_session('c', require('rinze.cmdline'), {}),
}

local function commit_or_hold(session, commit)
  if session.held_commit then
    session.held_commit = session.held_commit .. commit
  else
    session.surface.insert(session, commit)
  end
end

local function choose(version, word)
  local session = sessions.i
  local left = session.surface.left_context()
  local ok, res = pcall(request, session, 'choose', version, word, left)
  if not ok then
    vim.notify(tostring(res), vim.log.levels.ERROR)
    return
  end
  commit_or_hold(session, res.commit)
  show(session, res, left .. res.commit)
end

local function settle_menu(session, name)
  if not session.blink then
    return '', false
  end
  -- settle の vim.wait(0) は、blink.cmp が後回しにしたほかのキーの処理も走らせ、その中でもここを通る
  local outer = session.held_commit
  session.held_commit = ''
  local ok, consumed = pcall(session.blink.settle, name)
  local held = session.held_commit
  session.held_commit = outer
  if not ok then
    vim.notify(tostring(consumed), vim.log.levels.ERROR)
    consumed = false
  end
  return held, consumed
end

local function on_key(session, key)
  local held, consumed = settle_menu(session, key.name)
  if consumed then
    session.surface.insert(session, held)
    return
  end
  -- CursorMovedI は先読みが尽きるまで起きないので、続けて打たれたキーの前でも確かめる
  flush_if_cursor_left_preedit(session)
  local left = session.surface.left_context() .. held
  local ok, res = pcall(request, session, 'key', key.name, left)
  if not ok then
    vim.notify(tostring(res), vim.log.levels.ERROR)
    pass_through(session, key, held)
    return
  end
  if res.handled then
    session.surface.insert(session, held .. res.commit)
  else
    pass_through(session, key, held .. res.commit)
  end
  show(session, res, left .. res.commit)
end

local function on_escape(session, key)
  local held = settle_menu(session, ESCAPE)
  local ok, res = pcall(request, session, 'flush')
  pass_through(session, key, held .. (ok and res.commit or ''))
  if ok then
    show(session, res)
  end
end

function M._press(surface_id, index)
  local session = sessions[surface_id]
  local key = session.keys[index]
  local taken = session.held and session.held.keys[index]
  -- blink.cmp の fallback は、挿入モードに入ったときにあった rinze の割り当てを覚えていて、rinze が手を離したあとも呼ぶ
  if not taken or taken.aside then
    vim.api.nvim_feedkeys(key.lhsraw, 'ni', false)
  elseif key.name == ESCAPE then
    on_escape(session, key)
  else
    on_key(session, key)
  end
end

function M._resume(surface_id, index)
  local session = sessions[surface_id]
  local taken = session.held and session.held.keys[index]
  if taken and taken.aside then
    session.held.keys[index] = nil
  end
  sync(session)
end

function M.setup(opts)
  config = vim.tbl_extend('force', config, opts or {})
  vim.api.nvim_set_hl(0, 'RinzePreedit', { default = true, link = 'Underlined' })
  vim.api.nvim_set_hl(0, 'RinzeAnnotation', { default = true, link = 'Comment' })
end

function insert.attach(session)
  if config.blink then
    session.blink = require('rinze.blink')
    session.blink.attach(socket_path(), { choose = choose })
  end
  local group = vim.api.nvim_create_augroup('rinze', { clear = true })
  local function sync_insert()
    sync(session)
  end
  -- ModeChanged は InsertEnter のあとに起きるので、InsertEnter で割り当てを置くプラグインより後に置ける。
  -- 同じ文字で始まる長い割り当てより後に置いた <nowait> だけが、続きを待たずに働く
  vim.api.nvim_create_autocmd({ 'ModeChanged', 'BufEnter' }, {
    group = group,
    callback = sync_insert,
  })
  local function flush()
    flush_in_place(session)
  end
  -- InsertLeave は i_CTRL-O でも起きる。CTRL-O のあとは挿入モードに戻るので見出しを残し、
  -- 戻ったときにカーソルが動いていれば CursorMovedI で確定する
  vim.api.nvim_create_autocmd('InsertLeave', {
    group = group,
    callback = function()
      if not vim.api.nvim_get_mode().mode:match('^ni') then
        flush()
      end
    end,
  })
  -- 挿入モードのまま別のウィンドウへ移ったときは InsertLeave が起きない
  vim.api.nvim_create_autocmd({ 'WinLeave', 'BufLeave' }, {
    group = group,
    callback = flush,
  })
  vim.api.nvim_create_autocmd('CursorMovedI', {
    group = group,
    callback = function()
      flush_if_cursor_left_preedit(session)
    end,
  })
end

function insert.detach(session)
  vim.api.nvim_del_augroup_by_name('rinze')
  if session.blink then
    session.blink.detach()
    session.blink = nil
  end
end

local function enable(session, hooks)
  if session.enabled then
    return
  end
  local res = request(session, 'reset')
  session.surface.attach(session, hooks)
  session.enabled = true
  follow_mode(session, res)
  sync(session)
end

local function disable(session)
  if not session.enabled then
    return
  end
  flush_in_place(session)
  disconnect(session)
  session.enabled = false
  sync(session)
  session.surface.detach(session)
  close_float(session)
  session.preedit = ''
  session.mode = nil
  vim.cmd.redrawstatus()
end

local cmdline_hooks = {
  leave = function()
    local session = sessions.c
    disconnect(session)
    disable(session)
  end,
  moved = function()
    flush_if_cursor_left_preedit(sessions.c)
  end,
}

local function in_cmdline()
  return vim.api.nvim_get_mode().mode == 'c'
end

-- コマンドラインでは、そのコマンドラインの間だけ有効にする。挿入モードの状態とは別の接続で、別の入力状態を持つ
function M.enable()
  if in_cmdline() then
    enable(sessions.c, cmdline_hooks)
  else
    enable(sessions.i)
  end
end

function M.disable()
  disable(in_cmdline() and sessions.c or sessions.i)
end

function M.toggle()
  local session = in_cmdline() and sessions.c or sessions.i
  if session.enabled then
    M.disable()
  else
    M.enable()
  end
end

-- コマンドラインで有効な間はコマンドラインの状態を、それ以外は挿入モードの状態を返す
local function current()
  if in_cmdline() and sessions.c.enabled then
    return sessions.c
  end
  return sessions.i
end

function M.preedit()
  return current().preedit
end

-- 有効な間は `hiragana` `katakana` `halfwidth-katakana` `ascii` `fullwidth-latin` のどれか、無効なら nil
function M.mode()
  return current().mode
end

-- ステータスラインに出す文字列。無効なら空
function M.status()
  local mode = current().mode
  return mode and config.mode_labels[mode] or ''
end

return M
