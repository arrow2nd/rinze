local M = {}

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
  local cache = get('XDG_CACHE_HOME')
  if not cache then
    local home = get('HOME')
    if not home then
      error('rinze: HOME が無いので既定のソケットのパスを決められない')
    end
    cache = home .. '/.cache'
  end
  return cache .. '/rinze/rinze.sock'
end

function M.default()
  return M.path(vim.fn.has('mac') == 1 and 'macos' or 'other', vim.env)
end

function M.verify_owner(path)
  local stat = vim.uv.fs_lstat(path)
  if not stat then
    return false, 'not_found'
  end
  if stat.type ~= 'socket' then
    return false, 'rinze: ' .. path .. ' は Unix ドメインソケットでない'
  end
  local uid = vim.uv.getuid and vim.uv.getuid()
  if uid ~= nil and stat.uid ~= uid then
    return false, 'rinze: ソケット ' .. path .. ' の所有者が異なる'
  end
  return true
end

return M
