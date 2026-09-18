/**
 * 观影打卡卡片：纯布局/换行逻辑（可测）+ canvas 渲染（组件层调用）。
 * 设计语言：暗色胶片渐变 + sky 强调色，与面板科技简约风一致。
 */

export type RecapCardRatio = "3:4" | "9:16";

export type RecapCardData = {
  mediaTitle: string;
  timeLabel: string;
  body: string;
  quotes: string[];
  /** 已解码的截帧（可选）；缺省时退化为纯文字卡片。 */
  frameImage?: HTMLImageElement | null;
  ratio: RecapCardRatio;
};

export const RECAP_CARD_SIZE: Record<
  RecapCardRatio,
  { width: number; height: number }
> = {
  "3:4": { width: 720, height: 960 },
  "9:16": { width: 640, height: 1138 },
};

/** 与 measure 解耦的换行（纯函数，单测用）。 */
export function wrapPlainLines(
  text: string,
  maxWidthPx: number,
  measure: (value: string) => number,
): string[] {
  const lines: string[] = [];
  for (const rawLine of text.split("\n")) {
    let current = "";
    for (const char of rawLine) {
      const candidate = current + char;
      if (measure(candidate) > maxWidthPx && current) {
        lines.push(current);
        current = char;
      } else {
        current = candidate;
      }
    }
    lines.push(current);
  }
  return lines;
}

/** 把卡片绘制到 canvas（同步；帧图由调用方先解码成 Image 传入）。 */
export function renderRecapCard(
  canvas: HTMLCanvasElement,
  data: RecapCardData,
): void {
  const size = RECAP_CARD_SIZE[data.ratio];
  canvas.width = size.width;
  canvas.height = size.height;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("canvas 2d context unavailable");

  const W = size.width;
  const pad = Math.round(W * 0.06);
  const textWidth = W - pad * 2;
  const measure = (value: string) => ctx.measureText(value).width;
  const wrap = (text: string, font: string) => {
    const previous = ctx.font;
    ctx.font = font;
    const lines = wrapPlainLines(text, textWidth, measure);
    ctx.font = previous;
    return lines;
  };

  // 背景：暗色胶片渐变。
  const bg = ctx.createLinearGradient(0, 0, 0, size.height);
  bg.addColorStop(0, "#10161f");
  bg.addColorStop(1, "#0b0f14");
  ctx.fillStyle = bg;
  ctx.fillRect(0, 0, size.width, size.height);

  // 顶部：媒体标题。
  ctx.fillStyle = "#38bdf8";
  ctx.font = "600 20px system-ui, sans-serif";
  ctx.fillText(ellipsize(measure, data.mediaTitle || "Lumina", textWidth), pad, pad + 24);

  let cursorY = pad + 48;
  if (data.frameImage) {
    const frameHeight = Math.round((textWidth * 9) / 16);
    ctx.fillStyle = "#05080c";
    ctx.fillRect(pad, cursorY, textWidth, frameHeight);
    drawContain(ctx, data.frameImage, pad, cursorY, textWidth, frameHeight);
    cursorY += frameHeight + Math.round(W * 0.04);
  }

  // 时间戳徽章。
  ctx.fillStyle = "rgba(56,189,248,0.15)";
  ctx.fillRect(pad, cursorY, 92, 24);
  ctx.fillStyle = "#38bdf8";
  ctx.font = "600 13px system-ui, sans-serif";
  ctx.fillText(data.timeLabel, pad + 10, cursorY + 17);
  cursorY += 24 + Math.round(W * 0.05);

  // 台词引用。
  ctx.fillStyle = "#94a3b8";
  ctx.font = "16px system-ui, sans-serif";
  for (const quote of data.quotes) {
    for (const line of wrap(quote, "16px system-ui, sans-serif")) {
      ctx.fillText(line, pad + 8, cursorY);
      cursorY += 24;
    }
    cursorY += 6;
  }
  cursorY += 12;

  // 正文（批注/金句点评）。
  ctx.fillStyle = "#e5e7eb";
  const bodyLines = wrap(data.body, "18px system-ui, sans-serif");
  const maxBodyLines = Math.max(
    0,
    Math.floor((size.height - cursorY - pad - 36) / 27),
  );
  for (const line of bodyLines.slice(0, maxBodyLines)) {
    ctx.fillText(line, pad, cursorY);
    cursorY += 27;
  }
  if (bodyLines.length > maxBodyLines) {
    ctx.fillStyle = "#64748b";
    ctx.font = "14px system-ui, sans-serif";
    ctx.fillText("……（正文已截断）", pad, cursorY);
  }

  // 页脚。
  ctx.fillStyle = "#475569";
  ctx.font = "600 14px system-ui, sans-serif";
  ctx.fillText("Lumina · AI Video Reader", pad, size.height - pad - 6);
}

function ellipsize(
  measure: (value: string) => number,
  text: string,
  maxWidth: number,
): string {
  if (measure(text) <= maxWidth) return text;
  let out = text;
  while (out.length > 1 && measure(`${out}…`) > maxWidth) {
    out = out.slice(0, -1);
  }
  return `${out}…`;
}

/** contain 缩放贴图：帧图按宽高比居中放入目标框，不留拉伸。 */
function drawContain(
  ctx: CanvasRenderingContext2D,
  image: HTMLImageElement,
  x: number,
  y: number,
  width: number,
  height: number,
): void {
  const scale = Math.min(width / image.width, height / image.height);
  const drawWidth = image.width * scale;
  const drawHeight = image.height * scale;
  ctx.drawImage(
    image,
    x + (width - drawWidth) / 2,
    y + (height - drawHeight) / 2,
    drawHeight === 0 ? drawWidth : drawWidth,
    drawHeight,
  );
}

/** Base64 data URL → Image（导出前的帧图解码）。失败返回 null。 */
export function decodeImage(dataUrl: string): Promise<HTMLImageElement | null> {
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => resolve(null);
    image.src = dataUrl;
  });
}
