<script lang="ts" module>
  export interface DropdownItem {
    value: string;
    label: string;
    description?: string;
    disabled?: boolean;
  }
</script>

<script lang="ts" generics="T extends DropdownItem">
  import { onMount, type Snippet } from "svelte";
  import { Check, ChevronDown } from "@lucide/svelte";

  let {
    items,
    value = $bindable(""),
    label,
    placeholder = "请选择",
    disabled = false,
    class: className = "",
    selection,
    content,
    actions,
    leading,
    onchange,
  }: {
    items: T[];
    value?: string;
    label: string;
    placeholder?: string;
    disabled?: boolean;
    class?: string;
    selection?: Snippet<[T]>;
    content?: Snippet<[T]>;
    actions?: Snippet<[T, () => void]>;
    leading?: Snippet<[() => void]>;
    onchange?: (value: string) => void;
  } = $props();

  const uid = $props.id();
  const menuId = `${uid}-menu`;
  let trigger: HTMLButtonElement;
  let menu: HTMLDivElement;
  let open = $state(false);
  const selected = $derived(items.find((item) => item.value === value));
  let search = "";
  let lastTyped = 0;
  let focusLast = false;

  function controls() {
    return [
      ...menu.querySelectorAll<HTMLButtonElement>(
        '[role^="menuitem"]:not(:disabled)',
      ),
    ];
  }
  function position() {
    if (!open) return;
    const rect = trigger.getBoundingClientRect();
    const gap = 8;
    const width = Math.min(Math.max(rect.width, 320), window.innerWidth - 24);
    menu.style.width = `${width}px`;
    const below = window.innerHeight - rect.bottom - gap - 12;
    const above = rect.top - gap - 12;
    const height = Math.min(menu.scrollHeight + 2, 360);
    const upward = below < height && above > below;
    menu.style.maxHeight = `${Math.max(0, Math.min(360, upward ? above : below))}px`;
    menu.style.left = `${Math.max(12, Math.min(rect.left, window.innerWidth - width - 12))}px`;
    menu.style.top = `${upward ? Math.max(12, rect.top - gap - Math.min(height, above)) : rect.bottom + gap}px`;
  }
  function afterToggle(event: ToggleEvent) {
    if (event.newState !== "open") return;
    search = "";
    position();
    const buttons = controls();
    const current = menu.querySelector<HTMLButtonElement>(
      '[aria-checked="true"]:not(:disabled)',
    );
    (focusLast ? buttons.at(-1) : current || buttons[0])?.focus();
    focusLast = false;
  }
  function close(restoreFocus = true) {
    menu.hidePopover();
    open = false;
    if (restoreFocus) trigger.focus({ preventScroll: true });
  }
  function choose(item: T) {
    value = item.value;
    onchange?.(item.value);
    close();
  }
  function triggerKey(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      focusLast = event.key === "ArrowUp";
      trigger.click();
    }
  }
  function menuKey(event: KeyboardEvent) {
    const buttons = controls();
    const current = buttons.indexOf(
      document.activeElement as HTMLButtonElement,
    );
    let next: number | undefined;
    if (event.key === "ArrowDown") next = (current + 1) % buttons.length;
    if (event.key === "ArrowUp")
      next = (current - 1 + buttons.length) % buttons.length;
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = buttons.length - 1;
    if (next !== undefined) {
      event.preventDefault();
      buttons[next]?.focus();
    } else if (event.key === "Escape") {
      event.preventDefault();
      close();
    } else if (event.key === "Tab") {
      close();
    } else if (
      event.key.length === 1 &&
      event.key !== " " &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.altKey
    ) {
      event.preventDefault();
      const now = performance.now();
      search =
        (now - lastTyped < 700 ? search : "") + event.key.toLocaleLowerCase();
      lastTyped = now;
      const options = buttons.filter(
        (button) => button.getAttribute("role") === "menuitemradio",
      );
      const start = options.indexOf(
        document.activeElement as HTMLButtonElement,
      );
      for (let offset = 1; offset <= options.length; offset++) {
        const option = options[(start + offset) % options.length];
        if (option.dataset.label?.toLocaleLowerCase().startsWith(search)) {
          option.focus();
          break;
        }
      }
    }
  }
  onMount(() => {
    window.addEventListener("resize", position);
    window.addEventListener("scroll", position, true);
    return () => {
      window.removeEventListener("resize", position);
      window.removeEventListener("scroll", position, true);
    };
  });
  $effect(() => {
    if ((disabled || (!items.length && !leading)) && open) close(false);
  });
</script>

