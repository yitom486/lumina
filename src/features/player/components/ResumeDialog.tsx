import { formatTime } from "@/lib/format";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";

import { usePlayerStore } from "../store";

export function ResumeDialog() {
  const prompt = usePlayerStore((s) => s.resumePrompt);
  const resolveResume = usePlayerStore((s) => s.resolveResume);

  const open = prompt != null;

  return (
    <AlertDialog
      open={open}
      onOpenChange={(next) => {
        if (!next && prompt) {
          // Esc / overlay: default to continue from saved position
          void resolveResume("continue");
        }
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>从上次进度继续？</AlertDialogTitle>
          <AlertDialogDescription>
            {prompt
              ? `已检测到上次播放到 ${formatTime(prompt.positionMs)}。是否从头开始？`
              : null}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel
            onClick={(e) => {
              e.preventDefault();
              void resolveResume("continue");
            }}
          >
            继续播放
          </AlertDialogCancel>
          <AlertDialogAction
            onClick={(e) => {
              e.preventDefault();
              void resolveResume("restart");
            }}
          >
            从头开始
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
