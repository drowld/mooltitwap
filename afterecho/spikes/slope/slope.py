"""Continuous tap-filter slope, 6 to 48 dB/oct: a measured spike (owner asked, 2026-11-16).

Today a tap filter is 1, 2 or 4 of softsystems-fx's TPT SVFs in a chain (12, 24, 48 dB/oct). A
continuous Slope would pass through the orders 6 (a one-pole), 12, 24, 36 and 48 (1-4 SVFs) and
blend between the two orders either side. Two ways to blend are measured:

  A. parallel: (1 - f) * H_lo + f * H_hi            (both chains run, their outputs mixed)
  B. serial:   H_lo * ((1 - f) + f * H_stage)       (the next stage faded in, in the chain)
     (6 -> 12 replaces the one-pole with an SVF, so that segment is parallel in both.)

The SVF is the TPT (bilinear, prewarped) state-variable filter, so its response is the analog
prototype 1 / (s^2 + s/Q + 1) through the bilinear transform; evaluated exactly, no simulation.

Run: python3 slope.py  ->  metrics.md and response-*.svg next to this file.
"""

import math
import os

import numpy as np

SR = 48000.0
FC = 1000.0
HERE = os.path.dirname(os.path.abspath(__file__))

freqs = np.geomspace(20.0, 20000.0, 2000)


def s_of(f, fc):
    """The bilinear s, normalised to the prewarped cutoff, on the unit circle at f."""
    g = math.tan(math.pi * fc / SR)
    z = np.exp(1j * 2.0 * math.pi * f / SR)
    return (z - 1.0) / (z + 1.0) / g


def svf(f, fc, q, kind):
    s = s_of(f, fc)
    d = s * s + s / q + 1.0
    return {"lp": 1.0 / d, "hp": s * s / d, "bp": (s / q) / d}[kind]


def pole1(f, fc, kind):
    s = s_of(f, fc)
    return {"lp": 1.0 / (s + 1.0), "hp": s / (s + 1.0)}[kind]


ORDERS = [6, 12, 24, 36, 48]  # dB/oct; index i -> i SVF stages (0 = the one-pole)


def chain(f, fc, q, kind, i):
    return pole1(f, fc, kind) if i == 0 else svf(f, fc, q, kind) ** i


def blended(f, fc, q, kind, slope, how):
    slope = min(max(slope, 6.0), 48.0)
    i = max(k for k in range(len(ORDERS) - 1) if ORDERS[k] <= slope) if slope < 48 else 3
    lo, hi = ORDERS[i], ORDERS[i + 1]
    fr = (slope - lo) / (hi - lo)
    h_lo, h_hi = chain(f, fc, q, kind, i), chain(f, fc, q, kind, i + 1)
    if how == "A" or i == 0:
        return (1.0 - fr) * h_lo + fr * h_hi
    return h_lo * ((1.0 - fr) + fr * svf(f, fc, q, kind))


def tilt(f, fc, alpha, kind, n=2, span=64.0):
    """A fractional tilt: -6*alpha dB/oct past the cutoff, from 2n+1 staggered one-pole
    zero/pole pairs (Oustaloup's approximation) between fc and fc*span (fc/span for HP)."""
    if alpha <= 1e-6:
        return np.ones_like(f, dtype=complex)
    if kind == "hp":
        # the mirror image about the cutoff (s -> 1/s): each section (1 + w'/s) / (1 + w/s) with
        # its corners at fc^2/w, below the cutoff. Its corners sit at or under fc, so the
        # bilinear warp is small; the sections are evaluated through it like the rest.
        lo, hi = fc / span, fc
        h = np.ones_like(f, dtype=complex)
        for k in range(-n, n + 1):
            wz = lo * (hi / lo) ** ((k + n + 0.5 - alpha / 2) / (2 * n + 1))
            wp = lo * (hi / lo) ** ((k + n + 0.5 + alpha / 2) / (2 * n + 1))
            # at low f it tends to wz/wp < 1: the lows fall away below the cutoff
            h *= (1 + 1 / s_of(f, wz)) / (1 + 1 / s_of(f, wp))
        return h
    lo, hi = fc, fc * span
    h = np.ones_like(f, dtype=complex)
    for k in range(-n, n + 1):
        wz = lo * (hi / lo) ** ((k + n + 0.5 - alpha / 2) / (2 * n + 1))
        wp = lo * (hi / lo) ** ((k + n + 0.5 + alpha / 2) / (2 * n + 1))
        h *= (1 + s_of(f, wz)) / (1 + s_of(f, wp))  # zero below pole: falls between them
    return h


