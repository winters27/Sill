<script lang="ts">
  interface Props {
    checked: boolean;
    onchange: (value: boolean) => void;
    label?: string;
    /** Set while the change it would make is still being carried out. */
    disabled?: boolean;
  }

  let { checked = $bindable(), onchange, label, disabled = false }: Props = $props();

  function flip() {
    if (disabled) return;

    checked = !checked;
    onchange(checked);
  }
</script>

<!--
  A pill switch rather than a checkbox.

  A native checkbox reads as a form control; a switch reads as a setting that
  is on or off right now, which is what every row here is.
-->
<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  class="switch"
  class:on={checked}
  {disabled}
  onclick={flip}
>
  <span class="knob"></span>
</button>

<style>
  /* Dimmed rather than hidden while it is busy: the row still says what it is
     set to, it simply cannot be set again until the last change has landed. */
  .switch:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .switch {
    flex: none;
    position: relative;
    width: 40px;
    height: 22px;
    padding: 0;
    border: 1px solid var(--hairline);
    border-radius: var(--radius-pill);
    background-color: color-mix(in srgb, var(--core-background) 60%, transparent);
    cursor: pointer;
    /* Scoped, never `all`: see the WebView2 note in theme.css. */
    transition:
      background-color 0.22s var(--ease),
      border-color 0.22s var(--ease);
  }

  .switch.on {
    background-color: var(--core-accent);
    border-color: transparent;
  }

  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #f4f5f6;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.4);
    transition: translate 0.22s var(--ease);
  }

  .switch.on .knob {
    translate: 18px 0;
  }

  .switch:focus-visible {
    outline: 2px solid var(--core-accent);
    outline-offset: 2px;
  }
</style>