<button
  bind:this={trigger}
  type="button"
  class={`dropdown-trigger ${className}`}
  class:open
  aria-label={label}
  aria-haspopup="menu"
  aria-controls={menuId}
  aria-expanded={open}
  popovertarget={menuId}
  disabled={disabled || (!items.length && !leading)}
  onkeydown={triggerKey}
>
  <span class="dropdown-selection">
    {#if selected && selection}{@render selection(selected)}
    {:else}{selected?.label || placeholder}{/if}
  </span>
  <ChevronDown size={14} class="dropdown-chevron" aria-hidden="true" />
</button>
<div
  bind:this={menu}
  id={menuId}
  popover="auto"
  role="menu"
  aria-label={label}
  tabindex="-1"
  class="dropdown-menu"
  onkeydown={menuKey}
  onbeforetoggle={(event) => (open = event.newState === "open")}
  ontoggle={afterToggle}
>
  {#if leading}<div class="dropdown-leading">{@render leading(close)}</div>{/if}
  {#each items as item (item.value)}
    <div class="dropdown-row" class:selected={item.value === value}>
      <button
        type="button"
        role="menuitemradio"
        aria-checked={item.value === value}
        disabled={item.disabled}
        tabindex="-1"
        class="dropdown-option"
        data-label={item.label}
        onclick={() => choose(item)}
      >
        <span class="dropdown-content">
          {#if content}{@render content(item)}
          {:else}<span class="dropdown-label">{item.label}</span>
            {#if item.description}<small>{item.description}</small>{/if}
          {/if}
        </span>
        <Check
          size={16}
          class={item.value === value
            ? "dropdown-check"
            : "dropdown-check hidden-check"}
          aria-hidden="true"
        />
      </button>
      {#if actions}<div class="dropdown-actions">
          {@render actions(item, close)}
        </div>{/if}
    </div>
  {/each}
</div>

<style>
  .dropdown-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    min-height: 40px;
    padding: 8px 12px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: var(--panel);
    color: var(--text);
    font-size: 13px;
    font-weight: 600;
    text-align: left;
    transition:
      border-color 120ms,
      background 120ms;
  }
  .dropdown-trigger:hover:not(:disabled),
  .dropdown-trigger.open {
    border-color: var(--accent);
    background: var(--panel2);
  }
  .dropdown-trigger:disabled {
    opacity: 0.55;
  }
  .dropdown-selection {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dropdown-trigger :global(.dropdown-chevron) {
    flex: none;
    color: var(--dim);
    transition: transform 120ms;
  }
  .dropdown-trigger.open :global(.dropdown-chevron) {
    transform: rotate(180deg);
  }
  .dropdown-menu {
    position: fixed;
    inset: auto;
    width: 320px;
    max-width: calc(100vw - 24px);
    max-height: 360px;
    margin: 0;
    padding: 6px;
    overflow-y: auto;
    scrollbar-gutter: stable;
    overscroll-behavior: contain;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel);
    color: var(--text);
    box-shadow: 0 16px 48px #0003;
  }
  .dropdown-row {
    display: flex;
    align-items: center;
    gap: 2px;
    border-radius: 8px;
  }
  .dropdown-leading {
    padding-bottom: 6px;
    margin-bottom: 6px;
    border-bottom: 1px solid var(--line);
  }
  .dropdown-row + .dropdown-row {
    margin-top: 3px;
  }
  .dropdown-row.selected {
    background: var(--accentsoft);
  }
  .dropdown-option {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 12px;
    min-width: 0;
    min-height: 48px;
    padding: 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text);
    text-align: left;
  }
  .dropdown-option:hover:not(:disabled) {
    background: var(--panel2);
  }
  .selected .dropdown-option:hover {
    background: var(--accentsoft);
  }
  .dropdown-option:focus-visible {
    outline-offset: -2px;
  }
  .dropdown-option:disabled {
    opacity: 0.5;
  }
  .dropdown-content {
    display: block;
    flex: 1;
    min-width: 0;
  }
  .dropdown-label {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    font-weight: 600;
  }
  .dropdown-content small {
    display: block;
    margin-top: 3px;
    color: var(--dim);
  }
  .dropdown-option :global(.dropdown-check) {
    flex: none;
    color: var(--accent2);
  }
  .dropdown-option :global(.hidden-check) {
    visibility: hidden;
  }
  .dropdown-actions {
    display: flex;
    flex: none;
    align-items: center;
    padding-right: 4px;
  }
  @media (max-width: 600px) {
    .dropdown-trigger {
      min-height: 44px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .dropdown-trigger,
    .dropdown-trigger :global(.dropdown-chevron) {
      transition: none;
    }
  }
</style>
