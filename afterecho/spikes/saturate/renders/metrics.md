## Saturate, alone at Amount 100 %

| Curve | Drive | Gain at −6 dBFS (dB) | 2nd harmonic (dB) | 3rd harmonic (dB) | THD (%) | Aliasing, 7 kHz in (dB) | CPU (ns / stereo sample) |
|---|---|---|---|---|---|---|---|
| today (tanh) | 6 dB | +4.2 | -134 | -24 | 6.7 | -43 | 9 |
| today (tanh) | 18 dB | +8.2 | -129 | -11 | 30.0 | -18 | 11 |
| TUBE | 6 dB | -2.2 | -27 | -19 | 12.4 | -47 | 480 |
| TUBE | 18 dB | -8.3 | -24 | -11 | 33.0 | -47 | 489 |
| TAPE | 6 dB | -1.0 | -134 | -27 | 4.4 | -47 | 492 |
| TAPE | 18 dB | -8.1 | -128 | -11 | 33.2 | -47 | 505 |
| GRIT | 6 dB | -1.5 | -134 | -25 | 5.4 | -47 | 464 |
| GRIT | 18 dB | -7.2 | -129 | -11 | 30.5 | -47 | 482 |
| AMP | 6 dB | -2.6 | -134 | -20 | 10.2 | -47 | 534 |
| AMP | 18 dB | -7.1 | -129 | -12 | 27.3 | -47 | 589 |
| TRANSFO | 6 dB | -0.5 | -33 | -28 | 4.9 | -47 | 532 |
| TRANSFO | 18 dB | -7.2 | -39 | -11 | 33.2 | -47 | 588 |

Latency: today 0 samples; the crate's chain 20 samples (its 8× filters).

## The loop limit

| Limit | Feedback | Settled peak | Settled RMS | Tail brightness (centroid, Hz) |
|---|---|---|---|---|
| today 2·tanh(x/2) | 95 % | 0.158 | 0.034 | 12177 |
| today 2·tanh(x/2) | 110 % | 0.667 | 0.154 | 12159 |
| PolSat531 | 95 % | 0.175 | 0.036 | 12182 |
| PolSat531 | 110 % | 1.607 | 0.246 | 12190 |
