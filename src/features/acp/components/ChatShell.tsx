import { cn } from "@/lib/utils";

/** Unified max-width column for the whole chat surface. */
export function ChatShell({
  children,
  ...rest
}: {
  children: React.ReactNode;
} & React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div className="flex min-h-0 flex-1 flex-col" {...rest}>
      <div className="mx-auto flex h-full w-full max-w-2xl min-h-0 flex-col px-4">
        {children}
      </div>
    </div>
  );
}

type ColumnProps = {
  children: React.ReactNode;
  className?: string;
};

/** Same content width for messages, activity feed, composer. */
export function ChatColumn({ children, className }: ColumnProps) {
  return <div className={cn("w-full min-w-0", className)}>{children}</div>;
}
