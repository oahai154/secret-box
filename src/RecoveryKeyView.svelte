<script lang="ts">
  // 恢复密钥确认页：首次设置主密码后的强制步骤（见 ADR-0003）。
  // 不设"跳过"——唯一退出路径是完成抄写确认后进入主界面。
  let {
    recoveryKey,
    onConfirmed,
  }: {
    recoveryKey: string;
    onConfirmed: () => void;
  } = $props();

  let typed = $state("");
  let acknowledged = $state(false);
  let copied = $state(false);
  let error = $state("");

  const groups = $derived(recoveryKey.split("-"));

  // 归一化比对：与后端一致（大写、去连字符/空格）
  function normalize(raw: string): string {
    return raw
      .toUpperCase()
      .replace(/[^0-9A-Z]/g, "");
  }

  const matched = $derived(
    typed.length > 0 && normalize(typed) === normalize(recoveryKey)
  );
  const canContinue = $derived(matched && acknowledged);

  async function copy() {
    try {
      await navigator.clipboard.writeText(recoveryKey);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      error = "复制失败，请手动抄写";
    }
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!canContinue) return;
    onConfirmed();
  }
</script>

<div class="auth-view">
  <form class="auth-card recovery-card" onsubmit={submit}>
    <div class="auth-mark">🔑</div>
    <h1>保存你的恢复密钥</h1>
    <div class="subtitle">
      忘记主密码时,唯一能找回数据的凭据。它无法重置或找回——请抄写在纸上并妥善保管。
    </div>
    <div id="recoveryCode" class="recovery-code" aria-label="恢复密钥">
      {#each groups as group, i}
        <span class="recovery-group">{group}</span>{#if i < groups.length - 1}<span class="recovery-dash">-</span>{/if}
      {/each}
    </div>
    <button id="recoveryCopy" type="button" class="btn btn-ghost" onclick={copy}>
      {copied ? "已复制 ✓" : "复制恢复密钥"}
    </button>
    <input
      id="recoveryInput"
      type="text"
      placeholder="输入恢复密钥以确认(不区分大小写)"
      autocomplete="off"
      spellcheck="false"
      bind:value={typed}
    />
    <label class="recovery-ack">
      <input id="recoveryConfirm" type="checkbox" bind:checked={acknowledged} />
      我已将它保存到安全的地方
    </label>
    <button id="recoveryBtn" class="btn btn-primary" type="submit" disabled={!canContinue}>
      完成并进入
    </button>
    <div id="recoveryError" class="auth-error">{error}</div>
  </form>
</div>

<style>
  .recovery-card {
    max-width: 460px;
  }
  .recovery-code {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0.15em 0.1em;
    padding: 0.9em 0.6em;
    border: 1px solid var(--border);
    border-radius: 8px;
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 1.05rem;
    letter-spacing: 0.06em;
    user-select: all;
    word-break: break-all;
  }
  .recovery-group {
    font-weight: 600;
  }
  .recovery-dash {
    opacity: 0.45;
  }
  .recovery-ack {
    display: flex;
    align-items: center;
    gap: 0.5em;
    font-size: 0.92rem;
    cursor: pointer;
    text-align: left;
  }
  .recovery-ack input {
    width: auto;
  }
</style>
