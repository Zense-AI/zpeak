"""Build every Zpeak logo file: brand kit (same layout as the Zense AI kit), app icon and tray icons.

Usage: python3 branding/brand.py   (needs Google Chrome, Pillow, fontTools)
"""
import math
import os
import shutil
import subprocess
import tempfile

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTCollection
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
KIT = os.path.join(HERE, 'kit')
BUILD = os.path.join(HERE, 'build')
CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'

SKY, BLACK, WHITE = '#5BC2E7', '#000000', '#FFFFFF'
# CMYK (0–255 per channel) taken from the official Zense AI CMYK files: sky = C60 M0 Y5 K0.
CMYK = {SKY: (152, 0, 13, 0), BLACK: (0, 0, 0, 255), WHITE: (0, 0, 0, 0)}

# Four soundwave bars leaning at the Zense mark's 48° angle.
HEIGHTS, BAR_W, GAP, TILT = [330, 620, 440, 210], 180, 50, 42


def bar_segments(h, w=BAR_W):
    """One bar (a stadium: circular ends, straight sides) as cubic segments around (0, 0)."""
    h = max(h, w)
    r, k = w / 2, 0.5523 * w / 2
    y0, y1 = -h / 2 + r, h / 2 - r           # centres of the two end circles
    line = lambda a, b: (tuple(a[i] + (b[i] - a[i]) / 3 for i in (0, 1)), tuple(a[i] + 2 * (b[i] - a[i]) / 3 for i in (0, 1)), b)
    start = (0, y0 - r)
    segs = [
        ((k, y0 - r), (r, y0 - k), (r, y0)),
        line((r, y0), (r, y1)),
        ((r, y1 + k), (k, y1 + r), (0, y1 + r)),
        ((-k, y1 + r), (-r, y1 + k), (-r, y1)),
        line((-r, y1), (-r, y0)),
        ((-r, y0 - k), (-k, y0 - r), (0, y0 - r)),
    ]
    return start, segs


def mark_geometry():
    """Bars in absolute coordinates, plus their exact bounding box."""
    a = math.radians(TILT)
    ca, sa = math.cos(a), math.sin(a)
    step = (BAR_W + GAP) / ca
    bars = []
    for i, h in enumerate(HEIGHTS):
        cx = i * step
        tf = lambda p, cx=cx: (cx + p[0] * ca - p[1] * sa, p[0] * sa + p[1] * ca)
        start, segs = bar_segments(h)
        bars.append((tf(start), [tuple(tf(p) for p in s) for s in segs]))
    pts = []
    for start, segs in bars:
        p0 = start
        for c1, c2, p1 in segs:
            for i in range(41):
                t = i / 40
                pts.append(tuple((1 - t) ** 3 * p0[k] + 3 * (1 - t) ** 2 * t * c1[k] + 3 * (1 - t) * t * t * c2[k] + t ** 3 * p1[k] for k in (0, 1)))
            p0 = p1
    xs, ys = [p[0] for p in pts], [p[1] for p in pts]
    return bars, (min(xs), min(ys), max(xs), max(ys))


def mark_path(bars, dx, dy, s=1.0):
    f = lambda p: f'{(p[0] + dx) * s:.2f} {(p[1] + dy) * s:.2f}'
    out = []
    for start, segs in bars:
        out.append('M' + f(start) + ''.join('C' + ' '.join(f(p) for p in seg) for seg in segs) + 'Z')
    return ''.join(out)


def wordmark(text='Zpeak'):
    """'Zpeak' in Avenir Next Medium as outlines; returns (path d at 1000-unit cap scale, width, cap, descender)."""
    font = TTCollection('/System/Library/Fonts/Avenir Next.ttc').fonts[5]
    assert font['name'].getDebugName(4) == 'Avenir Next Medium'
    gs, cmap, hmtx = font.getGlyphSet(), font.getBestCmap(), font['hmtx']
    cap = font['OS/2'].sCapHeight
    pen = SVGPathPen(gs)
    x = 0
    for ch in text:
        g = cmap[ord(ch)]
        gs[g].draw(TransformPen(pen, (1, 0, 0, -1, x, cap)))
        x += hmtx[g][0]
    descender = -font['OS/2'].sTypoDescender
    return pen.getCommands(), x, cap, descender


