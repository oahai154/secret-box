<script lang="ts">
  // 修改密码弹窗（复刻 Go 版 changePasswordModal）
  let {
    onSubmit,
    onCancel,
  }: {
    onSubmit: (oldPassword: string, newPassword: string) => Promise<void>;
    onCancel: () => void;
  } = $props();

  let oldPassword = $state("");
  let newPassword = $state("");
  let confirmPassword = $state("");
  let error = $state("");
  let busy = $state(false);

  $effect(() => {
    // 打开时聚焦输入框（与 Go 版一致）
    setTimeout(() => oldPasswordInput?.focus(), 100);
  });

  let oldPasswordInput: HTMLInputElement | undefined = $state();

  async function confirm() {
    if (busy) return;
    error = "";
    if (!oldPassword) { error = "请输入当前密码"; return; }
    if (newPassword.length < 4) { error = "新密码至少 4 位"; return; }
    if (newPassword !== confirmPassword) { error = "两次输入的新密码不一致"; return; }
    busy = true;
    try {
      await onSubmit(oldPassword, newPassword);
    } catch (e) {
      error = typeof e === "string" ? e : String(e);
      busy = false;
    }
  }

  function cancel() {
    oldPassword = "";
    newPassword = "";
    confirmPassword = "";
    error = "";
    onCancel();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="modal-mask" onclick={(e) => {
  if (e.target === e.currentTarget) cancel();
}}>
  <div class="modal card-password">
    <h2 class="modal-title">修改密码</h2>

    <div class="password-form">
      <div class="form-group">
        <label class="field-label" for="oldPasswordInput">当前密码</label>
        <input id="oldPasswordInput" type="password" placeholder="输入当前密码" autocomplete="off"
          bind:this={oldPasswordInput} bind:value={oldPassword} />
      </div>
      <div class="form-group">
        <label class="field-label" for="newPasswordInput">新密码</label>
        <input id="newPasswordInput" type="password" placeholder="输入新密码(至少 4 位)" autocomplete="off"
          bind:value={newPassword} />
      </div>
      <div class="form-group">
        <label class="field-label" for="confirmPasswordInput">确认新密码</label>
        <input id="confirmPasswordInput" type="password" placeholder="再次输入新密码" autocomplete="off"
          bind:value={confirmPassword}
          onkeydown={(e) => {
            if (e.key === "Enter") confirm();
          }} />
      </div>
      <div id="passwordError" class="password-error">{error}</div>
    </div>

    <div class="modal-actions">
      <button id="cancelPasswordBtn" class="btn btn-ghost" type="button" onclick={cancel}>取消</button>
      <button id="confirmPasswordBtn" class="btn btn-primary" type="button" disabled={busy} onclick={confirm}>
        确认修改
      </button>
    </div>
  </div>
</div>
