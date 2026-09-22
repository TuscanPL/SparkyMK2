# Faceplate app icon

The icon reduces the SP-404MKII upper panel to four knobs, a round display,
and six effect buttons. All exports have transparent backgrounds and use the
same design as the in-app toolbar image at `../../src/assets/sparkymk2.png`.

## Editable sources

- `source/faceplate.svg`: 512-unit master for exports of 32 pixels and larger.
- `source/faceplate-16.svg`: optical drawing for the 16-pixel window icon.
- `source/faceplate-24.svg`: optical drawing for 20- and 24-pixel icons.

The small drawings simplify the waveform and align controls to their pixel
grids. Preserve these versions when exporting; reducing the master to 16 pixels
loses their spacing adjustments. The SVGs are self-contained and need no fonts.

## Export sizes and consumers

| Asset | Size or representations | Used by |
| --- | --- | --- |
| `icon.png` | 512 × 512 | Tauri and Linux launchers |
| `32x32.png`, `64x64.png`, `128x128.png` | Named dimensions | Tauri and Linux launchers |
| `128x128@2x.png` | 256 × 256 | Arch/AUR Linux launcher packages |
| `icon.ico` | 16, 20, 24, 32, 40, 48, 64, 96, 128, 256 | Windows executable, window, taskbar and installers |
| `icon.icns` | 128, 256, 512; Retina 16, 32, 128, 256, 512 | macOS app bundle (up to 1024 pixels) |
| `Square*Logo.png` | Named dimensions | Windows tile assets |
| `StoreLogo.png` | 50 × 50 | Windows store asset |
| `../../src/assets/sparkymk2.png` | 512 × 512 | In-app toolbar |

Rasterize the appropriate SVG directly at each target size with transparency.
For the ICO, retain every listed size as a separate RGBA frame, including the
dedicated small drawings. The ICNS uses PNG representations. Keep the toolbar
image identical to `icon.png` when updating the artwork.

See [ATTRIBUTION.txt](ATTRIBUTION.txt) for provenance and licensing.
