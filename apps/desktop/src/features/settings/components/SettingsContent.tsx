import { LibrarySettingsPanel } from "@/features/library";
import { SubtitleWorkshopPanel } from "@/features/transcript";
import { OnlineResourceSettingsPanel } from "@/features/ytdl";

import { AiAutomationSettingsPanel } from "./AiAutomationSettingsPanel";
import { PlaybackInterfaceSettingsPanel } from "./PlaybackInterfaceSettingsPanel";

import type { SettingsCategoryId } from "@/layouts/SettingsWorkspaceFrame";

export type { SettingsCategoryId } from "@/layouts/SettingsWorkspaceFrame";

type SettingsCategoryDefinition = {
  label: string;
  description: string;
};

const CATEGORY_DEFINITIONS: Record<
  SettingsCategoryId,
  SettingsCategoryDefinition
> = {
  playback: {
    label: "播放与界面",
    description: "播放器、外观与快捷键。",
  },
  subtitle: {
    label: "字幕工作坊",
    description: "字幕获取、识别、翻译与校对。",
  },
  library: {
    label: "影视库",
    description: "媒体根目录、扫描与元数据匹配。",
  },
  online: {
    label: "在线资源",
    description: "在线源、下载与解析设置。",
  },
  automation: {
    label: "AI 与自动化",
    description: "模型配置与自动化工作流。",
  },
};

function SettingsCategoryPanel({ category }: { category: SettingsCategoryId }) {
  switch (category) {
    case "playback":
      return <PlaybackInterfaceSettingsPanel />;
    case "subtitle":
      return <SubtitleWorkshopPanel />;
    case "library":
      return <LibrarySettingsPanel />;
    case "online":
      return <OnlineResourceSettingsPanel />;
    case "automation":
      return <AiAutomationSettingsPanel />;
  }
}

export type SettingsContentProps = {
  category?: SettingsCategoryId;
};

/**
 * Settings content only. The application shell owns navigation and placement;
 * this component owns the real feature settings entry point for the active
 * category. Keeping the category boundary here prevents inactive settings
 * panels from mounting queries, listeners, or mutations.
 */
export function SettingsContent({
  category = "playback",
}: SettingsContentProps) {
  const definition = CATEGORY_DEFINITIONS[category];

  return (
    <main className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4 text-sm">
      <header>
        <h1 className="text-lg font-semibold text-foreground">
          {definition.label}
        </h1>
        <p className="mt-1 text-xs text-muted-foreground">
          {definition.description}
        </p>
      </header>
      <section
        aria-labelledby={`settings-category-${category}`}
        className="min-h-0"
      >
        <h2 id={`settings-category-${category}`} className="sr-only">
          {definition.label}
        </h2>
        {SettingsCategoryPanel({ category })}
      </section>
    </main>
  );
}
