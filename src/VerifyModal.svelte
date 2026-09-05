<script lang="ts">
  // 主密码验证弹窗（复刻 Go 版 verifyPasswordModal），用于删除等敏感操作确认
  let {
    hint,
    onVerify,
    onCancel,
  }: {
    hint: string;
    onVerify: (password: string) => Promise<void>;
    onCancel: () => void;
  } = $props();

  let password = $state("");
  let error = $state("");
  let busy = $state(false);

  $effect(() => {
    // 打开时聚焦输入框
    setTimeout(() => passwordInput?.focus(), 100);
  });

  let passwordInput: HTMLInputElement | undefined = $state();

  async function confirm() {
    if (busy) return;
    if (!password) {
      error = "请输入密码";
      return;
    }
    busy = true;
    try {
      await onVerify(password);
    } catch {
      error = "密码不正确";
      busy = false;
      return;
    }
    busy = false;
  }

  function cancel() {
    password = "";
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
    <h2 class="modal-title">验证密码</h2>
    <p class="verify-hint">{hint}</p>

    <div class="password-form">
      <div class="form-group">
        <input
          id="verifyPasswordInput"
          type="password"
          placeholder="输入主密码"
          autocomplete="off"
          bind:this={passwordInput}
          bind:value={password}
          onkeydown={(e) => {
            if (e.key === "Enter") confirm();
          }}
        />
      </div>
      <div class="password-error">{error}</div>
    </div>

    <div class="modal-actions">
      <button class="btn btn-ghost" type="button" onclick={cancel}>取消</button>
      <button id="confirmVerifyBtn" class="btn btn-primary" type="button" disabled={busy} onclick={confirm}>
        确认
      </button>
    </div>
  </div>
</div>
