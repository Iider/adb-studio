<script lang="ts">
  import Dropdown from "$lib/components/ui/dropdown/dropdown.svelte";
  import type { Config, ConditionSample } from "$lib/workflow-editor";
  let {
    config: c,
    onchange,
  }: { config: Config; onchange: (config: Config) => void } = $props();
  function set(index: number, patch: Partial<ConditionSample>) {
    onchange({
      ...c,
      samples: c.samples?.map((sample, i) =>
        i === index ? { ...sample, ...patch } : sample,
      ),
    });
  }
</script>

<h3>判断样本</h3>
<p>
  用已知结果检查条件。页面文字需有“符合、不符合、无法确定”三种样本；应用和轮次需有符合与不符合样本。
</p>
{#each c.samples ?? [] as sample, index}
  <fieldset>
    <legend>样本 {index + 1}</legend>
    <label
      >预期结果<Dropdown
        label="预期结果"
        value={sample.expected}
        items={[
          { value: "yes", label: "符合" },
          { value: "no", label: "不符合" },
          { value: "unknown", label: "无法确定" },
        ]}
        onchange={(value) =>
          set(index, { expected: value as ConditionSample["expected"] })}
      /></label
    >
    {#if c.conditions?.some((item) => item.source === "text")}<label
        >样本页面文字<textarea
          rows="3"
          value={sample.text}
          oninput={(event) => set(index, { text: event.currentTarget.value })}
        ></textarea></label
      >{/if}
    {#if c.conditions?.some((item) => item.source === "app")}<label
        >样本应用编号<input
          value={sample.app}
          oninput={(event) => set(index, { app: event.currentTarget.value })}
        /></label
      >{/if}
    {#if c.conditions?.some((item) => item.source === "round")}<label
        >样本轮次<input
          type="number"
          min="1"
          value={sample.round}
          oninput={(event) =>
            set(index, { round: event.currentTarget.valueAsNumber || 1 })}
        /></label
      >{/if}
    <button
      onclick={() =>
        onchange({ ...c, samples: c.samples?.filter((_, i) => i !== index) })}
      >删除样本</button
    >
  </fieldset>
{/each}
<button
  disabled={(c.samples?.length ?? 0) >= 40}
  onclick={() =>
    onchange({
      ...c,
      samples: [
        ...(c.samples ?? []),
        { text: "", app: "", round: 1, expected: "yes" },
      ],
    })}>添加判断样本</button
>

<style>
  h3 {
    font-size: 14px;
    margin: 24px 0 8px;
  }
  p {
    font-size: 12px;
    line-height: 1.7;
    color: var(--dim);
  }
  label {
    font-size: 12px;
    margin: 12px 0;
  }
  fieldset {
    min-width: 0;
    border: 1px solid var(--line);
    border-radius: 9px;
    margin: 16px 0;
    padding: 0 12px 12px;
  }
  legend {
    font-size: 12px;
    padding: 0 6px;
    color: var(--dim);
  }
  textarea {
    width: 100%;
    padding: 8px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel2);
    color: var(--text);
    font: inherit;
    line-height: 1.6;
  }
  button {
    width: 100%;
    min-height: 40px;
    padding: 8px;
    border: 1px solid var(--line);
    border-radius: 8px;
    color: var(--text);
    background: var(--panel);
  }
</style>
