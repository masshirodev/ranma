# rpm replaces the binary but cannot do what install.sh does: check the new
# build against the user's config, or move their running servers onto it.
# $1 counts the installed copies: 2 or more is an upgrade.
if [ "$1" -ge 2 ]; then
  cat <<'MSG'
ranma was upgraded. As your user, check your config against the new build,
then move running servers onto it (shells and scrollback stay):
  ranma --check-config && ranma upgrade --all
If the check fails, a terminal that starts ranma falls back to a plain
shell until the config is fixed.
MSG
fi
