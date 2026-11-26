-- blink.cmp は add_source_provider で渡した module の名前でこのモジュールを require し、ソースとして new を呼ぶ
local M = {}

local rpc = require('rinze.rpc')
local socket = require('rinze.socket')

local SOURCE_ID = 'rinze'

local client = {
  path = nil,
  pipe = nil,
  connected = false,
  decode = nil,
  next_id = 0,
  inflight = nil,
  queued = nil,
}

local hooks
local midashi

local function close_client(reason)
  if client.pipe and not client.pipe:is_closing() then
    client.pipe:close()
  end
  local waiting = { client.inflight, client.queued }
  client.pipe, client.connected, client.decode = nil, false, nil
  client.inflight, client.queued = nil, nil
  for _, request in pairs(waiting) do
    vim.schedule(function()
      request.callback(nil, reason)
    end)
  end
end

local function send(request)
  client.next_id = client.next_id + 1
  request.id = client.next_id
  client.inflight = request
  client.pipe:write(vim.mpack.encode({ 0, request.id, 'lookup', { request.reading, request.left } }))
end

local function on_message(message)
  local kind, id, err, result = unpack(message)
  local done = client.inflight
  if kind ~= 1 or not done or id ~= done.id then
    return
  end
  client.inflight = nil
  if client.queued then
    local next_request = client.queued
    client.queued = nil
    send(next_request)
  end
  vim.schedule(function()
    if err ~= vim.NIL then
      done.callback(nil, err)
    else
      done.callback(result)
    end
  end)
end

local function on_read(read_err, data)
  if read_err or not data then
    close_client(read_err or 'rinzed との接続が閉じた')
    return
  end
  if client.decode then
    client.decode(data)
  end
end

local function connect()
  local ok_owner, owner_err = socket.verify_owner(client.path)
  if not ok_owner then
    close_client(owner_err)
    return
  end
  client.pipe = assert(vim.uv.new_pipe(false))
  client.decode = rpc.new_decoder(on_message, close_client)
  client.pipe:connect(client.path, function(err)
    if err then
      close_client(err)
      return
    end
    client.connected = true
    client.pipe:read_start(on_read)
    if client.queued then
      local first = client.queued
      client.queued = nil
      send(first)
    end
  end)
end

local function lookup(reading, left, callback)
  local request = { reading = reading, left = left, callback = callback }
  if not client.pipe then
    connect()
  end
  if client.inflight or not client.connected then
    client.queued = request
    return
  end
  send(request)
end

local function blink()
  local ok, cmp = pcall(require, 'blink.cmp')
  if not ok then
    return nil
  end
  local providers = require('blink.cmp.config').sources.providers
  -- blink.cmp の setup が設定を作り直すと、足したソースは消える
  if providers and providers[SOURCE_ID] == nil then
    cmp.add_source_provider(SOURCE_ID, { module = 'rinze.blink', name = 'rinze' })
  end
  return cmp
end

-- blink.cmp はメニューをカーソル前のキーワードの先頭に合わせる。見出しは virt_text でキーワードに含まれず、
-- 空白の無い日本語の行ではキーワードが行頭まで伸びるので、rinze のメニューを出す間だけカーソル (見出しの先頭) に合わせる
local align_before
local function align_menu_to_cursor(align)
  local draw = require('blink.cmp.config').completion.menu.draw
  if align and align_before == nil then
    align_before = draw.align_to
    draw.align_to = 'cursor'
  elseif not align and align_before ~= nil then
    draw.align_to = align_before
    align_before = nil
  end
end

function M.attach(socket_path, attached_hooks)
  if client.path ~= socket_path then
    close_client('接続先が変わった')
  end
  client.path = socket_path
  hooks = attached_hooks
end

function M.detach()
  M.update(nil)
  close_client('rinze を無効にした')
  hooks = nil
end

function M.update(res, left_after_commit)
  local reading = res and res.reading
  local cmp = blink()
  if not cmp or reading == nil or reading == vim.NIL then
    if midashi then
      midashi = nil
      cmp.hide()
      align_menu_to_cursor(false)
    end
    return
  end
  midashi = { reading = reading, left = left_after_commit, version = res.version }
  align_menu_to_cursor(true)
  cmp.show({ providers = { SOURCE_ID } })
end

-- キーを rinzed に送る前に呼ぶ。blink.cmp が vim.schedule で後回しにした選択と受け入れを、このキーより先に済ませる。
-- auto_insert で候補が仮に入っていれば、SKK で候補の表示中に次のキーを打ったときと同じく、その候補を確定する。
-- Enter はその確定だけに使ったとして true を返す
function M.settle(key_name)
  if not midashi then
    return false
  end
  vim.wait(0)
  local list = require('blink.cmp.completion.list')
  if list.preview_undo == nil then
    return false
  end
  local item = list.get_selected_item()
  if not (item and item.source_id == SOURCE_ID) then
    return false
  end
  list.undo_preview()
  require('blink.cmp').hide()
  hooks.choose(item.data.version, item.data.word)
  return key_name == 'enter'
end

local source = {}

function M.new()
  return setmetatable({}, { __index = source })
end

function source:enabled()
  return midashi ~= nil
end

local function parse_entry(entry)
  if type(entry) ~= 'table' then
    return entry, nil
  end
  local note = entry[2]
  if note == nil or note == vim.NIL or note == '' then
    return entry[1], nil
  end
  return entry[1], note
end

local function make_item(entry, index, total, ctx, version)
  local word, note = parse_entry(entry)
  local function cursor()
    return { line = ctx.cursor[1] - 1, character = ctx.cursor[2] }
  end
  local item = {
    label = word,
    kind = require('blink.cmp.types').CompletionItemKind.Text,
    -- メニューの絞り込みはカーソル前のバッファの文字 (キーワード) で行われ、見出しは virt_text で含まれないので、
    -- どの項目もキーワードに一致させて残す
    filterText = ctx.get_keyword(),
    -- 並びは rinzed が返した順にする。sortText だけでは受け入れ履歴 (frecency) で入れ替わる
    sortText = string.format('%04d', index),
    score_offset = (total - index) * 100,
    -- start と end を同じ表にすると、blink.cmp が仮挿入を取り消す範囲を求めるときに両方が書き換わる
    textEdit = { newText = word, range = { start = cursor(), ['end'] = cursor() } },
    data = { word = word, version = version },
  }
  if note then
    item.labelDetails = { description = note }
    item.documentation = { kind = 'plaintext', value = note }
  end
  return item
end

function source:get_completions(ctx, callback)
  local asked = midashi
  if not asked then
    callback({ items = {}, is_incomplete_forward = false, is_incomplete_backward = false })
    return
  end
  local cancelled = false
  lookup(asked.reading, asked.left, function(words)
    if cancelled or midashi ~= asked then
      return
    end
    local items = {}
    local list = words or {}
    for i, entry in ipairs(list) do
      items[i] = make_item(entry, i, #list, ctx, asked.version)
    end
    callback({ items = items, is_incomplete_forward = true, is_incomplete_backward = true })
  end)
  return function()
    cancelled = true
  end
end

-- default_implementation を呼ばないので、blink.cmp はバッファに何も書かない
function source:execute(_, item, callback)
  if hooks then
    hooks.choose(item.data.version, item.data.word)
  end
  callback()
end

return M
