export function runFeedback(message: string, detail?: string | null) {
  if (detail) return { summary: message, detail };
  if (message.includes("INJECT_EVENTS"))
    return {
      summary: "手机拒绝触控，请检查手机状态、调试权限或操作冲突。",
      detail: message,
    };
  if (message.includes("ADB 响应超时"))
    return {
      summary: "ADB 响应超时，请检查手机连接与其他 ADB 操作。",
      detail: message,
    };
  if (message.includes("\n\tat ") || message.includes("java.lang."))
    return { summary: "ADB 操作失败，请查看技术详情。", detail: message };
  return { summary: message, detail: null };
}

export function resumeBlocked(run: import("./api").Run) {
  const node = run.workflow.nodes.find((node) => node.id === run.node);
  return (
    !!run.pending &&
    !!node &&
    ["fixed_tap_loop", "tap_loop", "swipe", "vision_tap"].includes(node.type)
  );
}
