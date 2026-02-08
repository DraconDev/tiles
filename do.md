## Do

- editor should show file with its path instead of PROJECT

- round the corners in editor view too
- add a small status line in editor split mode showing active pane index/name
- add an optional setting for max preview file size (default 20MB)
- add minimal integration tests for view transitions (Files <-> Git <-> Editor)

## Maybe

- clean up high-noise warnings from `cargo clippy`
- add a debug toggle to show pane routing (`focused_pane_index`, target pane) in UI
- improve sidebar tree perf with lazy expansion cache for large projects

## Done

- `Esc` now exits Git/Monitor/Editor views reliably
- fixed editor split pane targeting when opening from project sidebar
- implemented `Symlink` action handling in app event loop
- copy refresh now targets destination-visible panes (not hardcoded pane 0)
- improved mouse/index arithmetic safety in file manager paths
- increased text preview size limit from 5MB to 20MB
- added QA checklist matrix: `docs/qa/matrix.md`
