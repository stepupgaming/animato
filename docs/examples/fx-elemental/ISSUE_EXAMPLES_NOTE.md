User request (in progress): put a short screen-record style example on GitHub issue #1
for the Elemental sandbox abilities (Frost Lance Q, Storm Lance E, Cinder Fall R).
Embed the `.gif` (or link the `.mp4`) from this folder in the issue/PR body.

Provenance: clips are rendered from `*_frames.json`, dumped by the
`dump_frost_lance` / `dump_storm_lance` / `dump_cinder_fall` cargo examples
driving the real `animato-fx-elemental` pipeline; re-run
`python3 render_fx_elemental.py --ability all --regen` to refresh.
