import { contextBridge, ipcRenderer } from "electron";

type DesktopState = {
  repoRoot: string;
  runtimeMode: "cargo" | "binary";
  watchRunning: boolean;
};

type AdapterAuthStatus = {
  adapters: Array<{
    name: string;
    command: string;
    available: boolean;
    logged_in: boolean | null;
    auth_method: string | null;
    login_supported: boolean;
    device_login_supported: boolean;
    login_command: string | null;
    device_login_command: string | null;
    detail: string;
  }>;
};

type AdapterLoginResult = {
  name: string;
  mode: string;
  available: boolean;
  success: boolean;
  exit_code: number;
  command: string | null;
  stdout: string;
  stderr: string;
  detail: string;
};

type RunRecord = {
  run_id: string;
  task_id: string;
  workflow: string;
  adapter: string;
  profile_id: string | null;
  gstack_id: string | null;
  gstack_fingerprint: string | null;
  state: string;
  created_at: string;
  updated_at: string;
  prompt: string | null;
  last_event_kind: string;
  last_error: string | null;
};

type RunListResponse = {
  runs: RunRecord[];
};

type RunStartResponse = {
  status: string;
  run_id: string;
  task_id: string;
  workflow: string;
  adapter: string;
  profile_id: string | null;
  gstack_id: string | null;
  gstack_fingerprint: string | null;
  queue_entry: number;
};

type WorkerTypeRecord = {
  id: string;
  display_name: string;
  description: string;
  workflow: string | null;
  adapter: string | null;
  gstack_id: string;
  file_path: string;
  instructions_markdown: string;
};

type WorkerTypeListResponse = {
  profiles: WorkerTypeRecord[];
};

type WorkerTypeSaveResponse = {
  profile: WorkerTypeRecord;
};

type RunLogRecord = {
  entry_id: number;
  applied_at: string;
  envelope: {
    run_id: string;
    ts: string;
    kind: string;
    payload: unknown;
  };
};

type RunLogsResponse = {
  run_id: string;
  logs: RunLogRecord[];
};

type RunMutationResponse = {
  run: RunRecord;
};

type RunIntegrateResponse = {
  run: RunRecord;
  integration: {
    workspace_root: string;
    change_id: string;
    detail: string;
  };
};

type LaunchReport = {
  considered: number;
  awaiting_review: number;
  failed: number;
  skipped: number;
};

type WatchEvent =
  | { type: "cycle"; cycle: number; considered: number; awaiting_review: number; failed: number; skipped: number }
  | { type: "summary"; cycles: number; considered: number; awaiting_review: number; failed: number; skipped: number }
  | { type: "stderr"; line: string }
  | { type: "stdout"; line: string }
  | { type: "error"; message: string }
  | { type: "exit"; code: number | null };

const api = {
  getState(): Promise<DesktopState> {
    return ipcRenderer.invoke("swb:get-state");
  },
  selectRepoRoot(): Promise<DesktopState> {
    return ipcRenderer.invoke("swb:select-repo-root");
  },
  listRuns(): Promise<RunListResponse> {
    return ipcRenderer.invoke("swb:run-list");
  },
  startRun(payload: {
    taskId: string;
    workflow?: string;
    adapter?: string;
    profile?: string;
    prompt?: string;
  }): Promise<RunStartResponse> {
    return ipcRenderer.invoke("swb:run-start", payload);
  },
  listWorkerTypes(): Promise<WorkerTypeListResponse> {
    return ipcRenderer.invoke("swb:profile-list");
  },
  saveWorkerType(payload: {
    id: string;
    displayName: string;
    description?: string;
    workflow?: string;
    adapter?: string;
    gstackId?: string;
    instructionsMarkdown?: string;
  }): Promise<WorkerTypeSaveResponse> {
    return ipcRenderer.invoke("swb:profile-save", payload);
  },
  getRunLogs(payload: { runId: string; limit: number }): Promise<RunLogsResponse> {
    return ipcRenderer.invoke("swb:run-logs", payload);
  },
  approveRun(payload: { runId: string; reason?: string }): Promise<RunMutationResponse> {
    return ipcRenderer.invoke("swb:run-approve", payload);
  },
  rejectRun(payload: { runId: string; reason?: string }): Promise<RunMutationResponse> {
    return ipcRenderer.invoke("swb:run-reject", payload);
  },
  integrateRun(payload: { runId: string; message?: string }): Promise<RunIntegrateResponse> {
    return ipcRenderer.invoke("swb:run-integrate", payload);
  },
  getAdapterAuthStatus(): Promise<AdapterAuthStatus> {
    return ipcRenderer.invoke("swb:adapter-auth-status");
  },
  loginAdapter(payload: { adapter: string; device: boolean }): Promise<AdapterLoginResult> {
    return ipcRenderer.invoke("swb:adapter-auth-login", payload);
  },
  launcherRunOnce(): Promise<LaunchReport> {
    return ipcRenderer.invoke("swb:launcher-run-once");
  },
  launcherWatchStart(intervalMs: number): Promise<DesktopState> {
    return ipcRenderer.invoke("swb:launcher-watch-start", { intervalMs });
  },
  launcherWatchStop(): Promise<DesktopState> {
    return ipcRenderer.invoke("swb:launcher-watch-stop");
  },
  onLauncherWatchEvent(listener: (event: WatchEvent) => void): () => void {
    const handler = (_event: Electron.IpcRendererEvent, payload: WatchEvent) => listener(payload);
    ipcRenderer.on("swb:launcher-watch-event", handler);
    return () => {
      ipcRenderer.off("swb:launcher-watch-event", handler);
    };
  },
};

contextBridge.exposeInMainWorld("workbench", api);

export type WorkbenchDesktopApi = typeof api;

declare global {
  interface Window {
    workbench: WorkbenchDesktopApi;
  }
}
