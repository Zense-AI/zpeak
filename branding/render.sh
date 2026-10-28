#!/bin/sh
# Usage: branding/render.sh <option-svg>  — regenerates app + tray icons from one mark.
set -eu
cd "$(dirname "$0")"
SRC="$1"
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
render() { # svg size out
  printf '<html><body style="margin:0"><img src="%s" width="%s" height="%s"></body></html>' "$1" "$2" "$2" > .render.html
  "$CHROME" --headless=new --disable-gpu --hide-scrollbars --default-background-color=00000000 \
    --window-size="$2,$2" --screenshot="$PWD/$3" "file://$PWD/.render.html" >/dev/null 2>&1
}
mkdir -p build
cp "$SRC" build/mark.svg
python3 - "$SRC" <<'PY'
import re,sys
s=open(sys.argv[1]).read()
# Recording variant: red dot. Template icons (macOS menu bar) are derived below.
open('build/mark-recording.svg','w').write(s.replace('</svg>','<circle cx="850" cy="200" r="150" fill="#EF4444" stroke="#fff" stroke-width="40"/></svg>'))
PY
render build/mark.svg 1024 build/icon-1024.png
render build/mark.svg 64 build/tray.png
render build/mark-recording.svg 64 build/tray-recording.png
python3 - <<'PY'
from PIL import Image
def template(src, out):
    im = Image.open(src).convert('RGBA'); px = im.load()
    for y in range(im.height):
        for x in range(im.width):
            r, g, b, a = px[x, y]
            px[x, y] = (0, 0, 0, int(a * (1 - min(r, g, b) / 255)))
    im.save(out)
template('build/tray.png', 'build/tray-template.png')
template('build/tray-recording.png', 'build/tray-template-recording.png')
PY
rm -f .render.html
