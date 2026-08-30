import { AppShell } from "@/layouts/AppShell";
import { MediaInfoPanel } from "@/features/media";
import {
  PlayerBar,
  usePlayerEvents,
  VideoSurface,
} from "@/features/player";
import { TranscriptPanel } from "@/features/transcript";

export default function App() {
  usePlayerEvents();

  return (
    <AppShell>
      <VideoSurface />
      <PlayerBar />
      <MediaInfoPanel />
      <TranscriptPanel />
    </AppShell>
  );
}
