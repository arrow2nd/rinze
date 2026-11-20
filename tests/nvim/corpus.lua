local rinzed, model, dict, socket, user_dict, corpus, passes = unpack(arg)
local learning = { unpack(arg, 8) }
local root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h:h')
vim.opt.runtimepath:prepend(root .. '/nvim')
vim.cmd.runtime('plugin/rinze.lua')
vim.o.showmode = false

local rinze = require('rinze')
local cmd = vim.list_extend({ rinzed, '--model', model, '--dict', dict, '--user-dict', user_dict }, learning)
rinze.setup({ cmd = cmd, socket = socket })
vim.cmd('RinzeEnable')

local MAX_SPACES = 60

-- headless の Neovim の print は挿入モードの間に改行を落とすことがあるので、標準出力へ直接書く
local function say(fmt, ...)
  io.stdout:write(string.format(fmt, ...), "\n")
end

local function parse_line(line)
  local text, keys = line:match('^([^\t]+)\t(.+)$')
  local segments = {}
  local rest = keys
  while rest ~= '' do
    local before, expect, after = rest:match('^(.-){([^}]*)}(.*)$')
    if not before then
      table.insert(segments, { keys = rest })
      break
    end
    table.insert(segments, { keys = before, expect = expect })
    rest = after
  end
  return { text = text, segments = segments }
end

local sentences = {}
for line in io.lines(corpus) do
  if line ~= '' and not line:match('^#') then
    table.insert(sentences, parse_line(line))
  end
end

-- headless の `nvim -l` は挿入モードのまま先読みが尽きると終了するので、1 文の打鍵を 1 回の feedkeys の中で処理し切る。
-- 1 文を 1 つの coroutine で打ち、打鍵のあとに step_key を送って、打鍵がすべて処理されたところで続きに戻る
local step_key = '<F9>'
local typing
vim.keymap.set('i', step_key, function()
  local ok, err = coroutine.resume(typing)
  if not ok then
    error(err)
  end
end)

local function feed(keys)
  vim.api.nvim_feedkeys(vim.keycode(keys .. step_key), 'mt', false)
  coroutine.yield()
end

local function float_text()
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    if vim.api.nvim_win_get_config(win).relative ~= '' then
      return table.concat(vim.api.nvim_buf_get_lines(vim.api.nvim_win_get_buf(win), 0, -1, false), '\n')
    end
  end
end

local function shown_inline(expect)
  local head = '▼' .. expect
  local preedit = rinze.preedit()
  return preedit:sub(1, #head) == head and preedit:sub(#head + 1):match('^%l*$') ~= nil
end

local function list_label(expect)
  local list = float_text()
  if not list then
    return nil
  end
  for label, word in list:gmatch('(%l):(%S+)') do
    if word == expect then
      return label
    end
  end
end

local function in_registration()
  return (float_text() or ''):find('辞書登録', 1, true) ~= nil
end

local function convert(expect)
  local first = rinze.preedit()
  if first:sub(1, #'▼') ~= '▼' or in_registration() then
    return nil, first
  end
  for spaces = 0, MAX_SPACES do
    if shown_inline(expect) then
      return spaces, first
    end
    local label = list_label(expect)
    if label then
      feed(label)
      return spaces, first
    end
    if in_registration() then
      return nil, first
    end
    feed('<Space>')
  end
  return nil, first
end

local function give_up(expect)
  local trailing = rinze.preedit():match('%l*$')
  for _ = 1, 5 do
    if rinze.preedit() == '' and not float_text() then
      break
    end
    feed('<C-g>')
  end
  local row, col = unpack(vim.api.nvim_win_get_cursor(0))
  vim.api.nvim_buf_set_text(0, row - 1, col, row - 1, col, { expect })
  vim.api.nvim_win_set_cursor(0, { row, col + #expect })
  feed(trailing)
end

local function type_corpus(pass)
  local results = {}
  local mismatched = 0
  for i, sentence in ipairs(sentences) do
    vim.cmd('enew!')
    typing = coroutine.create(function()
      for _, segment in ipairs(sentence.segments) do
        feed(segment.keys)
        if segment.expect then
          local spaces, first = convert(segment.expect)
          table.insert(results, { sentence = i, expect = segment.expect, spaces = spaces, first = first })
          say('%d周目 %2d\t%s\t%s\t%s', pass, i, segment.expect, spaces or 'FAIL', first)
          if not spaces then
            give_up(segment.expect)
          end
        end
      end
      vim.api.nvim_feedkeys(vim.keycode('<Esc>'), 'mt', false)
    end)
    vim.api.nvim_feedkeys(vim.keycode('i' .. step_key), 'mtx', false)
    local got = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), '\n')
    if got ~= sentence.text then
      mismatched = mismatched + 1
      say('FAIL 文 %d\n     実際: %s\n     期待: %s', i, got, sentence.text)
    end
  end
  return results, mismatched
end

local function summarize(pass, results, mismatched)
  local histogram, failed, first_hits = {}, 0, 0
  for _, r in ipairs(results) do
    if r.spaces then
      histogram[r.spaces] = (histogram[r.spaces] or 0) + 1
      if r.spaces == 0 then
        first_hits = first_hits + 1
      end
    else
      failed = failed + 1
    end
  end
  say('## %d 周目', pass)
  say('文: %d (バッファが期待と違う文 %d)', #sentences, mismatched)
  say('変換した区間: %d', #results)
  say('1 位正解率: %d/%d = %.1f%%', first_hits, #results, 100 * first_hits / #results)
  local spaces = vim.tbl_keys(histogram)
  table.sort(spaces)
  for _, n in ipairs(spaces) do
    say('Space %d 回: %d', n, histogram[n])
  end
  say('届かなかった: %d', failed)
end

local summaries = {}
local any_mismatched = false
for pass = 1, tonumber(passes) do
  local results, mismatched = type_corpus(pass)
  table.insert(summaries, { results, mismatched })
  any_mismatched = any_mismatched or mismatched > 0
end
for pass, s in ipairs(summaries) do
  summarize(pass, s[1], s[2])
end
os.exit(any_mismatched and 1 or 0)
