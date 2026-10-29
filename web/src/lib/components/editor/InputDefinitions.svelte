<script lang="ts">
  import Dropdown from "$lib/components/ui/dropdown/dropdown.svelte";
  import {
    inputError,
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
  function update(id: string, patch: Partial<InputDefinition>) {
    onchange(
      inputs.map((input) => (input.id === id ? { ...input, ...patch } : input)),
    );
  }
  function references(id: string) {
    return nodes
      .filter((node) =>
        Object.values(node.data.config).some(
          (value) => isReference(value) && value.input === id,
        ),
      )
      .map((node) => node.data.label);
  }
  function move(index: number, direction: number) {
    const next = [...inputs];
    [next[index], next[index + direction]] = [
      next[index + direction],
      next[index],
    ];
    onchange(next);
  }
</script>

<div class="definitions">
  <h3>运行变量</h3>
  <p>在其他节点选择使用变量；每次运行可临时填写，留空用默认值。</p>
  {#each inputs as input, index (input.id)}
    {@const used = references(input.id)}
    {@const error = !input.name.trim()
      ? "请填写变量名称"
      : inputs.some(
            (other) =>
              other.id !== input.id && other.name.trim() === input.name.trim(),
          )
        ? "变量名称不能重复"
        : inputError(input, input.default)}
    <fieldset>
      <legend>变量 {index + 1}</legend>
      <label
        >名称<input
          aria-label={`变量 ${index + 1} 名称`}
          maxlength="40"
          value={input.name}
          oninput={(event) =>
            update(input.id, { name: event.currentTarget.value })}
        /></label
      >
      <label
        >类型<Dropdown
          label={`变量 ${index + 1} 类型`}
          value={input.type}
          items={[
            { value: "integer", label: "整数（次数、轮数）" },
            { value: "number", label: "数字（可带小数）" },
          ]}
          onchange={(value) =>
            update(input.id, { type: value as InputDefinition["type"] })}
        /></label
      >
      <label
        >默认值<input
          aria-label={`变量 ${index + 1} 默认值`}
          type="number"
          step={input.type === "integer" ? 1 : "any"}
          value={input.default ?? ""}
          oninput={(event) =>
            update(input.id, {
              default: Number.isFinite(event.currentTarget.valueAsNumber)
                ? event.currentTarget.valueAsNumber
                : null,
            })}
        /></label
      >
      {#if error}<p class="error" role="status">{error}</p>{/if}
      <details>
        <summary>单位与范围（选填）</summary>
        <label
          >单位<Dropdown
            label={`变量 ${index + 1} 单位`}
            value={input.unit ?? ""}
            items={[
              { value: "", label: "无" },
              { value: "seconds", label: "秒" },
              { value: "milliseconds", label: "毫秒" },
            ]}
            onchange={(value) =>
              update(input.id, { unit: value as InputDefinition["unit"] })}
          /></label
        >
        <div class="range">
          <label
            >最小值<input
              type="number"
              step="any"
              value={input.min ?? ""}
              oninput={(event) =>
                update(input.id, {
                  min: Number.isFinite(event.currentTarget.valueAsNumber)
                    ? event.currentTarget.valueAsNumber
                    : null,
                })}
            /></label
          >
          <label
            >最大值<input
              type="number"
              step="any"
              value={input.max ?? ""}
              oninput={(event) =>
                update(input.id, {
                  max: Number.isFinite(event.currentTarget.valueAsNumber)
                    ? event.currentTarget.valueAsNumber
                    : null,
                })}
            /></label
          >
        </div>
        <label
          >说明<input
            maxlength="200"
            value={input.description ?? ""}
            oninput={(event) =>
              update(input.id, { description: event.currentTarget.value })}
          /></label
        >
      </details>
      {#if used.length}<p>已用于：{used.join("、")}。解除引用后可删除。</p>{/if}
      <div class="actions">
        <button
          disabled={index === 0}
          aria-label={`上移变量 ${index + 1}`}
          onclick={() => move(index, -1)}>上移</button
        >
        <button
          disabled={index === inputs.length - 1}
          aria-label={`下移变量 ${index + 1}`}
          onclick={() => move(index, 1)}>下移</button
        >
        <button
          disabled={used.length > 0}
          onclick={() =>
            onchange(inputs.filter((item) => item.id !== input.id))}
          >删除</button
        >
      </div>
    </fieldset>
  {/each}
  <button
    disabled={inputs.length >= 20}
    onclick={() =>
      onchange([
        ...inputs,
        { id: crypto.randomUUID(), name: "", type: "number", default: 0 },
      ])}>添加变量</button
  >
</div>

<style>
  .definitions {
    display: grid;
    gap: 12px;
    min-width: 0;
  }
  h3 {
    font-size: 14px;
  }
  fieldset {
    min-width: 0;
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  legend,
  label,
  summary,
  button {
    font-size: 13px;
  }
  label {
    display: grid;
    gap: 6px;
    margin: 10px 0;
  }
  .range {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }
  input {
    width: 100%;
    min-width: 0;
  }
  p {
    color: var(--dim);
    font-size: 12px;
    line-height: 1.7;
    overflow-wrap: anywhere;
  }
  .error {
    color: var(--red);
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
  button {
    padding: 7px 10px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--panel);
    color: var(--text);
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
