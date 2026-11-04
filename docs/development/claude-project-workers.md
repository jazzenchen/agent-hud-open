# Claude Desktop Project workers

On macOS, a Project worker card returns to the original Project conversation
instead of exposing its internal Local execution transcript. This integration
reads existing local transcript files and Desktop session metadata only. It
adds no cookie access, provider requests, polling job, or Cloud status source.

## Identity and navigation

- Active Desktop metadata must identify a Project child with a valid CCR binding.
- The transcript must contain a narrowly marked managed SDK wake with one exact
  Project/thread origin. Ordinary prompt text cannot establish that origin.
- Parsing is bounded; malformed or conflicting origins disable navigation.
- Workers with the same confirmed origin share one conversation card. Their
  existing usage remains in the ledger and in the conversation's worker details.
- The native adapter sends a validated Project/thread URL directly to Claude
  Desktop, preserving the thread query. A failed Project open never falls back
  to the internal Local transcript or log file.

Standalone Local sessions keep their existing resolver. Windows and Linux keep
their existing Local projection until their native Project route is supported.

## Presentation and compatibility

Local worker files cannot establish the original conversation's current status,
title, or total usage. The card therefore uses a generic Project label, shows
unknown status and unavailable totals, and lists local execution usage separately.
The managed envelope and Desktop metadata are undocumented compatibility points.

Tests cover provenance spoofing, ambiguity, bounds, worker grouping, checkpoint
reparsing without duplicate usage, unsupported-host Local behavior, and validated
native targets. Native acceptance should additionally verify an actual HUD click
against the original thread and regress standalone Local navigation. Test only
disposable sessions or navigate existing sessions read-only; do not commit their
titles, identifiers, transcripts, or screenshots.
