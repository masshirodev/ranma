# The sessions loop on ranma.masshiro.pro: detach, come back, the switcher.
# Recorded by site/scripts/captures.sh videos, in the staged home the demo
# uses (scripts/demo/stage.sh); run that, not vhs directly.

Output temp/site/sessions.mp4

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
# A second session to switch to, and some work to come back to.
Type "ranma open --session notes --cwd ~ -- bash && git --no-pager log --oneline -12"
Enter
Sleep 800ms
Ctrl+B
Sleep 300ms
Type "t"
Sleep 600ms
Type "btop"
Enter
Sleep 2s
Show

Sleep 1s
# leader d: the terminal is back at its shell; ranma keeps running.
Ctrl+B
Sleep 400ms
Type "d"
Sleep 1.6s
Type "ranma"
Sleep 300ms
Enter
Sleep 1.8s
# leader backspace: the session switcher.
Ctrl+B
Sleep 400ms
Backspace
Sleep 2.2s
Escape
Sleep 800ms
