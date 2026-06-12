export type ChatRole = "user" | "assistant";

export type ToolId =
  | "chatgpt_export_incremental"
  | "chatgpt_doctor"
  | "air_cde_backup_incremental";

export type Message = {
  id: string;
  role: ChatRole;
  content: string;
  choices?: string[];
};

export type ToolProposal = {
  proposalId: string;
  toolId: ToolId;
  label: string;
  rationale: string;
  cwd: string;
  commandPreview: string[];
  params?: Record<string, string>;
};

export type AssistantPayload = {
  message: string;
  choices: string[];
  proposedTool?: ToolProposal | null;
};

export type ToolLogEvent = {
  runId: string;
  toolId: ToolId;
  line: string;
  isError: boolean;
};

export type ToolDoneEvent = {
  runId: string;
  toolId: ToolId;
  success: boolean;
  status: string;
};

export type LogLine = {
  id: string;
  runId: string;
  line: string;
  isError: boolean;
};
