<script lang="ts">
  import { ipc, type Settings } from "./ipc";
  import AuthView from "./AuthView.svelte";
  import RecoveryKeyView from "./RecoveryKeyView.svelte";
  import RecoverView from "./RecoverView.svelte";
  import UpgradeWizard from "./UpgradeWizard.svelte";
  import MainView from "./MainView.svelte";
  import Toast from "./Toast.svelte";

  let ready = $state(false);
  let hasPassword = $state(true);
  let unlocked = $state(false);
  // v1 旧库待升级：非空时强制停留在升级向导（见 ADR-0003）
  let legacy = $state(false);
  // 首次设置后待确认的恢复密钥：非空时强制停留在确认页（见 ADR-0003）
  let pendingRecoveryKey = $state("");
  // 救援模式：从解锁页"忘记主密码？"进入
  let recovering = $state(false);
  let settings = $state<Settings>({});
  let toastMessage = $state("");
  let toastType = $state("");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let autoLockTimer: ReturnType<typeof setTimeout> | undefined;

  // ---------- 主题切换（与 Go 版一致） ----------
  const THEME_KEY = "secretbox-theme";

  function applyTheme(theme: string) {
    document.documentElement.setAttribute("data-theme", theme);
    try {
      localStorage.setItem(THEME_KEY, theme);
    } catch {
      /* 忽略存储失败 */
    }
    // 原生窗口边框/标题栏颜色跟随主题；system 需解析成实际明暗（浏览器测试环境无 Tauri，静默忽略）
    const effective: "light" | "dark" =
      theme === "system"
        ? window.matchMedia("(prefers-color-scheme: light)").matches
          ? "light"
          : "dark"
        : theme === "light"
          ? "light"
          : "dark";
    ipc.applyWindowTheme(effective).catch(() => {});
    themeMode = theme;
  }

  function savedTheme(): string {
    try {
      const saved = localStorage.getItem(THEME_KEY);
      if (saved === "light" || saved === "dark" || saved === "system") return saved;
    } catch {
      /* 忽略 */
    }
    return "system";
  }

  let themeMode = $state("system");

  // 系统主题变化时，若处于 system 模式则自动跟随
  // （CSS 的 prefers-color-scheme 已覆盖样式，这里只同步按钮选中态）
  $effect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const onChange = () => {
      if (themeMode === "system") applyTheme("system");
    };
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  });

  // ---------- Toast ----------
  function showToast(message: string, type = "") {
    toastMessage = message;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toastMessage = ""), 2300);
  }

  // ---------- 自动锁定 ----------
  function resetAutoLockTimer() {
    clearTimeout(autoLockTimer);
    const secs = parseInt(settings.auto_lock_seconds ?? "120", 10);
    if (secs > 0 && unlocked) {
      autoLockTimer = setTimeout(() => {
        showToast("长时间无操作,已自动锁定");
        lock();
      }, secs * 1000);
    }
  }

  // ---------- 锁定 ----------
  async function lock() {
    clearTimeout(autoLockTimer);
    try {
      await ipc.lock();
    } catch {
      /* 忽略 */
    }
    unlocked = false;
  }

  // 用户交互重置自动锁定计时（与 Go 版一致）
  $effect(() => {
    const events = ["click", "keydown", "scroll", "mousemove"] as const;
    const onActivity = () => {
      if (unlocked) resetAutoLockTimer();
    };
    events.forEach((evt) => document.addEventListener(evt, onActivity, { passive: true }));
    return () => events.forEach((evt) => document.removeEventListener(evt, onActivity));
  });

  // ---------- 拦截 WebView 默认行为 ----------
  // 右键菜单：输入框放行（右键粘贴是高频操作），其余拦截，避免"刷新/另存为/打印"暴露明文或破坏会话
  function isEditable(t: EventTarget | null): boolean {
    if (!(t instanceof HTMLElement)) return false;
    return t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable;
  }
  $effect(() => {
    const onContextMenu = (e: MouseEvent) => {
      if (!isEditable(e.target)) e.preventDefault();
    };
    // F5 / Ctrl+R 重载会丢掉解锁状态，Ctrl+P 会把明文送进打印流程，一律拦截
    const onKeyDown = (e: KeyboardEvent) => {
      const k = e.key.toLowerCase();
      if (e.key === "F5" || (e.ctrlKey && k === "r") || (e.ctrlKey && k === "p")) {
        e.preventDefault();
      }
    };
    window.addEventListener("contextmenu", onContextMenu);
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("contextmenu", onContextMenu);
      window.removeEventListener("keydown", onKeyDown, true);
    };
  });

  // ---------- 启动 ----------
  $effect(() => {
    applyTheme(savedTheme());
    ipc
      .getStatus()
      .then((status) => {
        hasPassword = status.has_password;
        legacy = status.legacy ?? false;
      })
      .catch((e) => {
        hasPassword = false;
        showToast("无法连接服务: " + e, "err");
      })
      .finally(() => {
        ready = true;
      });
  });

  async function handleUnlocked() {
    unlocked = true;
    try {
      settings = await ipc.getSettings();
    } catch {
      settings = {};
    }
    showToast("已解锁");
    resetAutoLockTimer();
  }

  async function handleSetupCompleted(recoveryKey: string) {
    // 首次设置主密码后强制经过恢复密钥确认页，确认完成才进入主界面
    pendingRecoveryKey = recoveryKey;
  }

  function handleRecoveryConfirmed() {
    pendingRecoveryKey = "";
    handleUnlocked();
  }

  function handleUpgraded(recoveryKey: string) {
    // 升级成功后强制经过恢复密钥确认页，确认完成才进入主界面
    legacy = false;
    pendingRecoveryKey = recoveryKey;
  }

  async function handleRecovered() {
    // 救援成功后后端已置为解锁态（会话密钥为 DEK）
    recovering = false;
    await handleUnlocked();
  }

  // 导入快照/清除痕迹后重置会话回到解锁页（数据的主密码状态可能已改变）
  async function handleSessionReset(hasPasswordNow: boolean) {
    hasPassword = hasPasswordNow;
    unlocked = false;
    recovering = false;
    // 导入的快照可能是 v1 格式（或清除后重新设置），重新拉取 legacy 标记
    try {
      const status = await ipc.getStatus();
      hasPassword = status.has_password;
      legacy = status.legacy ?? false;
    } catch {
      legacy = false;
    }
  }
</script>

{#if ready}
  {#if pendingRecoveryKey}
    <RecoveryKeyView recoveryKey={pendingRecoveryKey} onConfirmed={handleRecoveryConfirmed} />
  {:else if legacy}
    <UpgradeWizard onUpgraded={handleUpgraded} />
  {:else if !unlocked && recovering}
    <RecoverView onRecovered={handleRecovered} onBack={() => (recovering = false)} />
  {:else if !unlocked}
    <AuthView
      {hasPassword}
      onUnlocked={handleUnlocked}
      onSetupCompleted={handleSetupCompleted}
      onForgotPassword={() => (recovering = true)}
    />
  {:else}
    <MainView {settings} {themeMode} {applyTheme} onLock={lock} onToast={showToast} onSessionReset={handleSessionReset} />
  {/if}
{/if}

<Toast message={toastMessage} type={toastType} />
