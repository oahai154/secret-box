<script lang="ts">
  // 设置弹窗（复刻 Go 版 settingsModal，v2 增加恢复密钥区，见 ADR-0003）
  import { ipc, type Settings, type AppInfo } from "./ipc";

  let {
    settings,
    onSave,
    onClose,
    onChangePassword,
    onToast,
  }: {
    settings: Settings;
    onSave: (updates: Settings) => void;
    onClose: () => void;
    onChangePassword: () => void;
    onToast?: (message: string, type?: string) => void;
  } = $props();

  let autoLock = $state("120");
  let deleteRequiresPassword = $state(true);
  let deleteVersionRequiresPassword = $state(true);

  // 恢复密钥区
  let recoveryKey = $state<string | null>(null);
  let showingRecovery = $state(false);
  let regenerating = $state(false);
  let regenPassword = $state("");
  let regenError = $state("");
  let copiedRecovery = $state(false);

  // "关于"区：项目地址取自 git remote（GitHub 主仓 + Gitee 镜像），版本来自后端
  const GITHUB_URL = "https://github.com/oahai154/secret-box";
  const GITEE_URL = "https://gitee.com/mweidian/secret-box";
  let appInfo = $state<AppInfo | null>(null);

  $effect(() => {
    autoLock = settings.auto_lock_seconds ?? "120";
    deleteRequiresPassword = settings.delete_requires_password !== "false";
    deleteVersionRequiresPassword = settings.delete_version_requires_password !== "false";
    ipc.getAppInfo().then((info) => (appInfo = info)).catch(() => {});
  });

  async function viewRecoveryKey() {
    regenError = "";
    try {
      recoveryKey = await ipc.getRecoveryKey();
      showingRecovery = true;
    } catch (e) {
      onToast?.(typeof e === "string" ? e : String(e), "err");
    }
  }

  async function copyRecoveryKey() {
    if (!recoveryKey) return;
    try {
      await navigator.clipboard.writeText(recoveryKey);
      copiedRecovery = true;
      setTimeout(() => (copiedRecovery = false), 2000);
    } catch {
      regenError = "复制失败，请手动抄写";
    }
  }

  function startRegenerate() {
    regenerating = true;
    regenPassword = "";
    regenError = "";
  }

  async function confirmRegenerate() {
    regenError = "";
    if (!regenPassword) {
      regenError = "请输入主密码";
      return;
    }
    try {
      recoveryKey = await ipc.regenerateRecoveryKey(regenPassword);
      regenerating = false;
      regenPassword = "";
      onToast?.("恢复密钥已重新生成,旧恢复密钥已作废");
    } catch (e) {
      regenError = typeof e === "string" ? e : String(e);
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="modal-mask" onclick={(e) => {
  if (e.target === e.currentTarget) onClose();
}}>
  <div class="modal card-settings">
    <h2 class="modal-title">设置</h2>

    <div class="settings-section">
      <div class="settings-row">
        <div class="settings-label">
          <div class="settings-label-title">自动锁定</div>
          <div class="settings-label-desc">超过指定时间无操作自动锁定</div>
        </div>
        <div class="settings-control">
          <select
            id="autoLockSelect"
            class="settings-select"
            bind:value={autoLock}
            onchange={() => onSave({ auto_lock_seconds: autoLock })}
          >
            <option value="60">1 分钟</option>
            <option value="120">2 分钟</option>
            <option value="300">5 分钟</option>
            <option value="600">10 分钟</option>
            <option value="0">永不</option>
          </select>
        </div>
      </div>
    </div>

    <div class="settings-section">
      <div class="settings-row">
        <div class="settings-label">
          <div class="settings-label-title">删除条目需输入密码</div>
          <div class="settings-label-desc">防止误删,删除时要求验证主密码</div>
        </div>
        <div class="settings-control">
          <label class="toggle-switch">
            <input
              type="checkbox"
              id="deleteRequiresPassword"
              bind:checked={deleteRequiresPassword}
              onchange={() => onSave({ delete_requires_password: deleteRequiresPassword ? "true" : "false" })}
            />
            <span class="toggle-slider"></span>
          </label>
        </div>
      </div>
    </div>

    <div class="settings-section">
      <div class="settings-row">
        <div class="settings-label">
          <div class="settings-label-title">删除历史版本需输入密码</div>
          <div class="settings-label-desc">删除历史版本时要求验证主密码</div>
        </div>
        <div class="settings-control">
          <label class="toggle-switch">
            <input
              type="checkbox"
              id="deleteVersionRequiresPassword"
              bind:checked={deleteVersionRequiresPassword}
              onchange={() =>
                onSave({ delete_version_requires_password: deleteVersionRequiresPassword ? "true" : "false" })}
            />
            <span class="toggle-slider"></span>
          </label>
        </div>
      </div>
    </div>

    <div class="settings-section">
      <div class="settings-row">
        <div class="settings-label">
          <div class="settings-label-title">恢复密钥</div>
          <div class="settings-label-desc">忘记主密码时找回数据的唯一凭据,请妥善保管</div>
        </div>
        <div class="settings-control">
          {#if !showingRecovery}
            <button id="viewRecoveryBtn" class="btn btn-ghost btn-sm" type="button" onclick={viewRecoveryKey}>
              查看
            </button>
          {:else}
            <button id="regenerateRecoveryBtn" class="btn btn-ghost btn-sm" type="button" onclick={startRegenerate}>
              重新生成
            </button>
          {/if}
        </div>
      </div>
      {#if showingRecovery}
        <div class="recovery-view">
          <div id="recoveryKeyDisplay" class="recovery-code-inline">{recoveryKey ?? "未设置"}</div>
          <button id="copyRecoveryBtn" class="btn btn-ghost btn-sm" type="button" onclick={copyRecoveryKey}>
            {copiedRecovery ? "已复制 ✓" : "复制"}
          </button>
          {#if regenerating}
            <div class="regen-row">
              <input
                id="regenPasswordInput"
                type="password"
                placeholder="输入主密码以确认重生成"
                autocomplete="off"
                bind:value={regenPassword}
              />
              <button id="regenConfirmBtn" class="btn btn-primary btn-sm" type="button" onclick={confirmRegenerate}>
                确认重生成
              </button>
            </div>
          {/if}
          <div class="settings-label-desc">
            重新生成后旧恢复密钥立即作废,新恢复密钥需要重新保存。
          </div>
          <div class="auth-error">{regenError}</div>
        </div>
      {/if}
    </div>

    <div class="settings-section">
      <div class="settings-row">
        <div class="settings-label">
          <div class="settings-label-title">修改密码</div>
          <div class="settings-label-desc">更改主密码,所有数据将重新加密</div>
        </div>
        <div class="settings-control">
          <button id="changePasswordBtn" class="btn btn-ghost btn-sm" type="button" onclick={onChangePassword}>
            修改
          </button>
        </div>
      </div>
    </div>

    <div class="settings-section">
      <div class="settings-row">
        <div class="settings-label">
          <div class="settings-label-title">SecretBox{appInfo ? ` v${appInfo.version}` : ""}</div>
          <div class="settings-label-desc">本地加密密码管理器,数据只存在本地</div>
        </div>
        <div class="settings-control about-links">
          <button id="githubLink" class="btn btn-ghost btn-sm" type="button"
            title={GITHUB_URL} onclick={() => ipc.openExternal(GITHUB_URL)}>
            GitHub 仓库
          </button>
          <button id="giteeLink" class="btn btn-ghost btn-sm" type="button"
            title={GITEE_URL} onclick={() => ipc.openExternal(GITEE_URL)}>
            Gitee 镜像
          </button>
        </div>
      </div>
    </div>

    <div class="modal-actions">
      <button id="settingsCloseBtn" class="btn btn-ghost" type="button" onclick={onClose}>关闭</button>
    </div>
  </div>
</div>

<style>
  .about-links {
    display: flex;
    gap: 8px;
  }
  .recovery-view {
    margin-top: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .recovery-code-inline {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 0.95rem;
    letter-spacing: 0.04em;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    user-select: all;
    word-break: break-all;
    text-align: center;
  }
  .regen-row {
    display: flex;
    gap: 8px;
  }
  .regen-row input {
    flex: 1;
  }
</style>
