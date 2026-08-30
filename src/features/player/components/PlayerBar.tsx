import { RateSelect } from "./RateSelect";
import { SeekBar } from "./SeekBar";
import { StatusLine } from "./StatusLine";
import { TransportControls } from "./TransportControls";
import { VolumeControl } from "./VolumeControl";

/** Bottom chrome: transport + scrubber + volume/rate. Native video sits above. */
export function PlayerBar() {
  return (
    <div className="flex flex-col gap-3 border-t border-border px-6 py-4">
      <div className="flex flex-wrap items-center gap-3">
        <TransportControls />
        <StatusLine />
      </div>
      <SeekBar />
      <div className="flex flex-wrap items-center gap-6">
        <VolumeControl />
        <RateSelect />
      </div>
    </div>
  );
}
