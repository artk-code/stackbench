type DesktopState = Awaited<ReturnType<typeof window.workbench.getState>>;
type RunListResponse = Awaited<ReturnType<typeof window.workbench.listRuns>>;
type RunRecord = RunListResponse["runs"][number];
type RunLogsResponse = Awaited<ReturnType<typeof window.workbench.getRunLogs>>;
type RunLogRecord = RunLogsResponse["logs"][number];
type AdapterStatus = Awaited<
  ReturnType<typeof window.workbench.getAdapterAuthStatus>
>["adapters"][number];
type WatchEvent = Parameters<
  Parameters<typeof window.workbench.onLauncherWatchEvent>[0]
>[0];

const state = {
  desktop: null as DesktopState | null,
  adapters: [] as AdapterStatus[],
  runs: [] as RunRecord[],
  selectedRunId: "" as string,
  logs: [] as RunLogRecord[],
  feed: [] as string[],
};

const repoRootEl = requiredElement<HTMLDivElement>("repo-root");
const runtimeModeEl = requiredElement<HTMLDivElement>("runtime-mode");
const nextStepEl = requiredElement<HTMLDivElement>("next-step");
const launcherStatusEl = requiredElement<HTMLDivElement>("launcher-status");
const watchFeedEl = requiredElement<HTMLPreElement>("watch-feed");
const authCardsEl = requiredElement<HTMLDivElement>("auth-cards");
const runStartStatusEl = requiredElement<HTMLDivElement>("run-start-status");
const runsTableEl = requiredElement<HTMLDivElement>("runs-table");
const selectedRunMetaEl = requiredElement<HTMLDivElement>("selected-run-meta");
const logsMetaEl = requiredElement<HTMLDivElement>("logs-meta");
const logsViewEl = requiredElement<HTMLDivElement>("logs-view");
const taskIdInput = requiredElement<HTMLInputElement>("task-id");
const workflowInput = requiredElement<HTMLInputElement>("workflow-name");
const adapterInput = requiredElement<HTMLInputElement>("adapter-name");
const promptInput = requiredElement<HTMLTextAreaElement>("prompt");
const runActionNoteInput = requiredElement<HTMLInputElement>("run-action-note");
const logLimitInput = requiredElement<HTMLInputElement>("log-limit");
const watchIntervalInput = requiredElement<HTMLInputElement>("watch-interval");
const approveRunButton = requiredElement<HTMLButtonElement>("approve-run");
const rejectRunButton = requiredElement<HTMLButtonElement>("reject-run");
const integrateRunButton = requiredElement<HTMLButtonElement>("integrate-run");

document
  .getElementById("choose-repo")
  ?.addEventListener("click", async () => {
    updateDesktopState(await window.workbench.selectRepoRoot());
    state.selectedRunId = "";
    state.logs = [];
    state.feed = [];
    watchFeedEl.textContent = "";
    await refreshAll();
  });
document.getElementById("refresh-all")?.addEventListener("click", () => refreshAll());
document.getElementById("run-once")?.addEventListener("click", async () => {
  try {
    const report = await window.workbench.launcherRunOnce();
    setLauncherStatus(
      `Launcher once: considered ${report.considered}, awaiting_review ${report.awaiting_review}, failed ${report.failed}.`,
    );
    await refreshRunsAndLogs();
  } catch (error) {
    setLauncherStatus(messageOf(error), true);
  }
});
document.getElementById("start-watch")?.addEventListener("click", async () => {
  try {
    const next = await window.workbench.launcherWatchStart(Number(watchIntervalInput.value) || 1000);
    updateDesktopState(next);
    appendFeed("launcher watch started");
  } catch (error) {
    setLauncherStatus(messageOf(error), true);
  }
});
document.getElementById("stop-watch")?.addEventListener("click", async () => {
  const next = await window.workbench.launcherWatchStop();
  updateDesktopState(next);
  appendFeed("launcher watch stopped");
});
document
  .getElementById("start-run-form")
  ?.addEventListener("submit", async (event) => {
    event.preventDefault();
    try {
      const response = await window.workbench.startRun({
        taskId: taskIdInput.value.trim(),
        workflow: optionalValue(workflowInput.value),
        adapter: optionalValue(adapterInput.value),
        prompt: optionalValue(promptInput.value),
      });
      runStartStatusEl.textContent = `Dispatched ${response.run_id} on ${response.adapter}.`;
      taskIdInput.value = "";
      promptInput.value = "";
      await refreshRunsAndLogs(response.run_id);
    } catch (error) {
      runStartStatusEl.textContent = messageOf(error);
    }
  });
