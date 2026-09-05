  <script lang="ts">
  import { ipc } from "./ipc";

  let hasPassword = $state(true);
  let password = $state("");
  let error = $state("");
  let busy = $state(false);
  let unlocked = $state(false);
  let itemCount = $state(0);

  function focusOnMount(node: HTMLElement) {
    node.focus();
  }

  $effect(() => {
    ipc
      .getStatus()
      .then((status) => {
        hasPassword = status.has_password;
      })
      .catch((e) => {
        error = String(e);
      });
  });

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    busy = true;
    error = "";
    try {
      await ipc.unlock(password);
      const items = await ipc.listItems();
      itemCount = items.length;
      unlocked = true;
    } catch (e) {
      error = typeof e === "string" ? e : String(e);
    } finally {
      busy = false;
      password = "";
    }
  }
</script>

{#if !unlocked}
  <div class="auth-view">
    <form class="auth-card" onsubmit={submit}>
      <div class="auth-mark">🔒</div>
      <h1>SecretBox</h1>
      <div class="subtitle">隐 私 保 险 箱</div>
      {#if hasPassword}
        <input
          type="password"
          placeholder="输入主密码"
          autocomplete="off"
          use:focusOnMount
          bind:value={password}
        />
        <button class="btn btn-primary" type="submit" disabled={busy}>
          {busy ? "解锁中…" : "解锁"}
        </button>
      {:else}
        <div class="auth-hint">尚未设置主密码，该功能将在后续版本提供</div>
      {/if}
      <div class="auth-error">{error}</div>
    </form>
  </div>
{:else}
  <div class="unlocked-view">
    <div class="unlocked-card">
      <div class="auth-mark">🔓</div>
      <h1>已解锁</h1>
      <div class="subtitle" data-testid="item-count">共 {itemCount} 条条目</div>
    </div>
  </div>
{/if}
