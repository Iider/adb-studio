import { busy, type Run, type Schedule, type Workflow } from "./api";

export function summarizeTasks(
  workflows: Workflow[],
  runs: Run[],
  schedules: Schedule[],
) {
  const groups = new Map<string, Workflow[]>();
  for (const workflow of workflows) {
    const versions = groups.get(workflow.id) || [];
    versions.push(workflow);
    groups.set(workflow.id, versions);
  }
  return [...groups.values()]
    .map((versions) => {
      versions.sort((a, b) => b.version - a.version);
      const latest = versions[0];
      const active = versions.find((workflow) => workflow.active);
      const history = runs
        .filter((run) => run.workflow.id === latest.id)
        .sort((a, b) => b.started_at.localeCompare(a.started_at));
      return {
        workflow: active || latest,
        latest,
        active,
        history,
        latestRun: history[0],
        inProgress: history.filter(busy),
        scheduled: schedules.filter(
          (schedule) => schedule.enabled && schedule.workflow_id === latest.id,
        ).length,
      };
    })
    .sort((a, b) => a.workflow.name.localeCompare(b.workflow.name, "zh-CN"));
}
