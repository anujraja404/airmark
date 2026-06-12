import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AssistantPayload,
  LogLine,
  Message,
  ToolDoneEvent,
  ToolLogEvent,
  ToolProposal,
} from "../types";

const initialMessage: Message = {
  id: "assistant-welcome",
  role: "assistant",
  content: "What local archive work should I prepare?",
  choices: ["Export ChatGPT Chats", "Backup Codex, Claude, and Antigravity"],
};

const makeId = (prefix: string) =>
  `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2)}`;

export function useAssistant() {
  const [messages, setMessages] = useState<Message[]>([initialMessage]);
  const [pendingProposal, setPendingProposal] = useState<ToolProposal | null>(
    null,
  );
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const [isThinking, setIsThinking] = useState(false);
  const [isLogOpen, setIsLogOpen] = useState(false);
  const historyRef = useRef<Message[]>([initialMessage]);

  useEffect(() => {
    historyRef.current = messages;
  }, [messages]);

  useEffect(() => {
    const cleanups = [
      listen<AssistantPayload>("llm_done", (event) => {
        const payload = event.payload;
        setMessages((current) => [
          ...current,
          {
            id: makeId("assistant"),
            role: "assistant",
            content: payload.message,
            choices: payload.choices,
          },
        ]);
        setPendingProposal(payload.proposedTool ?? null);
        setIsThinking(false);
      }),
      listen<string>("llm_error", () => {
        setIsThinking(false);
      }),
      listen<ToolLogEvent>("tool_log", (event) => {
        setIsLogOpen(true);
        setLogs((current) => [
          ...current,
          {
            id: makeId("log"),
            runId: event.payload.runId,
            line: event.payload.line,
            isError: event.payload.isError,
          },
        ]);
      }),
      listen<ToolDoneEvent>("tool_done", (event) => {
        setActiveRunId(null);
        setIsLogOpen(true);
        setLogs((current) => [
          ...current,
          {
            id: makeId("log"),
            runId: event.payload.runId,
            line: `Process finished: ${event.payload.status}`,
            isError: !event.payload.success,
          },
        ]);
        setMessages((current) => [
          ...current,
          {
            id: makeId("assistant"),
            role: "assistant",
            content: event.payload.success
              ? "The local workflow finished successfully."
              : "The local workflow stopped with an error. I kept the logs visible.",
            choices: ["Export ChatGPT Chats", "Backup Codex, Claude, and Antigravity"],
          },
        ]);
      }),
    ];

    return () => {
      cleanups.forEach((cleanup) => {
        void cleanup.then((dispose) => dispose());
      });
    };
  }, []);

  const transcript = useMemo(
    () =>
      messages.map((message) => ({
        role: message.role,
        content: message.content,
      })),
    [messages],
  );

  const sendChat = useCallback(
    async (content: string) => {
      const trimmed = content.trim();
      if (!trimmed || isThinking) return;

      const userMessage: Message = {
        id: makeId("user"),
        role: "user",
        content: trimmed,
      };
      const nextMessages = [...historyRef.current, userMessage];
      historyRef.current = nextMessages;
      setMessages(nextMessages);
      setPendingProposal(null);
      setIsThinking(true);

      try {
        await invoke("send_chat_turn", {
          messages: nextMessages.map(({ role, content }) => ({ role, content })),
        });
      } catch {
        setIsThinking(false);
      }
    },
    [isThinking],
  );

  const choose = useCallback(
    async (choice: string) => {
      if (isThinking) return;
      const proposal = await invoke<ToolProposal | null>("propose_tool", {
        intent: choice,
      });
      if (!proposal) {
        await sendChat(choice);
        return;
      }

      const userMessage: Message = {
        id: makeId("user"),
        role: "user",
        content: choice,
      };
      const assistantMessage: Message = {
        id: makeId("assistant"),
        role: "assistant",
        content: `I can prepare this local workflow: ${proposal.label}.`,
        choices: ["Show me another option", "Check ChatGPT setup"],
      };
      const nextMessages = [...historyRef.current, userMessage, assistantMessage];
      historyRef.current = nextMessages;
      setMessages(nextMessages);
      setPendingProposal(proposal);
    },
    [isThinking, sendChat],
  );

  const approveTool = useCallback(async () => {
    if (!pendingProposal || activeRunId) return;
    setLogs([]);
    setIsLogOpen(true);
    const runId = await invoke<string>("approve_tool_run", {
      proposalId: pendingProposal.proposalId,
    });
    setActiveRunId(runId);
    setPendingProposal(null);
  }, [activeRunId, pendingProposal]);

  const cancelTool = useCallback(async () => {
    if (!activeRunId) return;
    await invoke("cancel_tool_run", { runId: activeRunId });
    setActiveRunId(null);
  }, [activeRunId]);

  return {
    messages,
    transcript,
    pendingProposal,
    logs,
    activeRunId,
    isThinking,
    isLogOpen,
    setIsLogOpen,
    sendChat,
    choose,
    approveTool,
    cancelTool,
    dismissProposal: () => setPendingProposal(null),
  };
}
