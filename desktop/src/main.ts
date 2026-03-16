import { spawn, type ChildProcessByStdio } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { createInterface } from "node:readline";
import type { Readable } from "node:stream";

import { app, BrowserWindow, dialog, ipcMain } from "electron";

type SwbRuntime = {
  command: string;
  baseArgs: string[];
  cwd: string;
};

type DesktopState = {
  repoRoot: string;
  runtimeMode: "cargo" | "binary";
  watchRunning: boolean;
};

type WatchProcess = ChildProcessByStdio<null, Readable, Readable>;

let mainWindow: BrowserWindow | null = null;
let selectedRepoRoot = defaultRepoRoot();
let watchProcess: WatchProcess | null = null;
const mainWindowViteDevServerUrl =
  typeof MAIN_WINDOW_VITE_DEV_SERVER_URL !== "undefined"
    ? MAIN_WINDOW_VITE_DEV_SERVER_URL
    : undefined;

function desktopAppRoot(): string {
  return process.env.SWB_DESKTOP_APP_ROOT ?? app.getAppPath();
}

function swbWorkspaceRoot(): string {
  return process.env.SWB_DESKTOP_WORKSPACE_ROOT ?? resolve(desktopAppRoot(), "..");
}

function defaultRepoRoot(): string {
  return process.env.SWB_DESKTOP_REPO_ROOT ?? swbWorkspaceRoot();
}

function resolveSwbRuntime(): SwbRuntime {
  const explicitBinary = process.env.SWB_DESKTOP_BIN;
  if (explicitBinary) {
    return {
      command: explicitBinary,
      baseArgs: [],
      cwd: swbWorkspaceRoot(),
    };
  }

  const packagedBinary = resolve(process.resourcesPath, "bin", "swb");
  if (app.isPackaged && existsSync(packagedBinary)) {
    return {
      command: packagedBinary,
      baseArgs: [],
      cwd: process.resourcesPath,
    };
  }

  return {
    command: "cargo",
    baseArgs: ["run", "-q", "-p", "swb-cli", "--"],
    cwd: swbWorkspaceRoot(),
  };
}

function currentDesktopState(): DesktopState {
  const runtime = resolveSwbRuntime();
  return {
    repoRoot: selectedRepoRoot,
    runtimeMode: runtime.command === "cargo" ? "cargo" : "binary",
    watchRunning: watchProcess !== null,
  };
}

function createMainWindow(): void {
  mainWindow = new BrowserWindow({
    width: 1420,
    height: 920,
    minWidth: 1120,
    minHeight: 760,
    backgroundColor: "#f2ede3",
    title: "Stackbench",
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      preload: resolve(desktopAppRoot(), ".vite/build/preload.cjs"),
    },
  });

  if (mainWindowViteDevServerUrl) {
    mainWindow.loadURL(mainWindowViteDevServerUrl).catch(console.error);
    mainWindow.webContents.openDevTools({ mode: "detach" });
  } else {
    mainWindow
      .loadFile(resolve(desktopAppRoot(), ".vite/renderer/main_window/index.html"))
      .catch(console.error);
  }
}

async function runSwbJson(args: string[]): Promise<unknown> {
  const runtime = resolveSwbRuntime();
  return new Promise((resolveResult, rejectResult) => {
    const child = spawn(runtime.command, [...runtime.baseArgs, ...args, "--json"], {
      cwd: runtime.cwd,
      env: {
        ...process.env,
        SWB_ROOT: selectedRepoRoot,
      },
      stdio: ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";

    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", (error) => {
      rejectResult(error);
    });
    child.on("close", (code) => {
      if (code !== 0) {
        rejectResult(new Error(stderr.trim() || stdout.trim() || `swb exited with code ${code}`));
        return;
      }
      try {
        resolveResult(JSON.parse(stdout.trim() || "{}"));
      } catch (error) {
        rejectResult(error);
      }
    });
  });
}

function sendWatchEvent(payload: unknown): void {
  mainWindow?.webContents.send("swb:launcher-watch-event", payload);
}

