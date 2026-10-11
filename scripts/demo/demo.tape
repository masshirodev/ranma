# doc/demo.gif. Recorded by scripts/demo/record.sh, which stages a home of its
# own (stage.sh) and runs this with it; run that, not vhs directly.

Output doc/demo.gif

Set Shell bash
Set FontSize 12
Set Width 1600
Set Height 900
Set Padding 0
Set Theme "Catppuccin Mocha"
Set TypingSpeed 65ms
Set CursorBlink false

Hide
Type "cd ~/projects/ranma && clear"
Enter
Sleep 300ms
Show

Type "ranma"
Sleep 300ms
Enter
Sleep 1.5s

# A pane is a PTY: an editor in the first.
Type "nvim src/main.rs"
Enter
Sleep 2s

# The leader (ctrl+b) enters WM mode. A key that opens a pane ends it; the
# rest (layouts, focus, float, workspaces) keep it until esc. An
# esc sent outside WM mode reaches the shell, where it eats the next letter,
# so the tape presses it only where the mode is still on.
Ctrl+B
Sleep 500ms
Type "t"
Sleep 600ms
Type "btop"
Enter
Sleep 2.5s

Ctrl+B
Sleep 300ms
Type "t"
Sleep 600ms
Type "git --no-pager log --oneline --graph -16"
Enter
Sleep 2.5s

# Layouts: space steps through tmux's presets; the arrows move focus.
Ctrl+B
Sleep 500ms
Space
Sleep 1.8s
Space
Sleep 1.8s
Space
Sleep 1.8s
Left
Sleep 800ms
Escape
Sleep 500ms

# A float over the tiling.
Ctrl+B
Sleep 300ms
Type "t"
Sleep 600ms
Ctrl+B
Sleep 300ms
Type "w"
Sleep 600ms
Escape
Sleep 500ms
Type "ls src"
Enter
Sleep 2s

# A second workspace, and back.
Ctrl+B
Sleep 300ms
Type "2"
Sleep 1.2s
Type "t"
Sleep 600ms
Type "echo a workspace of its own"
Enter
Sleep 1.5s
Ctrl+B
Sleep 300ms
Type "1"
Sleep 1.2s
Escape
Sleep 500ms

# Anything can raise a toast.
Type "sleep 1 && ranma notify 'tests passed'"
Enter
Sleep 3s

# Pause in WM mode and the which-key hint lists every key.
Ctrl+B
Sleep 3s
Escape
Sleep 800ms