def tilted(f, fc, q, kind, slope):
    """Slope in 6 dB steps from whole stages (a one-pole for an odd step, SVFs for pairs), and
    the fraction between from the tilt."""
    m = min(max(slope, 6.0), 48.0) / 6.0
    whole = int(math.floor(m + 1e-9))
    alpha = m - whole
    h = svf(f, fc, q, kind) ** (whole // 2)
    if whole % 2:
        h = h * pole1(f, fc, kind)
    return h * tilt(f, fc, alpha, kind)


def db(h):
    return 20.0 * np.log10(np.maximum(np.abs(h), 1e-12))


def at(f_hz, curve):
    return float(np.interp(math.log(f_hz), np.log(freqs), curve))


def metrics(curve, kind):
    """Level at the cutoff; the worst bump in the pass band; the slope one to two and two to
    three octaves into the stop band; the deepest notch (a dip that rises again after it)."""
    stop = [FC * 2, FC * 4, FC * 8] if kind == "lp" else [FC / 2, FC / 4, FC / 8]
    s12 = at(stop[0], curve) - at(stop[1], curve)
    s23 = at(stop[1], curve) - at(stop[2], curve)
    band = freqs <= FC if kind == "lp" else freqs >= FC
    bump = float(curve[band].max())
    # a notch: in the stop band, the curve should only fall as it goes away from the cutoff
    side = curve[freqs >= FC] if kind == "lp" else curve[freqs <= FC][::-1]
    notch = float((np.maximum.accumulate(side[::-1])[::-1] - side).max())
    return at(FC, curve), bump, s12, s23, notch


def svg_plot(path, title, curves):
    w, h, l, t, r, b = 720, 360, 50, 30, 10, 30
    x = lambda f: l + (math.log10(f) - math.log10(20)) / 3.0 * (w - l - r)
    y = lambda d: t + (6 - d) / 96.0 * (h - t - b)
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" font-family="sans-serif" font-size="11">',
           f'<rect width="{w}" height="{h}" fill="#16191c"/>', f'<text x="{l}" y="18" fill="#ddd">{title}</text>']
    for d in range(0, -91, -12):
        out.append(f'<line x1="{l}" x2="{w - r}" y1="{y(d):.1f}" y2="{y(d):.1f}" stroke="#333"/>')
        out.append(f'<text x="{l - 6}" y="{y(d) + 4:.1f}" fill="#888" text-anchor="end">{d}</text>')
    for f in [100, 1000, 10000]:
        out.append(f'<line x1="{x(f):.1f}" x2="{x(f):.1f}" y1="{t}" y2="{h - b}" stroke="#333"/>')
        out.append(f'<text x="{x(f):.1f}" y="{h - 10}" fill="#888" text-anchor="middle">{f} Hz</text>')
    for i, (label, c) in enumerate(curves):
        hue = int(270 - 230 * i / max(1, len(curves) - 1))
        pts = " ".join(f"{x(f):.1f},{y(max(d, -90)):.1f}" for f, d in zip(freqs[::4], c[::4]))
        out.append(f'<polyline points="{pts}" fill="none" stroke="hsl({hue},70%,65%)" stroke-width="1.4"/>')
        out.append(f'<text x="{w - r - 4}" y="{t + 14 + 13 * i}" fill="hsl({hue},70%,65%)" text-anchor="end">{label}</text>')
    out.append("</svg>")
    open(path, "w").write("\n".join(out))


def main():
    slopes = [6, 9, 12, 15, 18, 21, 24, 30, 36, 42, 48]
    lines = ["# Continuous tap-filter slope: measurements", "",
             f"Cutoff {FC:.0f} Hz, {SR / 1000:.0f} kHz. `python3 slope.py` writes this file. "
             "Columns: level at the cutoff; the highest point of the pass band (a bump above 0 dB); "
             "the drop over the first and second octave past the 2× point (dB/oct; the target is "
             "the Slope setting); the deepest notch (a dip that comes back up, in dB).", ""]
    for kind in ["lp", "hp"]:
        for q in [0.707, 4.0]:
            for how in ["A", "C"]:
                lines += [f"## {kind.upper()}, Q {q}, blend {how} ({'blend of two orders' if how == 'A' else 'whole stages + a fractional tilt'})", "",
                          "| Slope | at cutoff | pass-band peak | 2→4× | 4→8× | notch |", "|---|---|---|---|---|---|"]
                curves = []
                for sl in slopes:
                    c = db(blended(freqs, FC, q, kind, sl, how) if how == "A" else tilted(freqs, FC, q, kind, sl))
                    a, bump, s12, s23, notch = metrics(c, kind)
                    lines.append(f"| {sl} | {a:+.1f} | {bump:+.1f} | {s12:.1f} | {s23:.1f} | {notch:.1f} |")
                    if sl in (6, 9, 12, 18, 24, 30, 36, 42, 48):
                        curves.append((f"{sl} dB/oct", c))
                lines.append("")
                svg_plot(os.path.join(HERE, f"response-{kind}-q{q}-{how}.svg"),
                         f"{kind.upper()} Q {q}, blend {how}, Slope 6 to 48", curves)
    open(os.path.join(HERE, "metrics.md"), "w").write("\n".join(lines) + "\n")
    print("\n".join(lines))


main()
