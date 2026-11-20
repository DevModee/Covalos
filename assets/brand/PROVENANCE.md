# COVALOS Brand Assets (Provisional)

## Status

The current mark is **provisional**, based on the refined **Bond Prism** direction (*Precision Facet C*: superellipse C with a diagonal velocity channel and an understated central facet in Nocturne violet and cool blue).

It is intentionally centralized so the final owner-approved isotype can replace every surface in one step.

## Files

| File | Purpose |
| --- | --- |
| `covalos-mark.svg` | Master vector source (color) |
| `covalos-mark-mono-light.svg` | Flat white monochrome vector (`#FFFFFF`) |
| `covalos-mark-mono-dark.svg` | Flat dark monochrome vector (`#0B0D13`) |
| `covalos-mark-256.png` | Embedded 256×256 RGBA8 mark (About, Welcome, branding) |
| `covalos-mark-mono-64.png` | Embedded 64×64 white RGBA8 mask (tinted UI chrome) |
| `covalos-app-256.png` | Embedded 256×256 Nocturne squircle icon (window / taskbar / docs) |
| `covalos-icon.ico` | Multi-resolution Windows executable icon (16, 24, 32, 48, 64, 256 px) |

## Replacing with the final approved isotype

1. Replace `assets/brand/covalos-mark.svg` with the approved 256×256 SVG.
2. Run `python tools/generate_brand_assets.py`.
3. Rebuild `covalos.exe`; `src/brand.rs`, `src/editor/assets.rs`, and `build.rs` pick up all updated assets automatically.
