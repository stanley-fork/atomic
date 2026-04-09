import { markdown } from '@codemirror/lang-markdown';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';

/**
 * "Seamless" CodeMirror theme for inline editing.
 * Matches the surrounding prose: same font, same background, no chrome.
 */
export const editorTheme = EditorView.theme({
  '&': {
    backgroundColor: 'transparent',
    fontSize: 'inherit',
    lineHeight: 'inherit',
  },
  '&.cm-focused': {
    outline: 'none',
  },
  '.cm-scroller': {
    fontFamily: 'inherit',
    lineHeight: 'inherit',
    fontSize: 'inherit',
  },
  '.cm-content': {
    caretColor: 'var(--color-accent)',
    padding: '0',
    fontFamily: 'inherit',
    lineHeight: 'inherit',
    fontSize: 'inherit',
    color: 'var(--color-text-primary)',
  },
  '.cm-line': {
    padding: '0',
  },
  '.cm-activeLine': {
    backgroundColor: 'transparent',
  },
  '.cm-gutters': {
    display: 'none',
  },
  '.cm-cursor': {
    borderLeftColor: 'var(--color-accent)',
  },
  '&.cm-focused .cm-selectionBackground, ::selection': {
    backgroundColor: 'rgba(124, 58, 237, 0.3)',
  },
  '.cm-selectionBackground': {
    backgroundColor: 'rgba(124, 58, 237, 0.15)',
  },
  '.cm-placeholder': {
    color: 'var(--color-text-tertiary)',
    fontStyle: 'italic',
  },
});

/**
 * Syntax highlighting using CSS variables from the design system.
 */
const highlightStyle = HighlightStyle.define([
  { tag: tags.heading, fontWeight: 'bold', color: 'var(--color-text-primary)' },
  { tag: tags.strong, fontWeight: 'bold', color: 'var(--color-text-primary)' },
  { tag: tags.emphasis, fontStyle: 'italic', color: 'var(--color-text-primary)' },
  { tag: tags.strikethrough, textDecoration: 'line-through', color: 'var(--color-text-secondary)' },
  { tag: tags.link, color: 'var(--color-accent-light)', textDecoration: 'underline' },
  { tag: tags.url, color: 'var(--color-accent-light)' },
  { tag: tags.monospace, fontFamily: 'var(--font-mono)', color: 'var(--color-accent-light)' },
  { tag: tags.content, color: 'var(--color-text-primary)' },
  { tag: tags.processingInstruction, color: 'var(--color-text-tertiary)' },
  { tag: tags.meta, color: 'var(--color-text-tertiary)' },
  { tag: tags.list, color: 'var(--color-text-secondary)' },
  { tag: tags.quote, color: 'var(--color-text-secondary)', fontStyle: 'italic' },
  { tag: tags.angleBracket, color: 'var(--color-text-tertiary)' },
  { tag: tags.tagName, color: 'var(--color-accent-light)' },
  { tag: tags.attributeName, color: 'var(--color-text-secondary)' },
  { tag: tags.attributeValue, color: 'var(--color-accent-light)' },
]);

/** Get CodeMirror extensions for seamless inline markdown editing. */
export function getEditorExtensions() {
  return [
    markdown(),
    editorTheme,
    syntaxHighlighting(highlightStyle),
    EditorView.lineWrapping,
  ];
}
