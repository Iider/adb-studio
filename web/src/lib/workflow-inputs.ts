import type { DraftNode, Issue } from "./workflow-editor";

export type InputDefinition = {
  id: string;
  name: string;
  alias?: string;
  type: "integer" | "number";
  default: number | null;
  min?: number | null;
  max?: number | null;
  description?: string;
};
export type InputReference = { input: string };
export type NumericValue = number | InputReference;
export function inputLabel(input: InputDefinition) {
  return input.alias?.trim() || input.name.trim() || "未命名变量";
}
export function inputOptionLabel(input: InputDefinition) {
  const label = inputLabel(input);
  return input.name.trim() && label !== input.name.trim()
    ? `${label}（${input.name.trim()}）`
    : label;
}
export function isReference(value: unknown): value is InputReference {
  return (
    !!value &&
    typeof value === "object" &&
    "input" in value &&
    typeof value.input === "string"
  );
}
export function normalizeInputs(inputs: InputDefinition[]): InputDefinition[] {
  // 旧浏览器恢复副本可能带 unit，丢弃旧字段但不换算数值。
  return inputs.map((input) => {
    const { unit: _legacyUnit, ...value } = input as InputDefinition & {
      unit?: unknown;
    };
    return value;
  });
}
export function inputFields(kind: string): string[] {
  if (kind === "loop") return ["count", "interval"];
  if (kind === "tap") return ["count", "interval", "after_wait"];
  if (kind === "swipe")
    return ["count", "interval", "after_wait", "duration_ms"];
  if (kind === "wait") return ["seconds"];
  if (kind === "wait_condition") return ["timeout", "interval"];
  return [];
}
export function compatible(input: InputDefinition, key: string) {
  return key !== "count" || input.type === "integer";
}
export function inputError(
  input: InputDefinition,
  value: number | null | undefined,
): string {
  if (value == null || !Number.isFinite(value)) return "请填写有效数字";
  if (input.type === "integer" && !Number.isInteger(value)) return "请填写整数";
  if (input.min != null && input.max != null && input.min > input.max)
    return "最小值不能大于最大值";
  if (
    (input.min != null && value < input.min) ||
    (input.max != null && value > input.max)
  )
    return "超出变量设置的范围";
  return "";
}
export function fieldError(kind: string, key: string, value: number): string {
  if (key === "count")
    return Number.isInteger(value) && value >= 1 && value <= 10000
      ? ""
      : "次数需为 1～10000 的整数";
  if (key === "duration_ms")
    return value >= 100 && value <= 2000 ? "" : "滑动时长需为 100～2000 毫秒";
  return value >= 0 &&
    value <= 86400 &&
    (kind !== "wait_condition" || value > 0)
    ? ""
    : kind === "wait_condition"
      ? "等待时间需大于 0，最多 86400 秒"
      : "等待时间需为 0～86400 秒";
}
export function inputIssues(
  inputs: InputDefinition[],
  nodes: DraftNode[],
  overrides: Record<string, number> = {},
): Issue[] {
  const issues: Issue[] = [];
  const names = new Set<string>();
  for (const input of inputs) {
    const name = input.name.trim();
    const error =
      !name || names.has(name)
        ? "变量名称不能为空或重复"
        : Array.from(input.alias ?? "").length > 40
          ? "显示别名最多 40 字"
          : inputError(input, overrides[input.id] ?? input.default);
    names.add(name);
    if (error)
      issues.push({
        node: "__entry",
        message: `${inputLabel(input)}：${error}`,
      });
  }
  for (const node of nodes) {
    for (const [key, value] of Object.entries(node.config)) {
      if (!isReference(value)) continue;
      const input = inputs.find((input) => input.id === value.input);
      const error = !input
        ? "引用的变量已不存在"
        : !inputFields(node.kind).includes(key) || !compatible(input, key)
          ? "变量类型不匹配"
          : fieldError(
              node.kind,
              key,
              overrides[input.id] ?? input.default ?? NaN,
            );
      if (error)
        issues.push({
          node: node.id,
          message: `${node.label} · ${input ? inputLabel(input) : key}：${error}`,
        });
    }
  }
  return issues;
}
export function resolveNodes(
  nodes: DraftNode[],
  inputs: InputDefinition[],
  overrides: Record<string, number> = {},
): DraftNode[] {
  return nodes.map((node) => ({
    ...node,
    config: Object.fromEntries(
      Object.entries(node.config).map(([key, value]) => [
        key,
        isReference(value)
          ? (overrides[value.input] ??
            inputs.find((input) => input.id === value.input)?.default ??
            NaN)
          : value,
      ]),
    ),
  }));
}

export function parseInputValues(
  inputs: InputDefinition[],
  values: Record<string, string>,
): Record<string, number> {
  return Object.fromEntries(
    inputs
      .filter((input) => values[input.id]?.trim())
      .map((input) => [input.id, Number(values[input.id])]),
  );
}
