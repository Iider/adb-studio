<script lang="ts">
  import ConditionSamples from "./ConditionSamples.svelte";
  import {
    Plus,
    Trash2,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
  } from "@lucide/svelte";
  import {
    ports,
    newCondition,
    type CanvasNode,
    type Config,
    type Condition,
  } from "$lib/workflow-editor";
  let {
    node,
    nodes,
    entry,
    onLabel,
    onConfig,
    onTarget,
    onEntry,
    onRemove,
    onMove,
    onMark,
    onMarkCondition,
    deviceNames = {},
    operationLimit = null,
  }: {
    node: CanvasNode;
    nodes: CanvasNode[];
    entry: string;
    onLabel: (value: string) => void;
    onConfig: (value: Config) => void;
    onTarget: (port: string, target: string) => void;
    onEntry: () => void;
    onRemove: () => void;
    onMove: (x: number, y: number) => void;
    onMark: () => void;
    onMarkCondition: (index: number) => void;
    deviceNames?: Record<string, string>;
    operationLimit?: number | null;
  } = $props();
  const c = $derived(node.data.config);
  const kind = $derived(node.data.kind);
  const loops = $derived(nodes.filter((item) => item.data.kind === "loop"));
  type NumberField = {
    key: keyof Config;
    label: string;
    min: number;
    max: number;
    step?: number;
    percent?: boolean;
  };
  const fields = $derived.by((): NumberField[] => {
    const repeats: NumberField[] = [
      {
        key: "count",
        label: kind === "loop" ? "重复轮数" : "重复次数",
        min: 1,
        max: 10000,
        step: 1,
      },
      {
        key: "interval",
        label: kind === "loop" ? "轮间间隔（秒）" : "两次之间的间隔（秒）",
        min: 0,
        max: 86400,
        step: 0.1,
      },
    ];
    if (kind === "wait")
      return [
        {
          key: "seconds",
          label: "等待时长（秒）",
          min: 0,
          max: 86400,
          step: 0.5,
        },
      ];
    if (kind === "tap")
      return [
        { key: "x", label: "横向位置（%）", min: 0, max: 100, percent: true },
        { key: "y", label: "纵向位置（%）", min: 0, max: 100, percent: true },
        ...repeats,
      ];
    if (kind === "swipe")
      return [
        {
          key: "from_x",
          label: "起点横向（%）",
          min: 0,
          max: 100,
          percent: true,
        },
        {
          key: "from_y",
          label: "起点纵向（%）",
          min: 0,
          max: 100,
          percent: true,
        },
        {
          key: "to_x",
          label: "终点横向（%）",
          min: 0,
          max: 100,
          percent: true,
        },
        {
          key: "to_y",
          label: "终点纵向（%）",
          min: 0,
          max: 100,
          percent: true,
        },
        {
          key: "duration_ms",
          label: "单次滑动时长（毫秒）",
          min: 100,
          max: 2000,
          step: 50,
        },
        ...repeats,
      ];
    if (kind === "loop") return repeats;
    if (kind === "wait_condition")
      return [
        {
          key: "interval",
          label: "检查间隔（秒）",
          min: 0.1,
          max: 86400,
          step: 0.5,
        },
        {
          key: "timeout",
          label: "最多等待（秒）",
          min: 0.1,
          max: 86400,
          step: 1,
        },
      ];
    return [];
  });
  function numberValue(field: NumberField) {
    const value = c[field.key];
    return typeof value === "number"
      ? Math.round(value * (field.percent ? 100 : 1) * 1000) / 1000
      : undefined;
  }
  function setNumber(field: NumberField, value: number) {
    onConfig({
      ...c,
      [field.key]: Number.isFinite(value)
        ? value / (field.percent ? 100 : 1)
        : undefined,
    });
  }
  function setCondition(index: number, patch: Partial<Condition>) {
    onConfig({
      ...c,
      conditions: c.conditions?.map((condition, i) =>
        i === index ? { ...condition, ...patch } : condition,
      ),
    });
  }
</script>

