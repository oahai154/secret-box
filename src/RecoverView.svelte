<script lang="ts">
  import { ipc } from "./ipc";

  // 救援页：忘记主密码时凭恢复密钥重设主密码（见 ADR-0003）。
  // 这是救援的唯一路径——无密保问题、无验证码、无找回。
  let {
    onRecovered,
    onBack,
  }: {
    onRecovered: () => void;
    onBack: () => void;
  } = $props();

  let recoveryKey = $state("");
  let password = $state("");
  let password2 = $state("");
  let error = $state("");
  let busy = $state(false);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    error = "";
    if (!recoveryKey.trim()) {
      error = "请输入恢复密钥";
      return;
    }
    if (!password) {
      error = "请输入新主密码";
      return;
    }
    if (password !== password2) {
      error = "两次输入不一致";
      return;
    }
    busy = true;
    try {
      await ipc.recoverPassword(recoveryKey, password);
      onRecovered();
    } catch (e) {
      error = typeof e === "string" ? e : String(e);
    } finally {
      busy = false;
      recoveryKey = "";
      password = "";
      password2 = "";
    }
  }
</script>

<div class="auth-view">
  <form class="auth-card" onsubmit={submit}>
    <div class="auth-mark">🛟</div>
    <h1>恢复访问</h1>
    <div class="subtitle">
      输入恢复密钥并设置新主密码。数据保持不变；恢复密钥继续有效。
    </div>
    <input
      id="recoverKeyInput"
      type="text"
      placeholder="恢复密钥(不区分大小写)"
      autocomplete="off"
      spellcheck="false"
      bind:value={recoveryKey}
    />
    <input
      id="recoverPassword"
      type="password"
      placeholder="新主密码(≥4 位)"
      autocomplete="off"
      bind:value={password}
    />
    <input
      id="recoverPassword2"
      type="password"
      placeholder="再次确认新主密码"
      autocomplete="off"
      bind:value={password2}
    />
    <button id="recoverBtn" class="btn btn-primary" type="submit" disabled={busy}>
      {#if busy}恢复中…{:else}重设主密码并进入{/if}
    </button>
    <div id="recoverError" class="auth-error">{error}</div>
    <button id="recoverBack" type="button" class="btn btn-ghost" onclick={onBack}>
      返回解锁
    </button>
  </form>
</div>