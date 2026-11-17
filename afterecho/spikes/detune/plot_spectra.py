import numpy as np, wave, sys
root, out = sys.argv[1], sys.argv[2]
def rd(f):
    w = wave.open(f); return np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(float) / 32768
fs = 48000; N = 131072; a = 30000
win = np.hanning(N); f = np.fft.rfftfreq(N, 1 / fs)
files = [("Ideal: the chord synthesised +100 cents", "ideal"), ("A: old dual-head Hann (60 ms)", "A-dualhann"),
         ("B: WSOLA (80 ms, ±10 ms search)", "B-wsola"), ("C: phase vocoder, N=4096, phase-locked", "C-pvoc")]
lo, hi, dbmin = 300, 800, -90
sel = (f >= lo) & (f <= hi)
W, PH, PAD_L, PAD_T, GAP = 900, 150, 48, 34, 34
H = PAD_T + len(files) * (PH + GAP) + 30
parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" font-family="Helvetica, Arial, sans-serif" font-size="12">',
         f'<rect width="{W}" height="{H}" fill="#fff"/>',
         f'<text x="{PAD_L}" y="20" font-size="13" font-weight="bold">Pad chord (C3 G3 E4 B4 D5) shifted +100 cents: 2.7 s spectrum, {lo}–{hi} Hz. Sidebands around each line = flutter.</text>']
ref = None
for i, (title, tag) in enumerate(files):
    y = rd(f"{root}/chord/chord_p100_{tag}.wav")
    P = np.abs(np.fft.rfft(y[a:a + N] * win)) ** 2
    if ref is None: ref = P.max()
    db = np.clip(10 * np.log10(P[sel] / ref + 1e-12), dbmin, 0)
    fx = f[sel]
    # reduce points: max per pixel column
    cols = W - PAD_L - 50
    idx = ((fx - lo) / (hi - lo) * (cols - 1)).astype(int)
    colmax = np.full(cols, dbmin); np.maximum.at(colmax, idx, db)
    y0 = PAD_T + i * (PH + GAP) + 18
    pts = " ".join(f"{PAD_L + c},{y0 + (-v) / -dbmin * PH:.1f}" for c, v in enumerate(colmax))
    parts.append(f'<text x="{PAD_L}" y="{y0 - 6}" font-weight="bold">{title}</text>')
    for g in (0, -30, -60, -90):
        yy = y0 + (-g) / -dbmin * PH
        parts.append(f'<line x1="{PAD_L}" y1="{yy:.1f}" x2="{W - 50}" y2="{yy:.1f}" stroke="#ddd"/>')
        parts.append(f'<text x="{PAD_L - 6}" y="{yy + 4:.1f}" text-anchor="end" fill="#666">{g}</text>')
    parts.append(f'<polyline fill="none" stroke="#111" stroke-width="0.8" points="{pts}"/>')
yb = PAD_T + len(files) * (PH + GAP) - GAP + 18 + 16
for hz in range(lo, hi + 1, 50):
    x = PAD_L + (hz - lo) / (hi - lo) * (W - PAD_L - 50 - 1)
    parts.append(f'<text x="{x:.1f}" y="{yb}" text-anchor="middle" fill="#666">{hz}</text>')
parts.append(f'<text x="{PAD_L}" y="{yb + 16}" fill="#666">Hz (x) · dB relative to the strongest partial (y)</text>')
parts.append("</svg>")
open(out, "w").write("\n".join(parts))
