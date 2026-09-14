# Clean-room policy

This project reimplements an interoperable client for hardware the user owns. To keep it
safe to publish:

- **Specs are the boundary.** Reverse-engineering output goes into `docs/re/` only as
  facts: byte layouts, command IDs, field ranges, message sequences, error codes.
- **No copied code.** Never paste decompiled code, pseudocode, or disassembly into the
  repo, even in comments. Implement from the spec docs.
- **No Roland assets.** No logos, images, fonts, skins, or text lifted from the official
  app. UI strings are written fresh.
- **No binaries.** Official app binaries, firmware images, and factory sample content are
  never committed.
- **Captures stay local.** Raw `.pcapng` files may contain your samples. `captures/` is
  git-ignored; commit only trimmed, annotated excerpts inside the spec docs.
