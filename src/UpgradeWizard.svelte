<script lang="ts">
  import { ipc } from "./ipc";

  // v1 旧库强制升级向导（见 ADR-0003）：检测到旧版数据库时拦截，
  // 无"跳过"路径——完成升级（含恢复密钥确认）前无法进入主界面。
  let {
    onUpgraded,
  }: {
    onUpgraded: (recoveryKey: string) => void;
  } = $props();

  let password = $state("");
  let error = $state("");
  let busy = $state(false);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    error = "";
    if (!password) {
      error = "请输入主密码";
      return;
    }
    busy = true;
    try {
      const result = await ipc.upgradeV1(password);
      onUpgraded(result.recovery_key);
    } catch (e) {
      error = typeof e === "string" ? e : String(e);
    } finally {
      busy = false;
      password = "";
    }
  }
</script>

<div class="auth-view">
  <form class="auth-card" onsubmit={submit}>
    <div class="auth-mark">⬆️</div>
    <h1>升级存储格式</h1>
    <div class="subtitle">
      检测到旧版本数据库。为支持"忘记主密码时凭恢复密钥找回",需要一次性升级。
      数据与历史版本保持不变,主密码不变,升级完成后需保存新的恢复密钥。
    </div>
    <input
      id="upgradePassword"
      type="password"
      placeholder="输入当前主密码"
      autocomplete="off"
      bind:value={password}
    />
    <button id="upgradeBtn" class="btn btn-primary" type="submit" disabled={busy}>
      {#if busy}升级中,请稍候…{:else}开始升级{/if}
    </button>
    <div id="upgradeError" class="auth-error">{error}</div>
  </form>
</div>
