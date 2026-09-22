import { useRef } from "react";

import {
  DEFAULT_DOCK_WIDTH,
  MAX_DOCK_WIDTH,
  MIN_DOCK_WIDTH,
  useChatUiStore,
} from "@lumina/chat-ui/chatUiStore";
import { cn } from "@lumina/ui/utils";

const KEY_STEP = 12;

/**
 * Drag handle on the left edge of the right panel.
 * The chat dock and the sidebar panels (chapters/notes/…) are mutually
 * exclusive, so they share one persisted width (`dockWidth`).
 *
 * Pure flex sibling in normal flow — never overlays the HWND. Resizing only
 * reflows flex; the VideoSurface ResizeObserver picks that up and re-reports
 * bounds, so libmpv follows with no Rust changes.
 */
export function RightPanelResizeHandle() {
  const dockWidth = useChatUiStore((s) => s.dockWidth);
  const setDockWidth = useChatUiStore((s) => s.setDockWidth);
  const dragRef = useRef<{ startX: number; startWidth: number } | null>(null);
  const rafRef = useRef(0);

  const applyClientX = (clientX: number) => {
    const drag = dragRef.current;
    if (!drag) return;
    // 左边缘往左拖（clientX 变小）= 面板变宽。
    setDockWidth(drag.startWidth + (drag.startX - clientX));
  };

  const scheduleApply = (clientX: number) => {
    cancelAnimationFrame(rafRef.current);
    rafRef.current = requestAnimationFrame(() => applyClientX(clientX));
  };

  const endDrag = (clientX: number | null) => {
    cancelAnimationFrame(rafRef.current);
    if (clientX !== null) applyClientX(clientX);
    dragRef.current = null;
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="调整右侧面板宽度"
      aria-valuemin={MIN_DOCK_WIDTH}
      aria-valuemax={MAX_DOCK_WIDTH}
      aria-valuenow={Math.round(dockWidth)}
      aria-valuetext={`${Math.round(dockWidth)} 像素`}
      tabIndex={0}
      title="拖动调整右侧面板宽度，双击恢复默认"
      className={cn(
        "group z-10 flex w-1.5 shrink-0 cursor-col-resize items-stretch justify-center",
        "focus-visible:outline-none",
      )}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        dragRef.current = {
          startX: event.clientX,
          startWidth: useChatUiStore.getState().dockWidth,
        };
        try {
          event.currentTarget.setPointerCapture(event.pointerId);
        } catch {
          // happy-dom / 旧环境没有实现：拖动照常走 window 级 move 事件。
        }
      }}
      onPointerMove={(event) => {
        if (!dragRef.current) return;
        scheduleApply(event.clientX);
      }}
      onPointerUp={(event) => endDrag(event.clientX)}
      onPointerCancel={() => endDrag(null)}
      onDoubleClick={() => setDockWidth(DEFAULT_DOCK_WIDTH)}
      onKeyDown={(event) => {
        if (event.key === "ArrowLeft") {
          event.preventDefault();
          setDockWidth(dockWidth + KEY_STEP);
        } else if (event.key === "ArrowRight") {
          event.preventDefault();
          setDockWidth(dockWidth - KEY_STEP);
        } else if (event.key === "Home") {
          event.preventDefault();
          setDockWidth(MIN_DOCK_WIDTH);
        } else if (event.key === "End") {
          event.preventDefault();
          setDockWidth(MAX_DOCK_WIDTH);
        }
      }}
    >
      <span
        aria-hidden="true"
        className="w-px bg-border transition-colors group-hover:bg-accent group-focus-visible:bg-accent"
      />
    </div>
  );
}
