import { useState } from "react";
import { Link2 } from "lucide-react";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { OnlineSourceSettings } from "@/features/ytdl/components/OnlineSourceSettings";

import { usePlayerStore } from "../store";

export function OpenUrlButton() {
  const busy = usePlayerStore((s) => s.busy);
  const openUrl = usePlayerStore((s) => s.openUrl);
  const [url, setUrl] = useState("");
  const [open, setOpen] = useState(false);

  const canSubmit =
    /^https?:\/\//i.test(url.trim()) && url.trim().length > 10;

  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <AlertDialogTrigger asChild>
            <Button
              type="button"
              variant="secondary"
              size="icon-sm"
              disabled={busy}
              aria-label="打开链接"
            >
              <Link2 className="size-4" />
            </Button>
          </AlertDialogTrigger>
        </TooltipTrigger>
        <TooltipContent>打开在线视频链接</TooltipContent>
      </Tooltip>

      <AlertDialogContent className="max-h-[90vh] overflow-y-auto">
        <AlertDialogHeader>
          <AlertDialogTitle>打开在线视频</AlertDialogTitle>
          <AlertDialogDescription>
            粘贴 YouTube / Bilibili 等页面链接。可选授权浏览器登录态以获取更高清晰度。
          </AlertDialogDescription>
        </AlertDialogHeader>
        <input
          type="url"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="https://"
          className="mt-2 w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
          autoFocus
          onKeyDown={(e) => {
            if (e.key === "Enter" && canSubmit) {
              e.preventDefault();
              void openUrl(url).then(() => {
                setOpen(false);
                setUrl("");
              });
            }
          }}
        />
        <OnlineSourceSettings />
        <AlertDialogFooter>
          <AlertDialogCancel>取消</AlertDialogCancel>
          <AlertDialogAction
            disabled={!canSubmit || busy}
            onClick={(e) => {
              e.preventDefault();
              void openUrl(url).then(() => {
                setOpen(false);
                setUrl("");
              });
            }}
          >
            打开
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
