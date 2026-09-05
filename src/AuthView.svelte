<script lang="ts">
  import { ipc } from "./ipc";

  let {
    hasPassword,
    onUnlocked,
    onSetupCompleted,
  }: {
    hasPassword: boolean;
    onUnlocked: () => void;
    onSetupCompleted: () => void;
  } = $props();

  let password = $state("");
  let password2 = $state("");
  let error = $state("");
  let busy = $state(false);

  // 首次使用（未设置主密码）时进入设置模式，文案与 Go 版 configureAuthMode 一致
  const isSetup = $derived(!hasPassword);

  function focusOnMount(node: HTMLElement) {
    node.focus();
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    error = "";
    if (!password) {
      error = "请输入主密码";
      return;
    }
    if (isSetup && password !== password2) {
      error = "两次输入不一致";
      return;
    }
    busy = true;
    try {
      if (isSetup) {
        await ipc.setupPassword(password);
        onSetupCompleted();
      } else {
        await ipc.unlock(password);
        onUnlocked();
      }
    } catch (e) {
      error = typeof e === "string" ? e : String(e);
    } finally {
      busy = false;
      password = "";
      password2 = "";
    }
  }
</script>

<div class="auth-view">
  <form class="auth-card" onsubmit={submit}>
    <div class="auth-mark">🔒</div>
    <h1>SecretBox</h1>
    <div class="subtitle">
      {isSetup ? "首次使用,设置主密码以加密存储" : "请输入主密码解锁"}
    </div>
    <input
      id="authPassword"
      type="password"
      placeholder={isSetup ? "设置主密码(≥4 位)" : "主密码"}
      autocomplete="off"
      use:focusOnMount
      bind:value={password}
    />
    {#if isSetup}
      <input
        id="authPassword2"
        type="password"
        placeholder="再次确认主密码"
        autocomplete="off"
        bind:value={password2}
      />
    {/if}
    <button id="authBtn" class="btn btn-primary" type="submit" disabled={busy}>
      {#if busy}解锁中…{:else}{isSetup ? "创建并进入" : "解锁"}{/if}
    </button>
    <div id="authError" class="auth-error">{error}</div>
  </form>
</div>
