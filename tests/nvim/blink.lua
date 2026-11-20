local rinzed, model, dict, socket, user_dict, history, blink_path = unpack(arg)
local root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h:h')
vim.opt.runtimepath:prepend(root .. '/nvim')
vim.opt.runtimepath:prepend(blink_path)
vim.o.swapfile = false
vim.cmd.runtime('plugin/rinze.lua')

local failures = 0
local function check(label, got, want)
  local ok = vim.deep_equal(got, want)
  print(string.format('%s %s: %s', ok and 'ok  ' or 'FAIL', label, vim.inspect(got, { newline = ' ', indent = '' })))
  if not ok then
    print('     期待: ' .. vim.inspect(want, { newline = ' ', indent = '' }))
    failures = failures + 1
  end
end

local preset = os.getenv('RINZE_BLINK_PRESET') or 'default'
local keymaps = {
  default = { preset = 'default' },
  ['enter-readme-override'] = {
    preset = 'enter',
    ['<CR>'] = {
      function(cmp)
        if require('rinze').preedit() == '' then
          return cmp.accept()
        end
      end,
      'fallback',
    },
  },
  ['enter-unchanged'] = { preset = 'enter' },
}
print('keymap: ' .. preset)

local cmp = require('blink.cmp')
cmp.setup({
  keymap = assert(keymaps[preset], preset),
  fuzzy = { implementation = 'rust' },
  sources = { default = { 'buffer' } },
})
assert(
  vim.wait(5000, function()
    return require('blink.cmp.completion.trigger').buffer_events ~= nil
  end, 10),
  'blink.cmp の初期化が終わらない'
)

local rinze = require('rinze')
local cmd = { rinzed, '--model', model, '--dict', dict, '--user-dict', user_dict, '--history', history }
rinze.setup({ cmd = cmd, socket = socket, blink = true })
vim.cmd('RinzeEnable')

local function menu_items()
  if not cmp.is_menu_visible() then
    return nil
  end
  return vim.tbl_map(function(item)
    return item.label
  end, cmp.get_items())
end

local function buffer_text()
  return table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), '\n')
end

local function float_text()
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    local config = vim.api.nvim_win_get_config(win)
    if
      config.relative ~= ''
      and vim.api.nvim_win_get_buf(win) ~= require('blink.cmp.completion.windows.menu').win:get_buf()
    then
      return table.concat(vim.api.nvim_buf_get_lines(vim.api.nvim_win_get_buf(win), 0, -1, false), '\n')
    end
  end
end

local snapshots
_G.T = {}

function T.wait_menu()
  local shown = vim.wait(5000, function()
    return menu_items() ~= nil and #menu_items() > 0
  end, 5)
  if not shown then
    table.insert(snapshots, { timeout = true })
  end
end

function T.wait(ms)
  vim.wait(ms, function()
    return false
  end, 5)
end

function T.snap()
  table.insert(snapshots, {
    preedit = rinze.preedit(),
    buffer = buffer_text(),
    menu = menu_items(),
    list = float_text(),
    at = vim.uv.hrtime(),
  })
end

local wait_menu = '<Cmd>lua T.wait_menu()<CR>'
local snap = '<Cmd>lua T.snap()<CR>'
local settle = '<Cmd>lua T.wait(200)<CR>'

local function type_into(line, keys)
  snapshots = {}
  vim.cmd('enew!')
  vim.api.nvim_buf_set_lines(0, 0, -1, false, { line })
  vim.api.nvim_feedkeys(vim.keycode('A' .. keys .. settle .. snap .. '<Esc>'), 'x', false)
  return buffer_text(), snapshots
end

local function in_history(prefix)
  for _, line in ipairs(vim.fn.filereadable(history) == 1 and vim.fn.readfile(history) or {}) do
    if vim.startswith(line, prefix) then
      return true
    end
  end
  return false
end

local function converted_order(line, reading_keys)
  local order = {}
  local _, snaps = type_into(line, reading_keys .. string.rep('<Space>' .. snap, 12) .. '<C-g><C-g>')
  for _, s in ipairs(snaps) do
    if s.list then
      for word in s.list:gmatch('%a:(%S+)') do
        if not vim.tbl_contains(order, word) then
          table.insert(order, word)
        end
      end
    elseif s.preedit and s.preedit:match('^▼') then
      local word = s.preedit:gsub('^▼', '')
      if not vim.tbl_contains(order, word) then
        table.insert(order, word)
      end
    end
  end
  return order
