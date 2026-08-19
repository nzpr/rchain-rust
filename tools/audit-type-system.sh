#!/usr/bin/env bash
# Audit the Rust port for type-system violations — the authoritative gate.
#
# Supersedes `audit-partiality.sh` (a candidate finder). This script *strips* `#[cfg(test)]` test
# blocks (brace-depth aware) so it classifies production vs test correctly, applies an explicit
# whitelist of justified sites, and exits non-zero when a hard violation is found.
#
# Hard violations (exit 1):
#   panic   — `.unwrap()`, `.expect(`, `panic!`, `unreachable!`, `todo!`, `unimplemented!` in
#             production code. Whitelisted: `sdk/src/primitive.rs` (the Scala `getUnsafe` escape
#             hatch) and `node/src/dag/implementation.rs` / `regex/src/regex_pattern.rs` (the Scala
#             `NotImplementedError("TODO")` stubs). The rholang parser's `self.expect(Tok)` method is
#             excluded (a method, not `Result::expect`).
#   unsafe  — `unsafe {` (must be zero: the crate graph is entirely safe Rust).
#   silent  — silent defaulting of a fallible numeric conversion: `try_into().unwrap()`,
#             `try_into().expect(`, `try_into().unwrap_or(`, `try_from(..).unwrap_or(`,
#             `..parse(..).unwrap_or(`. A fallible conversion must not be flattened to 0/Default.
#
# Soft reports (exit 0, informational — refined by `cargo clippy` + manual review):
#   cast    — narrowing / signedness-changing numeric casts (`as i8/i32/i64/u8/u32/..`).
#   lax     — silent parse/hex escapes: `from_str_radix(..).unwrap_or(..)` and `base16::unsafe_decode`
#             (a hex decode that skips non-hex and never length-checks).
#   get     — index access and `.get(..).unwrap()`-style lookups.
#
# Usage: tools/audit-type-system.sh [panic|unsafe|silent|cast|lax|get]   (default: all)

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CRATES=(sdk shared crypto graphz models block-storage comm rspace rholang casper regex node)

# ---------------------------------------------------------------------------
# Brace-depth-aware test-block stripper.
#
# A `#[cfg(test)]` attribute in this codebase is *always* followed by a
# `mod <name> { .. }` or a file-based `mod <name>;`. We skip from the attribute until the brace
# depth (tracked from the attribute line) returns to zero; a brace-less item (`mod property_tests;`)
# is exactly the one following line. Standalone `#[test]` / `#[tokio::test]` fns are skipped the
# same way.
# ---------------------------------------------------------------------------
STRIP_AWK='
function braces(s,   o,c,i,ch){ o=0; c=0; for(i=1;i<=length(s);i++){ ch=substr(s,i,1); if(ch=="{")o++; else if(ch=="}")c++ } return o-c }
BEGIN { skip=0; depth=0 }
{
  if (skip == 0) {
    if ($0 ~ /^[[:space:]]*#\[cfg\(test\)\]/) { skip=1; depth=0; next }
    if ($0 ~ /^[[:space:]]*#\[(tokio::)?test\]/) { skip=1; depth=braces($0); if (depth<=0) skip=0; next }
    print
    next
  }
  depth += braces($0)
  if (depth <= 0) skip=0
  next
}
'

# Files that are entirely test-only (not gated by an inline `mod` in another file).
TEST_ONLY_FILE_RE='(_tests?|test_)\.rs$|/property_tests\.rs$'

# Panic-class whitelist (suffix-matched against the file path). These are the deliberate escapes:
# get_unsafe (Scala `getUnsafe`) and the Scala-oracle `TODO`/`NotImplementedError` stubs.
WHITELIST_PANIC=(
  '/sdk/src/primitive.rs'
  '/node/src/dag/implementation.rs'
  '/regex/src/regex_pattern.rs'
)

hard_failures=0

is_whitelisted() {
  # $1 = file path; true if it matches any panic whitelist suffix.
  local f="$1" w
  for w in "${WHITELIST_PANIC[@]}"; do
    case "$f" in
      *"$w") return 0 ;;
    esac
  done
  return 1
}

note() {
  local kind="$1" file="$2" line="$3" text="$4"
  printf '  %s\n' "$file:$line: $text"
  case "$kind" in
    panic|unsafe|silent) hard_failures=$((hard_failures + 1)) ;;
  esac
}

scan() {
  # $1 = kind; $2 = grep -E pattern; $3 = "panic" to apply the panic whitelist, else "".
  local kind="$1" pattern="$2" whitelist="$3"
  local c f line text
  for c in "${CRATES[@]}"; do
    local dir="$ROOT/$c/src"
    [ -d "$dir" ] || continue
    while IFS= read -r f; do
      if [ "$whitelist" = "panic" ] && is_whitelisted "$f"; then
        continue
      fi
      while IFS=: read -r line text; do
        [ -n "$line" ] || continue
        note "$kind" "$f" "$line" "$text"
      done < <(awk "$STRIP_AWK" "$f" \
                 | grep -nE "$pattern" \
                 | grep -vE 'self\.expect\(')
    done < <(find "$dir" -name '*.rs' | grep -vE "$TEST_ONLY_FILE_RE")
  done
}

run_class() {
  local cls="$1"
  echo "===== class: $cls ====="
  case "$cls" in
    panic)   scan panic '\.unwrap\(\)|\.expect\(|panic!|unreachable!|todo!|unimplemented!' panic ;;
    unsafe)  scan unsafe 'unsafe[[:space:]]*\{' '' ;;
    silent)  scan silent 'try_into\(\)\.(unwrap|expect)\(|try_(into\(\)|from\(.*\))\.unwrap_or(\(0\)|_default\(\))|\.parse(::<[^>]+>)?\(\)\.unwrap_or(\(0\)|_default\(\))' '' ;;
    cast)    scan cast '\bas (i8|i16|i32|i64|u8|u16|u32|u64|usize|isize|f32|f64)\b' '' ;;
    lax)     scan lax 'from_str_radix\([^)]*\)\.(unwrap_or|unwrap|expect)\(|unsafe_decode\(' '' ;;
    get)     scan get '\.get\([^)]*\)\.(unwrap|expect)\(|\.(next|last|first|pop)\(\)\.(unwrap|expect)\(|\b[a-zA-Z_]+\[[0-9]+\]' '' ;;
    *)
      echo "unknown class: $cls (expected panic|unsafe|silent|cast|lax|get)" >&2
      exit 2
      ;;
  esac
}

classes=("${@:-panic unsafe silent cast lax get}")
for cls in "${classes[@]}"; do
  run_class "$cls"
done

echo
echo "===== summary ====="
if [ "$hard_failures" -gt 0 ]; then
  echo "FAIL: $hard_failures hard violation(s) (panic/unsafe/silent) in production code."
  exit 1
fi
echo "OK: no hard production violations (panic/unsafe/silent)."
