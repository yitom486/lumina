import { describe, expect, it } from "vitest";

import type { ChatTurn } from "@/features/acp/types";

import {
  isProposeAnnotationTool,
  shouldFetchAnnotationProposal,
  shouldRescanTurnForProposal,
  shouldTrackProposeToolCall,
  turnNeedsProposalFetch,
} from "./annotationProposal";

describe("annotationProposal", () => {
  it("detects propose annotation tool by title", () => {
    expect(isProposeAnnotationTool("lumina_propose_video_annotation")).toBe(true);
    expect(
      isProposeAnnotationTool("mcp.lumina.lumina_propose_video_annotation"),
    ).toBe(true);
    expect(isProposeAnnotationTool("lumina_get_transcript_window")).toBe(false);
  });

  it("tracks propose tool on initial toolCall", () => {
    expect(
      shouldTrackProposeToolCall({
        type: "toolCall",
        toolCallId: "1",
        title: "mcp.lumina.lumina_propose_video_annotation",
      }),
    ).toBe(true);
  });

  it("loads proposal when completed update lacks title but toolCallId was tracked", () => {
    expect(
      shouldFetchAnnotationProposal(
        {
          type: "toolCallUpdate",
          toolCallId: "1",
          status: "completed",
        },
        { trackedProposeToolCallIds: new Set(["1"]) },
      ),
    ).toBe(true);
  });

  it("loads proposal when completed update lacks title but activity title matches", () => {
    expect(
      shouldFetchAnnotationProposal(
        {
          type: "toolCallUpdate",
          toolCallId: "1",
          status: "completed",
        },
        {
          activities: [
            {
              id: "tool-1",
              kind: "tool",
              toolCallId: "1",
              title: "mcp.lumina.lumina_propose_video_annotation",
              status: "running",
            },
          ],
        },
      ),
    ).toBe(true);
  });

  it("falls back to finished turn scan when propose tool completed", () => {
    const turn: ChatTurn = {
      id: "t1",
      userText: "写批注",
      answer: "已生成",
      status: "done",
      activities: [
        {
          id: "tool-1",
          kind: "tool",
          toolCallId: "1",
          title: "mcp.lumina.lumina_propose_video_annotation",
          status: "completed",
        },
      ],
      showActivities: false,
    };
    expect(turnNeedsProposalFetch(turn)).toBe(true);
  });

  it("only rescans the latest turn for a missing proposal", () => {
    const older: ChatTurn = {
      id: "turn-1",
      userText: "写批注",
      answer: "已生成",
      status: "done",
      activities: [
        {
          id: "tool-1",
          kind: "tool",
          toolCallId: "1",
          title: "mcp.lumina.lumina_propose_video_annotation",
          status: "completed",
        },
      ],
      showActivities: false,
    };
    const latest: ChatTurn = {
      id: "turn-2",
      userText: "再来一次",
      answer: "好的",
      status: "done",
      activities: [],
      showActivities: false,
    };
    expect(shouldRescanTurnForProposal(older, [older, latest])).toBe(false);
    expect(shouldRescanTurnForProposal(latest, [older, latest])).toBe(false);
  });

  it("rescans only when the latest turn still needs a proposal", () => {
    const olderSaved: ChatTurn = {
      id: "turn-1",
      userText: "写批注",
      answer: "已保存",
      status: "done",
      activities: [],
      showActivities: false,
      annotationProposalSaved: true,
    };
    const latestPending: ChatTurn = {
      id: "turn-2",
      userText: "再来一次",
      answer: "已生成",
      status: "done",
      activities: [
        {
          id: "tool-2",
          kind: "tool",
          toolCallId: "2",
          title: "mcp.lumina.lumina_propose_video_annotation",
          status: "completed",
        },
      ],
      showActivities: false,
    };
    const turns = [olderSaved, latestPending];

    expect(shouldRescanTurnForProposal(olderSaved, turns)).toBe(false);
    expect(shouldRescanTurnForProposal(latestPending, turns)).toBe(true);
  });

  it("does not rescan turns that already have a saved or pending proposal", () => {
    const withCard: ChatTurn = {
      id: "turn-1",
      userText: "写批注",
      answer: "待确认",
      status: "done",
      activities: [],
      showActivities: false,
      annotationProposal: {
        proposalId: "proposal-1",
        mediaPath: "D:\\show.mkv",
        positionMs: 1,
        body: "a",
        includeQuotes: false,
        quotes: [],
        previewMarkdown: "a",
        createdAtMs: 1,
      },
    };
    expect(shouldRescanTurnForProposal(withCard, [withCard])).toBe(false);
  });
});