def svg_doc(w, h, body):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.2f} {h:.2f}">{body}</svg>\n'


def logo_only(ink):
    bars, (x0, y0, x1, y1) = mark_geometry()
    return svg_doc(x1 - x0, y1 - y0, f'<path fill="{ink}" d="{mark_path(bars, -x0, -y0)}"/>')


def horizontal(ink_mark, ink_text):
    bars, (x0, y0, x1, y1) = mark_geometry()
    mw, mh = x1 - x0, y1 - y0
    d, tw, cap, desc = wordmark()
    k = (mh * 0.62) / cap                     # cap height relative to the mark, matching the Zense lockup
    gap = mh * 0.42
    ty = (mh - cap * k) / 2                   # caps centred on the mark
    total_w = mw + gap + tw * k
    total_h = max(mh, ty + (cap + desc) * k)
    body = (f'<path fill="{ink_mark}" d="{mark_path(bars, -x0, -y0)}"/>'
            f'<path fill="{ink_text}" transform="translate({mw + gap:.2f} {ty:.2f}) scale({k:.5f})" d="{d}"/>')
    return svg_doc(total_w, total_h, body)


def render_png(svg_text, out, width, ss=2):
    """Render on white and on black, recover alpha from the difference (Chrome's transparent mode drops anti-aliasing)."""
    vb = [float(v) for v in svg_text.split('viewBox="')[1].split('"')[0].split()]
    height = round(width * vb[3] / vb[2])
    shots = {}
    with tempfile.TemporaryDirectory() as tmp:
        open(os.path.join(tmp, 'm.svg'), 'w').write(svg_text)
        for bg in ('ffffff', '000000'):
            html = os.path.join(tmp, f'{bg}.html')
            open(html, 'w').write(f'<html><body style="margin:0;background:#{bg}">'
                                  f'<img src="m.svg" width="{width}" height="{height}" style="display:block"></body></html>')
            shot = os.path.join(tmp, f'{bg}.png')
            subprocess.run([CHROME, '--headless=new', '--disable-gpu', '--hide-scrollbars', f'--force-device-scale-factor={ss}',
                            f'--window-size={width},{height}', f'--screenshot={shot}', f'file://{html}'], check=True, capture_output=True)
            shots[bg] = Image.open(shot).convert('RGB').crop((0, 0, width * ss, height * ss))
    on_w, on_b = shots['ffffff'].getdata(), shots['000000'].getdata()
    px = []
    for (wr, wg, wb), (br, bgc, bb) in zip(on_w, on_b):
        a = 255 - round(((wr - br) + (wg - bgc) + (wb - bb)) / 3)
        a = max(0, min(255, a))
        px.append((0, 0, 0, 0) if a == 0 else (min(255, round(br * 255 / a)), min(255, round(bgc * 255 / a)), min(255, round(bb * 255 / a)), a))
    im = Image.new('RGBA', (width * ss, height * ss))
    im.putdata(px)
    im = im.resize((width, height), Image.LANCZOS)
    im.save(out, dpi=(72, 72))
    return im


def flatten(im, bg_hex, out_rgb, out_cmyk, fg_hex):
    bg = Image.new('RGBA', im.size, bg_hex)
    Image.alpha_composite(bg, im).convert('RGB').save(out_rgb, quality=95, dpi=(72, 72))
    # CMYK built from coverage so brand colours land on the exact official CMYK values.
    a = im.getchannel('A')
    fgc, bgc = CMYK[fg_hex], CMYK[bg_hex]
    chans = [a.point(lambda v, f=f, b=b: round(b + (f - b) * v / 255)) for f, b in zip(fgc, bgc)]
    Image.merge('CMYK', chans).save(out_cmyk, quality=95, dpi=(72, 72))


