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

- [x] Patch Git->Files bleed-over
- [x] Patch Editor second page behavior
- [x] Implement Symlink event handling
- [x] Patch arithmetic/refresh safety issues
- [x] Re-run build/tests and update matrix

## Final Results

Code-level verification completed on 2026-02-08:

| ID | Final | Notes |
|---|---|---|
| G1 | FIXED | Added Git->Files transition cleanup (mode/input reset, Git selection reset, git:// preview cleanup). |
| G2 | FIXED | Same transition cleanup applies in split mode; refresh path retained. |
| E1 | PASS | Existing flow unchanged; compiles and routes as expected. |
| E2 | FIXED | Removed forced editor split-collapse and aligned editor pane geometry with renderer. |
| E3 | FIXED | Editor mouse/area targeting now uses shared pane-area calculation; focus routing stabilized. |
| S1 | FIXED | `AppEvent::Symlink` now executed with status feedback and refresh. |
| S2 | FIXED | Symlink handling refreshes panes whose `current_path` matches destination parent. |
| R1 | FIXED | Copy now refreshes destination-matching panes instead of hardcoded pane `0`. |
| A1 | FIXED | Added arithmetic guards (`saturating_add`, pane-width guards, offset underflow safety). |

Manual interactive validation is still recommended for UI feel/regression checks in a real terminal session.
