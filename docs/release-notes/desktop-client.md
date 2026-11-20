---
type: feature
---

- `lore-desktop`: Added a graphical desktop client for Windows, macOS and Linux, built from this
  repository on the `lore` library. It opens repository instances in tabs it remembers, and covers
  the daily loop: status with scan, stage, unstage, revert and commit; sync and push with progress;
  history with each revision's files and side-by-side diffs; branches with create, switch and
  merge, resolving conflicts by keeping mine or theirs, or aborting; and file locks. It refreshes
  when someone pushes, branches or locks, or when its window gains focus where the remote sends no
  notifications.
