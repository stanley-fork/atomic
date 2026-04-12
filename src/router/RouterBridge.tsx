import { useEffect, useRef } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { useUIStore, type OverlayNav, type OverlayNavEntry } from '../stores/ui';
import { parseLocation, type ParsedRoute } from './routes';
import { setNavigateFn, type NavigateState } from './navigate-ref';

/// Glue between react-router-dom (URL) and Zustand (UI store).
///
/// Responsibilities:
///   1. Expose the live `navigate` function to non-React code via
///      `setNavigateFn` so store actions can write URLs.
///   2. Reconcile the store to the URL on every location change. URL is the
///      source of truth for routed state (viewMode, selectedTagId,
///      readerState, wikiReaderState, localGraph). The store is the source
///      of truth for UI-only state (editing, saveStatus, panel widths, etc.)
///
/// The stack reconciliation uses a `seq` embedded in `history.state` to
/// distinguish forward navigation from back navigation (popstate). Without
/// it, every browser-back would look like "URL doesn't match top of stack"
/// and Bridge would *append* instead of decrementing the index — the stack
/// would grow without bound across back/forward cycles.

/// Does this stack entry correspond to the parsed URL?
function matchesEntry(entry: OverlayNavEntry, parsed: ParsedRoute): boolean {
  if (parsed.kind === 'reader') {
    return entry.type === 'reader' && entry.atomId === parsed.atomId;
  }
  if (parsed.kind === 'graph') {
    return entry.type === 'graph' && entry.atomId === parsed.atomId;
  }
  if (parsed.kind === 'wiki-reader') {
    return entry.type === 'wiki' && entry.tagId === parsed.tagId;
  }
  return false;
}

/// Decide how to transform `overlayNav` given the URL we just arrived at
/// and the direction we moved in. Returns the *same* reference when nothing
/// needs to change, so callers can short-circuit a setState.
function reconcileOverlayNav(
  prev: OverlayNav,
  parsed: ParsedRoute,
  newEntry: OverlayNavEntry,
  direction: 'forward' | 'back' | 'unknown',
): OverlayNav {
  const current = prev.stack[prev.index];
  if (current && matchesEntry(current, parsed)) return prev; // idempotent

  if (direction === 'back') {
    // Search the whole stack: on popstate the user may jump back multiple
    // entries in one step (not just index-1). Find the matching one and
    // snap the index to it without disturbing forward entries.
    const idx = prev.stack.findIndex((e) => matchesEntry(e, parsed));
    if (idx >= 0) return { stack: prev.stack, index: idx };
  }

  if (direction === 'forward') {
    const next = prev.stack[prev.index + 1];
    if (next && matchesEntry(next, parsed)) {
      return { stack: prev.stack, index: prev.index + 1 };
    }
  }

  // Fresh navigation (forward push, cold-load, or directly-typed URL) —
  // truncate anything ahead of the current index and append.
  return {
    stack: [...prev.stack.slice(0, prev.index + 1), newEntry],
    index: prev.index + 1,
  };
}

/// Build the overlay-stack entry for a URL. Shape has to match what actions
/// push so `matchesEntry` correctly identifies "already there" cases.
function entryForRoute(parsed: ParsedRoute): OverlayNavEntry | null {
  if (parsed.kind === 'reader') {
    return { type: 'reader', atomId: parsed.atomId };
  }
  if (parsed.kind === 'graph') {
    return { type: 'graph', atomId: parsed.atomId };
  }
  if (parsed.kind === 'wiki-reader') {
    return { type: 'wiki', tagId: parsed.tagId, tagName: parsed.tagName ?? '' };
  }
  return null;
}