end

local function prefix(list, n)
  return vim.list_slice(list, 1, n)
end

local function finish()
  if failures > 0 then
    print(failures .. ' 件失敗')
    os.exit(1)
  end
  print('すべて通過')
  os.exit(0)
end

local _, text, snaps
if preset == 'enter-readme-override' then
  text, snaps = type_into('', 'Asita' .. wait_menu .. snap .. '<CR>' .. settle .. snap)
  check(
    'enter preset で Enter を上書きすると、メニュー表示中の Enter は読みのまま確定する',
    { snaps[1].menu ~= nil, text, snaps[2].menu },
    { true, 'あした', nil }
  )
  text = type_into('', 'Asita' .. wait_menu .. '<CR>ka')
  check(
    'fallback から呼ばれた rinze のキーも、続けて打ったキーと順序が入れ替わらない',
    text,
    'あしたか'
  )
  finish()
elseif preset == 'enter-unchanged' then
  text, snaps = type_into('', 'Asita' .. wait_menu .. snap .. '<CR>')
  check(
    'enter preset のままでは、メニュー表示中の Enter は先頭の候補を確定する',
    text,
    snaps[1].menu[1]
  )
  finish()
end

for _, case in ipairs({ { '', 'Asita' }, { '今日は', 'Asita' }, { 'この', 'Kanji' } }) do
  local line, keys = unpack(case)
  _, snaps = type_into(line, keys .. wait_menu .. snap)
  local menu = snaps[1].menu or {}
  print(string.format('左文脈 %q の %s でのメニュー: %s', line, keys, table.concat(menu, ' ')))
  check(
    string.format('左文脈 %q で %s と打つとメニューが出て、見出しは消えない', line, keys),
    { #menu > 2, snaps[1].preedit:sub(1, #'▽'), snaps[1].buffer },
    { true, '▽', line }
  )
  local order = converted_order(line, keys)
  check(
    string.format('左文脈 %q の %s のメニューの並びは Space で送る並びと同じ', line, keys),
    menu,
    prefix(order, #menu)
  )
end

_, snaps = type_into('', 'Asita' .. wait_menu .. snap)
local menu = snaps[1].menu

text, snaps = type_into('', 'Asita' .. wait_menu .. '<C-y>' .. settle .. snap)
check(
  'C-y で先頭の候補を確定し、見出しとメニューを消す',
  { text, snaps[1].preedit, snaps[1].menu },
  { menu[1], '', nil }
)
check('先頭の候補の確定は学習の履歴に書かない', in_history('あした\t'), false)

text, snaps = type_into('', 'Asita' .. wait_menu .. '<C-n>' .. settle .. snap .. '<C-y>' .. settle .. snap)
check('C-n で選んだ候補が仮に入る', { snaps[1].buffer, snaps[1].preedit }, { menu[2], '▽あした' })
check('C-y で 2 番目の候補を確定する', { text, snaps[2].preedit, snaps[2].menu }, { menu[2], '', nil })
check('先頭以外の候補の確定は学習の履歴に書く', in_history('あした\t' .. menu[2] .. '\t'), true)

_, snaps = type_into('', 'Asita' .. wait_menu .. snap)
check(
  '学習したあとのメニューの並びも Space で送る並びと同じ',
  snaps[1].menu,
  prefix(converted_order('', 'Asita'), #snaps[1].menu)
)
local asita = snaps[1].menu[1]

local function menu_of(keys)
  local _, s = type_into('', keys .. wait_menu .. snap)
  return s[1].menu or {}
end

local kanji = menu_of('Kanji')
text = type_into('', 'Kanji' .. wait_menu .. '<C-n>' .. settle .. 'ha')
check(
  'C-n で仮に入れたあと文字を打つと、選んだ候補を確定してからその文字を処理する',
  text,
  kanji[2] .. 'は'
)
kanji = menu_of('Kanji')
text = type_into('', 'Kanji' .. wait_menu .. '<C-n>ha')
check('C-n の直後に間を空けずに打っても同じ', text, kanji[2] .. 'は')
kanji = menu_of('Kanji')
text = type_into('', 'Kanji' .. wait_menu .. '<C-n><C-n>' .. settle .. '<CR>')
check('仮に入れたあとの Enter は確定だけを行い改行しない', text, kanji[3])
kanji = menu_of('Kanji')
text = type_into('', 'Kanji' .. wait_menu .. '<C-n>' .. settle .. 'Asita' .. wait_menu .. '<C-y>')
check('仮に入れたあとの大文字は確定してから次の見出しを始める', text, kanji[2] .. asita)
kanji = menu_of('Kanji')

text = type_into('', 'Kanji' .. wait_menu .. '<C-y>ha')
check('C-y の直後に間を空けずに打った文字を失わない', text, kanji[1] .. 'は')
text = type_into('', 'Kanji' .. wait_menu .. '<C-y>Asita<Space><CR>')
check('C-y の直後に間を空けずに打った見出しと変換も失わない', text, kanji[1] .. asita)

text, snaps = type_into('', 'Asita' .. wait_menu .. '<CR>' .. settle .. snap)
check(
  'default preset のメニュー表示中の Enter は読みのまま確定し、改行しない',
  { text, snaps[1].menu },
  { 'あした', nil }
)
_, snaps = type_into('', 'Asita' .. wait_menu .. '<Space>' .. settle .. snap)
check(
  'default preset のメニュー表示中の Space は変換を始め、メニューを閉じる',
  { snaps[1].preedit, snaps[1].menu },
  { '▼' .. asita, nil }
)

_, snaps = type_into('', 'AT' .. settle .. snap)
check(
  '送りがなの入力中はメニューを出さない',
  { snaps[1].preedit, snaps[1].menu },
  { '▽あ*t', nil }
)

local press = rinze._press
local key_ms
rinze._press = function(...)
  local started = vim.uv.hrtime()
  press(...)
  table.insert(key_ms, (vim.uv.hrtime() - started) / 1e6)
end

local function median(values)
  local sorted = vim.deepcopy(values)
  table.sort(sorted)
  return sorted[math.ceil(#sorted / 2)], sorted[#sorted]
end

key_ms = {}
_, snaps = type_into('', 'Asitanoasa' .. snap)
check(
  '一気に打っても打鍵の順序が入れ替わらない',
  { snaps[1].preedit, snaps[1].buffer },
  { '▽あしたのあさ', '' }
)
_, snaps = type_into('', 'Nihongonyuuryoku' .. snap .. wait_menu .. snap)
local burst_ms = (snaps[2].at - snaps[1].at) / 1e6
check(
  '一気に打ったあとのメニューは最後の読みの候補',
  { snaps[1].preedit, snaps[2].menu },
  { '▽にほんごにゅうりょく', { '日本語入力' } }
)
print(string.format('一気に打ち終えてから最後の読みのメニューが出るまで: %.1fms', burst_ms))

local function measure(label, before_space)
  local round = 'Kanji' .. before_space .. '<Space><C-g><C-g>Asitanoasa<C-g>'
  key_ms = {}
  for i = 1, 5 do
    local left_context_unseen_by_rinzed = label .. i
    type_into(left_context_unseen_by_rinzed, round)
  end
  local typing, spaces = {}, {}
  local space_index = #'Kanji' + 1
  local esc_typed_by_type_into = 1
  local per_round = #'Kanji' + 3 + #'Asitanoasa' + 1 + esc_typed_by_type_into
  for i, ms in ipairs(key_ms) do
    table.insert((i - 1) % per_round + 1 == space_index and spaces or typing, ms)
  end
  local typing_median, typing_max = median(typing)
  local space_median = median(spaces)
  print(
    string.format(
      '%s: 見出しの打鍵 %d 回の中央値 %.2fms 最大 %.2fms、Space %d 回の中央値 %.2fms',
      label,
      #typing,
      typing_median,
      typing_max,
      #spaces,
      space_median
    )
  )
  return typing_median, space_median
end
local with_blink, space_after_menu = measure('連携あり', wait_menu)
vim.cmd('RinzeDisable')
rinze.setup({ cmd = cmd, socket = socket, blink = false })
vim.cmd('RinzeEnable')
local _, space_without_menu = measure('連携なし', '<Cmd>lua T.wait(100)<CR>')
check(
  '連携ありでも 1 打鍵の処理は候補の問い合わせ (数 ms 以上) を待たない',
  with_blink < 2,
  true
)
check(
  'メニューを出したあとの Space は点数づけを待たない',
  space_after_menu < space_without_menu,
  true
)
rinze._press = press

finish()
