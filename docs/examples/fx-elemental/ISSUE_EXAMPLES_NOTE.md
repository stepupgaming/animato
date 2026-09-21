User request (in progress): put a short screen-record style example on GitHub issue #1
for the Elemental sandbox first ability (Frost Lance Q: aim → fracture front →
spike eruption → impact punch → withdrawal). Embed `frost_lance.gif` (or link
`frost_lance.mp4`) from this folder in the issue/PR body.

Provenance: the clip is rendered from `frost_lance_frames.json`, which is dumped
by the `dump_frost_lance` cargo example driving the real `animato-fx-elemental`
pipeline (`cargo run -p animato-fx-elemental --example dump_frost_lance`); re-run
`render_fx_elemental.py --regen` to refresh both.
