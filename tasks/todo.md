# Todo

Live working state. This is not a handoff document — a handoff is a point-in-time
export of a conversation; this file is the current truth and gets edited in place.
Work items live in GitHub issues; this file holds only what is too small for one.

## In flight

- [ ] #14 panel — draft PR, built and hosted-tested; waits on Devon's layout call (two-pane / tabs / single column).

## Next

- [ ] #15 acceptance graphs — after #14 merges.
- [ ] #16 pack, install, verify, tag `hmod-v0.1.0` — after #15, and after the non-commercial-models bundling call.

## Blocked

- [ ] `ci.yml` (#6) — blocked by: the SDK-in-CI decision (`decisions.md`). When Actions is enabled for it, disable `stale.yml`, `main.yml`, `build-windows.yml` in the same sitting.
- [ ] Bundling the three CC BY-NC-SA models in the `.hmod` — blocked by: Devon's call. `sync-engine.ps1 -NoNonCommercial` already drops them.

## Done (recent)

- [x] #28 folder progress counts files; #30 inputs take `module.json` defaults; #32 cancel reported as cancel — 2026-10-08
- [x] Tools, assets, jobs, perf baseline (#9–#13, #7) — 2026-10-08
- [x] Upstream remote added (push disabled), `hmod/main` created — 2026-10-08
