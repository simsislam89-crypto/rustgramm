"""Атлас Apple-эмодзи из кэша AyuGram (локально). Предсказание по порядку emoji.txt (Unicode 16),
уточнение выравниванием в узкой полосе по признакам цвета/формы. В git не класть."""
import re
import sys

import numpy as np
from PIL import Image

sys.stdout.reconfigure(encoding="utf-8")
ROOT = "D:/working WIN/rustgramm/crates/rg-ui/assets/"
CACHE = "D:/download WIN/AyuGram/tdata/emoji/"
EMOJI_TXT, OUT = sys.argv[1], sys.argv[2]
BAND = int(sys.argv[3]) if len(sys.argv) > 3 else 40
SIZE, CELL, COLS = 24, 36, 32
W = COLS * SIZE

full = np.concatenate(
    [
        np.frombuffer(open(CACHE + f"cache_{SIZE}_{i}", "rb").read()[16:-32], dtype=np.uint8).reshape(-1, W, 4)
        for i in range(8)
    ],
    axis=0,
)
ncell = (full.shape[0] // SIZE) * COLS


def cell_rgba(k):
    r, c = divmod(k, COLS)
    cell = full[r * SIZE : (r + 1) * SIZE, c * SIZE : (c + 1) * SIZE]
    a = cell[..., 3].astype(np.float32)
    out = np.zeros(cell.shape, np.float32)
    for ch, src in enumerate((2, 1, 0)):
        out[..., ch] = np.where(a > 0, cell[..., src] * 255.0 / np.maximum(a, 1), 0)
    out[..., 3] = a
    return np.clip(out, 0, 255).astype(np.uint8)


def feat(rgba, g=4):
    h, w = rgba.shape[:2]
    af = rgba[..., 3].astype(np.float32) / 255.0
    f = []
    for i in range(g):
        for j in range(g):
            ys, ye, xs, xe = i * h // g, (i + 1) * h // g, j * w // g, (j + 1) * w // g
            al = af[ys:ye, xs:xe]
            s = al.sum() + 1e-6
            col = (rgba[ys:ye, xs:xe, :3].astype(np.float32) * al[..., None]).sum((0, 1)) / s
            f.extend(list(col / 255.0))
            f.append(al.mean() * 1.5)
    return np.array(f, np.float32)


t = open(EMOJI_TXT, encoding="utf-8").read()
items = re.findall(r'"([^"]+)"', t)
index = {}
for it in items:
    b = it.replace("\ufe0f", "")
    if b not in index:
        index[b] = len(index)

chars = [int(l.strip(), 16) for l in open(ROOT + "emoji.idx") if l.strip()]
tw = np.array(Image.open(ROOT + "emoji.png").convert("RGBA"))
order = sorted([n for n, cp in enumerate(chars) if chr(cp) in index], key=lambda n: index[chr(chars[n])])
pred = np.array([index[chr(chars[n])] for n in order])
A = np.array([feat(tw[(n // COLS) * CELL : (n // COLS + 1) * CELL, (n % COLS) * CELL : (n % COLS + 1) * CELL]) for n in order])
B = np.array([feat(cell_rgba(k)) for k in range(ncell)])
d2 = (A**2).sum(1)[:, None] + (B**2).sum(1)[None, :] - 2 * A @ B.T
S = 3.0 - np.sqrt(np.maximum(d2, 0))
nA, m = S.shape
js = np.arange(m)[None, :]
S = np.where(np.abs(js - pred[:, None]) <= BAND, S - 0.01 * np.abs(js - pred[:, None]), -1e6)
GAP = 1.0
prev = np.zeros(m + 1, np.float32)
curs, runs = [], [prev]
for i in range(nA):
    cm = np.full(m + 1, -1e9, np.float32)
    cm[1:] = prev[:-1] + S[i]
    cs = prev - GAP
    cur = np.maximum(cm, cs)
    curs.append((cm, cs, cur))
    prev = np.maximum.accumulate(cur)
    runs.append(prev)
j = int(np.argmax(runs[nA]))
i = nA - 1
match = {}
while i >= 0 and j > 0:
    cm, cs, cur = curs[i]
    while j > 0 and runs[i + 1][j] > cur[j] + 1e-6:
        j -= 1
    if j == 0:
        break
    if cm[j] >= cs[j] and S[i][j - 1] > -1e5:
        match[order[i]] = j - 1
        i -= 1
        j -= 1
    else:
        i -= 1
print("сопоставлено", len(match), "из", len(order), "среднее смещение", np.mean([abs(match[order[k]] - pred[k]) for k in range(nA) if order[k] in match]))

out = Image.fromarray(tw)
for n, k in match.items():
    im = Image.fromarray(cell_rgba(k)).resize((CELL, CELL), Image.LANCZOS)
    out.paste(im, ((n % COLS) * CELL, (n // COLS) * CELL))
out.save(OUT, optimize=True)
print(OUT)
