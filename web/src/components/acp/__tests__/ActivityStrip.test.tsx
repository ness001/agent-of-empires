// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ActivityStrip } from "../ActivityStrip";
import type { AsyncTask, BackgroundAgent } from "../../../lib/acpTypes";

const agent: BackgroundAgent = {
  agentId: "child-1",
  toolCallId: "",
  description: "Inspect mobile layout",
  prompt: "",
  model: "",
  wireTracked: true,
  status: "running",
  startedAt: "2026-10-29T00:00:00Z",
  endedAt: null,
  toolCount: 1,
  tools: [],
  lastTool: "Read",
  lastText: "Checking the activity toolbar",
  result: null,
  warning: null,
};
const workflow: AsyncTask = {
  id: "wf-1",
  name: "Verify activity",
  task_type: "workflow",
  description: "Run focused checks",
  state: "running",
  show_in_transcript: true,
  can_stop: false,
  summary: "Reviewing mobile output",
};

describe("ActivityStrip", () => {
  it("retains unmatched agent tasks and only removes positively matched duplicates", () => {
    const task = { ...workflow, task_type: "agent", name: "Standalone agent", tool_call_id: "task-1" };
    const { rerender } = render(<ActivityStrip goal={null} agents={[]} tasks={[task]} />);
    fireEvent.click(screen.getByRole("button"));
    expect(screen.getByText(/Standalone agent/)).toBeTruthy();
    rerender(<ActivityStrip goal={null} agents={[{ ...agent, toolCallId: "task-1" }]} tasks={[task]} />);
    expect(screen.queryByText(/Standalone agent/)).toBeNull();
  });
  it("keeps observed goal, subagent and workflow state discoverable without a side pane", () => {
    const { rerender } = render(
      <ActivityStrip
        goal={{ objective: "Make activity visible", status: "active", iterations: 3 }}
        agents={[agent]}
        tasks={[workflow]}
      />,
    );
    const toggle = screen.getByRole("button");
    expect(toggle.textContent).toContain("Goal · active");
    expect(toggle.textContent).toContain("Subagents · 1 running / 1");
    expect(toggle.textContent).toContain("Workflows · 1 running / 1");
    expect(screen.queryByText("Make activity visible")).toBeNull();
    fireEvent.click(toggle);
    expect(screen.getByText("Make activity visible")).toBeTruthy();
    expect(screen.getByText("Iterations · 3")).toBeTruthy();
    expect(screen.getByText(/Inspect mobile layout/)).toBeTruthy();
    expect(screen.getByText(/Workflow · Verify activity · running/)).toBeTruthy();
    rerender(
      <ActivityStrip
        goal={{ objective: "Make activity visible", status: "active" }}
        agents={[{ ...agent, status: "completed", endedAt: "2026-10-29T00:01:00Z" }]}
        tasks={[{ ...workflow, state: "completed" }]}
      />,
    );
    expect(toggle.textContent).toContain("Goal · active");
    expect(toggle.textContent).toContain("Subagents · 0 running / 1");
    expect(toggle.textContent).toContain("Workflows · 0 running / 1");
  });

  it("does not invent status when nothing was reported", () => {
    const { container } = render(<ActivityStrip goal={null} agents={[]} tasks={[]} />);
    expect(container.textContent).toBe("");
  });
});
