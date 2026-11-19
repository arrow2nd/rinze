local M = {}

-- rinzed (crates/rinzed/src/main.rs の socket_path) も同じ規則でパスを決める。
-- 両者の一致は crates/rinzed/tests/fixtures/default_socket.json の表で確かめる。
function M.path(os, env)
  local function get(name)
    local value = env[name]
    if value ~= '' then
      return value
    end
  end
  if os == 'macos' then
    local home = get('HOME')
    if not home then
      error('rinze: HOME が無いので既定のソケットのパスを決められない')
    end
    return home .. '/Library/Caches/rinze/rinze.sock'
  end
  local dir = get('XDG_RUNTIME_DIR')
  if dir then
    return dir .. '/rinze.sock'
  end
  return '/tmp/rinze-' .. (get('USER') or '') .. '.sock'
end

function M.default()
  return M.path(vim.fn.has('mac') == 1 and 'macos' or 'other', vim.env)
end

return M
