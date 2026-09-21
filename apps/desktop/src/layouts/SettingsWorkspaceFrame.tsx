import type { ReactNode } from "react";
import {
  Captions,
  Cloud,
  Library,
  MonitorPlay,
  Sparkles,
  type LucideIcon,
} from "lucide-react";

import { Button } from "@lumina/ui/button";
import { cn } from "@lumina/ui/utils";

export type SettingsCategoryId =
  | "playback"
  | "subtitle"
  | "library"
  | "online"
  | "automation";

export type SettingsCategory = {
  id: SettingsCategoryId;
  label: string;
  description: string;
  icon: LucideIcon;
};

export const SETTINGS_CATEGORIES: readonly SettingsCategory[] = [
  {
    id: "playback",
    label: "播放与界面",
    description: "播放器、外观与快捷键",
    icon: MonitorPlay,
  },
  {
    id: "subtitle",
    label: "字幕工作坊",
    description: "识别、翻译与样式",
    icon: Captions,
  },
  {
    id: "library",
    label: "影视库",
    description: "媒体整理与匹配",
    icon: Library,
  },
  {
    id: "online",
    label: "在线资源",
    description: "视频源、字幕与存储",
    icon: Cloud,
  },
  {
    id: "automation",
    label: "AI 与自动化",
    description: "模型、提示词与工作流",
    icon: Sparkles,
  },
];

export type SettingsWorkspaceFrameProps = {
  activeCategory: SettingsCategoryId;
  onCategoryChange: (category: SettingsCategoryId) => void;
  children?: ReactNode;
  renderContent?: (category: SettingsCategoryId) => ReactNode;
  className?: string;
};

/**
 * Shared settings shell. It owns only category navigation and the content
 * slot; feature settings remain responsible for their real controls/data.
 * The two columns are ordinary flex siblings so the frame never covers the
 * native player surface.
 */
export function SettingsWorkspaceFrame({
  activeCategory,
  onCategoryChange,
  children,
  renderContent,
  className,
}: SettingsWorkspaceFrameProps) {
  const content = renderContent
    ? renderContent(activeCategory)
    : children;

  return (
    <section
      aria-label="设置工作区"
      className={cn(
        "flex min-h-0 min-w-0 flex-1 overflow-hidden bg-surface text-surface-foreground",
        className,
      )}
    >
      <nav
        aria-label="设置分类"
        className="flex w-64 shrink-0 flex-col border-r border-border bg-surface-elevated px-3 py-5 max-[760px]:w-16 max-[760px]:px-2"
      >
        <div className="px-3 pb-5 max-[760px]:px-0 max-[760px]:text-center">
          <h1 className="text-xl font-semibold tracking-tight max-[760px]:text-sm">
            设置
          </h1>
          <p className="mt-1 text-xs text-muted-foreground max-[760px]:sr-only">
            播放、字幕与 AI 工作流
          </p>
        </div>
        <ul className="flex list-none flex-col gap-1 p-0">
          {SETTINGS_CATEGORIES.map((category) => {
            const selected = activeCategory === category.id;
            const Icon = category.icon;
            return (
              <li key={category.id}>
                <Button
                  type="button"
                  variant="ghost"
                  className={cn(
                    "h-auto min-h-14 w-full justify-start gap-3 rounded-lg border-l-2 px-3 py-2 text-left max-[760px]:justify-center max-[760px]:gap-0 max-[760px]:px-1",
                    selected
                      ? "border-ai bg-ai-muted text-ai hover:bg-ai-muted"
                      : "border-transparent text-muted-foreground hover:bg-surface-subtle hover:text-foreground",
                  )}
                  aria-label={category.label}
                  aria-current={selected ? "page" : undefined}
                  aria-pressed={selected}
                  data-active={selected ? "true" : "false"}
                  onClick={() => onCategoryChange(category.id)}
                >
                  <Icon className="size-5 shrink-0" />
                  <span className="min-w-0 max-[760px]:sr-only">
                    <span className="block truncate text-sm font-medium">
                      {category.label}
                    </span>
                    <span className="mt-0.5 block truncate text-[11px] text-muted-foreground">
                      {category.description}
                    </span>
                  </span>
                </Button>
              </li>
            );
          })}
        </ul>
      </nav>
      <div className="min-h-0 min-w-0 flex-1 overflow-auto bg-surface px-5 py-6 max-[760px]:px-3 max-[760px]:py-4">
        {content}
      </div>
    </section>
  );
}
