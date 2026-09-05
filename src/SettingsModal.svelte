<script lang="ts">
  // 设置弹窗（复刻 Go 版 settingsModal）
  import type { Settings } from "./ipc";

  let {
    settings,
    onSave,
    onClose,
    onChangePassword,
  }: {
    settings: Settings;
    onSave: (updates: Settings) => void;
    onClose: () => void;
    onChangePassword: () => void;
  } = $props();

  let autoLock = $state("120");
  let deleteRequiresPassword = $state(true);
  let deleteVersionRequiresPassword = $state(true);

  $effect(() => {
    autoLock = settings.auto_lock_seconds ?? "120";
    deleteRequiresPassword = settings.delete_requires_password !== "false";
    deleteVersionRequiresPassword = settings.delete_version_requires_password !== "false";
  });
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

    <div class="modal-actions">
      <button id="settingsCloseBtn" class="btn btn-ghost" type="button" onclick={onClose}>关闭</button>
    </div>
  </div>
</div>
