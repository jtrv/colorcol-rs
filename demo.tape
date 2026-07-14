# vhs demo.tape  -> demo.gif
# Self-contained: builds the binary, puts it on PATH, runs kak -n (no user kakrc).

Output demo.gif

Set Shell bash
Set FontSize 20
Set Width 1200
Set Height 600
Set Padding 15
Set Framerate 24
Set TypingSpeed 50ms
Set Theme "Catppuccin Mocha"

Env COLORTERM truecolor

# --- setup (not recorded) ---
Hide
Type `export PATH="$PWD/target/release:$PATH"; cargo build --release -q` Enter
Wait@180s
Type `cd $(mktemp -d)` Enter
Type `printf '%s\n' '/* palette.css */' ':root {' '  --red:    #ff5555;' '  --orange: #ffb86c;' '  --yellow: #f1fa8c;' '  --green:  #50fa7b;' '  --cyan:   #8be9fd;' '  --purple: #bd93f9;' '} ' '.card   { background: rgb(40 42 54); color: #f8f8f2; }' '.badge  { background: hsl(265 89% 78%); }' '.hint   { color: oklch(0.75 0.12 200); }' '.kakrc  { face: rgb:6272a4; }' > palette.css` Enter
Type `clear` Enter
Type `kak -n -e 'evaluate-commands %sh{ colorcol }' palette.css` Enter
Sleep 2s
Show

# --- plain buffer ---
Sleep 1500ms

# --- enable: markers appear after each literal ---
Type ":colorcol-enable"
Sleep 500ms
Enter
Sleep 2500ms

# --- background mode: the literals themselves get their color ---
Type ":colorcol-mode background"
Sleep 500ms
Enter
Sleep 3s

# --- live: recolor as you type ---
Type ":colorcol-refresh-continuous"
Sleep 500ms
Enter
Sleep 1s

Type "/ff5555"
Enter
Sleep 800ms
Type "c"
Sleep 300ms
Type "00bfff"
Escape
Sleep 3s

Hide
Type ":q!"
Enter
