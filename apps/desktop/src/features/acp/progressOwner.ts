/**
 * 进度行归属：全局只有一行 `progress`，但事件来自多轮 run 的 Channel，
 * 到达顺序不保证（迟到/重复/串台）。归属者才能写行、清行。
 *
 * - 每一轮 run / 新建 / 切换在启动时 claim 自己的 turnId（系统动作用 sys token）；
 * - started/progress 只有归属者未 seal 才能写；
 * - finished/failed 照常更新自己的气泡，但清行 + seal 只有归属者能做；
 * - 新一轮 claim 直接顶掉旧归属，旧轮的迟到事件从此全部失效。
 */
export type ProgressOwner = {
  turnId: string;
  sealed: boolean;
} | null;

export function claimProgressOwner(turnId: string): ProgressOwner {
  return { turnId, sealed: false };
}

export function acceptsProgressEvent(
  owner: ProgressOwner,
  turnId: string,
): boolean {
  return owner !== null && owner.turnId === turnId && !owner.sealed;
}

export function sealProgressOwner(
  owner: ProgressOwner,
  turnId: string,
): ProgressOwner {
  if (owner?.turnId !== turnId) return owner;
  return { turnId, sealed: true };
}
