# Red Bear OS Script Behavior Matrix

## Purpose

This document centralizes what the main repository scripts do and do not handle under the Red Bear
overlay model.

The goal is to remove guesswork from the sync/fetch/apply/build workflow.

## Matrix

| Script | Primary role | What it handles | What it does **not** guarantee |
|---|---|---|---|
| `local/scripts/sync-upstream.sh` | Refresh top-level upstream repo state | fetches upstream, reports conflict risk, rebases repo commits, reapplies build-system overlays via `apply-patches.sh` | does not automatically solve every subsystem overlay conflict; does not by itself make upstream WIP recipes safe shipping inputs |
| `local/scripts/apply-patches.sh` | Reapply durable Red Bear overlays | applies build-system patches, relinks recipe patch symlinks, relinks local recipe overlays into `recipes/` | does not fully rebase stale patch carriers; does not validate runtime behavior; does not decide WIP ownership for you |
| `local/scripts/build-redbear.sh` | Build Red Bear profiles from upstream base + local overlay | applies overlays, builds cookbook if needed, validates profile naming, launches the actual image build | does not guarantee every nested upstream source tree is fresh; does not replace explicit subsystem/runtime validation |
| `scripts/fetch-all-sources.sh` | Fetch recipe source inputs for builds | downloads recipe sources for upstream and local recipes, reports status/preflight, supports config-scoped fetches | does not mean fetched upstream WIP source is the durable shipping source of truth |
| `local/scripts/fetch-sources.sh` | Fetch local overlay source inputs | fetches local overlay recipe sources and keeps the local side ready for build work | does not decide whether upstream should replace the local overlay |

## Policy Mapping

### Upstream sync

Use `local/scripts/sync-upstream.sh` when the goal is to refresh the top-level upstream Redox base.

This is a repository sync operation, not a guarantee that every local subsystem overlay is already
rebased cleanly.

### Overlay reapplication

Use `local/scripts/apply-patches.sh` when the goal is to reconstruct Red Bear’s overlay on top of a
fresh upstream tree.

This is the core durable-state recovery path.

### Build execution

Use `local/scripts/build-redbear.sh` when the goal is to build a tracked Red Bear profile from the
current upstream base plus local overlay.

### Source refresh

Use `scripts/fetch-all-sources.sh` and `local/scripts/fetch-sources.sh` when the goal is to refresh
recipe source inputs, but do not confuse fetched upstream WIP source with a trusted shipping source.

## WIP Rule in Script Terms

If a subsystem is still upstream WIP, the scripts should be interpreted this way:

- fetching upstream WIP source is allowed and useful,
- syncing upstream WIP source is allowed and useful,
- but shipping decisions should still prefer the local overlay until upstream promotion and reevaluation happen.

That means “script fetched it successfully” is not the same as “Red Bear should now ship upstream’s
WIP version directly.”
