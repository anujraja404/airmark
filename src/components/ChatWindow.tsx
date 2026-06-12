import { AnimatePresence, motion } from "framer-motion";
import { Send, Terminal, X } from "lucide-react";
import { FormEvent, useEffect, useRef, useState } from "react";
import type { LogLine, Message, ToolProposal } from "../types";
import { DialogBox } from "./DialogBox";
import { TerminalLogs } from "./TerminalLogs";
import { ToolConfirmSheet } from "./ToolConfirmSheet";

type ChatWindowProps = {
  messages: Message[];
  pendingProposal: ToolProposal | null;
  logs: LogLine[];
  activeRunId: string | null;
  isThinking: boolean;
  isLogOpen: boolean;
  onSubmit: (content: string) => void;
  onChoice: (choice: string) => void;
  onApproveTool: () => void;
  onDismissTool: () => void;
  onCancelTool: () => void;
  onToggleLogs: () => void;
  onClose: () => void;
};

export function ChatWindow({
  messages,
  pendingProposal,
  logs,
  activeRunId,
  isThinking,
  isLogOpen,
  onSubmit,
  onChoice,
  onApproveTool,
  onDismissTool,
  onCancelTool,
  onToggleLogs,
  onClose,
}: ChatWindowProps) {
  const [input, setInput] = useState("");
  const messagesEndRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages, isThinking]);

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    const value = input.trim();
    if (!value) return;
    onSubmit(value);
    setInput("");
  }

  return (
    <div className="assistant-stage">
      <AnimatePresence>
        {isLogOpen && (
          <motion.div
            initial={{ opacity: 0, y: -12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -12 }}
            className="terminal-wrap"
          >
            <TerminalLogs
              logs={logs}
              activeRunId={activeRunId}
              onClose={onToggleLogs}
              onCancel={onCancelTool}
            />
          </motion.div>
        )}
      </AnimatePresence>

      <motion.section
        className="chat-window"
        initial={{ opacity: 0, y: -18, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: -18, scale: 0.98 }}
        transition={{ type: "spring", stiffness: 280, damping: 26 }}
      >
        <header className="chat-header">
          <div className="chat-title">
            <Terminal size={18} />
            <span>AirAssistant</span>
          </div>
          <div className="chat-actions">
            <button
              aria-label="Toggle execution logs"
              className={isLogOpen ? "active" : ""}
              onClick={onToggleLogs}
              type="button"
            >
              <Terminal size={16} />
            </button>
            <button aria-label="Close AirAssistant" onClick={onClose} type="button">
              <X size={17} />
            </button>
          </div>
        </header>

        <div className="dialog-scroll">
          {messages.map((message) => (
            <DialogBox
              key={message.id}
              message={message}
              onChoice={onChoice}
            />
          ))}
          {isThinking && (
            <div className="thinking-row">
              <span />
              <span />
              <span />
            </div>
          )}
          <div ref={messagesEndRef} />
        </div>

        <AnimatePresence>
          {pendingProposal && (
            <motion.div
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 12 }}
            >
              <ToolConfirmSheet
                proposal={pendingProposal}
                onApprove={onApproveTool}
                onDismiss={onDismissTool}
              />
            </motion.div>
          )}
        </AnimatePresence>

        <form className="chat-input-row" onSubmit={handleSubmit}>
          <input
            aria-label="Type your own response"
            autoFocus
            disabled={isThinking}
            onChange={(event) => setInput(event.target.value)}
            placeholder="Type your own response..."
            value={input}
          />
          <button aria-label="Send message" disabled={isThinking} type="submit">
            <Send size={16} />
          </button>
        </form>
      </motion.section>
    </div>
  );
}
