import { QualitySelect } from "@/features/player/components/QualitySelect";
import { usePlayerStore } from "@/features/player";
import { useUiStore } from "@/features/player/uiStore";

export function PlaybackInterfaceSettingsPanel() {
  const currentFile = usePlayerStore((state) => state.currentFile);
  const sourceKind = usePlayerStore((state) => state.sourceKind);
  const fullscreen = useUiStore((state) => state.fullscreen);
  const setFullscreen = useUiStore((state) => state.setFullscreen);

  return (
    <section className="space-y-3 rounded-lg border border-border bg-card/40 p-4">
      <div>
        <h2 className="text-base font-medium text-foreground">播放与界面</h2>
        <p className="mt-1 text-xs text-muted-foreground">
          播放显示和当前媒体相关的界面偏好。播放器仍由原生播放服务负责。
        </p>
      </div>
      <label className="flex items-center justify-between gap-3 rounded-md border border-border/70 bg-muted/20 px-3 py-2 text-sm">
        <span>
          <span className="block font-medium">全屏播放</span>
          <span className="mt-0.5 block text-xs text-muted-foreground">
            使用原生窗口全屏，不覆盖播放器画面。
          </span>
        </span>
        <input
          type="checkbox"
          checked={fullscreen}
          onChange={(event) => void setFullscreen(event.target.checked)}
          aria-label="全屏播放"
        />
      </label>
      {sourceKind === "remote" ? (
        <div className="flex items-center justify-between gap-3 rounded-md border border-border/70 bg-muted/20 px-3 py-2 text-sm">
          <span>
            <span className="block font-medium">在线视频清晰度</span>
            <span className="mt-0.5 block text-xs text-muted-foreground">
              使用当前在线媒体真实可用的格式列表。
            </span>
          </span>
          <QualitySelect />
        </div>
      ) : null}
      <p className="truncate text-xs text-muted-foreground">
        当前媒体：{currentFile ?? "未打开"}
      </p>
    </section>
  );
}
