<script lang="ts">
  // 单行口令输入弹窗（替代 Go 版的 window.prompt —— WebView 中不可靠）
  let {
    title,
    hint = "",
    placeholder = "",
    onSubmit,
    onCancel,
  }: {
    title: string;
    hint?: string;
    placeholder?: string;
    onSubmit: (value: string) => Promise<void>;
    onCancel: () => void;
  } = $props();

  let value = $state("");
  let error = $state("");
  let busy = $state(false);

  $effect(() => {
    setTimeout(() => valueInput?.focus(), 100);
  });

  let valueInput: HTMLInputElement | undefined = $state();

  async function confirm() {
    if (busy) return;
    busy = true;
    try {
      await onSubmit(value);
    } catch (e) {
      error = typeof e === "string" ? e : String(e);
      busy = false;
    }
  }

  function cancel() {
    value = "";
    error = "";
    onCancel();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="modal-mask" onclick={(e) => {
  if (e.target === e.currentTarget) cancel();
}}>
  <div class="modal card-verify">
    <h2 class="modal-title">{title}</h2>
    {#if hint}<p class="verify-hint">{hint}</p>{/if}

    <div class="password-form">
      <div class="form-group">
        <input
          id="promptInput"
          type="password"
          {placeholder}
          autocomplete="off"
          bind:this={valueInput}
          bind:value={value}
          onkeydown={(e) => {
            if (e.key === "Enter") confirm();
          }}
        />
      </div>
      <div id="promptError" class="password-error">{error}</div>
    </div>

    <div class="modal-actions">
      <button class="btn btn-ghost" type="button" onclick={cancel}>取消</button>
      <button id="promptConfirmBtn" class="btn btn-primary" type="button" disabled={busy} onclick={confirm}>
        确认
      </button>
    </div>
  </div>
</div>
