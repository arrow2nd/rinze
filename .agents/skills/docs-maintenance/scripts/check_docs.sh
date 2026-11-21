#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

errors=0
warnings=0

err() {
  printf 'ERROR: %s\n' "$1" >&2
  errors=$((errors + 1))
}

warn() {
  printf 'WARN: %s\n' "$1" >&2
  warnings=$((warnings + 1))
}

# GitHub-compatible heading slugifier for checking #anchor targets.
# Lowercases ASCII, removes punctuation except hyphens/spaces/word chars, turns spaces into hyphens.
slugify() {
  local heading="$1"
  # Strip leading/trailing whitespace and trailing #
  heading="$(printf '%s' "$heading" | sed -E 's/^[[:space:]]+//; s/[[:space:]]+#*[[:space:]]*$//')"
  # Remove inline markdown links [text](url) -> text, backticks
  heading="$(printf '%s' "$heading" | sed -E 's/\[([^]]+)\]\([^)]*\)/\1/g; s/`//g')"
  # Lowercase ASCII letters
  heading="$(printf '%s' "$heading" | tr 'A-Z' 'a-z')"
  # Remove common punctuation (keep spaces, hyphens, alphanumerics, non-ASCII UTF-8)
  heading="$(printf '%s' "$heading" | sed -E 's/[].,:;!?()[{}"'\''\/+*~<>@#$%^&|=\\]//g')"
  # Convert spaces to hyphens
  heading="$(printf '%s' "$heading" | sed -E 's/[[:space:]]/-/g')"
  printf '%s' "$heading"
}

# Check if a markdown file contains a heading whose slug matches $2.
has_anchor() {
  local file="$1"
  local want="$2"
  local in_fence=0
  local line heading slug
  declare -A seen=()

  while IFS= read -r line || [ -n "$line" ]; do
    if [[ "$line" =~ ^\`\`\` ]]; then
      in_fence=$((1 - in_fence))
      continue
    fi
    if [ "$in_fence" -eq 1 ]; then
      continue
    fi
    if [[ "$line" =~ ^#{1,6}[[:space:]]+(.*)$ ]]; then
      heading="${BASH_REMATCH[1]}"
      slug="$(slugify "$heading")"
      [ -n "$slug" ] || continue
      local candidate="$slug"
      if [ -n "${seen[$slug]+x}" ]; then
        local count="${seen[$slug]}"
        seen[$slug]=$((count + 1))
        candidate="${slug}-${count}"
      else
        seen[$slug]=1
      fi
      if [ "$candidate" = "$want" ]; then
        return 0
      fi
    fi
  done < "$file"

  return 1
}

# Extract relative markdown link targets from a file, skipping fenced code blocks.
extract_links() {
  local file="$1"
  awk '
    /^```/ { fence = !fence; next }
    fence { next }
    {
      line = $0
      while (match(line, /\]\([^)[:space:]]+\)/)) {
        raw = substr(line, RSTART + 2, RLENGTH - 3)
        print NR "\t" raw
        line = substr(line, RSTART + RLENGTH)
      }
    }
  ' "$file"
}

if [ ! -d docs ]; then
  err "docs/ directory not found"
  exit 1
fi

if [ ! -f docs/README.md ]; then
  err "docs/README.md not found"
fi

# 1. Check every category directory under docs/
for dir in docs/*/; do
  [ -d "$dir" ] || continue
  cat_name="${dir#docs/}"
  cat_name="${cat_name%/}"
  cat_readme="docs/${cat_name}/README.md"

  if [ ! -f "$cat_readme" ]; then
    err "Category directory missing index: $cat_readme"
    continue
  fi

  if [ -f docs/README.md ] && ! grep -qF "(${cat_name}/README.md)" docs/README.md; then
    err "docs/README.md does not link category index: ${cat_name}/README.md"
  fi

  if [ -f README.md ] && ! grep -qF "(docs/${cat_name}/README.md)" README.md; then
    err "README.md does not link category index: docs/${cat_name}/README.md"
  fi

  # 2. Check every topic markdown file under docs/<category>/
  for md in "docs/${cat_name}"/*.md; do
    [ -f "$md" ] || continue
    base="$(basename "$md")"
    [ "$base" = "README.md" ] && continue

    if ! grep -qF "(${base})" "$cat_readme"; then
      err "$cat_readme does not list topic file: $base"
    fi

    if [ -f docs/README.md ] && ! grep -qF "(${cat_name}/${base})" docs/README.md; then
      err "docs/README.md does not list topic file: ${cat_name}/${base}"
    fi

    lines="$(wc -l < "$md")"
    if [ "$lines" -gt 180 ]; then
      warn "$md has $lines lines (> 180); consider splitting"
    fi
  done
done

if [ -f README.md ]; then
  root_lines="$(wc -l < README.md)"
  if [ "$root_lines" -gt 80 ]; then
    warn "README.md has $root_lines lines (> 80); keep details in docs/"
  fi
fi

# 3. Validate all relative markdown links and anchors in README.md and docs/**/*.md
mapfile -t md_files < <(
  [ -f README.md ] && printf '%s\n' README.md
  find docs -type f -name '*.md' | sort
)

for md in "${md_files[@]}"; do
  dir="$(dirname "$md")"
  while IFS=$'\t' read -r lineno target; do
    [ -n "$target" ] || continue
    # Skip external links
    case "$target" in
      http://*|https://*|mailto:*) continue ;;
    esac

    path_part="${target%%#*}"
    anchor_part=""
    if [[ "$target" == *#* ]]; then
      anchor_part="${target#*#}"
    fi

    if [ -z "$path_part" ]; then
      resolved="$md"
    else
      resolved="$(realpath -m --relative-to="$ROOT" "${dir}/${path_part}")"
    fi

    if [ ! -e "$resolved" ]; then
      err "${md}:${lineno}: broken link target '${target}' (resolved: ${resolved})"
      continue
    fi

    if [ -n "$anchor_part" ] && [ -f "$resolved" ]; then
      if ! has_anchor "$resolved" "$anchor_part"; then
        err "${md}:${lineno}: anchor '#${anchor_part}' not found in ${resolved}"
      fi
    fi
  done < <(extract_links "$md")
done

# 4. Validate docs/... file paths mentioned in source code comments
while IFS=: read -r src_file lineno content; do
  [ -n "$src_file" ] || continue
  while IFS= read -r doc_ref; do
    [ -n "$doc_ref" ] || continue
    if [ ! -e "$doc_ref" ]; then
      err "${src_file}:${lineno}: code references missing doc path '${doc_ref}'"
    fi
  done < <(printf '%s\n' "$content" | grep -oE 'docs/[A-Za-z0-9._/-]+\.md' || true)
done < <(git grep -n 'docs/' -- ':!README.md' ':!docs/' ':!.agents/' || true)

printf 'Checked %d markdown files: %d error(s), %d warning(s)\n' "${#md_files[@]}" "$errors" "$warnings"

if [ "$errors" -gt 0 ]; then
  exit 1
fi