<section class="step-inspector">
  <label
    >步骤名称<input
      maxlength="120"
      disabled={kind === "entry"}
      value={node.data.label}
      oninput={(event) => {
        const value = event.currentTarget.value;
        if (value.trim()) onLabel(value);
      }}
      onchange={(event) => {
        const value = event.currentTarget.value.trim() || node.data.label;
        event.currentTarget.value = value;
        onLabel(value);
      }}
    /></label
  >
  {#if kind !== "entry"}
    <button
      class="inspector-button"
      class:entry-active={entry === node.id}
      onclick={onEntry}
      disabled={entry === node.id}
      >{entry === node.id ? "已设为开始步骤" : "从这一步开始执行"}</button
    >
  {/if}

  {#if kind === "tap" || kind === "swipe"}
    <button class="inspector-button" onclick={onMark}
      >在手机截图上标注{kind === "tap" ? "位置" : "路线"}</button
    >
    {#each c.profiles ?? [] as profile, index}
      <div class="profile-card">
        <p>
          {deviceNames[profile.device_id] ?? profile.device_id}<br />屏幕 {profile.width}
          × {profile.height}
        </p>
        <button
          class="inspector-button danger"
          onclick={() =>
            onConfig({
              ...c,
              profiles: c.profiles?.filter((_, i) => i !== index),
            })}>移除此标注</button
        >
      </div>
    {/each}
    {#if !c.profiles?.length}<p class="inspector-hint">
        保存可运行版本前，请先完成手机位置标注。
      </p>{/if}
  {/if}
  {#each fields.filter((field) => !field.percent) as field (field.key)}
    <label
      >{field.label}<input
        type="number"
        min={field.min}
        max={field.max}
        step={field.step ?? 0.1}
        value={numberValue(field)}
        oninput={(event) => setNumber(field, event.currentTarget.valueAsNumber)}
      /></label
    >
  {/each}
  {#if ["tap", "swipe", "loop"].includes(kind)}<p class="inspector-hint">
      第一次立即执行，间隔只在两次之间等待。
    </p>{/if}

  {#if ["loop", "tap", "swipe"].includes(kind)}
    <p class="inspector-hint">
      {operationLimit === null
        ? "连接完整后显示手机操作次数上限。"
        : `整个工作流最多执行 ${operationLimit} 次手机操作（含循环和重复次数；分支不一定全部执行）。`}
    </p>
  {/if}
  {#if kind === "condition" || kind === "wait_condition"}
    <label
      >组合方式<select
        value={c.mode}
        onchange={(event) =>
          onConfig({ ...c, mode: event.currentTarget.value as "all" | "any" })}
        ><option value="all">全部条件满足</option><option value="any"
          >任一条件满足</option
        ></select
      ></label
    >
    {#each c.conditions ?? [] as condition, i}
      <fieldset class="condition-card">
        <legend>条件 {i + 1}</legend>
        <label
          >判断内容<select
            value={condition.source}
            onchange={(event) => {
              const source = event.currentTarget.value as Condition["source"];
              setCondition(i, {
                source,
                operator:
                  source === "text"
                    ? "contains"
                    : source === "app"
                      ? "is"
                      : "eq",
                value: "",
              });
            }}
            ><option value="text">页面文字</option><option value="app"
              >当前应用</option
            >{#if kind === "condition"}<option value="round"
                >当前循环轮次</option
              >{/if}</select
          ></label
        >
        <label
          >判断方式<select
            value={condition.operator}
            onchange={(event) =>
              setCondition(i, {
                operator: event.currentTarget.value as Condition["operator"],
              })}
          >
            {#if condition.source === "text"}<option value="contains"
                >包含文字</option
              ><option value="not_contains">不包含文字</option>
            {:else if condition.source === "app"}<option value="is"
                >是这个应用</option
              ><option value="is_not">不是这个应用</option>
            {:else}<option value="eq">等于</option><option value="gt"
                >大于</option
              ><option value="lt">小于</option>{/if}
          </select></label
        >
        <label
          >{condition.source === "text"
            ? "要查找的文字"
            : condition.source === "app"
              ? "应用编号"
              : "轮次"}<input
            value={condition.value}
            maxlength="500"
            placeholder={condition.source === "app"
              ? "例如 com.example.app"
              : ""}
            oninput={(event) =>
              setCondition(i, { value: event.currentTarget.value })}
          /></label
        >
        {#if condition.source === "text"}<button
            class="inspector-button"
            onclick={() => onMarkCondition(i)}
            >框选识别区域（已标注 {condition.regions?.length ?? 0} 个屏幕）</button
          ><label
            >页面依据<input
              value={condition.guard}
              maxlength="500"
              placeholder="用于确认当前页面的文字"
              oninput={(event) =>
                setCondition(i, { guard: event.currentTarget.value })}
            /></label
          >{/if}
        <button
          class="inspector-button danger"
          onclick={() =>
            onConfig({
              ...c,
              conditions: c.conditions?.filter((_, index) => index !== i),
            })}><Trash2 size={14} />删除此条件</button
        >
      </fieldset>
    {/each}
    <button
      class="inspector-button"
      disabled={(c.conditions?.length ?? 0) >= 20}
      onclick={() =>
        onConfig({
          ...c,
          conditions: [...(c.conditions ?? []), newCondition()],
        })}><Plus size={16} />添加条件</button
    >
  {/if}

  {#if kind === "condition" || kind === "wait_condition"}<ConditionSamples
      config={c}
      onchange={onConfig}
    />{/if}
  {#if kind === "break" || kind === "continue"}
    <label
      >所属循环<select
        value={c.loop ?? ""}
        onchange={(event) =>
          onConfig({ ...c, loop: event.currentTarget.value })}
        ><option value="">请选择循环</option>{#each loops as loop}<option
            value={loop.id}>{loop.data.label}</option
          >{/each}</select
      ></label
    >
    <p class="inspector-hint">
      {kind === "break"
        ? "走所属循环的完成出口。"
        : "本轮计入轮数，跳过余下动作。"}
    </p>
  {/if}
  {#if kind === "end"}<label
      >结束结果<select
        value={c.result}
        onchange={(event) =>
          onConfig({
            ...c,
            result: event.currentTarget.value as Config["result"],
          })}
        ><option value="succeeded">任务完成</option><option value="needs_help"
          >需要人工处理</option
        ></select
      ></label
    >{/if}

  {#if ports(kind).length}<h3>连接去向</h3>{/if}
  {#each ports(kind) as port (port.id)}
    <label
      >{port.label}<select
        value={kind === "entry" ? entry : (node.data.targets[port.id] ?? "")}
        onchange={(event) => onTarget(port.id, event.currentTarget.value)}
        ><option value="">尚未连接</option
        >{#each nodes.filter((item) => item.id !== node.id && item.id !== "__entry") as target}<option
            value={target.id}>{target.data.label}</option
          >{/each}</select
      ></label
    >
  {/each}
  {#if kind !== "entry"}
    <h3>移动节点</h3>
    <div class="move-buttons">
      <button aria-label="节点左移" onclick={() => onMove(-40, 0)}
        ><ArrowLeft size={16} /></button
      ><button aria-label="节点上移" onclick={() => onMove(0, -40)}
        ><ArrowUp size={16} /></button
      ><button aria-label="节点下移" onclick={() => onMove(0, 40)}
        ><ArrowDown size={16} /></button
      ><button aria-label="节点右移" onclick={() => onMove(40, 0)}
        ><ArrowRight size={16} /></button
      >
    </div>
    <button class="inspector-button danger remove-step" onclick={onRemove}
      ><Trash2 size={16} />删除步骤</button
    >
  {/if}
</section>

<style>
  label {
    font-size: 13px;
    line-height: 1.5;
    margin: 16px 0;
  }
  input,
  select {
    font-size: 14px;
  }
  h3 {
    font-size: 14px;
    margin: 24px 0 8px;
  }
  .inspector-button,
  .move-buttons button {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    min-height: 40px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel);
    color: var(--text);
    padding: 8px 10px;
    font: inherit;
  }
  .inspector-button {
    width: 100%;
    font-size: 13px;
  }
  .inspector-button:disabled {
    color: var(--accent2);
    background: var(--accentsoft);
    opacity: 1;
  }
  .inspector-hint {
    color: var(--dim);
    font-size: 12px;
    line-height: 1.7;
    margin: 12px 0;
  }
  .condition-card {
    min-width: 0;
    padding: 0 12px 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    margin: 20px 0 12px;
  }
  legend {
    font-size: 13px;
    color: var(--dim);
    padding: 0 6px;
  }
  .danger {
    color: var(--red);
  }
  .profile-card {
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 10px;
    margin: 12px 0;
  }
  .profile-card p {
    font-size: 12px;
    line-height: 1.7;
    overflow-wrap: anywhere;
    margin-bottom: 8px;
  }
  .move-buttons {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }
  .remove-step {
    margin-top: 20px;
  }
</style>
