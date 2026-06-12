mod macos_shim;

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    path::Path,
    process::Stdio,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

const OVERLAY_WINDOW_LABEL: &str = "overlay";
const TRIGGER_WINDOW_LABEL: &str = "trigger";
const DEFAULT_LLM_URL: &str = "http://localhost:8080/v1/chat/completions";
const DEFAULT_LLM_MODEL: &str = "mlx";

#[derive(Default)]
struct AssistantState {
    pending: Mutex<HashMap<String, ToolProposal>>,
    running: Mutex<HashMap<String, u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct LlmRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Debug, Deserialize)]
struct LlmChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct LlmResponse {
    choices: Vec<LlmChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssistantTurn {
    message: String,
    #[serde(default)]
    choices: Vec<String>,
    #[serde(default)]
    proposed_tool: Option<ToolProposalRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolProposalRequest {
    tool_id: ToolId,
    label: Option<String>,
    rationale: Option<String>,
    #[serde(default)]
    params: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolProposal {
    proposal_id: String,
    tool_id: ToolId,
    label: String,
    rationale: String,
    cwd: String,
    command_preview: Vec<String>,
    #[serde(default)]
    params: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
enum ToolId {
    ChatgptExportIncremental,
    ChatgptDoctor,
    AirCdeBackupIncremental,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssistantPayload {
    message: String,
    choices: Vec<String>,
    proposed_tool: Option<ToolProposal>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolLogPayload {
    run_id: String,
    tool_id: ToolId,
    line: String,
    is_error: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolDonePayload {
    run_id: String,
    tool_id: ToolId,
    success: bool,
    status: String,
}

#[tauri::command]
fn set_interaction_mode(app: AppHandle, mode: String) -> Result<(), String> {
    let interactive = match mode.as_str() {
        "interactive" => true,
        "passThrough" => false,
        other => return Err(format!("unsupported interaction mode: {other}")),
    };

    let Some(overlay) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return Ok(());
    };

    overlay
        .set_focusable(interactive)
        .map_err(|e| format!("failed to update overlay focus mode: {e}"))?;
    macos_shim::set_click_through(&overlay, !interactive)?;

    if interactive {
        overlay
            .set_focus()
            .map_err(|e| format!("failed to focus overlay: {e}"))?;
        macos_shim::bring_settings_window_to_front(&overlay)?;
    }

    Ok(())
}

#[tauri::command]
fn set_assistant_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    if let Some(overlay) = app.get_webview_window(OVERLAY_WINDOW_LABEL) {
        if visible {
            position_overlay_window(&app, &overlay)?;
            overlay
                .show()
                .map_err(|e| format!("failed to show overlay: {e}"))?;
            set_interaction_mode(app.clone(), "interactive".to_string())?;
            app.emit("assistant_visibility", true)
                .map_err(|e| format!("failed to emit visibility event: {e}"))?;
        } else {
            app.emit("assistant_visibility", false)
                .map_err(|e| format!("failed to emit visibility event: {e}"))?;
            set_interaction_mode(app.clone(), "passThrough".to_string())?;
            overlay
                .hide()
                .map_err(|e| format!("failed to hide overlay: {e}"))?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn send_chat_turn(
    app: AppHandle,
    state: State<'_, AssistantState>,
    messages: Vec<ChatMessage>,
) -> Result<(), String> {
    let result = request_assistant_turn(messages).await;
    match result {
        Ok(turn) => {
            app.emit("llm_delta", turn.message.clone())
                .map_err(|e| format!("failed to emit llm delta: {e}"))?;

            let proposal = match turn.proposed_tool {
                Some(request) => Some(create_tool_proposal(
                    request.tool_id,
                    request.label,
                    request.rationale,
                    request.params,
                )?),
                None => classify_tool_from_intent(&turn.message)
                    .map(|tool_id| create_tool_proposal(tool_id, None, None, HashMap::new()))
                    .transpose()?,
            };

            if let Some(proposal) = proposal.clone() {
                state
                    .pending
                    .lock()
                    .expect("pending proposal mutex poisoned")
                    .insert(proposal.proposal_id.clone(), proposal);
            }

            let payload = AssistantPayload {
                message: turn.message,
                choices: default_choices(turn.choices),
                proposed_tool: proposal,
            };
            app.emit("llm_done", payload)
                .map_err(|e| format!("failed to emit llm done: {e}"))?;
            Ok(())
        }
        Err(error) => {
            let fallback = AssistantPayload {
                message: format!(
                    "I could not reach the local MLX server. Check AIRASSISTANT_LLM_URL or start the server at {DEFAULT_LLM_URL}."
                ),
                choices: vec![
                    "Export ChatGPT Chats".to_string(),
                    "Backup Codex, Claude, and Antigravity".to_string(),
                ],
                proposed_tool: None,
            };
            app.emit("llm_error", error.clone())
                .map_err(|e| format!("failed to emit llm error: {e}"))?;
            app.emit("llm_done", fallback)
                .map_err(|e| format!("failed to emit fallback response: {e}"))?;
            Err(error)
        }
    }
}

#[tauri::command]
fn propose_tool(
    state: State<'_, AssistantState>,
    intent: String,
) -> Result<Option<ToolProposal>, String> {
    let Some(tool_id) = classify_tool_from_intent(&intent) else {
        return Ok(None);
    };
    let proposal = create_tool_proposal(tool_id, None, None, HashMap::new())?;
    state
        .pending
        .lock()
        .expect("pending proposal mutex poisoned")
        .insert(proposal.proposal_id.clone(), proposal.clone());
    Ok(Some(proposal))
}

#[tauri::command]
async fn approve_tool_run(
    app: AppHandle,
    state: State<'_, AssistantState>,
    proposal_id: String,
) -> Result<String, String> {
    let proposal = {
        let mut guard = state
            .pending
            .lock()
            .expect("pending proposal mutex poisoned");
        guard
            .remove(&proposal_id)
            .ok_or_else(|| "tool proposal was not found or already used".to_string())?
    };

    let run_id = unique_id("run");
    let command_spec = command_spec_for_tool(proposal.tool_id)?;
    if let Some(parent) = command_spec.output_parent {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create output folder {parent}: {e}"))?;
    }

    let mut child = Command::new(command_spec.program)
        .args(command_spec.args)
        .current_dir(command_spec.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start {}: {e}", proposal.label))?;

    if let Some(pid) = child.id() {
        state
            .running
            .lock()
            .expect("running process mutex poisoned")
            .insert(run_id.clone(), pid);
    }

    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(app.clone(), run_id.clone(), proposal.tool_id, stdout, false);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(app.clone(), run_id.clone(), proposal.tool_id, stderr, true);
    }

    let app_for_wait = app.clone();
    let run_for_wait = run_id.clone();
    tauri::async_runtime::spawn(async move {
        let status = child.wait().await;
        app_for_wait
            .state::<AssistantState>()
            .running
            .lock()
            .expect("running process mutex poisoned")
            .remove(&run_for_wait);

        let payload = match status {
            Ok(status) => ToolDonePayload {
                run_id: run_for_wait,
                tool_id: proposal.tool_id,
                success: status.success(),
                status: status.to_string(),
            },
            Err(error) => ToolDonePayload {
                run_id: run_for_wait,
                tool_id: proposal.tool_id,
                success: false,
                status: format!("failed to wait for process: {error}"),
            },
        };
        let _ = app_for_wait.emit("tool_done", payload);
    });

    Ok(run_id)
}

#[tauri::command]
fn cancel_tool_run(
    state: State<'_, AssistantState>,
    run_id: String,
) -> Result<(), String> {
    let pid = state
        .running
        .lock()
        .expect("running process mutex poisoned")
        .remove(&run_id)
        .ok_or_else(|| "tool run was not found or already finished".to_string())?;

    #[cfg(target_family = "unix")]
    {
        std::process::Command::new("/bin/kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .map_err(|e| format!("failed to cancel tool run: {e}"))?;
    }

    #[cfg(not(target_family = "unix"))]
    {
        return Err("cancel is only implemented for Unix-like systems".to_string());
    }

    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AssistantState::default())
        .on_menu_event(|app, event| {
            if let Err(error) = handle_tray_menu_event(app, event.id.as_ref()) {
                eprintln!("tray event error: {error}");
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            create_overlay_window(&app.handle())?;
            create_trigger_window(&app.handle())?;
            create_tray(&app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_interaction_mode,
            set_assistant_visible,
            send_chat_turn,
            propose_tool,
            approve_tool_run,
            cancel_tool_run,
            quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn create_overlay_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(existing) = app.get_webview_window(OVERLAY_WINDOW_LABEL) {
        return Ok(existing);
    }

    let overlay = WebviewWindowBuilder::new(
        app,
        OVERLAY_WINDOW_LABEL,
        WebviewUrl::App("index.html?view=overlay".into()),
    )
    .title("AirAssistant")
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .resizable(false)
    .focusable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()
    .map_err(|e| format!("failed to create overlay window: {e}"))?;

    overlay
        .set_visible_on_all_workspaces(true)
        .map_err(|e| format!("failed to set overlay workspace visibility: {e}"))?;
    position_overlay_window(app, &overlay)?;
    macos_shim::apply_overlay_window_behavior(&overlay)?;
    macos_shim::set_click_through(&overlay, true)?;
    Ok(overlay)
}

fn create_trigger_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(existing) = app.get_webview_window(TRIGGER_WINDOW_LABEL) {
        return Ok(existing);
    }

    let trigger = WebviewWindowBuilder::new(
        app,
        TRIGGER_WINDOW_LABEL,
        WebviewUrl::App("index.html?view=trigger".into()),
    )
    .title("AirAssistant Trigger")
    .inner_size(96.0, 96.0)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .resizable(false)
    .focusable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(true)
    .build()
    .map_err(|e| format!("failed to create trigger window: {e}"))?;

    trigger
        .set_visible_on_all_workspaces(true)
        .map_err(|e| format!("failed to set trigger workspace visibility: {e}"))?;
    position_trigger_window(app, &trigger)?;
    Ok(trigger)
}

fn position_overlay_window(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    let Some(display) = primary_display(app)? else {
        return Ok(());
    };
    let target_width = ((display.width as f64) * 0.78)
        .clamp(420.0, 840.0)
        .min(display.width as f64) as u32;
    let target_height = ((display.height as f64) * 0.58)
        .clamp(420.0, 660.0)
        .min(display.height as f64) as u32;
    let x = display.x + ((display.width.saturating_sub(target_width)) / 2) as i32;
    let y = display.y + 24;

    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| format!("failed to position overlay: {e}"))?;
    window
        .set_size(PhysicalSize::new(target_width, target_height))
        .map_err(|e| format!("failed to size overlay: {e}"))?;
    Ok(())
}

fn position_trigger_window(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    let Some(display) = primary_display(app)? else {
        return Ok(());
    };
    let size = 96_u32;
    let margin = 24_i32;
    let x = display.x + display.width as i32 - size as i32 - margin;
    let y = display.y + display.height as i32 - size as i32 - margin;
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| format!("failed to position trigger: {e}"))?;
    window
        .set_size(PhysicalSize::new(size, size))
        .map_err(|e| format!("failed to size trigger: {e}"))?;
    Ok(())
}

fn primary_display(app: &AppHandle) -> Result<Option<DisplayFrame>, String> {
    let monitor = app
        .primary_monitor()
        .map_err(|e| format!("failed to read primary display: {e}"))?;
    Ok(monitor.map(|monitor| {
        let work_area = monitor.work_area();
        DisplayFrame {
            x: work_area.position.x,
            y: work_area.position.y,
            width: work_area.size.width,
            height: work_area.size.height,
        }
    }))
}

#[derive(Debug)]
struct DisplayFrame {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn create_tray(app: &AppHandle) -> Result<(), String> {
    let menu = build_tray_menu(app)?;
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))
            .map_err(|e| format!("failed to attach tray menu: {e}"))?;
        return Ok(());
    }

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .icon_as_template(true);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone()).icon_as_template(true);
    }
    builder
        .build(app)
        .map_err(|e| format!("failed to build tray icon: {e}"))?;
    Ok(())
}

fn build_tray_menu(app: &AppHandle) -> Result<Menu<tauri::Wry>, String> {
    let open_item = MenuItem::with_id(app, "open", "Open AirAssistant", true, None::<&str>)
        .map_err(|e| format!("failed to create open menu item: {e}"))?;
    let hide_item = MenuItem::with_id(app, "hide", "Hide AirAssistant", true, None::<&str>)
        .map_err(|e| format!("failed to create hide menu item: {e}"))?;
    let llm_item = MenuItem::with_id(
        app,
        "llm_status",
        format!("LLM: {}", llm_url()),
        false,
        None::<&str>,
    )
    .map_err(|e| format!("failed to create LLM status menu item: {e}"))?;
    let separator =
        PredefinedMenuItem::separator(app).map_err(|e| format!("failed to create separator: {e}"))?;
    let quit_item =
        MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)
            .map_err(|e| format!("failed to create quit menu item: {e}"))?;

    Menu::with_items(app, &[&open_item, &hide_item, &llm_item, &separator, &quit_item])
        .map_err(|e| format!("failed to build tray menu: {e}"))
}

fn handle_tray_menu_event(app: &AppHandle, event_id: &str) -> Result<(), String> {
    match event_id {
        "open" => set_assistant_visible(app.clone(), true),
        "hide" => set_assistant_visible(app.clone(), false),
        "quit" => {
            app.exit(0);
            Ok(())
        }
        _ => Ok(()),
    }
}

async fn request_assistant_turn(messages: Vec<ChatMessage>) -> Result<AssistantTurn, String> {
    let mut request_messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system_prompt(),
    }];
    request_messages.extend(messages);

    let request = LlmRequest {
        model: llm_model(),
        messages: request_messages,
        temperature: 0.4,
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;
    let response = client
        .post(llm_url())
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("failed to connect to local MLX server: {e}"))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("failed to read local MLX response: {e}"))?;
    if !status.is_success() {
        return Err(format!("local MLX server returned {status}: {text}"));
    }

    let content = serde_json::from_str::<LlmResponse>(&text)
        .ok()
        .and_then(|parsed| parsed.choices.into_iter().next())
        .map(|choice| choice.message.content)
        .unwrap_or(text);

    parse_assistant_turn(&content)
}

fn parse_assistant_turn(content: &str) -> Result<AssistantTurn, String> {
    let trimmed = content.trim();
    if let Ok(turn) = serde_json::from_str::<AssistantTurn>(trimmed) {
        return Ok(turn);
    }

    let json_block = trimmed
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
        .or_else(|| trimmed.strip_prefix("```").and_then(|value| value.strip_suffix("```")));
    if let Some(block) = json_block {
        if let Ok(turn) = serde_json::from_str::<AssistantTurn>(block.trim()) {
            return Ok(turn);
        }
    }

    Ok(AssistantTurn {
        message: trimmed.to_string(),
        choices: Vec::new(),
        proposed_tool: None,
    })
}

fn default_choices(choices: Vec<String>) -> Vec<String> {
    let filtered = choices
        .into_iter()
        .map(|choice| choice.trim().to_string())
        .filter(|choice| !choice.is_empty())
        .take(2)
        .collect::<Vec<_>>();

    if filtered.is_empty() {
        vec![
            "Export ChatGPT Chats".to_string(),
            "Backup Codex, Claude, and Antigravity".to_string(),
        ]
    } else {
        filtered
    }
}

fn system_prompt() -> String {
    [
        "You are AirAssistant, a concise local macOS AI operator.",
        "Return only JSON matching this TypeScript shape:",
        r#"{"message":"short user-facing reply","choices":["choice one","choice two"],"proposedTool":{"toolId":"chatgpt_export_incremental|chatgpt_doctor|air_cde_backup_incremental","label":"optional","rationale":"optional","params":{}}}"#,
        "Offer at most two choices. The UI always provides a third free-text option.",
        "Only propose a tool when the user clearly asks to export, back up, diagnose, or run a supported local workflow.",
        "Never invent unsupported tool IDs. Never ask for secrets or tokens.",
    ]
    .join(" ")
}

fn llm_url() -> String {
    env::var("AIRASSISTANT_LLM_URL").unwrap_or_else(|_| DEFAULT_LLM_URL.to_string())
}

fn llm_model() -> String {
    env::var("AIRASSISTANT_LLM_MODEL").unwrap_or_else(|_| DEFAULT_LLM_MODEL.to_string())
}

fn classify_tool_from_intent(intent: &str) -> Option<ToolId> {
    let lowered = intent.to_ascii_lowercase();
    if lowered.contains("doctor") && lowered.contains("chatgpt") {
        return Some(ToolId::ChatgptDoctor);
    }
    if lowered.contains("chatgpt") && (lowered.contains("export") || lowered.contains("backup")) {
        return Some(ToolId::ChatgptExportIncremental);
    }
    if lowered.contains("codex")
        || lowered.contains("claude")
        || lowered.contains("antigravity")
        || lowered.contains("memory")
        || lowered.contains("knowledge")
    {
        return Some(ToolId::AirCdeBackupIncremental);
    }
    None
}

fn create_tool_proposal(
    tool_id: ToolId,
    label: Option<String>,
    rationale: Option<String>,
    params: HashMap<String, String>,
) -> Result<ToolProposal, String> {
    let spec = command_spec_for_tool(tool_id)?;
    Ok(ToolProposal {
        proposal_id: unique_id("proposal"),
        tool_id,
        label: label.unwrap_or_else(|| spec.label.to_string()),
        rationale: rationale.unwrap_or_else(|| spec.rationale.to_string()),
        cwd: spec.cwd.to_string(),
        command_preview: std::iter::once(spec.program.to_string())
            .chain(spec.args.iter().map(|arg| arg.to_string()))
            .collect(),
        params,
    })
}

struct CommandSpec<'a> {
    label: &'a str,
    rationale: &'a str,
    cwd: &'a str,
    program: &'a str,
    args: &'a [&'a str],
    output_parent: Option<&'a str>,
}

