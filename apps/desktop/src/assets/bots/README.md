# Companion artwork v2

Original generated companion atlas, replacing the initial geometric SVG bots.

`companions-v2.png`: 1536 × 1024 RGBA PNG, six columns and four rows.

Rows: pearl ceramic / graphite / lavender plush / warm ceramic.
Columns: idle / focused working / waiting / success / concerned / resting.

The atlas has real alpha transparency. CSS selects a cell without runtime cropping,
network generation, or user-controlled asset URLs. Identity seeds select a stable
family and restrained palette variation. Active states animate with CSS; reduced
motion disables animation. Keep the full atlas in the production bundle.

Art direction: tactile rounded companions, expressive faces, oversized heads, and
small bodies, taking inspiration from the Muse/Grok companion direction requested
by the user. The artwork is original and includes no third-party logos.

## Frame boundaries

The generated characters do not fit an exact grid: some feet/ears cross the
nominal 256px cell boundaries. Equal-grid rendering exposed fragments of adjacent
characters. `frames.json` records the measured subject rectangles (x, y, width,
height), including a 1px edge allowance. Per-family/pose CSS uses those bounds
and fits the intact character into 90% of the avatar box without stretching.
Do not switch back to `background-size: 600% 400%` or rounded grid clipping.
