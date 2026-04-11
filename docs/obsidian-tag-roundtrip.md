# Obsidian Tag Round-Trip

How to reconcile Obsidian's freeform tag system with Atomic's hierarchical, AI-curated one — and eventually flow auto-extracted tags back into Obsidian vaults.

## The tension

Atomic constrains AI-extracted tags to live under a small set of seeded top-level categories (Topics, People, Locations, Organizations, Events). The auto-tagger only ever creates sub-tags under these parents, which keeps the tag tree coherent.

Obsidian vaults, by contrast, are a free-for-all: tags come from folder structure, freeform `#hashtags`, and YAML frontmatter, with no enforced hierarchy. Two problems follow:

1. If we import an Obsidian vault wholesale (treating folder names as top-level tags), those imported tags become candidates for the AI to create sub-tags under — even though they may not make sense as auto-tag targets.
2. The AI-extracted tags Atomic generates never make it back to the user's Obsidian vault, where they could provide enormous value cleaning up unorganized "tag graveyards."

This doc plans a path through both.

## Part 1 — Constraining auto-tag targets

> **Detailed plan:** see [`auto-tag-targets.md`](./auto-tag-targets.md) for the concrete schema, migration, onboarding step, and settings UI design. The summary below is the conceptual sketch.

### The flag

Add `is_autotag_target: bool` to the `tags` table. The five seeded categories get it set on creation; everything else (including imported tags from Obsidian folders) defaults to `false`.

The auto-tagger in `atomic-core` filters its candidate-parent list by this flag before constructing the LLM prompt. Imported folder tags coexist with the seeded categories in the tag tree, but the AI only ever extends the flagged ones.

### Granularity

The flag should apply to *any* tag, not just top-level ones. Use cases:

- Mark `Topics/Programming` as a target so the AI creates `Topics/Programming/Rust` rather than dumping language names directly under `Topics/`.
- Unmark `Topics/` itself so the AI is forced to be more specific.

This is a one-line cost in the filter logic and gives users far more leverage over the tag tree's shape.

### Edge cases

- **Zero flagged tags:** Auto-tagging becomes a no-op. Legitimate, but the settings UI should explain why nothing is happening.
- **User unflags a category mid-stream:** Existing sub-tags stay; only new auto-tag runs are affected. No retroactive cleanup.

### UI surface

- Checkbox in the tag context menu / tag editor.
- Filtered view in tag-management settings: "Tags the AI can extend."
- When importing an Obsidian vault, show a one-time prompt: "Treat folder names as auto-tag targets?" Default no.

## Part 2 — Surfacing Atomic tags inside Obsidian

Auto-writing files in the user's vault is perilous. Instead, ship this in phases that build trust before any background mutation.

### Phase 1: Read-only display

Sidebar view in the Obsidian plugin: "Atomic suggests these tags" for the active note. Each suggested tag is a chip with a one-click "Apply to frontmatter" action. No background writes; nothing happens unless the user clicks.

This is a sidebar view, not a sync subsystem. It reuses the existing `getAtomBySourceUrl` lookup the plugin already does, plus a new endpoint to fetch the atom's tags. Implementation cost is small.

The value: users see AI-suggested tags in the UI they already trust, on the note they're already looking at. No fear of files being rewritten behind their back.

### Phase 2: Opt-in batch apply

Command: "Apply Atomic tags to all notes." Generates a preview diff (file-by-file: tags to add, tags already present) and waits for user confirmation before writing anything.

Writing rules:
- Touch only the `tags` field of YAML frontmatter. Preserve every other field, including ordering and comments where Obsidian's API permits.
- Use Obsidian's nested tag convention: `Topics/AI/LLMs` → `topic/ai/llms`. Lowercase by default; configurable.
- Never remove existing user tags. Only additive.
- After write, the content hash (computed post-frontmatter strip) doesn't change, so the plugin won't trigger a re-sync of the same note. No loop.

### Phase 3: Background auto-write

**Off by default.** Setting toggle with a warning notice on enable: "Atomic will modify YAML frontmatter in your vault."

Even with this enabled, writes should be additive only — never silently remove tags the user has stripped out, since removal is a meaningful signal we shouldn't fight.

This phase requires solving the conflict-resolution problems below, which is why it comes last.

## Conflict resolution (deferred to Phase 3)

Hard questions that don't need answers until Phase 3 ships:

- **User removes a tag in Obsidian.** Re-add on next pull, or treat removal as authoritative? Probably need a per-file shadow state tracking "tags Atomic wrote" separately from "tags that exist now," so removals can be detected and respected.
- **User adds a tag in Obsidian that doesn't exist in Atomic.** Import back? As what type? Probably attach to the atom but don't auto-create a new top-level category.
- **AI re-runs auto-tagging and changes its mind.** Silently rewrite? Probably not — once written, an applied tag should stick unless the user explicitly removes it.

## The bigger unlock: tag unification

The juiciest version of this work: detect when an existing Obsidian tag (`#productivity`) overlaps with an AI-extracted tag (`Topics/Productivity`) and unify them — both in Atomic's tag tree and on the Obsidian side. That turns Atomic into a tag-graveyard cleaner, which is a much sharper value prop than "we add more tags."

This depends on Part 1 being solid first. The user needs control over which parents the AI uses before they'll trust it to merge their existing tags into Atomic's hierarchy.

Mechanically, unification needs:
- A similarity check (string + embedding) between Obsidian tag names and Atomic tag paths.
- A user-facing review UI: "These look like the same thing. Merge?"
- A merge operation in `atomic-core` that re-points all `atom_tags` rows from one tag to another and deletes the loser.

## Implementation order

1. Add `is_autotag_target` column + filter in auto-tagger. Seed the five default categories with it set. Add UI to toggle it.
2. Phase 1 sidebar view in the Obsidian plugin.
3. Phase 2 batch-apply command with preview diff.
4. Tag unification UI (depends on 1).
5. Phase 3 background auto-write — only if Phases 1–2 prove the trust model works.
