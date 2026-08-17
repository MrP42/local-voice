"""Generate the Sprechstift app and tray icons.

The mark is a pen nib with two speech arcs off its tip — dictation as writing.
Drawn on a Signalgelb tile with an ink glyph, which is the design system's
recommendation for a static app icon: it stays legible on light and dark task
bars alike, whereas a yellow glyph needs a dark ground to be visible at all
(design-system_wolffappliedai, references/logo.md, "Solo-W").

Tray icons are the inverse — a bare ink or white glyph with no tile — because
Windows draws them on the taskbar's own background.

Run: py -3 apps/local-voice/scripts/make-icons.py
"""

from pathlib import Path
from PIL import Image, ImageDraw

WAI_YELLOW = (255, 221, 0, 255)
INK = (17, 20, 24, 255)
WHITE = (255, 255, 255, 255)

ROOT = Path(__file__).resolve().parents[1]
ICONS = ROOT / "src-tauri" / "icons"
RESOURCES = ROOT / "src-tauri" / "resources"

# Icon geometry on a 128-unit grid, scaled per output size. The pen occupies
# the left two thirds, the arcs the right third, and everything stays inside a
# 14-unit margin so nothing clips at the rounded corners.
NIB = [(60, 30), (78, 48), (38, 88), (24, 93), (29, 79)]
NIB_SLIT = [(60, 30), (78, 48)]
# (centre-relative radius, stroke weight); centred on the pen tip's far side.
WAVE_CENTRE = (74, 66)
WAVES = [(0.16, 1.0), (0.27, 1.0)]


def draw_mark(size: int, glyph, tile=None, pad_ratio: float = 0.0) -> Image.Image:
    """Render the pen mark at `size` px. `tile=None` leaves the ground clear."""
    ss = 8  # supersample, then downscale — keeps the diagonals clean
    s = size * ss
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    if tile is not None:
        radius = int(s * 0.22)
        d.rounded_rectangle([0, 0, s - 1, s - 1], radius=radius, fill=tile)

    k = s / 128.0
    inset = size * pad_ratio * ss

    def p(pt):
        x, y = pt
        return (inset + x * k * (1 - 2 * pad_ratio),
                inset + y * k * (1 - 2 * pad_ratio))

    d.polygon([p(pt) for pt in NIB], fill=glyph)
    # The slit down the nib, cut back out so the pen reads as a nib not a blob.
    d.line([p(NIB_SLIT[0]), p(NIB_SLIT[1])],
           fill=tile if tile is not None else (0, 0, 0, 0),
           width=max(1, int(s * 0.018)))

    # Speech arcs off the writing tip.
    cx, cy = p((96, 74))
    for rel_r, width_scale in WAVES:
        r = s * rel_r * (1 - 2 * pad_ratio)
        d.arc([cx - r, cy - r, cx + r, cy + r], start=-58, end=58,
              fill=glyph, width=max(1, int(s * 0.021 * width_scale / 1.9)))

    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    ICONS.mkdir(parents=True, exist_ok=True)

    # App icon: ink glyph on the yellow tile.
    app_sizes = {
        "32x32.png": 32, "64x64.png": 64, "128x128.png": 128,
        "128x128@2x.png": 256, "icon.png": 512, "logo.png": 512,
        "StoreLogo.png": 50, "Square30x30Logo.png": 30,
        "Square44x44Logo.png": 44, "Square71x71Logo.png": 71,
        "Square89x89Logo.png": 89, "Square107x107Logo.png": 107,
        "Square142x142Logo.png": 142, "Square150x150Logo.png": 150,
        "Square284x284Logo.png": 284, "Square310x310Logo.png": 310,
    }
    for name, size in app_sizes.items():
        draw_mark(size, glyph=INK, tile=WAI_YELLOW).save(ICONS / name)
    print(f"{len(app_sizes)} app icons")

    ico = ICONS / "icon.ico"
    base = draw_mark(256, glyph=INK, tile=WAI_YELLOW)
    base.save(ico, sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    print("icon.ico")

    # Tray icons carry no tile — Windows draws them on the taskbar's own
    # background. tray.rs picks by theme: the plain names are the light glyphs
    # used on a dark taskbar, the `_dark` ones are the ink glyphs for a light
    # one, and the `Colored` set is the Linux fallback. Recording is yellow in
    # every theme so the "we are listening" state is unmistakable.
    tray = {
        "tray_idle.png": WHITE,
        "tray_recording.png": WAI_YELLOW,
        "tray_transcribing.png": WHITE,
        "tray_idle_dark.png": INK,
        "tray_recording_dark.png": WAI_YELLOW,
        "tray_transcribing_dark.png": INK,
        "handy.png": WAI_YELLOW,
        "recording.png": WAI_YELLOW,
        "transcribing.png": WAI_YELLOW,
    }
    RESOURCES.mkdir(parents=True, exist_ok=True)
    for name, colour in tray.items():
        draw_mark(64, glyph=colour, tile=None, pad_ratio=0.06).save(RESOURCES / name)
    print(f"{len(tray)} tray icons")


if __name__ == "__main__":
    main()