def build_kit():
    if os.path.isdir(KIT):
        shutil.rmtree(KIT)
    variants = [('Color', SKY, BLACK, WHITE), ('BW', BLACK, BLACK, WHITE), ('Reverse', WHITE, WHITE, BLACK)]
    folders = {'Color': '', 'BW': 'Black and White', 'Reverse': 'Reverse'}
    for group, maker, png_w in [('Logo only', lambda m, t: logo_only(m), 1200), ('Horizontal Version', horizontal, 1400)]:
        tag = 'Logo-Only' if group == 'Logo only' else 'Logo-H'
        for name, ink_mark, ink_text, bg in variants:
            svg = maker(ink_mark, ink_text)
            base = os.path.join(KIT, group)
            for sub in ['SVG', 'PNG', os.path.join(folders[name], 'RGB', 'JPG'), os.path.join(folders[name], 'CMYK', 'JPG')]:
                os.makedirs(os.path.join(base, sub), exist_ok=True)
            open(os.path.join(base, 'SVG', f'SVG-Zpeak-{tag}-{name}-RGB.svg'), 'w').write(svg)
            im = render_png(svg, os.path.join(base, 'PNG', f'RGB-Zpeak-{tag}-{name}.png'), png_w)
            flatten(im, bg,
                    os.path.join(base, folders[name], 'RGB', 'JPG', f'RGB-Zpeak-{tag}-{name}.jpg'),
                    os.path.join(base, folders[name], 'CMYK', 'JPG', f'CMYK-Zpeak-{tag}-{name}.jpg'),
                    ink_mark)


def app_icon_svg():
    """White rounded tile (macOS grid: 824 body in 1024) with the sky mark."""
    bars, (x0, y0, x1, y1) = mark_geometry()
    mw, mh = x1 - x0, y1 - y0
    s = 560 / mw
    dx, dy = (1024 - mw * s) / 2 / s - x0, (1024 - mh * s) / 2 / s - y0
    return svg_doc(1024, 1024,
                   '<defs><filter id="s" x="-20%" y="-20%" width="140%" height="140%">'
                   '<feDropShadow dx="0" dy="10" stdDeviation="14" flood-color="#1E4078" flood-opacity="0.22"/></filter></defs>'
                   '<rect x="100" y="100" width="824" height="824" rx="185" fill="#FFFFFF" filter="url(#s)"/>'
                   f'<path fill="{SKY}" d="{mark_path(bars, dx, dy, s)}"/>')


def tray_svg(ink, recording, dot_ink='#EF4444'):
    bars, (x0, y0, x1, y1) = mark_geometry()
    mw, mh = x1 - x0, y1 - y0
    side = max(mw, mh) * 1.04
    dx, dy = (side - mw) / 2 - x0, (side - mh) / 2 - y0
    path = mark_path(bars, dx, dy)
    if not recording:
        return svg_doc(side, side, f'<path fill="{ink}" d="{path}"/>')
    cx, cy, r = side * 0.8, side * 0.2, side * 0.17
    # The dot sits in a transparent ring cut out of the bars so it reads at 16px and as a template image.
    return svg_doc(side, side,
                   f'<defs><mask id="m"><rect width="{side:.1f}" height="{side:.1f}" fill="#fff"/>'
                   f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r * 1.35:.1f}" fill="#000"/></mask></defs>'
                   f'<path fill="{ink}" mask="url(#m)" d="{path}"/>'
                   f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r:.1f}" fill="{dot_ink}"/>')


def build_app():
    os.makedirs(BUILD, exist_ok=True)
    open(os.path.join(BUILD, 'mark.svg'), 'w').write(logo_only(SKY))
    open(os.path.join(BUILD, 'app-icon.svg'), 'w').write(app_icon_svg())
    render_png(app_icon_svg(), os.path.join(BUILD, 'icon-1024.png'), 1024)
    for rec in (False, True):
        suffix = '-recording' if rec else ''
        render_png(tray_svg(SKY, rec), os.path.join(BUILD, f'tray{suffix}.png'), 64)
        render_png(tray_svg(BLACK, rec, dot_ink=BLACK), os.path.join(BUILD, f'tray-template{suffix}.png'), 64)


if __name__ == '__main__':
    build_kit()
    build_app()
    print('kit →', KIT)
    print('app →', BUILD)
