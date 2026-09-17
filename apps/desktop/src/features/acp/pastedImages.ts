import type { ChatImageAttachment } from "@lumina/chat-ui/types";

/** Mirror of the backend guardrails (model.rs): caps must match. */
export const PASTED_IMAGE_LIMIT = 4;
export const PASTED_IMAGE_MAX_BYTES = 8 * 1024 * 1024;

const ALLOWED_MIME = ["image/png", "image/jpeg", "image/webp", "image/gif"];

export type PastedFileMeta = {
  name: string;
  type: string;
  size: number;
};

export type PastedImageOutcome = {
  accepted: ChatImageAttachment[];
  rejected: string[];
};

export function pastedImageId(): string {
  return `pasted-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

/**
 * Pure pick: which clipboard files become attachments. IO-free so it is
 * unit-testable; reading bytes stays in `readPastedFile`.
 */
export function pickAcceptableFiles<T extends PastedFileMeta>(
  files: T[],
  alreadyAttached: number,
): { accepted: T[]; rejected: string[] } {
  const accepted: T[] = [];
  const rejected: string[] = [];
  for (const file of files) {
    if (!ALLOWED_MIME.includes(file.type.toLowerCase())) {
      rejected.push(`不支持的图片格式，已忽略：${file.name || "未命名文件"}`);
      continue;
    }
    if (file.size > PASTED_IMAGE_MAX_BYTES) {
      rejected.push(`图片过大（超过 8MB），已忽略：${file.name || "未命名文件"}`);
      continue;
    }
    if (alreadyAttached + accepted.length >= PASTED_IMAGE_LIMIT) {
      rejected.push("一次最多附带 4 张图片，多余的已忽略");
      continue;
    }
    accepted.push(file);
  }
  return { accepted, rejected };
}

export function readPastedFile(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === "string") {
        resolve(reader.result);
      } else {
        reject(new Error("read failed"));
      }
    };
    reader.onerror = () => reject(new Error("read failed"));
    reader.readAsDataURL(file);
  });
}

/** dataURL → prompt wire input (raw base64, no `data:` prefix). */
export function toPromptImageInput(attachment: ChatImageAttachment): {
  mimeType: string;
  data: string;
} | null {
  const comma = attachment.dataUrl.indexOf(",");
  if (!attachment.dataUrl.startsWith("data:") || comma < 0) return null;
  const data = attachment.dataUrl.slice(comma + 1).trim();
  if (!data) return null;
  return { mimeType: attachment.mimeType, data };
}
