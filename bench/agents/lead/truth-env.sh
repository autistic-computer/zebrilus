#!/bin/bash
# truth-env.sh REPO_DIR_UNDER_HOME: env var names the code reads, by regex, one per line
cd "$HOME/$1" || exit 1
N='([A-Z][A-Z0-9_]+)'
{
  rg -o -N --no-filename -r '$2' -g '!third_party' -g '!node_modules' "env::var(_os)?\(\s*\"$N\"" .
  rg -o -N --no-filename -r '$2' -g '!third_party' -g '!node_modules' "(option_)?env!\(\s*\"$N\"" .
  rg -o -N --no-filename -r '$1' -g '!third_party' -g '!node_modules' "process\.env\.$N" .
  rg -o -N --no-filename -r '$1' -g '!third_party' -g '!node_modules' "process\.env\[\s*['\"]$N['\"]" .
  rg -o -N --no-filename -r '$1' -g '!third_party' -g '!node_modules' "import\.meta\.env\.$N" .
  rg -o -N --no-filename -r '$2' -g '!third_party' -g '!node_modules' "os\.environ(\.get)?[\[(]\s*['\"]$N['\"]" .
  rg -o -N --no-filename -r '$1' -g '!third_party' -g '!node_modules' "os\.getenv\(\s*['\"]$N['\"]" .
  rg -o -N --no-filename -r '$2' -g '!third_party' -g '!node_modules' "os\.(Getenv|LookupEnv)\(\s*\"$N\"" .
} | sort -u
