<script lang="ts">
  import { tick } from "svelte";
  import { Plus, Pencil, Trash2, X } from "@lucide/svelte";
  import Dropdown from "$lib/components/ui/dropdown/dropdown.svelte";
  import {
    inputError,
    inputLabel,
    isReference,
    type InputDefinition,
  } from "$lib/workflow-inputs";
  import type { CanvasNode } from "$lib/workflow-editor";

  let {
    inputs,
    nodes,
    onchange,
  }: {
    inputs: InputDefinition[];
    nodes: CanvasNode[];
    onchange: (inputs: InputDefinition[]) => void;
  } = $props();

  const uid = $props.id();
  const titleId = `${uid}-variable-title`;
  let dialog: HTMLDialogElement;
  let editingId = $state<string | null>(null);
  let draft = $state<InputDefinition | null>(null);
  let order = $state(0);
  const error = $derived(draft ? validate(draft) : "");

  function references(id: string) {
    return nodes
      .filter((node) =>
        Object.values(node.data.config).some(
          (value) => isReference(value) && value.input === id,
        ),
      )
      .map((node) => node.data.label);
  }
  function validate(value: InputDefinition) {
    if (!value.name.trim()) return "请填写变量名称";
    if (
      inputs.some(
        (input) =>
          input.id !== value.id && input.name.trim() === value.name.trim(),
      )
    )
      return "变量名称不能重复";
    if (Array.from(value.alias ?? "").length > 40) return "显示别名最多 40 字";
    return inputError(value, value.default);
  }
  async function open(input?: InputDefinition) {
    editingId = input?.id ?? null;
    order = input
      ? inputs.findIndex((item) => item.id === input.id)
      : inputs.length;
    draft = input
      ? { ...input, alias: input.alias ?? "" }
      : {
          id: crypto.randomUUID(),
          name: "",
          alias: "",
          type: "number",
          default: 0,
        };
    await tick();
    dialog.showModal();
    dialog
      .querySelector<HTMLInputElement>('input[aria-label="变量名称"]')
      ?.focus();
  }
  function number(event: Event) {
    const value = (event.currentTarget as HTMLInputElement).valueAsNumber;
    return Number.isFinite(value) ? value : null;
  }
  function save(event: SubmitEvent) {
    event.preventDefault();
    if (!draft || error || (!editingId && inputs.length >= 20)) return;
    const next = inputs.filter((input) => input.id !== editingId);
    next.splice(Math.max(0, Math.min(order, next.length)), 0, { ...draft });
    onchange(next);
    dialog.close();
  }
  function remove(input: InputDefinition) {
    if (!references(input.id).length)
      onchange(inputs.filter((item) => item.id !== input.id));
  }
</script>

