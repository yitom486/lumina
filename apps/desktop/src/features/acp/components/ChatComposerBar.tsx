import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef } from "react";
import { ArrowUp, ListPlus, Loader2, Zap } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import type { AcpStatus, PermissionMode } from "../types";
import { previewQueuedText, type QueuedPrompt } from "../promptQueue";
import { useAgentModelControls } from "../useAgentModelControls";
import { focusComposerTextarea } from "./composerFocus";
import { ChatColumn } from "./ChatShell";

type Props = {
  value: string;
  disabled?: boolean;
  busy?: boolean;
  placeholder?: string;
  status: AcpStatus | undefined;
  sessionConnected?: boolean;
  queue?: QueuedPrompt[];
  onChange: (value: string) => void;
  onSend: () => void;
  onCancel?: () => void;
  onBargeIn?: () => void;
  onRemoveQueued?: (id: string) => void;
  onClearQueue?: () => void;
};

export type ChatComposerBarHandle = {
  focusInput: () => void;
};

const compactSelectClassName =
  "h-7 max-w-[9.5rem] truncate rounded-md border border-border bg-background px-2 text-[11px] text-foreground outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-60";

export const ChatComposerBar = forwardRef<ChatComposerBarHandle, Props>(
  function ChatComposerBar(
    {
      value,
      disabled,
      busy,
      placeholder = "输入问题…",
      status,
      sessionConnected,
      queue = [],
      onChange,
      onSend,
      onCancel,
      onBargeIn,
      onRemoveQueued,
      onClearQueue,
    },
    ref,
  ) {
    const {
      permissionMode,
      modelId,
      reasoningEffort,
      modelOptions,
      reasoningOptions,
      controlsDisabled,
      controlError,
      hasModelOptions,
      hasReasoningOptions,
      patchSettings,
      applyModelSelection,
    } = useAgentModelControls({
      status,
      busy,
      sessionConnected,
    });

    const textareaRef = useRef<HTMLTextAreaElement>(null);
    const wasBusyRef = useRef(false);

    const focusInput = useCallback(() => {
      focusComposerTextarea(textareaRef.current);
    }, []);

    useImperativeHandle(ref, () => ({ focusInput }), [focusInput]);

    useEffect(() => {
      if (wasBusyRef.current && !busy && !disabled) {
        focusInput();
      }
      wasBusyRef.current = Boolean(busy);
    }, [busy, disabled, focusInput]);

    const hasText = value.trim().length > 0;
    const canSendIdle = !disabled && !busy && hasText;
    const canEnqueue = !disabled && Boolean(busy) && hasText;
    const canBargeIn = canEnqueue && Boolean(onBargeIn);
    const canPrimarySend = busy ? canEnqueue : canSendIdle;

    return (
      <ChatColumn className="shrink-0 border-t border-border py-3">
        {queue.length > 0 ? (
          <div className="mb-2 rounded-md border border-border/70 bg-muted/30 px-2 py-1.5">
            <div className="mb-1 flex items-center justify-between gap-2">
              <p className="text-[11px] text-muted-foreground">
                排队 {queue.length} 条 · 当前回合结束后自动发送
              </p>
              {onClearQueue ? (
                <button
                  type="button"
                  className="text-[10px] text-muted-foreground hover:text-foreground"
                  onClick={onClearQueue}
                >
                  清空
                </button>
              ) : null}
            </div>
            <ul className="space-y-1">
              {queue.map((item, index) => (
                <li
                  key={item.id}
                  className="flex items-center gap-2 text-[11px] text-foreground"
                >
                  <span className="shrink-0 text-muted-foreground">
                    {index + 1}.
                  </span>
                  <span className="min-w-0 flex-1 truncate">
                    {previewQueuedText(item.text)}
                  </span>
                  {onRemoveQueued ? (
                    <button
                      type="button"
                      className="shrink-0 text-[10px] text-muted-foreground hover:text-destructive"
                      aria-label="移除排队"
                      onClick={() => onRemoveQueued(item.id)}
                    >
                      移除
                    </button>
                  ) : null}
                </li>
              ))}
            </ul>
          </div>
        ) : null}

        <div
          className={cn(
            "rounded-lg border border-border bg-background",
            busy && "chat-composer-active p-px",
          )}
        >
          <textarea
            ref={textareaRef}
            className={cn(
              "min-h-[72px] w-full resize-none rounded-t-lg bg-transparent px-3 py-2 text-sm",
              "outline-none focus-visible:ring-0",
              "disabled:opacity-60",
              busy && "bg-muted/20",
            )}
            placeholder={
              busy
                ? "回复进行中：Enter 排队，Ctrl+Enter 插队发送…"
                : placeholder
            }
            value={value}
            disabled={disabled}
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key !== "Enter" || e.shiftKey) return;
              e.preventDefault();
              if ((e.ctrlKey || e.metaKey) && canBargeIn) {
                onBargeIn?.();
                return;
              }
              if (canPrimarySend) onSend();
            }}
          />

          <div className="flex items-center gap-1.5 border-t border-border/70 px-2 py-1.5">
            <select
              className={cn(compactSelectClassName, "max-w-[7.5rem]")}
              value={permissionMode}
              disabled={controlsDisabled}
              aria-label="权限模式"
              onChange={(e) =>
                patchSettings({
                  permissionMode: e.target.value as PermissionMode,
                })
              }
            >
              <option value="auto">自动批准</option>
              <option value="ask">每次询问</option>
            </select>

            {hasModelOptions ? (
              <select
                className={compactSelectClassName}
                value={modelId}
                disabled={controlsDisabled}
                aria-label="模型"
                onChange={(e) => {
                  void applyModelSelection({ modelId: e.target.value });
                }}
              >
                <option value="">Agent 默认</option>
                {modelOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.name}
                  </option>
                ))}
              </select>
            ) : (
              <input
                className={cn(compactSelectClassName, "min-w-[7rem]")}
                value={modelId}
                disabled={controlsDisabled}
                placeholder="模型 ID"
                aria-label="模型 ID"
                onChange={(e) => patchSettings({ modelId: e.target.value })}
                onBlur={() => {
                  if (modelId.trim()) {
                    void applyModelSelection({ modelId });
                  }
                }}
              />
            )}

            {hasReasoningOptions ? (
              <select
                className={compactSelectClassName}
                value={reasoningEffort}
                disabled={controlsDisabled}
                aria-label="思考程度"
                onChange={(e) => {
                  void applyModelSelection({ reasoningEffort: e.target.value });
                }}
              >
                <option value="">默认</option>
                {reasoningOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.name}
                  </option>
                ))}
              </select>
            ) : null}

            <div className="ml-auto flex items-center gap-1">
              {busy ? (
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  className="size-8 shrink-0"
                  aria-label="取消当前回合"
                  title="取消当前回合（队列保留）"
                  onClick={() => onCancel?.()}
                >
                  <span className="text-[11px]">取消</span>
                </Button>
              ) : null}
              {busy && onBargeIn ? (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  className="h-8 gap-1 px-2 text-[11px]"
                  disabled={!canBargeIn}
                  aria-label="插队发送"
                  title="打断当前回合并优先发送（Ctrl+Enter）"
                  onClick={() => onBargeIn()}
                >
                  <Zap className="size-3.5" />
                  插队
                </Button>
              ) : null}
              <Button
                type="button"
                size="icon"
                className="size-8 shrink-0 rounded-full"
                disabled={!canPrimarySend}
                aria-label={busy ? "加入排队" : "发送"}
                title={busy ? "加入排队（Enter）" : "发送（Enter）"}
                onClick={onSend}
              >
                {busy ? (
                  canEnqueue ? (
                    <ListPlus className="size-4" />
                  ) : (
                    <Loader2 className="size-4 animate-spin" />
                  )
                ) : (
                  <ArrowUp className="size-4" />
                )}
              </Button>
            </div>
          </div>
        </div>

        {controlError ? (
          <p className="mt-1 text-[10px] leading-relaxed text-destructive">
            {controlError}
          </p>
        ) : null}
      </ChatColumn>
    );
  },
);
