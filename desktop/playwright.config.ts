import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  reporter: "line",
  timeout: 90_000,
  expect: {
    timeout: 15_000,
  },
  use: {
    screenshot: "only-on-failure",
  },
});
