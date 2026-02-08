## Do

- | G2 | Git -> Files transition | Split pane | Enter Git, switch back, switch pane focus | Both panes show correct Files UI and state | FAIL (suspected) |
  - the infor section is quite artifacty, some letters are teal more are white,
  - also we see random tch) and push)
- history section 

## Maybe

- clean up high-noise warnings from `cargo clippy`
- add a debug toggle to show pane routing (`focused_pane_index`, target pane) in UI
- improve sidebar tree perf with lazy expansion cache for large projects

## Done

- added optional setting for max preview file size (default `20MB`) with persistence
- added minimal view-transition regression tests (Git escape + editor pane targeting)
- `Esc` now exits Git/Monitor/Editor views reliably
- fixed editor split pane targeting when opening from project sidebar
- implemented `Symlink` action handling in app event loop
- copy refresh now targets destination-visible panes (not hardcoded pane 0)
- improved mouse/index arithmetic safety in file manager paths
- increased text preview size limit from 5MB to 20MB
- added QA checklist matrix: `docs/qa/matrix.md`
- editor sidebar now shows current path in title (instead of `PROJECT`)
- editor split panes now render with rounded borders
- editor split panes now show active pane status/title (`P1/P2 ACTIVE`)
