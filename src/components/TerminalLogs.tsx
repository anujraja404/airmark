import { Terminal, X } from "lucide-react";
import { useEffect, useRef } from "react";
import type { LogLine } from "../types";

type TerminalLogsProps = {
  logs: LogLine[];
  activeRunId: string | null;
  onClose: () => void;
  onCancel: () => void;
};

export function TerminalLogs({
  logs,
  activeRunId,
  onClose,
  onCancel,
}: TerminalLogsProps) {
  const endRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [logs]);

  return (
    <section className="terminal-panel" aria-label="Execution logs">
      <header className="terminal-header">
        <span>
          <Terminal size={14} />
          Execution Logs
        </span>
        <div className="terminal-actions">
          {activeRunId && (
            <button type="button" onClick={onCancel}>
              Stop
            </button>
          )}
          <button aria-label="Close logs" type="button" onClick={onClose}>
            <X size={15} />
          </button>
        </div>
      </header>
      <div className="terminal-body">
        {logs.length === 0 ? (
          <div className="terminal-muted">Waiting for local output...</div>
        ) : (
          logs.map((log) => (
            <div
              className={log.isError ? "terminal-line error" : "terminal-line"}
              key={log.id}
            >
              {log.line}
            </div>
          ))
        )}
        <div ref={endRef} />
      </div>
    </section>
  );
}
