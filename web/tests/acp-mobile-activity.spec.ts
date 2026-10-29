import { devices } from "@playwright/test";
import { test, expect } from "./helpers/mockedTest";
import {
  agentMessageChunk,
  backgroundAgentLaunched,
  mockAcpSession,
  openStructuredSession,
  stopped,
} from "./helpers/acpMock";

test.use({ ...devices["iPhone 13"] });

test("mobile activity stays visible after the parent turn and clears with the conversation", async ({ page }) => {
  const task = {
    id: "wf-1",
    name: "Inspect mobile activity",
    task_type: "workflow",
    description: "Verify reported task state",
    state: "running",
    show_in_transcript: true,
    can_stop: false,
    summary: "Checking the mobile toolbar",
  };
  const mock = await mockAcpSession(page, {
    title: "mobile-activity",
    initialEvents: [
      agentMessageChunk("The parent reply is complete while background work continues."),
      {
        GoalUpdated: { goal: { objective: "Make Claude activity visible on mobile", status: "active", iterations: 2 } },
      },
      backgroundAgentLaunched({
        agent_id: "child-1",
        tool_call_id: "",
        description: "Inspect mobile layout",
        prompt: "",
        model: "",
      }),
      { AsyncTaskUpdated: { task } },
      stopped(),
    ],
  });
  await openStructuredSession(page, mock);
  const activity = page.getByRole("region", { name: "Session activity" });
  await expect(activity).toBeVisible();
  const toggle = activity.getByRole("button");
  await expect(toggle).toContainText("Goal · active");
  await expect(toggle).toContainText("Subagents · 1 running / 1");
  await expect(toggle).toContainText("Workflows · 1 running / 1");
  await toggle.tap();
  await expect(activity.getByText("Make Claude activity visible on mobile")).toBeVisible();
  await activity.locator("summary").filter({ hasText: "Workflow" }).tap();
  await expect(activity.getByText("Checking the mobile toolbar")).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: "test-results/mobile-activity.png" });
  mock.pushEvents([{ AsyncTaskUpdated: { task: { ...task, state: "completed", summary: "Verified mobile output" } } }]);
  await expect(toggle).toContainText("Workflows · 0 running / 1");
  await expect(toggle).toContainText("Goal · active");
  await expect(activity.getByText("Verified mobile output")).toBeVisible();
  mock.pushEvents(["SessionCleared"]);
  await expect(activity).toHaveCount(0);
});
