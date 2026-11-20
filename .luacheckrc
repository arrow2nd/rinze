std = 'luajit'
codes = true
-- 行の長さは stylua が整える。stylua が折れない長い文字列とコメントは許す
max_line_length = false
max_cyclomatic_complexity = 10
-- blink.cmp の source は self を受け取るメソッドとして定義する
self = false

local writable = { read_only = false, other_fields = true }
read_globals = {
  vim = {
    other_fields = true,
    fields = { o = writable, bo = writable, wo = writable, b = writable },
  },
}

files['tests/nvim'] = { globals = { 'T' } }
