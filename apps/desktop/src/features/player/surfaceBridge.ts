/** Lets open() refresh HWND bounds without coupling store → React refs. */

type Reporter = () => Promise<void>;

let reporter: Reporter | null = null;

export function setSurfaceReporter(next: Reporter | null): void {
  reporter = next;
}

export async function ensureSurfaceBounds(): Promise<void> {
  if (reporter) {
    await reporter();
  }
}