approveRunButton.addEventListener("click", () => {
  void actOnSelectedRun("approve");
});
rejectRunButton.addEventListener("click", () => {
  void actOnSelectedRun("reject");
});
integrateRunButton.addEventListener("click", () => {
  void actOnSelectedRun("integrate");
});

window.workbench.onLauncherWatchEvent((event) => {
  appendFeed(renderWatchEvent(event));
  if (event.type === "cycle" || event.type === "summary") {
    void refreshRunsAndLogs(state.selectedRunId || undefined);
  }
  if (event.type === "exit" && state.desktop) {
    updateDesktopState({ ...state.desktop, watchRunning: false });
  }
});

void refreshAll();
window.setInterval(() => {
  void refreshRunsAndLogs(state.selectedRunId || undefined);
}, 5000);

async function refreshAll(): Promise<void> {
  updateDesktopState(await window.workbench.getState());
  await Promise.all([refreshAdapters(), refreshRunsAndLogs(state.selectedRunId || undefined)]);
}

async function refreshAdapters(): Promise<void> {
  try {
    const response = await window.workbench.getAdapterAuthStatus();
    state.adapters = response.adapters;
    if (!adapterInput.value.trim() && state.adapters[0]) {
      adapterInput.value = state.adapters[0].name;
    }
    renderAuthCards();
  } catch (error) {
    authCardsEl.textContent = messageOf(error);
  }
}

async function refreshRunsAndLogs(nextSelectedRunId?: string): Promise<void> {
  try {
    const response = await window.workbench.listRuns();
    state.runs = response.runs;
    if (nextSelectedRunId) {
      state.selectedRunId = nextSelectedRunId;
    } else if (!state.selectedRunId && state.runs[0]) {
      state.selectedRunId = state.runs[0].run_id;
    } else if (
      state.selectedRunId &&
      !state.runs.some((run) => run.run_id === state.selectedRunId)
    ) {
      state.selectedRunId = state.runs[0]?.run_id ?? "";
    }
    renderRuns();
    if (state.selectedRunId) {
      const logs = await window.workbench.getRunLogs({
        runId: state.selectedRunId,
        limit: Number(logLimitInput.value) || 200,
      });
      state.logs = logs.logs;
    } else {
      state.logs = [];
    }
    renderLogs();
  } catch (error) {
    runsTableEl.textContent = messageOf(error);
  }
}

function updateDesktopState(next: DesktopState): void {
  state.desktop = next;
  repoRootEl.textContent = next.repoRoot;
  runtimeModeEl.textContent = `${next.runtimeMode} runtime${next.watchRunning ? " | watch running" : ""}`;
  setLauncherStatus(
    next.watchRunning
      ? "The watch is draining queued work in the background."
      : "Run a single pulse or start the watch when you want the queue moving.",
  );
  renderNextStep();
}

function renderAuthCards(): void {
  authCardsEl.replaceChildren();
  if (state.adapters.length === 0) {
    authCardsEl.textContent = "No adapters configured.";
    return;
  }

  for (const adapter of state.adapters) {
    const card = document.createElement("article");
    card.className = "auth-card";

    const head = document.createElement("div");
    head.className = "auth-head";
    const title = document.createElement("strong");
    title.textContent = adapter.name;
    head.appendChild(title);
    head.appendChild(makeTag(statusLabel(adapter), statusKind(adapter)));

    const actions = document.createElement("div");
    actions.className = "actions";
    actions.appendChild(
      makeButton("Refresh", "secondary", async () => {
        await refreshAdapters();
      }),
    );
    actions.appendChild(
      makeButton("Login", "secondary", async () => {
        await triggerLogin(adapter.name, false);
      }, !adapter.login_supported),
    );
    actions.appendChild(
      makeButton("Device Login", "", async () => {
        await triggerLogin(adapter.name, true);
      }, !adapter.device_login_supported),
    );

    card.append(head, actions, detailBlock(authDetail(adapter)));
    authCardsEl.appendChild(card);
  }
}

