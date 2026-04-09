/** Bridge between AtomReaderContent (owns the editor) and MainView (renders titlebar buttons).
 *  AtomReaderContent populates this ref; MainView reads it to dispatch actions. */
export interface ReaderEditorActions {
  startEditing: (offset?: number) => void;
  stopEditing: () => void;
  undo: () => void;
  redo: () => void;
}

export const readerEditorActions: { current: ReaderEditorActions | null } = { current: null };
