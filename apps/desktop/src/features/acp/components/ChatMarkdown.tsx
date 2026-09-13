import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";

import { enhanceCitationChildren } from "@/features/evidence/EvidenceLink";
import { cn } from "@/lib/utils";

import "katex/dist/katex.min.css";

type Props = {
  content: string;
  className?: string;
};

/** Agent reply renderer — GFM markdown + KaTeX math + verified citations. */
export function ChatMarkdown({ content, className }: Props) {
  return (
    <div className={cn("chat-markdown", className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={{
          a: ({ href, children }) => (
            <a
              href={href}
              target="_blank"
              rel="noreferrer noopener"
              className="font-medium text-sky-400 underline underline-offset-2 hover:text-sky-300"
            >
              {children}
            </a>
          ),
          // Citations inside code/pre/link never linkify (see enhanceCitationChildren).
          p: ({ children, ...props }) => (
            <p {...props}>{enhanceCitationChildren(children)}</p>
          ),
          li: ({ children, ...props }) => (
            <li {...props}>{enhanceCitationChildren(children)}</li>
          ),
          td: ({ children, ...props }) => (
            <td {...props}>{enhanceCitationChildren(children)}</td>
          ),
          th: ({ children, ...props }) => (
            <th {...props}>{enhanceCitationChildren(children)}</th>
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
