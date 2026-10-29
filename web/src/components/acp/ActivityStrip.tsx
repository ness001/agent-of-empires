import { useState } from "react";
import { ChevronDown } from "lucide-react";

import type { AsyncTask, BackgroundAgent, Goal } from "../../lib/acpTypes";

interface Props {
  goal: Goal | null;
  agents: BackgroundAgent[];
  tasks: AsyncTask[];
}

export function ActivityStrip({ goal, agents, tasks }: Props) {
  const [expanded, setExpanded] = useState(false);
  const visibleTasks = tasks.filter(
    (task) =>
      task.task_type !== "agent" ||
      !agents.some(
        (agent) =>
          agent.agentId === task.id ||
          (!!task.tool_call_id && !!agent.toolCallId && agent.toolCallId === task.tool_call_id),
      ),
  );
  if (!goal && agents.length === 0 && visibleTasks.length === 0) return null;

  const runningAgents = agents.filter((agent) => !agent.endedAt).length;
  const workflows = visibleTasks.filter((task) => task.task_type === "workflow");
  const runningWorkflows = workflows.filter((task) => task.state === "running" || task.state === "paused").length;

  return (
    <section aria-label="Session activity" className="shrink-0 border-b border-surface-700 bg-surface-900">
      <button
        type="button"
        aria-expanded={expanded}
        onClick={() => setExpanded((value) => !value)}
        className="flex min-h-10 w-full items-center gap-2 px-4 py-2 text-left text-xs text-text-secondary"
      >
        <span className="flex min-w-0 flex-1 flex-wrap gap-x-3 gap-y-1">
          {goal && <span>Goal · {goal.status}</span>}
          {agents.length > 0 && (
            <span>
              Subagents · {runningAgents} running / {agents.length}
            </span>
          )}
          {workflows.length > 0 && (
            <span>
              Workflows · {runningWorkflows} running / {workflows.length}
            </span>
          )}
          {visibleTasks.length > workflows.length && (
            <span>Background tasks · {visibleTasks.length - workflows.length}</span>
          )}
        </span>
        <ChevronDown className={`h-4 w-4 shrink-0 ${expanded ? "rotate-180" : ""}`} />
      </button>
      {expanded && (
        <div className="max-h-[40dvh] space-y-3 overflow-y-auto overscroll-contain border-t border-surface-800 px-4 py-3 text-xs">
          {goal && (
            <div className="space-y-1">
              <h3 className="font-medium text-text-primary">Goal · {goal.status}</h3>
              <p className="whitespace-pre-wrap break-words text-text-secondary">{goal.objective}</p>
              {goal.iterations != null && <p className="text-text-dim">Iterations · {goal.iterations}</p>}
              {goal.last_reason && <p className="break-words text-text-secondary">{goal.last_reason}</p>}
            </div>
          )}
          {agents.map((agent) => (
            <details key={agent.agentId} className="border-t border-surface-800 pt-2">
              <summary className="min-h-8 cursor-pointer break-words text-text-primary">
                {agent.description || "Subagent"} · {agent.status}
              </summary>
              <div className="space-y-1 whitespace-pre-wrap break-words text-text-secondary">
                {agent.lastTool && <p>Tool · {agent.lastTool}</p>}
                {agent.lastText && <p>{agent.lastText}</p>}
                {agent.result && <p>{agent.result}</p>}
                {agent.warning && <p>{agent.warning}</p>}
                {agent.model && <p className="text-text-dim">{agent.model}</p>}
              </div>
            </details>
          ))}
          {visibleTasks.map((task) => (
            <details key={task.id} className="border-t border-surface-800 pt-2">
              <summary className="min-h-8 cursor-pointer break-words text-text-primary">
                {task.task_type === "workflow" ? "Workflow" : "Task"} · {task.name || task.description} · {task.state}
              </summary>
              <div className="space-y-1 whitespace-pre-wrap break-words text-text-secondary">
                {task.description && <p>{task.description}</p>}
                {task.last_tool && <p>Tool · {task.last_tool}</p>}
                {task.summary && <p>{task.summary}</p>}
              </div>
            </details>
          ))}
        </div>
      )}
    </section>
  );
}
