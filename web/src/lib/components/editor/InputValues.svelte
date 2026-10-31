<script lang="ts">
  import {
    compatible,
    fieldError,
    inputError,
    inputLabel,
    isReference,
    isRandom,
    inputReferences,
    randomError,
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
        if (!inputReferences(reference).includes(input.id)) continue;
        if (isRandom(reference)) {
          const problem = randomError(
            node.kind,
            key,
            reference,
            inputs,
            Object.fromEntries(
              inputs.map((item) => [
                item.id,
                values[item.id]?.trim()
                  ? Number(values[item.id])
                  : (item.default ?? NaN),
              ]),
            ),
          );
          if (problem) return `${node.label}：${problem}`;
          continue;
        }
        if (!isReference(reference)) continue;
        const problem = !compatible(input, key)
          ? "类型不匹配"
          : fieldError(node.kind, key, value!);
        if (problem) return `${node.label}：${problem}`;
      }
    return "";
  }
</script>

{#if inputs.length}
  <fieldset {disabled} aria-label="本次运行参数">
    <div class="input-fields">
      {#each inputs as input (input.id)}
        {@const issue = error(input)}
        <div class="input-field">
          <label for={`run-input-${input.id}`}>{inputLabel(input)}</label>
          <input
            id={`run-input-${input.id}`}
            inputmode="decimal"
            autocomplete="off"
            value={values[input.id] ?? input.default ?? ""}
            aria-invalid={!!issue}
            aria-describedby={issue || input.description
              ? `run-input-help-${input.id}`
              : undefined}
            oninput={(event) =>
              (values = { ...values, [input.id]: event.currentTarget.value })}
          />
          {#if issue || input.description}
            <p id={`run-input-help-${input.id}`} class:error={!!issue}>
              {issue || input.description}
            </p>
          {/if}
        </div>
      {/each}
    </div>
  </fieldset>
{/if}

<style>
  fieldset {
    border: 0;
    padding: 0;
    min-width: 0;
    width: fit-content;
    max-width: 100%;
    box-sizing: border-box;
    margin: 16px 0 0;
  }
  .input-fields {
    display: flex;
    flex-wrap: wrap;
    gap: 12px 16px;
  }
  .input-field {
    flex: 0 1 180px;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  label {
    font-size: 13px;
    display: block;
    margin: 0 0 6px;
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
</style>
