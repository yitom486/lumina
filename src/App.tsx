import { AppShell } from "@/layouts/AppShell";
import {
  PlayerBar,
  usePlayerEvents,
  VideoSurface,
} from "@/features/player";

export default function App() {
  usePlayerEvents();

  return (
    <AppShell>
      <VideoSurface />
      <PlayerBar />
    </AppShell>
  );
}