fn command_spec_for_tool(tool_id: ToolId) -> Result<CommandSpec<'static>, String> {
    let spec = match tool_id {
        ToolId::ChatgptExportIncremental => CommandSpec {
            label: "Export ChatGPT Chats",
            rationale: "Runs the local ChatGPT Download Engine incremental archive workflow.",
            cwd: "/Users/macbookpro/Developer/chatgpt-download-engine",
            program: "/Users/macbookpro/Developer/chatgpt-download-engine/scripts/download-incremental.sh",
            args: &[],
            output_parent: None,
        },
        ToolId::ChatgptDoctor => CommandSpec {
            label: "Check ChatGPT Export Setup",
            rationale: "Runs the ChatGPT Download Engine doctor command without exporting data.",
            cwd: "/Users/macbookpro/Developer/chatgpt-download-engine",
            program: "python3",
            args: &["-m", "chatgpt_download_engine", "doctor"],
            output_parent: None,
        },
        ToolId::AirCdeBackupIncremental => CommandSpec {
            label: "Backup Codex, Claude, and Antigravity",
            rationale: "Runs air-cde-2 incrementally and packages the local developer-agent archive.",
            cwd: "/Users/macbookpro/Documents/antigravity/fervent-galileo/air-cde-2",
            program: "node",
            args: &[
                "bin/cli.js",
                "--incremental",
                "--zip",
                "--output",
                "/Users/macbookpro/Documents/antigravity/lively-tesla/exports/air-cde",
            ],
            output_parent: Some("/Users/macbookpro/Documents/antigravity/lively-tesla/exports"),
        },
    };

    validate_command_spec(&spec)?;
    Ok(spec)
}

fn validate_command_spec(spec: &CommandSpec<'_>) -> Result<(), String> {
    let cwd = Path::new(spec.cwd);
    if !cwd.is_absolute() || spec.cwd.contains("..") {
        return Err(format!("invalid tool cwd: {}", spec.cwd));
    }
    if spec.program.contains("..") || spec.args.iter().any(|arg| arg.contains("..")) {
        return Err("tool command failed path traversal validation".to_string());
    }
    Ok(())
}

fn spawn_log_reader<R>(
    app: AppHandle,
    run_id: String,
    tool_id: ToolId,
    stream: R,
    is_error: bool,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app.emit(
                "tool_log",
                ToolLogPayload {
                    run_id: run_id.clone(),
                    tool_id,
                    line,
                    is_error,
                },
            );
        }
    });
}

fn unique_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("{prefix}-{millis}")
}
