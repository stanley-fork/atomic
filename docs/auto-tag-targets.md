# Auto-Tag Targets

A user-controllable mechanism for which top-level tags the AI is allowed to extend during auto-tagging. This is the foundation for the broader Obsidian tag round-trip work (see `obsidian-tag-roundtrip.md`), but it stands alone as an Atomic feature.

## Motivation

Today, the auto-tagger creates sub-tags under five hardcoded categories: Topics, People, Locations, Organizations, Events. These are seeded once on first DB init (`crates/atomic-core/src/lib.rs:213-226`) and the LLM prompt in `crates/atomic-core/src/extraction.rs` instructs the model to use only these as `parent_name`.

Two limitations:

1. Users can't opt out of categories they don't care about (e.g., a vault that's purely technical research has no use for People/Locations/Events).
2. Users can't add their own (e.g., "Methodologies", "Projects", "Books") and have the AI extend them.

The fix is a single boolean column on `tags` plus UI in onboarding and settings to manage it.

## Schema change

Add `is_autotag_target` to the `tags` table in `crates/atomic-core/src/db.rs`. The existing schema (line 221):

```sql
CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE,
    parent_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    UNIQUE(name COLLATE NOCASE)
);
```

Becomes:

```sql
CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE,
    parent_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    is_autotag_target INTEGER NOT NULL DEFAULT 0,
    UNIQUE(name COLLATE NOCASE)
);
```

For existing databases, add a new migration block (next free version is **V11** — V10 is the highest currently in `db.rs`):

```rust
if version < 11 {
    conn.execute_batch(
        "ALTER TABLE tags ADD COLUMN is_autotag_target INTEGER NOT NULL DEFAULT 0;
         UPDATE tags SET is_autotag_target = 1
           WHERE parent_id IS NULL
             AND name IN ('Topics', 'People', 'Locations', 'Organizations', 'Events');
         PRAGMA user_version = 11;",
    )?;
}
```

The seed loop in `lib.rs:213-226` should also be updated to insert with `is_autotag_target = 1` for new databases, so the seed and the migration agree.

## Auto-tagger filter

`crates/atomic-core/src/extraction.rs` builds a tag tree to feed the LLM via `get_tag_tree_for_llm()` (around line 355–380 per the explore). The change is one filter:

```rust
// Before: SELECT id, name FROM tags WHERE parent_id IS NULL
// After:  SELECT id, name FROM tags WHERE parent_id IS NULL AND is_autotag_target = 1
```

The system prompt at lines 105–134 already lists categories generically — it should keep working unchanged as long as the filtered list is non-empty. Two things to add:

1. **Empty-list guard.** If no tags are flagged, skip auto-tagging entirely and log a debug message. Don't send a degenerate prompt.
2. **Prompt wording.** The current prompt may mention the five defaults by name as examples. Re-read it and remove any hardcoded category names so it adapts to whatever the user has flagged.

**Scope decision: top-level only for the first pass.** A flag on a mid-tree tag (e.g., `Topics/Programming` as a target so the AI creates `Topics/Programming/Rust`) is a natural extension but expands the prompt-construction logic non-trivially. Defer to a follow-up. Document the limitation in the settings UI.

## Tag CRUD changes

`Tag` struct in `crates/atomic-core/src/models.rs` gains `is_autotag_target: bool`. All read paths (`list_tags`, `get_tag`, etc.) populate it. The tag CRUD module accepts it on create (defaulting to `false`) and exposes either:

- A focused method `set_tag_autotag_target(id: &str, value: bool)`, or
- An expanded `update_tag` that accepts an optional flag

I'd lean toward the focused method — it makes the audit trail clearer and doesn't entangle the flag with name/parent updates.

REST surface in `crates/atomic-server/src/routes/tags.rs`:

- `PATCH /api/tags/:id/autotag-target` with `{ "value": true|false }`, or
- Fold into existing `PATCH /api/tags/:id`

The frontend transport command map gains a corresponding `set_tag_autotag_target` command.

## Onboarding step

The Atomic onboarding wizard is at `src/components/onboarding/OnboardingWizard.tsx`, with steps defined in `src/components/onboarding/useOnboardingState.ts:261-266`:

```typescript
export const STEPS: { id: StepId; label: string; required: boolean }[] = [
  { id: 'welcome', label: 'Welcome', required: true },
  { id: 'ai-provider', label: 'AI Provider', required: true },
  { id: 'integrations', label: 'Integrations', required: false },
  { id: 'tutorial', label: 'Tutorial', required: false },
];
```

Insert a new step `tag-categories` after `ai-provider`:

```typescript
{ id: 'ai-provider', label: 'AI Provider', required: true },
{ id: 'tag-categories', label: 'Tag Categories', required: false },
{ id: 'integrations', label: 'Integrations', required: false },
```

**Conditional skip.** The AI Provider step saves `auto_tagging_enabled`. If the user disabled auto-tagging there, skip the tag categories step entirely (no point picking targets if the AI won't run).

**New file:** `src/components/onboarding/steps/TagCategoriesStep.tsx`. UI:

- Heading: "Choose your tag categories"
- Description: "Atomic auto-tags your notes by creating sub-tags under top-level categories you choose. Pick the ones that fit your knowledge base — you can change these later in Settings."
- Five default checkboxes (Topics, People, Locations, Organizations, Events), all checked by default
- "Add custom category" text input + "Add" button
- Custom categories appear as removable chips below the input
- Footer: "Back" / "Continue" buttons

**State shape:**

```typescript
interface TagCategoriesState {
  selectedDefaults: Set<string>;  // starts with all 5
  customCategories: string[];
}
```

**On step complete**, call a new backend command `configure_autotag_targets({ keep_defaults: string[], add_custom: string[] })` that:

1. For each name in `keep_defaults`: create the tag if it doesn't exist, then ensure `is_autotag_target = 1`.
2. For each well-known default NOT in `keep_defaults`: if the tag exists with no atoms and no children, **delete it** (the safe case during first-run onboarding). Otherwise unflag it (so re-running the wizard from settings after the user has tagged things stays non-destructive).
3. For each name in `add_custom`: create a new top-level tag (or flag an existing one) with `is_autotag_target = 1`.

This is idempotent and safe to re-run.

**Important: defaults are no longer auto-seeded on DB creation.** A brand-new database starts with zero tags. The onboarding wizard (or the user via the settings tab / API) is solely responsible for deciding which categories exist. Headless/no-onboarding paths get a clean slate; the auto-tagger gracefully no-ops when no tags are flagged.

## Settings panel section

Atomic's settings live in `src/components/settings/SettingsModal.tsx`. Tabs are defined at lines 48–57:

```typescript
const SETTINGS_TABS: { id: SettingsTab; label: string }[] = [
  { id: 'general', label: 'General' },
  { id: 'ai', label: 'AI Models' },
  { id: 'connection', label: 'Connection' },
  { id: 'feeds', label: 'Feeds' },
  { id: 'integrations', label: 'Integrations' },
  { id: 'databases', label: 'Databases' },
];
```

Add a new tab `'tag-categories'` (or `'tags'` if we want a broader Tags tab eventually):

```typescript
{ id: 'tag-categories', label: 'Tag Categories' },
```

**Component:** `TagCategoriesTab` — either inline in `SettingsModal.tsx` (matches the existing pattern per the explore) or as a separate file if it grows. UI sections:

1. **Header / explanation.** "Tags marked as auto-tag targets are the only ones the AI will create new sub-tags under. Top-level tags only — sub-tag targets aren't supported yet."
2. **Active targets.** List of all top-level tags where `is_autotag_target = true`. Each row: name, atom count, toggle to remove the flag.
3. **Available top-level tags.** List of all top-level tags where `is_autotag_target = false` (e.g., folder-imported ones, or defaults the user previously unflagged). Each row: toggle to mark as a target.
4. **Create new target.** Text input + "Add" button. Creates a new top-level tag with the flag set.
5. **Empty-state warning.** If the active list is empty, show a yellow notice: "No auto-tag targets configured. Auto-tagging will be skipped for new atoms."

This tab overlaps slightly with the broader tag management UI (if one exists). The doc deliberately scopes this to *target management* — full tag CRUD (rename, merge, delete) belongs elsewhere.

## Edge cases & open questions

- **Existing tag management UI.** I haven't mapped the full tag-management surface yet. Before building the settings tab, check whether there's already a tag editor that should host the toggle inline rather than living in a separate tab.
- **Mid-tree targets.** Out of scope for the first pass. When added, the auto-tagger needs to walk the tag tree differently and the prompt construction needs to express "extend these specific sub-trees, but don't create new top-level tags."
- **Multi-database.** The flag is per-database (lives in the data DB, not registry.db), so each knowledge base has its own auto-tag target set. Onboarding only runs once globally — additional databases inherit nothing. Consider whether a per-database "first sync" flow should re-prompt for targets.
- **Custom category name validation.** Reject empty names, names matching existing tags (case-insensitive — the table already has `COLLATE NOCASE`), and names containing `/` (which we use as a hierarchy separator in display).
- **Telemetry.** None for now, but worth noting which categories users actually select if/when telemetry exists — informs future defaults.

## File-by-file change list

Backend:
- `crates/atomic-core/src/db.rs` — add column to `CREATE TABLE`, add V11 migration block
- `crates/atomic-core/src/lib.rs:213-226` — set `is_autotag_target = 1` in seed insert
- `crates/atomic-core/src/models.rs` — add field to `Tag` struct
- `crates/atomic-core/src/tags.rs` (or wherever tag CRUD lives) — read/write the field, add `set_tag_autotag_target` and `configure_autotag_targets`
- `crates/atomic-core/src/extraction.rs` — filter `get_tag_tree_for_llm()` by the flag, add empty-list guard, scrub hardcoded names from prompt
- `crates/atomic-core/src/lib.rs` (the `AtomicCore` facade) — expose new methods
- `crates/atomic-server/src/routes/tags.rs` — REST endpoints for the new methods
- `src-tauri/src/lib.rs` (if applicable) — Tauri command wrappers

Frontend:
- `src/components/onboarding/useOnboardingState.ts` — add `tag-categories` step + state
- `src/components/onboarding/OnboardingWizard.tsx` — render switch case for new step
- `src/components/onboarding/steps/TagCategoriesStep.tsx` — **new file**
- `src/components/settings/SettingsModal.tsx` — add tab + `TagCategoriesTab` component
- `src/transport/commandMap.ts` (or wherever commands are mapped) — add `configure_autotag_targets`, `set_tag_autotag_target`
- `src/types/` (if tag types live there) — add `is_autotag_target` field

## Implementation order

1. Schema + migration + `Tag` struct field. Verify with `cargo test` and a manual sqlite3 check on a fresh DB.
2. Auto-tagger filter + empty-list guard. Test by unflagging all defaults and confirming new atoms get no AI tags.
3. Backend tag CRUD methods + REST routes.
4. Frontend transport wiring.
5. Settings tab (lets us iterate on the UI before touching onboarding).
6. Onboarding step.
7. Manual end-to-end test: fresh DB → onboarding → uncheck Locations and Events, add "Methodologies" → create an atom about a research paper → confirm it gets a `Methodologies/...` sub-tag and no Locations sub-tag.
