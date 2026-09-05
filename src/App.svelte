<script lang="ts">
  import { ipc, type Settings } from "./ipc";
  import AuthView from "./AuthView.svelte";
  import MainView from "./MainView.svelte";
  import Toast from "./Toast.svelte";

  let ready = $state(false);
  let hasPassword = $state(true);
  let unlocked = $state(false);
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

  // ---------- 启动 ----------
  $effect(() => {
    applyTheme(savedTheme());
    ipc
      .getStatus()
      .then((status) => {
        hasPassword = status.has_password;
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

  async function handleSetupCompleted() {
    // 首次设置主密码后直接进入主界面
    await handleUnlocked();
  }
</script>

{#if ready}
  {#if !unlocked}
    <AuthView {hasPassword} onUnlocked={handleUnlocked} onSetupCompleted={handleSetupCompleted} />
  {:else}
    <MainView {settings} {themeMode} {applyTheme} onLock={lock} onToast={showToast} />
  {/if}
{/if}

<Toast message={toastMessage} type={toastType} />
