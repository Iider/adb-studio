<script lang="ts">
  import {
    compatible,
    fieldError,
    inputError,
    isReference,
    unitLabel,
    type InputDefinition,
  } from "$lib/workflow-inputs";
  import type { DraftNode } from "$lib/workflow-editor";
  let {
    inputs,
    nodes,
    values = $bindable({}),
    disabled = false,
  }: {
    inputs: InputDefinition[];
    nodes: DraftNode[];
    values: Record<string, string>;
    disabled?: boolean;
  } = $props();
  function error(input: InputDefinition) {
    const text = values[input.id]?.trim() ?? "";
    const value = text ? Number(text) : input.default;
    const invalid = inputError(input, value);
    if (invalid) return invalid;
    for (const node of nodes)
      for (const [key, reference] of Object.entries(node.config)) {
        if (!isReference(reference) || reference.input !== input.id) continue;
        const problem = !compatible(input, key)
          ? "类型或单位不匹配"
          : fieldError(node.kind, key, value!);
        if (problem) return `${node.label}：${problem}`;
      }
    return "";
  }
</script>

{#if inputs.length}
  <fieldset {disabled}>
    <legend>本次运行参数</legend>
    <p>留空使用默认值，只影响这次运行。</p>
    {#each inputs as input (input.id)}
      {@const issue = error(input)}
      <label for={`run-input-${input.id}`}
        >{input.name || "未命名变量"}{unitLabel(input)
          ? `（${unitLabel(input)}）`
          : ""}</label
      >
      <input
        id={`run-input-${input.id}`}
        inputmode="decimal"
        autocomplete="off"
        value={values[input.id] ?? ""}
        aria-invalid={!!issue}
        aria-describedby={`run-input-help-${input.id}`}
        oninput={(event) =>
          (values = { ...values, [input.id]: event.currentTarget.value })}
      />
      <p id={`run-input-help-${input.id}`} class:error={!!issue}>
        {issue ||
          `${values[input.id]?.trim() ? "默认值" : "使用默认值"}：${input.default ?? "未填写"}${unitLabel(input)}`}
        {#if input.min != null || input.max != null}
          · 范围 {input.min ?? "不限"}～{input.max ?? "不限"}{/if}
      </p>
      {#if input.description}<p>{input.description}</p>{/if}
    {/each}
    <button type="button" onclick={() => (values = {})}>全部恢复默认</button>
  </fieldset>
{/if}

<style>
  fieldset {
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px;
    min-width: 0;
    margin-top: 16px;
  }
  legend,
  label {
    font-size: 13px;
  }
  label {
    display: block;
    margin: 12px 0 6px;
  }
  input {
    width: 100%;
    min-width: 0;
  }
  p {
    font-size: 12px;
    color: var(--dim);
    line-height: 1.7;
    margin: 5px 0;
  }
  .error {
    color: var(--red);
  }
  button {
    padding: 7px 10px;
    margin-top: 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--panel);
    color: var(--text);
    font-size: 13px;
  }
</style>
