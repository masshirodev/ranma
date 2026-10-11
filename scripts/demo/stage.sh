#!/bin/sh
# Stages the world doc/demo.gif is recorded in: a home of its own with a plain
# bash prompt, btop showing only its CPU box (a process list would record the
# machine's own processes, and the CPU box alone fits the smallest layout), a shallow clone of ranma to work in,
# and a ranma config that loads no plugin manager and checks for no updates.
#
#   scripts/demo/stage.sh DIR    # prints the home it made
set -eu
root=$1
repo=${RANMA_REPO:-$(cd "$(dirname "$0")/../.." && pwd)}
rm -rf "$root"
home=$root/home
mkdir -p "$home/.config/ranma" "$home/.config/btop" "$home/projects" "$root/run"
chmod 700 "$root/run"

cat > "$home/.bashrc" <<'SH'
PS1='\[\e[2m\]\w\[\e[0m\] $ '
# The distribution's bashrc titles a pane user@host:dir; the bar shows titles.
PROMPT_COMMAND='printf "\e]0;%s\a" "${PWD/#$HOME/\~}"'
SH
cat > "$home/.config/btop/btop.conf" <<'CONF'
shown_boxes = "cpu"
update_ms = 1000
CONF
cat > "$home/.config/ranma/init.lua" <<'LUA'
-- hako would clone itself from GitHub on the first start, mid-recording.
ranma.set { plugin_manager = "none" }
LUA

git clone -q --depth 40 "file://$repo" "$home/projects/ranma"
echo "$home"
