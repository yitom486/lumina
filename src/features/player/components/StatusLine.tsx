import { usePlayerStore } from "../store";

export function StatusLine() {
  const status = usePlayerStore((s) => s.status);
  const statusMessage = usePlayerStore((s) => s.statusMessage);
  const error = usePlayerStore((s) => s.error);

  return (
    <div className="min-w-0 flex-1 text-sm text-muted-foreground">
      <p className="truncate">
        {status} · {statusMessage}
      </p>
      {error ? (
        <p className="truncate text-red-600">
          [{error.code}] {error.message}
        </p>
      ) : null}
    </div>
  );
}
