import { chmod, mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import os from "node:os";
import path from "node:path";

import { _electron as electron, expect, test, type ElectronApplication } from "@playwright/test";

const currentFile = fileURLToPath(import.meta.url);
const currentDir = path.dirname(currentFile);
const repoRoot = path.resolve(currentDir, "..", "..");
const desktopRoot = path.resolve(currentDir, "..");
const swbBin = process.env.SWB_DESKTOP_BIN ?? path.join(repoRoot, "target", "debug", "swb");
const screenshotPath = process.env.SWB_DESKTOP_SCREENSHOT_PATH
  ? path.resolve(desktopRoot, process.env.SWB_DESKTOP_SCREENSHOT_PATH)
  : null;

test.describe("Stackbench Desktop", () => {
  test("launches, shows adapter auth, and completes an approval/integration loop @desktop-smoke", async () => {
    const fixture = await createFixtureRepo();
    const app = await launchDesktop(fixture.repoRoot);
    try {
      const page = await app.firstWindow();

      await expect(page.getByText("Keep local agent runs legible, reviewable, and under human control.")).toBeVisible();
      await expect(page.locator("#repo-root")).toContainText(path.basename(fixture.repoRoot));
      await expect(page.locator(".auth-card")).toContainText("codex");
      await expect(page.locator(".auth-card")).toContainText("logged in");

      await page.locator("#task-id").fill("TASK-DESKTOP-SMOKE");
      await page.locator("#prompt").fill("Implement the desktop smoke flow");
      await page.getByRole("button", { name: "Dispatch Run" }).click();

      await expect(page.locator("#run-start-status")).toContainText("Dispatched");
      const firstRun = page.locator(".run-row").first();
      await expect(firstRun).toContainText("TASK-DESKTOP-SMOKE");

      await page.getByRole("button", { name: "Pulse Queue" }).click();
      await expect(page.locator("#selected-run-meta")).toContainText("awaiting_review");

      await page.getByRole("button", { name: "Approve" }).click();
      await expect(page.locator("#selected-run-meta")).toContainText("approved");

      await page.locator("#run-action-note").fill("Ship the tested desktop path");
      await page.getByRole("button", { name: "Integrate" }).click();
      await expect(page.locator("#selected-run-meta")).toContainText("integrated");
      await expect(page.locator("#logs-view")).toContainText("run_integrated");

      await page.locator("#task-id").fill("TASK-DESKTOP-REVIEW");
      await page.locator("#prompt").fill("Leave a second run ready for review");
      await page.getByRole("button", { name: "Dispatch Run" }).click();
      await expect(page.locator("#run-start-status")).toContainText("Dispatched");
      await page.getByRole("button", { name: "Pulse Queue" }).click();
      await expect(page.locator("#selected-run-meta")).toContainText("awaiting_review");
      await expect(
        page.getByRole("button", { name: /TASK-DESKTOP-REVIEW • codex/ }),
      ).toBeVisible();

      if (screenshotPath) {
        await page.screenshot({ path: screenshotPath, fullPage: true });
      }
    } finally {
      await app.close();
      await cleanupFixtureRepo(fixture.root);
    }
  });

  test("processes a queued run through launcher watch and supports rejection", async () => {
    const fixture = await createFixtureRepo();
    const app = await launchDesktop(fixture.repoRoot);
    try {
      const page = await app.firstWindow();

      await page.locator("#task-id").fill("TASK-DESKTOP-WATCH");
      await page.locator("#prompt").fill("Exercise launcher watch");
      await page.getByRole("button", { name: "Dispatch Run" }).click();
      await expect(page.locator("#run-start-status")).toContainText("Dispatched");

      await page.locator("#watch-interval").fill("250");
      await page.getByRole("button", { name: "Start Watch" }).click();
      await expect(page.locator("#runtime-mode")).toContainText("watch running");
      await expect(page.locator("#selected-run-meta")).toContainText("awaiting_review");
      await expect(page.locator("#watch-feed")).toContainText("cycle");

      await page.locator("#run-action-note").fill("Reject from automated watch test");
      await page.getByRole("button", { name: "Reject" }).click();
      await expect(page.locator("#selected-run-meta")).toContainText("rejected");

      await page.getByRole("button", { name: "Stop Watch" }).click();
      await expect(page.locator("#runtime-mode")).not.toContainText("watch running");
    } finally {
      await app.close();
      await cleanupFixtureRepo(fixture.root);
    }
  });
});

async function launchDesktop(swbRepoRoot: string): Promise<ElectronApplication> {
  return electron.launch({
    args: [path.resolve(desktopRoot, ".vite/build/main.cjs")],
    cwd: desktopRoot,
    env: {
      ...process.env,
      SWB_DESKTOP_APP_ROOT: desktopRoot,
      SWB_DESKTOP_WORKSPACE_ROOT: repoRoot,
      SWB_DESKTOP_REPO_ROOT: swbRepoRoot,
      SWB_DESKTOP_BIN: swbBin,
    },
  });
}

async function createFixtureRepo(): Promise<{ root: string; repoRoot: string }> {
  const root = await mkdtemp(path.join(os.tmpdir(), "stackbench-smoke-"));
  const repoRoot = path.join(root, "repo");
  const binRoot = path.join(root, "bin");
  const fakeCodex = path.join(binRoot, "codex");
  const fakeJj = path.join(binRoot, "jj");
  const fakeSwbJj = path.join(repoRoot, "fake-swb-jj.sh");

  await mkdir(repoRoot, { recursive: true });
  await mkdir(binRoot, { recursive: true });
  await writeExecutable(
    fakeCodex,
    [
      "#!/usr/bin/env bash",
      "set -euo pipefail",
      "if [[ \"$*\" == \"login status\" ]]; then",
      "  echo \"Logged in using ChatGPT\"",
      "  exit 0",
      "fi",
      "if [[ \"$*\" == \"login\" ]]; then",
      "  echo \"Login completed\"",
      "  exit 0",
      "fi",
      "if [[ \"$*\" == \"login --device-auth\" ]]; then",
      "  echo \"Open browser and enter code\"",
      "  exit 0",
      "fi",
      "if [[ \"$1\" == \"exec\" ]]; then",
      "  prompt=\"${@: -1}\"",
      "  printf '%s' \"$prompt\" > execution.txt",
      "  echo \"Executed: $prompt\"",
      "  exit 0",
      "fi",
      "echo \"unexpected codex invocation: $*\" >&2",
      "exit 2",
    ].join("\n"),
  );
  await writeExecutable(
    fakeJj,
    [
      "#!/usr/bin/env bash",
      "set -euo pipefail",
      "if [[ \"$1\" == \"log\" ]]; then",
      "  echo change-desktop-smoke-123",
      "  exit 0",
      "fi",
      "echo \"unexpected jj invocation: $*\" >&2",
      "exit 2",
    ].join("\n"),
  );
  await writeExecutable(
    fakeSwbJj,
    [
      "#!/usr/bin/env bash",
      "set -euo pipefail",
      "if [[ \"$1\" == \"lane-add\" ]]; then",
      "  mkdir -p \"$4\"",
      "  exit 0",
      "fi",
      "if [[ \"$1\" == \"integrate\" ]]; then",
      "  echo \"$*\"",
      "  exit 0",
      "fi",
      "echo \"unexpected swb-jj invocation: $*\" >&2",
      "exit 2",
    ].join("\n"),
  );
  await writeFile(
    path.join(repoRoot, "swb.toml"),
    `
[integration]
script_path = "${fakeSwbJj}"
jj_bin = "${fakeJj}"
base_revset = "trunk()"

[[adapters]]
name = "codex"
command = "${fakeCodex}"
args = ["exec", "--skip-git-repo-check"]
auth_strategy = "codex_login_status"
auth_status_args = ["login", "status"]
auth_login_args = ["login"]
auth_login_device_args = ["login", "--device-auth"]
prompt_mode = "argv_last"

[adapters.capabilities]
streaming = true
cancellation = true
artifacts = true
auth = true

[[workflows]]
name = "default"
adapters = ["codex"]

[[evaluators]]
name = "repo_checks"
commands = ["test -f execution.txt"]
`.trimStart(),
  );

  return { root, repoRoot };
}

async function writeExecutable(targetPath: string, contents: string): Promise<void> {
  await writeFile(targetPath, contents);
  await chmod(targetPath, 0o755);
}

async function cleanupFixtureRepo(root: string): Promise<void> {
  await rm(root, { recursive: true, force: true });
}