function renderRuns(): void {
  runsTableEl.replaceChildren();
  if (state.runs.length === 0) {
    runsTableEl.textContent = "No runs yet.";
    return;
  }

  for (const run of state.runs) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = `run-row${run.run_id === state.selectedRunId ? " selected" : ""}`;
    row.addEventListener("click", async () => {
      state.selectedRunId = run.run_id;
      renderRuns();
      await refreshRunsAndLogs(run.run_id);
    });

    const main = document.createElement("div");
    main.className = "run-main";
    const title = document.createElement("strong");
    title.textContent = `${run.task_id} • ${run.adapter}`;
    main.append(title, makeTag(run.state, stateTagKind(run.state)));

    const meta = document.createElement("div");
    meta.className = "run-meta";
    meta.textContent = `${run.run_id} | workflow ${run.workflow} | updated ${run.updated_at}`;

    row.append(main, meta);
    runsTableEl.appendChild(row);
  }
}

function renderLogs(): void {
  logsViewEl.replaceChildren();
  const selectedRun = selectedRunRecord();
  if (selectedRun) {
    selectedRunMetaEl.textContent =
      `${selectedRun.task_id} is ${selectedRun.state} on ${selectedRun.adapter}. Last recorded event: ${selectedRun.last_event_kind}.`;
  } else {
    selectedRunMetaEl.textContent = "Select a run to review its timeline and decide what happens next.";
  }
  syncRunActionButtons(selectedRun);
  renderNextStep();
  if (!state.selectedRunId) {
    logsMetaEl.textContent = "Select a run to inspect its canonical timeline.";
    logsViewEl.textContent = "";
    return;
  }
  logsMetaEl.textContent = `${state.selectedRunId} • ${state.logs.length} events`;
  if (state.logs.length === 0) {
    logsViewEl.textContent = "No logs yet.";
    return;
  }

  for (const record of state.logs) {
    const item = document.createElement("article");
    item.className = "log-item";
    const head = document.createElement("div");
    head.className = "log-head";
    const kind = document.createElement("strong");
    kind.textContent = record.envelope.kind;
    const ts = document.createElement("span");
    ts.textContent = `${record.envelope.ts} • #${record.entry_id}`;
    head.append(kind, ts);

    const body = document.createElement("div");
    body.className = "log-body";
    body.textContent = JSON.stringify(record.envelope.payload, null, 2);

    item.append(head, body);
    logsViewEl.appendChild(item);
  }
}

async function triggerLogin(adapter: string, device: boolean): Promise<void> {
  try {
    const result = await window.workbench.loginAdapter({ adapter, device });
    appendFeed(
      `${adapter} ${device ? "device login" : "login"}: ${result.success ? "ok" : "failed"}${result.detail ? ` | ${result.detail}` : ""}${result.command ? ` | ${result.command}` : ""}`,
    );
    await refreshAdapters();
  } catch (error) {
    appendFeed(messageOf(error));
  }
}

async function actOnSelectedRun(action: "approve" | "reject" | "integrate"): Promise<void> {
  const run = selectedRunRecord();
  if (!run) {
    appendFeed("select a run first");
    return;
  }

  const note = optionalValue(runActionNoteInput.value);
  try {
    if (action === "approve") {
      const result = await window.workbench.approveRun({ runId: run.run_id, reason: note });
      appendFeed(`approved ${result.run.run_id}`);
    } else if (action === "reject") {
      const result = await window.workbench.rejectRun({ runId: run.run_id, reason: note });
      appendFeed(`rejected ${result.run.run_id}`);
    } else {
      const result = await window.workbench.integrateRun({ runId: run.run_id, message: note });
      appendFeed(
        `integrated ${result.run.run_id} as ${result.integration.change_id}${result.integration.detail ? ` | ${result.integration.detail}` : ""}`,
      );
    }
    runActionNoteInput.value = "";
    await refreshRunsAndLogs(run.run_id);
  } catch (error) {
    appendFeed(`${action} failed: ${messageOf(error)}`);
  }
}

function setLauncherStatus(message: string, danger = false): void {
  launcherStatusEl.textContent = message;
  launcherStatusEl.style.color = danger ? "var(--danger)" : "var(--muted)";
}

function appendFeed(message: string): void {
  state.feed.unshift(message);
  state.feed = state.feed.slice(0, 18);
  watchFeedEl.textContent = state.feed.join("\n");
}

