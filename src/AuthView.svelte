<script lang="ts">
  import { ipc, type AppInfo } from "./ipc";

  let {
    hasPassword,
    onUnlocked,
    onSetupCompleted,
    onForgotPassword,
  }: {
    hasPassword: boolean;
    onUnlocked: () => void;
    onSetupCompleted: (recoveryKey: string) => void;
    onForgotPassword: () => void;
  } = $props();

  let password = $state("");
  let password2 = $state("");
  let error = $state("");

  // 页脚版本标识：来自后端（与安装包一致），取不到就静默不显示
  let appInfo = $state<AppInfo | null>(null);
  $effect(() => {
    ipc.getAppInfo().then((info) => (appInfo = info)).catch(() => {});
  });
  // busy 只反映手动提交（回车/按钮），按钮的"解锁中…"动画只随它变化；
  // 自动验证走 autoVerifying，界面上不显示动画。
  let busy = $state(false);
  let autoVerifying = $state(false);

  // 首次使用（未设置主密码）时进入设置模式，文案与 Go 版 configureAuthMode 一致
  const isSetup = $derived(!hasPassword);

  // 解锁模式的自动登录：输入停顿后用当前输入尝试解锁，正确即自动进入。
  // 密码对错只能由后端 GCM 解包判定，只能逐次尝试；失败静默（多半只是没打完的前缀）。
  const AUTO_UNLOCK_DELAY_MS = 250;
  let autoTimer: ReturnType<typeof setTimeout> | undefined;
  let verifySeq = 0;

  // 解锁成功后本组件被卸载，清掉挂起的防抖定时器，防止卸载后再触发 unlock
  $effect(() => () => clearTimeout(autoTimer));

  function focusOnMount(node: HTMLElement) {
    node.focus();
  }

  // 手动提交（回车/按钮）与自动验证互斥，保证任意时刻至多一个 unlock 在跑
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy || autoVerifying) return;
    clearTimeout(autoTimer);
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
        const result = await ipc.setupPassword(password);
        onSetupCompleted(result.recovery_key);
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

  function onPasswordInput() {
    error = "";
    if (isSetup) return;
    clearTimeout(autoTimer);
    if (!password) return;
    autoTimer = setTimeout(tryAutoUnlock, AUTO_UNLOCK_DELAY_MS);
  }

  async function tryAutoUnlock() {
    if (isSetup || busy || autoVerifying || !password) return;
    const seq = ++verifySeq;
    const attempted = password;
    autoVerifying = true;
    try {
      await ipc.unlock(attempted);
      // 后端会话已解锁，即使期间用户又输入了也要进入，否则 UI 滞留登录页
      onUnlocked();
    } catch {
      // 静默：想看明确报错仍可回车或点"解锁"按钮
    } finally {
      if (seq === verifySeq) {
        autoVerifying = false;
        // 验证期间用户又输入了：补验最新输入，保证最终停顿的值一定被尝试
        if (password !== attempted) onPasswordInput();
      }
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
      oninput={onPasswordInput}
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
    {#if !isSetup}
      <button id="forgotPassword" type="button" class="btn btn-ghost btn-sm" onclick={onForgotPassword}>
        忘记主密码？
      </button>
    {/if}
  </form>
  {#if appInfo}
    <footer id="authFooter" class="auth-footer">{appInfo.name} v{appInfo.version}</footer>
  {/if}
</div>
