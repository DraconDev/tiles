# Tiles QA Matrix

Status legend: `PASS` | `FAIL` | `FIXED`

## Environment
- Date: 2026-02-08
- Build target: local dev
- Focus areas:
  - Git page UI bleed into Files page
  - Editor second pane/page behavior
  - Symlink action flow

## Baseline Matrix

| ID | Flow | Mode | Steps | Expected | Baseline |
|---|---|---|---|---|---|
| G1 | Git -> Files transition | Single pane | Open Git view, navigate list, return to Files | Files UI has only Files elements, no Git artifacts | FAIL (user-reported) |
| G2 | Git -> Files transition | Split pane | Enter Git, switch back, switch pane focus | Both panes show correct Files UI and state | FAIL (suspected) |
| E1 | Open Editor from Files | Single pane | Select file, open editor, return | Works consistently | PASS (initial) |
| E2 | Open Editor from Files | Split pane | Open file in pane 2 / second page path | Second pane/page interactive and renders correctly | FAIL (user-reported) |
| E3 | Editor pane focus swap | Split pane | Switch pane focus while in Editor and edit | Input applies to focused editor pane | FAIL (suspected) |
| S1 | Drag-drop Link action | Single pane | Drag item to folder, choose Link | Symlink created at destination | FAIL (known unhandled event) |
| S2 | Drag-drop Link action | Split pane | Same as S1 with opposite pane target | Symlink created and pane refreshes | FAIL (known unhandled event) |
| R1 | Copy action refresh | Split pane | Copy from pane 2, watch destination pane | Correct pane refreshes | FAIL (known hardcoded pane refresh) |
| A1 | Mouse move/drag stability | Any | Move/drag over file table extensively | No panic or overflow | FAIL (panic in debug.log) |

## Work Log

- [ ] Patch Git->Files bleed-over
- [ ] Patch Editor second page behavior
- [ ] Implement Symlink event handling
- [ ] Patch arithmetic/refresh safety issues
- [ ] Re-run this matrix and convert statuses to `FIXED`/`PASS`/`FAIL`

## Final Results

To be filled after fixes and validation.