<div class="definitions">
  <div class="definitions-heading">
    <h3>运行变量</h3>
    <button
      type="button"
      class="icon-button"
      aria-label="添加变量"
      title="添加变量"
      disabled={inputs.length >= 20}
      onclick={() => open()}><Plus size={16} /></button
    >
  </div>
  {#each inputs as input (input.id)}
    {@const used = references(input.id)}
    {@const label = inputLabel(input)}
    {@const value = `${input.default ?? "未填写"}`}
    <div class="variable-row">
      <span class="variable-label" title={label}>{label}</span>
      <span class="variable-value" title={`默认值：${value}`}>{value}</span>
      <button
        type="button"
        class="icon-button"
        aria-label={`编辑变量 ${label}`}
        title="编辑变量"
        onclick={() => open(input)}><Pencil size={14} /></button
      >
      <button
        type="button"
        class="icon-button delete-button"
        aria-label={`删除变量 ${label}`}
        title={used.length
          ? `已用于：${used.join("、")}。解除引用后可删除。`
          : "删除变量"}
        disabled={used.length > 0}
        onclick={() => remove(input)}><Trash2 size={14} /></button
      >
    </div>
  {/each}
</div>

<dialog
  bind:this={dialog}
  class="variable-dialog"
  aria-labelledby={titleId}
  onclose={() => (draft = null)}
>
  {#if draft}
    <form onsubmit={save}>
      <div class="dialog-heading">
        <h2 id={titleId}>{editingId ? "编辑变量" : "添加变量"}</h2>
        <button
          type="button"
          class="icon-button"
          aria-label="关闭变量弹窗"
          onclick={() => dialog.close()}><X size={18} /></button
        >
      </div>
      <div class="dialog-fields">
        <label
          >类型<Dropdown
            label="变量类型"
            value={draft.type}
            items={[
              { value: "integer", label: "整数（次数、轮数）" },
              { value: "number", label: "数字（可带小数）" },
            ]}
            onchange={(value) => {
              if (draft) draft.type = value as InputDefinition["type"];
            }}
          /></label
        >
        <label
          >变量名称<input
            aria-label="变量名称"
            maxlength="40"
            bind:value={draft.name}
          /></label
        >
        <label
          >显示别名（选填）<input
            aria-label="显示别名"
            maxlength="40"
            placeholder="例如：点击次数"
            bind:value={draft.alias}
          /></label
        >
        <label
          >默认值<input
            aria-label="变量默认值"
            type="number"
            step={draft.type === "integer" ? 1 : "any"}
            value={draft.default ?? ""}
            oninput={(event) => {
              if (draft) draft.default = number(event);
            }}
          /></label
        >
        <details>
          <summary>更多设置</summary>
          <div class="optional-fields">
            <div class="range">
              <label
                >最小值<input
                  aria-label="变量最小值"
                  type="number"
                  step="any"
                  value={draft.min ?? ""}
                  oninput={(event) => {
                    if (draft) draft.min = number(event);
                  }}
                /></label
              >
              <label
                >最大值<input
                  aria-label="变量最大值"
                  type="number"
                  step="any"
                  value={draft.max ?? ""}
                  oninput={(event) => {
                    if (draft) draft.max = number(event);
                  }}
                /></label
              >
            </div>
            <label
              >说明<input
                aria-label="变量说明"
                maxlength="200"
                bind:value={draft.description}
              /></label
            >
            {#if inputs.length + (editingId ? 0 : 1) > 1}
              <label
                >显示顺序<Dropdown
                  label="变量显示顺序"
                  value={String(order)}
                  items={Array.from(
                    { length: inputs.length + (editingId ? 0 : 1) },
                    (_, index) => ({
                      value: String(index),
                      label: `第 ${index + 1} 项`,
                    }),
                  )}
                  onchange={(value) => (order = Number(value))}
                /></label
              >
            {/if}
          </div>
        </details>
        {#if error}<p class="error" role="status">{error}</p>{/if}
      </div>
      <div class="dialog-actions">
        <button type="button" onclick={() => dialog.close()}>取消</button>
        <button type="submit" class="primary" disabled={!!error}>保存</button>
      </div>
    </form>
  {/if}
</dialog>

<style>
  .definitions {
    display: grid;
    gap: 8px;
    min-width: 0;
  }
  .definitions-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h3 {
    font-size: 14px;
  }
  .variable-row {
    display: flex;
    align-items: center;
    gap: 0;
    min-width: 0;
    padding: 4px 6px 4px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel2);
  }
  .variable-label {
    flex: 1;
    min-width: 0;
    margin-right: 4px;
    font-size: 13px;
  }
  .variable-value {
    max-width: 64px;
    margin-right: 12px;
    font-size: 12px;
    color: var(--dim);
  }
  .variable-label,
  .variable-value {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  button {
    border: 1px solid var(--line);
    border-radius: 7px;
    background: var(--panel);
    color: var(--text);
    padding: 8px 12px;
    font-size: 13px;
  }
  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--dim);
  }
  .icon-button:hover:not(:disabled) {
    color: var(--accent2);
    background: var(--accentsoft);
  }
  .delete-button:hover:not(:disabled) {
    color: var(--red);
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .variable-dialog {
    width: 480px;
    max-width: calc(100vw - 32px);
    max-height: calc(100dvh - 32px);
    padding: 0;
    margin: auto;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: var(--panel);
    color: var(--text);
    overflow: hidden;
  }
  form {
    display: flex;
    flex-direction: column;
    max-height: calc(100dvh - 32px);
  }
  .dialog-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 20px 24px 16px;
    flex: none;
  }
  h2 {
    font-size: 18px;
  }
  .dialog-fields {
    display: grid;
    gap: 14px;
    padding: 0 24px 4px;
    min-height: 0;
    overflow-y: auto;
    scrollbar-gutter: stable;
  }
  label {
    display: grid;
    gap: 6px;
    min-width: 0;
    margin: 0;
    font-size: 13px;
  }
  input {
    width: 100%;
    min-width: 0;
  }
  summary {
    font-size: 13px;
    color: var(--dim);
    cursor: pointer;
  }
  .optional-fields {
    display: grid;
    gap: 14px;
    margin-top: 14px;
  }
  .range {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }
  .error {
    color: var(--red);
    font-size: 12px;
    line-height: 1.7;
    overflow-wrap: anywhere;
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 18px 24px 20px;
    flex: none;
  }
  .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--primary-ink);
  }
</style>