function renderWatchEvent(event: WatchEvent): string {
  switch (event.type) {
    case "cycle":
      return `cycle ${event.cycle}: considered ${event.considered}, review ${event.awaiting_review}, failed ${event.failed}`;
    case "summary":
      return `summary: cycles ${event.cycles}, review ${event.awaiting_review}, failed ${event.failed}`;
    case "stderr":
      return `stderr: ${event.line}`;
    case "stdout":
      return `stdout: ${event.line}`;
    case "error":
      return `error: ${event.message}`;
    case "exit":
      return `watch exited with code ${event.code ?? "signal"}`;
    default:
      return JSON.stringify(event);
  }
}

function makeTag(label: string, kind: "success" | "warning" | "danger" | "neutral"): HTMLSpanElement {
  const tag = document.createElement("span");
  tag.className = `tag${kind === "neutral" ? "" : ` ${kind}`}`;
  tag.textContent = label;
  return tag;
}

function makeButton(
  label: string,
  className: string,
  onClick: () => Promise<void>,
  disabled = false,
): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = label;
  button.disabled = disabled;
  if (className) {
    button.className = className;
  }
  button.addEventListener("click", () => {
    void onClick();
  });
  return button;
}

function detailBlock(text: string): HTMLDivElement {
  const detail = document.createElement("div");
  detail.className = "detail";
  detail.textContent = text;
  return detail;
}

function statusLabel(adapter: AdapterStatus): string {
  if (!adapter.available) {
    return "missing";
  }
  if (adapter.logged_in === true) {
    return `logged in${adapter.auth_method ? ` (${adapter.auth_method})` : ""}`;
  }
  if (adapter.logged_in === false) {
    return "login required";
  }
  return "status unknown";
}

function authDetail(adapter: AdapterStatus): string {
  const lines = [adapter.detail];
  if (adapter.login_command) {
    lines.push(`login command: ${adapter.login_command}`);
  }
  if (adapter.device_login_command) {
    lines.push(`device login command: ${adapter.device_login_command}`);
  }
  return lines.filter((line) => line.trim() !== "").join("\n");
}

function statusKind(adapter: AdapterStatus): "success" | "warning" | "danger" | "neutral" {
  if (!adapter.available) {
    return "danger";
  }
  if (adapter.logged_in === true) {
    return "success";
  }
  if (adapter.logged_in === false) {
    return "warning";
  }
  return "neutral";
}

function selectedRunRecord(): RunRecord | undefined {
  return state.runs.find((run) => run.run_id === state.selectedRunId);
}

function renderNextStep(): void {
  const run = selectedRunRecord();
  if (run?.state === "awaiting_review") {
    nextStepEl.textContent = "Read the timeline and decide whether this run deserves approval.";
    return;
  }
  if (run?.state === "approved") {
    nextStepEl.textContent = "The run is approved. Integrate it when you are ready to land the change.";
    return;
  }
  if (run?.state === "failed" || run?.state === "rejected") {
    nextStepEl.textContent = "Use the log trail to see what broke, then dispatch a tighter follow-up run.";
    return;
  }
  if (state.desktop?.watchRunning) {
    nextStepEl.textContent = "The watch is active. Keep reviewing while new runs move forward.";
    return;
  }
  if (state.runs.length > 0) {
    nextStepEl.textContent = "Select a run from the board to inspect its trail and make the next call.";
    return;
  }
  nextStepEl.textContent = "Choose a workspace, check adapter readiness, and dispatch the first run.";
}

function syncRunActionButtons(run: RunRecord | undefined): void {
  approveRunButton.disabled = !run || !["awaiting_review", "rejected"].includes(run.state);
  rejectRunButton.disabled = !run || !["awaiting_review", "approved"].includes(run.state);
  integrateRunButton.disabled = !run || run.state !== "approved";
}

function stateTagKind(stateValue: string): "success" | "warning" | "danger" | "neutral" {
  if (stateValue === "awaiting_review" || stateValue === "approved" || stateValue === "integrated") {
    return "success";
  }
  if (stateValue === "failed" || stateValue === "rejected" || stateValue === "cancelled") {
    return "danger";
  }
  if (stateValue === "running" || stateValue === "evaluating") {
    return "warning";
  }
  return "neutral";
}

function requiredElement<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`missing element: ${id}`);
  }
  return element as T;
}

function optionalValue(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed === "" ? undefined : trimmed;
}

function messageOf(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
