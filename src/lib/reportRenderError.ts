/** Log render failures for developers; never shown in product UI. */
export function reportRenderError(
  scope: string,
  error: unknown,
  componentStack?: string | null,
): void {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`[render:${scope}]`, message, componentStack ?? "");
}