async function startLauncherWatch(intervalMs: number): Promise<DesktopState> {
  if (watchProcess) {
    return currentDesktopState();
  }

  const runtime = resolveSwbRuntime();
  const child = spawn(
    runtime.command,
    [
      ...runtime.baseArgs,
      "launcher",
      "watch",
      "--interval-ms",
      String(intervalMs),
      "--json",
    ],
    {
      cwd: runtime.cwd,
      env: {
        ...process.env,
        SWB_ROOT: selectedRepoRoot,
      },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  watchProcess = child;

  const stdoutLines = createInterface({ input: child.stdout });
  stdoutLines.on("line", (line) => {
    if (line.trim() === "") {
      return;
    }
    try {
      sendWatchEvent(JSON.parse(line));
    } catch {
      sendWatchEvent({ type: "stdout", line });
    }
  });

  const stderrLines = createInterface({ input: child.stderr });
  stderrLines.on("line", (line) => {
    if (line.trim() !== "") {
      sendWatchEvent({ type: "stderr", line });
    }
  });

  child.on("close", (code) => {
    watchProcess = null;
    sendWatchEvent({ type: "exit", code });
  });
  child.on("error", (error) => {
    watchProcess = null;
    sendWatchEvent({ type: "error", message: error.message });
  });

  return currentDesktopState();
}

async function stopLauncherWatch(): Promise<DesktopState> {
  if (watchProcess) {
    watchProcess.kill();
    watchProcess = null;
  }
  return currentDesktopState();
}

app.whenReady().then(() => {
  ipcMain.handle("swb:get-state", async () => currentDesktopState());
  ipcMain.handle("swb:select-repo-root", async () => {
    const result = await dialog.showOpenDialog({
      defaultPath: selectedRepoRoot,
      properties: ["openDirectory"],
    });
    if (!result.canceled && result.filePaths[0]) {
      if (watchProcess && result.filePaths[0] !== selectedRepoRoot) {
        await stopLauncherWatch();
      }
      selectedRepoRoot = result.filePaths[0];
    }
    return currentDesktopState();
  });
  ipcMain.handle("swb:run-list", async () => runSwbJson(["run", "list"]));
  ipcMain.handle("swb:run-start", async (_, payload: { taskId: string; workflow?: string; adapter?: string; prompt?: string }) => {
    const args = ["run", "start", payload.taskId];
    if (payload.workflow) {
      args.push("--workflow", payload.workflow);
    }
    if (payload.adapter) {
      args.push("--adapter", payload.adapter);
    }
    if (payload.prompt) {
      args.push("--prompt", payload.prompt);
    }
    return runSwbJson(args);
  });
  ipcMain.handle("swb:run-logs", async (_, payload: { runId: string; limit: number }) =>
    runSwbJson(["run", "logs", payload.runId, "--limit", String(payload.limit)]),
  );
  ipcMain.handle("swb:run-approve", async (_, payload: { runId: string; reason?: string }) => {
    const args = ["run", "approve", payload.runId];
    if (payload.reason) {
      args.push("--reason", payload.reason);
    }
    return runSwbJson(args);
  });
  ipcMain.handle("swb:run-reject", async (_, payload: { runId: string; reason?: string }) => {
    const args = ["run", "reject", payload.runId];
    if (payload.reason) {
      args.push("--reason", payload.reason);
    }
    return runSwbJson(args);
  });
  ipcMain.handle("swb:run-integrate", async (_, payload: { runId: string; message?: string }) => {
    const args = ["run", "integrate", payload.runId];
    if (payload.message) {
      args.push("--message", payload.message);
    }
    return runSwbJson(args);
  });
  ipcMain.handle("swb:adapter-auth-status", async () =>
    runSwbJson(["adapter", "auth", "status"]),
  );
  ipcMain.handle(
    "swb:adapter-auth-login",
    async (_, payload: { adapter: string; device: boolean }) => {
      const args = ["adapter", "auth", "login", payload.adapter];
      if (payload.device) {
        args.push("--device");
      }
      return runSwbJson(args);
    },
  );
  ipcMain.handle("swb:launcher-run-once", async () =>
    runSwbJson(["launcher", "run-once"]),
  );
  ipcMain.handle("swb:launcher-watch-start", async (_, payload: { intervalMs: number }) =>
    startLauncherWatch(payload.intervalMs),
  );
  ipcMain.handle("swb:launcher-watch-stop", async () => stopLauncherWatch());

  createMainWindow();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow();
    }
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});

app.on("before-quit", () => {
  if (watchProcess) {
    watchProcess.kill();
    watchProcess = null;
  }
});

declare const MAIN_WINDOW_VITE_DEV_SERVER_URL: string | undefined;