export function RouterBridge() {
  const location = useLocation();
  const navigate = useNavigate();
  const prevSeqRef = useRef<number | null>(null);
  const didInjectSeqRef = useRef(false);

  // Publish the live navigate fn to the module-scope ref so store actions
  // can use it.
  useEffect(() => {
    setNavigateFn(navigate);
  }, [navigate]);

  useEffect(() => {
    // Read the seq our own `navigateTo` writes into history.state, or inject
    // seq=0 into the cold-load entry so later back-navigations here have a
    // known seq to compare against. Without this injection, a cold-loaded
    // URL has no seq; after one forward navigation, browser-back to the
    // cold URL would look like "seq became null" → direction undetectable.
    const incomingState = (location.state ?? null) as NavigateState | null;
    let seq = incomingState?.seq ?? null;
    if (seq === null && !didInjectSeqRef.current) {
      const existing = window.history.state ?? {};
      window.history.replaceState({ ...existing, seq: 0 }, '', window.location.href);
      seq = 0;
    }
    didInjectSeqRef.current = true;

    const prevSeq = prevSeqRef.current;
    const direction: 'forward' | 'back' | 'unknown' =
      prevSeq === null || seq === null
        ? 'unknown'
        : seq > prevSeq
        ? 'forward'
        : seq < prevSeq
        ? 'back'
        : 'unknown'; // same seq = replace navigation; don't mutate stack
    prevSeqRef.current = seq;

    const parsed = parseLocation(location.pathname, location.search);
    const store = useUIStore.getState();

    if (parsed.kind === 'view') {
      const needsClear =
        store.readerState.atomId !== null ||
        store.wikiReaderState.tagId !== null ||
        store.overlayNav.stack.length > 0;

      if (needsClear || store.viewMode !== parsed.viewMode || store.selectedTagId !== parsed.tagId) {
        useUIStore.setState({
          viewMode: parsed.viewMode,
          selectedTagId: parsed.tagId,
          readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' },
          wikiReaderState: { tagId: null, tagName: null },
          overlayNav: { stack: [], index: -1 },
          localGraph: { ...store.localGraph, isOpen: false },
          ...(store.leftPanelOpenBeforeReader
            ? { leftPanelOpen: true, leftPanelOpenBeforeReader: false }
            : {}),
        });
      }
      return;
    }

    // Overlay-kind (reader / graph / wiki-reader) share the same stack
    // reconciliation + left-panel auto-collapse.
    const newEntry = entryForRoute(parsed);
    if (!newEntry) return;
    const nextNav = reconcileOverlayNav(store.overlayNav, parsed, newEntry, direction);
    const becameFirstOverlay = store.overlayNav.index === -1 && nextNav.index !== -1;

    if (parsed.kind === 'reader') {
      const sameAtom = store.readerState.atomId === parsed.atomId;
      useUIStore.setState({
        selectedTagId: parsed.tagId,
        readerState: {
          atomId: parsed.atomId,
          highlightText: sameAtom ? store.readerState.highlightText : null,
          editing: sameAtom ? store.readerState.editing : false,
          saveStatus: sameAtom ? store.readerState.saveStatus : 'idle',
        },
        wikiReaderState: { tagId: null, tagName: null },
        // MainView gives `localGraph.isOpen` priority over reader in its
        // dispatch — close it here or chevron-back from graph leaves the
        // graph visible behind the "new" reader URL.
        localGraph: { ...store.localGraph, isOpen: false },
        overlayNav: nextNav,
        ...(becameFirstOverlay && store.leftPanelOpen
          ? { leftPanelOpen: false, leftPanelOpenBeforeReader: true }
          : {}),
      });
    } else if (parsed.kind === 'graph') {
      useUIStore.setState({
        selectedTagId: parsed.tagId,
        readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' },
        wikiReaderState: { tagId: null, tagName: null },
        localGraph: {
          isOpen: true,
          centerAtomId: parsed.atomId,
          depth: store.localGraph.depth,
          navigationHistory: [parsed.atomId],
        },
        overlayNav: nextNav,
        ...(becameFirstOverlay && store.leftPanelOpen
          ? { leftPanelOpen: false, leftPanelOpenBeforeReader: true }
          : {}),
      });
    } else if (parsed.kind === 'wiki-reader') {
      useUIStore.setState({
        wikiReaderState: {
          tagId: parsed.tagId,
          tagName: parsed.tagName ?? store.wikiReaderState.tagName,
        },
        readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' },
        localGraph: { ...store.localGraph, isOpen: false },
        overlayNav: nextNav,
        ...(becameFirstOverlay && store.leftPanelOpen
          ? { leftPanelOpen: false, leftPanelOpenBeforeReader: true }
          : {}),
      });
    }
  }, [location.pathname, location.search, location.state]);

  return null;
}
