# The copy-mode loop on ranma.masshiro.pro: search back, select, yank.
# Recorded by site/scripts/captures.sh videos, in the staged home the demo
# uses (scripts/demo/stage.sh); run that, not vhs directly.

Output temp/site/copy.mp4

Set Shell bash
Set FontSize 14
Set Width 1200
Set Height 675
Set Padding 0
Set Theme "Catppuccin Mocha"
Set TypingSpeed 65ms
Set CursorBlink false

Hide
Type "cd ~/projects/ranma && clear"
Enter
Sleep 300ms
Type "ranma"
Enter
Sleep 1.5s
Type "git --no-pager log --oneline -60"
Enter
Sleep 1s
Show

Sleep 800ms
# leader /: search the pane's history, most recent match first.
Ctrl+B
Sleep 400ms
Type "/"
Sleep 500ms
Type "layout"
Sleep 400ms
Enter
Sleep 1.2s
# n: the next match back; V selects the line, y yanks it to the clipboard.
Type "n"
Sleep 900ms
Type "V"
Sleep 700ms
Type "y"
Sleep 2s
