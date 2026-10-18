#!/bin/sh
# Prints Markdown release notes from the `Changelog:` trailers of the commits
# in <rev-or-range> (default: all of HEAD), oldest first, grouped by kind.
# Commits without the trailer are left out; see CONTRIBUTING.md.
#
# Usage: changelog.sh [<rev-or-range>]
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
git -C "$root" log --reverse --format='%(trailers:key=Changelog,valueonly,separator=%x2C)%x09%s' ${1:+"$1"} |
awk -F '\t' '
BEGIN { n = split("added changed deprecated removed fixed security performance", kind, " ")
        split("Added Changed Deprecated Removed Fixed Security Performance", title, " ") }
{ k = tolower($1); gsub(/^[ ]+|[ ]+$/, "", k); if (k == "") next
  s = $0; sub(/^[^\t]*\t/, "", s); items[k] = items[k] "- " s "\n" }
END { out = 0
      for (i = 1; i <= n; i++) if (kind[i] in items) {
          printf "%s### %s\n\n%s", (out ? "\n" : ""), title[i], items[kind[i]]; out = 1; delete items[kind[i]] }
      for (k in items) printf "warning: unknown changelog category: %s\n", k > "/dev/stderr"
      if (!out) print "No user-visible changes." }'
