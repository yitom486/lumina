/**
 * Layout bridge for the pristine @lumina/ui menu items.
 *
 * The vendor item owns the indicator markup, while the desktop app owns the
 * spacing/layout classes that Tailwind must compile from an app-side source.
 * Keeping the indicator in its own grid column prevents it from overlapping
 * the label when a vendor utility is not present in the app bundle.
 */
export const dropdownSelectionItemClass =
  "grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2 pl-2 [&>span]:static [&>span]:flex [&>span]:size-4 [&>span]:shrink-0 [&>span]:items-center [&>span]:justify-center";
