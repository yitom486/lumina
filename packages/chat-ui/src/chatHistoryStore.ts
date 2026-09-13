import { create } from "zustand";
import { persist } from "zustand/middleware";

import type { ChatTurn } from "./types";

export type SavedChatConversation = {
  id: string;
  title: string;
  cwd: string | null;
  profileId: string;
  updatedAtMs: number;
  turns: ChatTurn[];
};

const MAX_CONVERSATIONS = 40;

function conversationTitle(turns: ChatTurn[]): string {
  const first = turns.find((turn) => turn.userText.trim());
  if (first) return first.userText.trim().slice(0, 64);
  const answer = turns.find((turn) => turn.answer.trim());
  if (answer) return answer.answer.trim().slice(0, 64);
  return "未命名对话";
}

function hasConversationContent(turns: ChatTurn[]): boolean {
  return turns.some((turn) => turn.userText.trim() || turn.answer.trim());
}

type ChatHistoryStore = {
  conversations: SavedChatConversation[];
  activeConversationId: string | null;
  upsertActiveConversation: (input: {
    id: string;
    cwd: string | null;
    profileId: string;
    turns: ChatTurn[];
  }) => void;
  setActiveConversationId: (id: string | null) => void;
  deleteConversation: (id: string) => void;
  clearAll: () => void;
};

/** Local chat transcripts — separate from ACP session resume hints. */
export const useChatHistoryStore = create<ChatHistoryStore>()(
  persist(
    (set) => ({
      conversations: [],
      activeConversationId: null,
      upsertActiveConversation: ({ id, cwd, profileId, turns }) => {
        if (!hasConversationContent(turns)) return;
        const entry: SavedChatConversation = {
          id,
          title: conversationTitle(turns),
          cwd,
          profileId,
          updatedAtMs: Date.now(),
          turns: turns.map((turn) => ({
            ...turn,
            status: turn.status === "streaming" ? "done" : turn.status,
            showActivities: false,
            activities: [],
            agentDraft: undefined,
            agentSegments: undefined,
          })),
        };
        set((state) => {
          const rest = state.conversations.filter((item) => item.id !== id);
          const conversations = [entry, ...rest].slice(0, MAX_CONVERSATIONS);
          return {
            conversations,
            activeConversationId: id,
          };
        });
      },
      setActiveConversationId: (activeConversationId) => set({ activeConversationId }),
      deleteConversation: (id) =>
        set((state) => ({
          conversations: state.conversations.filter((item) => item.id !== id),
          activeConversationId:
            state.activeConversationId === id ? null : state.activeConversationId,
        })),
      clearAll: () => set({ conversations: [], activeConversationId: null }),
    }),
    {
      name: "lumina-acp-chat-history",
      partialize: (state) => ({
        conversations: state.conversations,
        activeConversationId: state.activeConversationId,
      }),
    },
  ),
);

export function listConversationsForScope(
  conversations: SavedChatConversation[],
  cwd: string | null | undefined,
  profileId: string,
  includeAll: boolean,
): SavedChatConversation[] {
  const sorted = [...conversations].sort(
    (a, b) => b.updatedAtMs - a.updatedAtMs,
  );
  if (includeAll) return sorted;
  return sorted.filter(
    (item) => item.profileId === profileId && item.cwd === (cwd ?? null),
  );
}

export function formatConversationTime(updatedAtMs: number): string {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(updatedAtMs));
}
