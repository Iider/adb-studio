import { busy, type Run, type Schedule, type Workflow } from "./api";

export function summarizeTasks(
  workflows: Workflow[],
  runs: Run[],
  schedules: Schedule[],
) {
  return workflows
    .map((workflow) => {
      const history = runs
        .filter((run) => run.workflow.id === workflow.id)
        .sort((a, b) => b.started_at.localeCompare(a.started_at));
      return {
        workflow,
        history,
        latestRun: history[0],
        inProgress: history.filter(busy),
        scheduled: schedules.filter(
          (schedule) =>
            schedule.enabled && schedule.workflow_id === workflow.id,
        ).length,
      };
    })
    .sort((a, b) => a.workflow.name.localeCompare(b.workflow.name, "zh-CN"));
}
