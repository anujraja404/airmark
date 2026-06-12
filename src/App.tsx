import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ChatWindow } from "./components/ChatWindow";
import { TriggerButton } from "./components/TriggerButton";
import { useAssistant } from "./hooks/useAssistant";
import "./App.css";

function viewMode() {
  return new URLSearchParams(window.location.search).get("view") ?? "overlay";
}

function TriggerView() {
  return (
    <main className="trigger-root">
      <TriggerButton
        label="Open AirAssistant"
        onClick={() => invoke("set_assistant_visible", { visible: true })}
      />
    </main>
  );
}

function OverlayView() {
  const assistant = useAssistant();
  const [isVisible, setIsVisible] = useState(true);

  useEffect(() => {
    const cleanup = listen<boolean>("assistant_visibility", (event) => {
      setIsVisible(event.payload);
    });
    return () => {
      void cleanup.then((dispose) => dispose());
    };
  }, []);

  useEffect(() => {
    void invoke("set_interaction_mode", {
      mode: isVisible ? "interactive" : "passThrough",
    });
  }, [isVisible]);

  return (
    <main className="overlay-root" aria-hidden={!isVisible}>
      {isVisible && (
        <ChatWindow
          messages={assistant.messages}
          pendingProposal={assistant.pendingProposal}
          logs={assistant.logs}
          activeRunId={assistant.activeRunId}
          isThinking={assistant.isThinking}
          isLogOpen={assistant.isLogOpen}
          onSubmit={assistant.sendChat}
          onChoice={assistant.choose}
          onApproveTool={assistant.approveTool}
          onDismissTool={assistant.dismissProposal}
          onCancelTool={assistant.cancelTool}
          onToggleLogs={() => assistant.setIsLogOpen(!assistant.isLogOpen)}
          onClose={() => invoke("set_assistant_visible", { visible: false })}
        />
      )}
    </main>
  );
}

export default function App() {
  const mode = useMemo(viewMode, []);
  return mode === "trigger" ? <TriggerView /> : <OverlayView />;
}
