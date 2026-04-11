import type { OnboardingState, OnboardingAction } from '../useOnboardingState';
import { DEFAULT_TAG_CATEGORIES } from '../useOnboardingState';

interface TagCategoriesStepProps {
  state: OnboardingState;
  dispatch: React.Dispatch<OnboardingAction>;
}

export function TagCategoriesStep({ state, dispatch }: TagCategoriesStepProps) {
  // If auto-tagging is disabled, this step is a no-op informational screen.
  if (!state.autoTaggingEnabled) {
    return (
      <div className="space-y-4">
        <div>
          <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">Tag categories</h2>
          <p className="text-sm text-[var(--color-text-secondary)] mt-1">
            Auto-tagging is turned off, so there's nothing to configure here. You can come back to <strong>Settings → Tag Categories</strong> any time.
          </p>
        </div>
      </div>
    );
  }

  const isSelected = (name: string) => state.selectedDefaultCategories.includes(name);

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">Choose your tag categories</h2>
        <p className="text-sm text-[var(--color-text-secondary)] mt-1">
          The AI auto-tagger creates new sub-tags under categories you choose. Pick the ones that fit your knowledge base — you can change these later in Settings.
        </p>
      </div>

      <div className="space-y-2">
        <div className="text-xs font-medium uppercase tracking-wide text-[var(--color-text-tertiary)]">
          Default categories
        </div>
        <div className="space-y-1">
          {DEFAULT_TAG_CATEGORIES.map(name => (
            <label
              key={name}
              className="flex items-center gap-3 px-3 py-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-main)] cursor-pointer hover:bg-[var(--color-bg-hover)]"
            >
              <input
                type="checkbox"
                checked={isSelected(name)}
                onChange={() => dispatch({ type: 'TOGGLE_DEFAULT_CATEGORY', name })}
                className="accent-[var(--color-accent)]"
              />
              <span className="text-sm text-[var(--color-text-primary)]">{name}</span>
            </label>
          ))}
        </div>
      </div>

      <div className="space-y-2">
        <div className="text-xs font-medium uppercase tracking-wide text-[var(--color-text-tertiary)]">
          Custom categories
        </div>
        {state.customCategories.length > 0 && (
          <div className="flex flex-wrap gap-2">
            {state.customCategories.map(name => (
              <span
                key={name}
                className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-[var(--color-accent)]/15 text-xs text-[var(--color-text-primary)] border border-[var(--color-accent)]/40"
              >
                {name}
                <button
                  type="button"
                  onClick={() => dispatch({ type: 'REMOVE_CUSTOM_CATEGORY', name })}
                  className="text-[var(--color-text-tertiary)] hover:text-[var(--color-text-primary)]"
                  aria-label={`Remove ${name}`}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}
        <div className="flex gap-2">
          <input
            type="text"
            value={state.customCategoryInput}
            onChange={e => dispatch({ type: 'SET_CUSTOM_CATEGORY_INPUT', value: e.target.value })}
            onKeyDown={e => {
              if (e.key === 'Enter') {
                e.preventDefault();
                dispatch({ type: 'ADD_CUSTOM_CATEGORY' });
              }
            }}
            placeholder="e.g., Methodologies, Projects, Books"
            className="flex-1 bg-[var(--color-bg-main)] border border-[var(--color-border)] rounded px-3 py-1.5 text-sm text-[var(--color-text-primary)] outline-none focus:border-[var(--color-accent)]"
          />
          <button
            type="button"
            onClick={() => dispatch({ type: 'ADD_CUSTOM_CATEGORY' })}
            disabled={!state.customCategoryInput.trim()}
            className="px-3 py-1.5 text-sm rounded bg-[var(--color-accent)] text-white hover:bg-[var(--color-accent-hover)] disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Add
          </button>
        </div>
      </div>

      {state.categoriesError && (
        <div className="text-xs text-red-400">{state.categoriesError}</div>
      )}

      {state.selectedDefaultCategories.length === 0 && state.customCategories.length === 0 && (
        <div className="rounded-lg border border-yellow-500/40 bg-yellow-500/10 px-3 py-2 text-xs text-yellow-200">
          No categories selected. Auto-tagging will be skipped until you add at least one in Settings.
        </div>
      )}
    </div>
  );
}
