/** Best-effort focus for Tauri WebView / WebView2 after async chat turns. */
export function focusComposerTextarea(element: HTMLTextAreaElement | null) {
  if (!element) return;

  const attempt = () => {
    if (element.disabled) return false;
    element.focus({ preventScroll: true });
    const end = element.value.length;
    try {
      element.setSelectionRange(end, end);
    } catch {
      // Some environments reject selection on empty textarea.
    }
    return document.activeElement === element;
  };

  if (attempt()) return;

  requestAnimationFrame(() => {
    if (attempt()) return;
    window.setTimeout(() => attempt(), 50);
    window.setTimeout(() => attempt(), 150);
  });
}
