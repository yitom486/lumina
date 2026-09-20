import { MediaLibraryPanel } from "./MediaLibraryPanel";

/**
 * Settings-owned library operations. The workspace entry deliberately uses
 * the presentation-only default of MediaLibraryPanel.
 */
export function LibrarySettingsPanel() {
  return <MediaLibraryPanel view="settings" />;
}
