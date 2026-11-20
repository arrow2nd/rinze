-- コマンドラインの入力の場所。未確定文字列はコマンドラインに入れず、カーソルの位置に重ねたフロートに出す。
-- コマンドラインに入れると、'incsearch' と 'inccommand' が ▽ を含む文字列で検索し直して画面が飛び、
-- 中断した行は ▽ ごと履歴に残る
local M = {}

M.map_mode = 'c'

local ns = vim.api.nvim_create_namespace('rinze-cmdline')
-- <silent> の割り当ての中で setcmdline で書き換えたコマンドラインは、次のキーまで描き直されない
M.silent_keys = false

-- この記号と空白より前は、コマンドの構文 (`%s/`、`g/` など) か別の欄なので、左文脈に含めない
local DELIMITED = '^.*[%s!-/:-@%[-`{-~]'

function M.accepts_keys()
  return vim.api.nvim_get_mode().mode == 'c'
end

function M.left_context()
  local before = vim.fn.getcmdline():sub(1, vim.fn.getcmdpos() - 1)
  return (before:gsub(DELIMITED, ''))
end

local function expect_current(session)
  session.expect = { line = vim.fn.getcmdline(), pos = vim.fn.getcmdpos() }
end

-- 確定文字列を `pos` に入れる。setcmdline は CmdlineChanged と CursorMovedC を起こすので、
-- 先に期待する状態を覚えておき、自分の書き換えをカーソルの移動と取り違えない
local function insert_at(session, pos, text)
  local line, cursor = vim.fn.getcmdline(), vim.fn.getcmdpos()
  pos = math.min(pos, #line + 1)
  local new_cursor = cursor >= pos and cursor + #text or cursor
  session.expect = { line = line:sub(1, pos - 1) .. text .. line:sub(pos), pos = new_cursor }
  vim.fn.setcmdline(session.expect.line, new_cursor)
end

function M.insert(session, text)
  if text ~= '' then
    insert_at(session, vim.fn.getcmdpos(), text)
  end
end

function M.insert_flushed(session, text)
  if text ~= '' then
    insert_at(session, session.expect and session.expect.pos or vim.fn.getcmdpos(), text)
  end
end

function M.cursor_left_preedit(session)
  local expect = session.expect
  return session.preedit ~= ''
    and expect ~= nil
    and (vim.fn.getcmdline() ~= expect.line or vim.fn.getcmdpos() ~= expect.pos)
end

-- コマンドラインの画面上の位置。長い行は折り返して上へ伸び、'cmdheight' が 2 以上なら領域の先頭から描く
local function geometry()
  local columns = vim.o.columns
  local line = vim.fn.getcmdline()
  local offset = vim.fn.getcmdscreenpos() - 1
  local prompt = offset - vim.fn.strdisplaywidth(line:sub(1, vim.fn.getcmdpos() - 1))
  local rows = math.floor((prompt + vim.fn.strdisplaywidth(line)) / columns) + 1
  local top = vim.o.lines - math.max(rows, vim.o.cmdheight)
  return {
    top = top,
    cursor_row = top + math.floor(offset / columns),
    cursor_col = offset % columns,
  }
end

local PREEDIT_ZINDEX = 250

local function close_overlay(session)
  local overlay = session.overlay
  if overlay and overlay.win and vim.api.nvim_win_is_valid(overlay.win) then
    vim.api.nvim_win_close(overlay.win, true)
  end
  if overlay then
    overlay.win = nil
  end
end

-- フロートは後ろの文字を隠すので、カーソルより後ろのコマンドラインも続けて描き、挿入モードの inline の仮想テキストと同じに見せる
function M.show_preedit(session, preedit)
  expect_current(session)
  if preedit == '' then
    close_overlay(session)
    return
  end
  session.overlay = session.overlay or {}
  local overlay = session.overlay
  if not (overlay.buf and vim.api.nvim_buf_is_valid(overlay.buf)) then
    overlay.buf = vim.api.nvim_create_buf(false, true)
  end
  local line = vim.fn.getcmdline()
  local after = line:sub(vim.fn.getcmdpos())
  vim.api.nvim_buf_set_lines(overlay.buf, 0, -1, false, { preedit .. after })
  vim.api.nvim_buf_set_extmark(overlay.buf, ns, 0, 0, {
    id = 1,
    end_col = #preedit,
    hl_group = 'RinzePreedit',
  })
  local where = geometry()
  local opts = {
    relative = 'editor',
    row = where.cursor_row,
    col = where.cursor_col,
    width = math.max(1, math.min(vim.fn.strdisplaywidth(preedit .. after), vim.o.columns - where.cursor_col)),
    height = 1,
    style = 'minimal',
    -- 'winborder' の枠がつくと、コマンドラインの文字に重ならない
    border = 'none',
    focusable = false,
    zindex = PREEDIT_ZINDEX,
  }
  if overlay.win and vim.api.nvim_win_is_valid(overlay.win) then
    vim.api.nvim_win_set_config(overlay.win, opts)
  else
    opts.noautocmd = true
    overlay.win = vim.api.nvim_open_win(overlay.buf, false, opts)
    vim.wo[overlay.win].winhighlight = 'Normal:MsgArea,NormalFloat:MsgArea'
  end
end

-- 候補の一覧と辞書登録の入力欄は、コマンドラインのすぐ上にカーソルの桁から出す
function M.float_config(width, height)
  local where = geometry()
  return {
    relative = 'editor',
    anchor = 'SW',
    row = where.top,
    col = math.max(0, math.min(where.cursor_col, vim.o.columns - width)),
    width = width,
    height = height,
    style = 'minimal',
    focusable = false,
    zindex = PREEDIT_ZINDEX,
  }
end

-- コマンドラインモードでは、開いたフロートは次に画面を描き直すまで出ない
function M.redraw()
  vim.cmd.redraw()
end

-- blink.cmp のコマンドラインの補完は、打鍵と CmdlineChanged に反応してメニューを出す。
-- 設定の cmdline.enabled はソースを集めるたびに読まれるので、rinze が有効な間だけ false にしてメニューを出させない
local function suppress_blink(session)
  if not package.loaded['blink.cmp'] then
    return
  end
  local ok, blink_config = pcall(require, 'blink.cmp.config')
  if not (ok and blink_config.cmdline) then
    return
  end
  session.blink_cmdline = { config = blink_config.cmdline, enabled = blink_config.cmdline.enabled }
  blink_config.cmdline.enabled = false
  pcall(require('blink.cmp').hide)
end

local function release_blink(session)
  local saved = session.blink_cmdline
  if saved then
    saved.config.enabled = saved.enabled
    session.blink_cmdline = nil
  end
end

function M.attach(session, hooks)
  local group = vim.api.nvim_create_augroup('rinze-cmdline', { clear = true })
  -- 有効にしたコマンドラインを抜けたら、入力途中のものを捨てる。次にコマンドラインに入ったときは無効から始まる
  vim.api.nvim_create_autocmd('CmdlineLeave', {
    group = group,
    callback = hooks.leave,
  })
  vim.api.nvim_create_autocmd({ 'CursorMovedC', 'CmdlineChanged' }, {
    group = group,
    callback = hooks.moved,
  })
  suppress_blink(session)
end

function M.detach(session)
  vim.api.nvim_del_augroup_by_name('rinze-cmdline')
  close_overlay(session)
  release_blink(session)
  session.expect = nil
end

return M
