/** Audio / subtitle / rate controls for the HTML sidebar (never over HWND). */

import { RateSelect } from "./RateSelect";
import { TrackControlButtons } from "./TrackControlButtons";

/** Compact track + rate row for the reader sidebar. */
export function TrackMenus() {
  return (
    <div className="flex flex-wrap items-center gap-1.5 border-b border-border px-2 py-1.5">
      <TrackControlButtons variant="sidebar" />
      <RateSelect />
    </div>
  );
}
