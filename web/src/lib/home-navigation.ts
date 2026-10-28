export const homeTabs = [
  "devices",
  "workflows",
  "schedules",
  "runs",
  "settings",
] as const;

export type HomeTab = (typeof homeTabs)[number];

export interface HomeNavigation {
  tab: HomeTab;
  selectedDeviceId: string;
  deviceDetailTab: "parameters" | "task";
  workflowOpen: boolean;
  selectedWorkflow: string;
  selectedRun: string;
}

export function readHomeNavigation(url: URL): HomeNavigation {
  const params = url.searchParams;
  const candidate = params.get("tab");
  const tab = homeTabs.find((item) => item === candidate) ?? "devices";
  const workflow = tab === "workflows" ? params.get("workflow") : null;
  return {
    tab,
    selectedDeviceId: tab === "devices" ? (params.get("device") ?? "") : "",
    deviceDetailTab:
      tab === "devices" && params.get("section") === "task"
        ? "task"
        : "parameters",
    workflowOpen: !!workflow,
    selectedWorkflow: workflow || "swipe",
    selectedRun: tab === "runs" ? (params.get("run") ?? "") : "",
  };
}

export function homeNavigationUrl(url: URL, state: HomeNavigation): URL {
  const next = new URL(url);
  const params = next.searchParams;
  for (const key of ["tab", "device", "section", "workflow", "version", "run"])
    params.delete(key);
  if (state.tab !== "devices") params.set("tab", state.tab);
  if (state.tab === "devices" && state.selectedDeviceId) {
    params.set("device", state.selectedDeviceId);
    if (state.deviceDetailTab === "task") params.set("section", "task");
  }
  if (state.tab === "workflows" && state.workflowOpen) {
    params.set("workflow", state.selectedWorkflow);
  }
  if (state.tab === "runs" && state.selectedRun)
    params.set("run", state.selectedRun);
  return next;
}
