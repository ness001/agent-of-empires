import { describe, expect, it } from "vitest";
import {
  applyEvent,
  applyReducedState,
  emptyAcpState,
  isVisiblyBusy,
  type AcpEvent,
  type AsyncTask,
  type ReducedState,
} from "./acpTypes";

const task: AsyncTask = {
  id: "wf-1",
  name: "Inspect",
  task_type: "workflow",
  description: "Verify mobile output",
  state: "running",
  show_in_transcript: true,
  can_stop: false,
};

function events(...events: AcpEvent[]) {
  return events.reduce(
    (state, event, index) => applyEvent(state, { session_id: "s-1", seq: index + 1, event }),
    emptyAcpState(),
  );
}

describe("structured activity state", () => {
  it("keeps workflow-only activity visibly busy without opening the parent turn", () => {
    for (const status of ["running", "paused", "completed"]) {
      const state = events({ AsyncTaskUpdated: { task: { ...task, state: status } } });
      expect(state.turnActive).toBe(false);
      expect(isVisiblyBusy(state)).toBe(status !== "completed");
    }
  });
  it("retains the goal beyond a turn, replaces task updates, and clears conversation-scoped activity", () => {
    const state = events(
      { GoalUpdated: { goal: { objective: "Make activity visible", status: "active" } } },
      { AsyncTaskUpdated: { task } },
      { AsyncTaskUpdated: { task: { ...task, state: "completed", summary: "Verified" } } },
    );
    expect(state.goal?.status).toBe("active");
    expect(state.turnActive).toBe(false);
    expect(state.asyncTasks).toEqual([{ ...task, state: "completed", summary: "Verified" }]);
    const cleared = applyEvent(state, { session_id: "s-1", seq: 4, event: "SessionCleared" });
    expect(cleared.goal).toBeNull();
    expect(cleared.asyncTasks).toEqual([]);
    const switched = applyEvent(state, {
      session_id: "s-1",
      seq: 4,
      event: { AgentSwitched: { from: "claude", to: "codex", reason: "user" } },
    });
    expect(switched.goal).toBeNull();
    expect(switched.asyncTasks).toEqual([]);
  });

  it("adopts authoritative activity on reconnect and respects elided fields", () => {
    const state = events(
      { GoalUpdated: { goal: { objective: "Existing goal", status: "active" } } },
      { AsyncTaskUpdated: { task } },
    );
    const reduced: ReducedState = {
      agent: "claude",
      model: null,
      mode: "Default",
      current_plan: null,
      in_flight_tool: null,
      pending_approvals: [],
      pending_elicitations: [],
      thinking: null,
      rate_limit: null,
      available_commands: [],
      available_modes: [],
      current_mode_id: null,
      turn_active: false,
      cancelling: false,
      compacting: false,
    };
    const held = applyReducedState(state, reduced, ["goal", "async_tasks"]);
    expect(held.goal).toEqual(state.goal);
    expect(held.asyncTasks).toEqual([task]);
    const adopted = applyReducedState(emptyAcpState(), { ...reduced, goal: state.goal, async_tasks: [task] });
    expect(adopted.goal).toEqual(state.goal);
    expect(adopted.asyncTasks).toEqual([task]);
    const cleared = applyReducedState(adopted, { ...reduced, goal: null, async_tasks: [] });
    expect(cleared.goal).toBeNull();
    expect(cleared.asyncTasks).toEqual([]);
  });
});
