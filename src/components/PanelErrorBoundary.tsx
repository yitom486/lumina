import { Component, type ErrorInfo, type ReactNode } from "react";

import { reportRenderError } from "@/lib/reportRenderError";

export type PanelErrorBoundaryProps = {
  children: ReactNode;
  /** User-visible title (Chinese, no stack traces). */
  title: string;
  hint?: string;
  /** Logging scope, e.g. sidebar:acp */
  scope?: string;
  /** Changing this clears the error and retries children. */
  resetKey?: string | number;
  className?: string;
};

type State = {
  error: Error | null;
};

const DEFAULT_HINT =
  "播放与其它面板仍可使用。可点击重试；若反复失败，请重启应用或清除站点存储。";

/** Isolates a feature panel so render errors do not blank the WebView. */
export class PanelErrorBoundary extends Component<PanelErrorBoundaryProps, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    reportRenderError(this.props.scope ?? "panel", error, info.componentStack);
  }

  componentDidUpdate(prevProps: PanelErrorBoundaryProps) {
    if (
      this.state.error &&
      prevProps.resetKey !== this.props.resetKey
    ) {
      this.setState({ error: null });
    }
  }

  private retry = () => {
    this.setState({ error: null });
  };

  render() {
    if (this.state.error) {
      return (
        <div
          className={
            this.props.className ??
            "flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-4 py-8 text-center"
          }
        >
          <p className="text-sm font-medium text-foreground">{this.props.title}</p>
          <p className="max-w-sm text-xs leading-relaxed text-muted-foreground">
            {this.props.hint ?? DEFAULT_HINT}
          </p>
          <button
            type="button"
            className="mt-2 rounded-md border border-border px-3 py-1.5 text-xs text-foreground hover:bg-muted"
            onClick={this.retry}
          >
            重试
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

/** Last-resort shell boundary — keeps native player chrome mountable when possible. */
export function AppErrorBoundary({ children }: { children: ReactNode }) {
  return (
    <PanelErrorBoundary
      scope="app-root"
      title="应用界面出现问题"
      hint="请尝试重试或重启 Lumina。若仅某个侧边栏标签页出错，可先切换到其它标签。"
      className="flex min-h-svh flex-col items-center justify-center gap-3 bg-background px-6 py-12 text-center text-foreground"
    >
      {children}
    </PanelErrorBoundary>
  );
}
