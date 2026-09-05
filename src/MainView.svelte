<script lang="ts">
  import { ipc, type Item, type Settings, type Version } from "./ipc";
  import ConfirmModal from "./ConfirmModal.svelte";
  import VerifyModal from "./VerifyModal.svelte";
  import SettingsModal from "./SettingsModal.svelte";

  let {
    settings,
    themeMode,
    applyTheme,
    onLock,
    onToast,
  }: {
    settings: Settings;
    themeMode: string;
    applyTheme: (theme: string) => void;
    onLock: () => void;
    onToast: (message: string, type?: string) => void;
  } = $props();

  let items = $state<Item[]>([]);
  let currentId = $state<number | null>(null);
  let detail = $state<Item | null>(null);
  let versions = $state<Version[]>([]);
  let searchQuery = $state("");
  let valueVisible = $state(false);
  let creating = $state(false);
  let categoryOpen = $state(false);
  /** 正在"查看"的历史版本号（null 表示显示当前内容） */
  let viewingVersion = $state<number | null>(null);

  // 弹窗状态：确认弹窗与主密码验证弹窗由待执行动作驱动
  let confirmAction = $state<{
    title: string;
    message: string;
    confirmText: string;
    danger?: boolean;
    run: () => void;
  } | null>(null);
  let verifyAction = $state<{
    hint: string;
    run: (password: string) => Promise<void>;
  } | null>(null);
  let showSettings = $state(false);

  const CATEGORIES = ["", "账号密码", "API密钥", "应用密钥", "私钥", "其他"];

  // ---------- 时间格式（与 Go 版 formatTime 一致） ----------
  function formatTime(iso: string): string {
    if (!iso) return "";
    const d = new Date(iso);
    if (isNaN(d.getTime())) return iso;
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  }

  // ---------- 列表（客户端搜索过滤，与 Go 版 renderList 一致） ----------
  const filteredItems = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return items;
    return items.filter(
      (it) => it.title.toLowerCase().includes(q) || it.category.toLowerCase().includes(q),
    );
  });

  // ---------- 加载与选择 ----------
  $effect(() => {
    loadItems();
  });

  async function loadItems() {
    items = await ipc.listItems();
    if (items.length) {
      await selectItem(items[0].id);
    } else {
      clearEditor();
    }
  }

  async function selectItem(id: number) {
    currentId = id;
    creating = false;
    valueVisible = false;
    viewingVersion = null;
    try {
      const [it, vs] = await Promise.all([ipc.getItem(id), ipc.listVersions(id)]);
      it.value = it.value ?? "";
      it.note = it.note ?? "";
      detail = it;
      versions = vs;
    } catch (e) {
      onToast(typeof e === "string" ? e : String(e), "err");
    }
  }

  function clearEditor() {
    currentId = null;
    creating = false;
    detail = null;
    versions = [];
  }

  // ---------- 新增 / 保存 / 删除（与 Go 版行为一致） ----------
  function createNew() {
    // 清空编辑区，进入"新增"模式
    creating = true;
    currentId = null;
    valueVisible = false;
    viewingVersion = null;
    detail = {
      id: 0,
      title: "",
      category: "",
      note: "",
      created_at: "",
      updated_at: "",
      value: "",
      version_count: 0,
    };
    versions = [];
    titleInput?.focus();
  }

  async function save() {
    if (!detail) return;
    const title = detail.title.trim();
    if (!title) {
      onToast("标题不能为空", "err");
      return;
    }
    const input = {
      title,
      category: detail.category,
      note: detail.note ?? "",
      value: detail.value ?? "",
    };
    try {
      if (creating) {
        currentId = await ipc.createItem(input);
        creating = false;
        onToast("已保存");
      } else {
        detail = await ipc.updateItem(currentId!, input);
        onToast("已保存,已记录新版本");
      }
      // 刷新列表数据，并重新读取当前条目的最新信息
      items = await ipc.listItems();
      detail = await ipc.getItem(currentId!);
      versions = await ipc.listVersions(currentId!);
    } catch (e) {
      onToast(typeof e === "string" ? e : String(e), "err");
    }
  }

  function remove() {
    if (currentId === null || creating) {
      onToast("请先选择条目", "err");
      return;
    }
    confirmAction = {
      title: "删除确认",
      message: "确定删除该条目吗?其所有历史版本也将被删除。",
      confirmText: "确定删除",
      danger: true,
      run: () => {
        if (settings.delete_requires_password === "true") {
          verifyAction = {
            hint: "删除条目需要验证主密码",
            run: async (password) => {
              await ipc.verifyPassword(password);
              await doDelete();
            },
          };
        } else {
          doDelete();
        }
      },
    };
  }

  async function doDelete() {
    try {
      await ipc.deleteItem(currentId!);
      onToast("已删除");
      items = await ipc.listItems();
      currentId = null;
      if (items.length) {
        await selectItem(items[0].id);
      } else {
        clearEditor();
      }
    } catch (e) {
      onToast(typeof e === "string" ? e : String(e), "err");
    }
  }

  // ---------- 历史版本：查看 / 恢复 / 删除（恢复/删除与 Go 版行为一致） ----------
  async function viewVersion(version: number) {
    if (!detail || currentId === null) return;
    try {
      const snapshot = await ipc.getVersionSnapshot(currentId, version);
      if (detail) detail.value = snapshot;
      valueVisible = true;
      viewingVersion = version;
      onToast("正在查看 v" + version + " 的内容");
    } catch (e) {
      onToast(typeof e === "string" ? e : String(e), "err");
    }
  }

  function restoreVersion(version: number) {
    confirmAction = {
      title: "还原确认",
      message: "确定将当前内容还原为该版本吗?还原会生成一条新的修改记录。",
      confirmText: "还原",
      run: async () => {
        try {
          const it = await ipc.restoreVersion(currentId!, version);
          it.value = it.value ?? "";
          it.note = it.note ?? "";
          detail = it;
          viewingVersion = null;
          items = await ipc.listItems();
          versions = await ipc.listVersions(currentId!);
          onToast("已还原到 v" + version);
        } catch (e) {
          onToast(typeof e === "string" ? e : String(e), "err");
        }
      },
    };
  }

  function deleteVersion(version: number) {
    if (settings.delete_version_requires_password === "true") {
      verifyAction = {
        hint: "删除历史版本需要验证主密码",
        run: async (password) => {
          await ipc.verifyPassword(password);
          await doDeleteVersion(version);
        },
      };
    } else {
      doDeleteVersion(version);
    }
  }

  async function doDeleteVersion(version: number) {
    try {
      await ipc.deleteVersion(currentId!, version);
      onToast("已删除 v" + version);
      if (viewingVersion === version) viewingVersion = null;
      versions = await ipc.listVersions(currentId!);
    } catch (e) {
      onToast(typeof e === "string" ? e : String(e), "err");
    }
  }

  // ---------- 分类选择（与 Go 版自定义下拉一致） ----------
  function setCategory(value: string) {
    if (detail) detail.category = value;
    categoryOpen = false;
  }

  // ---------- 保密内容显示/隐藏 ----------
  function toggleValue() {
    valueVisible = !valueVisible;
  }

  // 文本框按内容自动调整高度（配合 CSS 的 min/max-height）
  function autoResize(node: HTMLTextAreaElement) {
    const resize = () => {
      node.style.height = "auto";
      node.style.height = node.scrollHeight + "px";
    };
    node.addEventListener("input", resize);
    resize();
    return {
      destroy() {
        node.removeEventListener("input", resize);
      },
    };
  }

  let titleInput: HTMLInputElement | undefined = $state();

  // Ctrl + S 快速保存
  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        save();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  });

  // ---------- 设置保存 ----------
  function saveSettings(updates: Settings) {
    Object.assign(settings, updates);
    ipc
      .updateSettings(updates)
      .catch((e) => onToast("保存设置失败: " + (typeof e === "string" ? e : String(e)), "err"));
  }

  async function handleLock() {
    await onLock();
  }
</script>

<div class="main-view">
  <header class="topbar">
    <div class="brand">
      <span class="brand-badge">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0 1 10 0v4" />
        </svg>
      </span>
      <span>SecretBox</span>
    </div>
    <div class="topbar-actions">
      <button id="dbBtn" class="btn btn-ghost btn-sm" title="数据库位置与状态">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <ellipse cx="12" cy="5" rx="9" ry="3" />
          <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3" />
          <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5" />
        </svg>
        <span>数据</span>
      </button>
      <button id="settingsBtn" class="btn btn-ghost btn-sm" title="设置" onclick={() => (showSettings = true)}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
        <span>设置</span>
      </button>
      <div class="theme-switch" id="themeSwitch" role="group" aria-label="主题切换">
        <button class="theme-opt" class:active={themeMode === "system"} title="跟随系统" onclick={() => applyTheme("system")}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10" />
            <path d="M12 2a10 10 0 0 1 0 20 10 10 0 0 1 0-20z" fill="currentColor" opacity="0.3" />
            <path d="M12 2a10 10 0 0 0 0 20z" />
          </svg>
          <span class="label">系统</span>
        </button>
        <button class="theme-opt" class:active={themeMode === "light"} title="浅色" onclick={() => applyTheme("light")}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="5" />
            <line x1="12" y1="1" x2="12" y2="3" />
            <line x1="12" y1="21" x2="12" y2="23" />
            <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
            <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
            <line x1="1" y1="12" x2="3" y2="12" />
            <line x1="21" y1="12" x2="23" y2="12" />
            <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
            <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
          </svg>
          <span class="label">浅色</span>
        </button>
        <button class="theme-opt" class:active={themeMode === "dark"} title="深色" onclick={() => applyTheme("dark")}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
          </svg>
          <span class="label">深色</span>
        </button>
      </div>
      <button id="lockBtn" class="btn btn-ghost btn-sm" title="锁定" onclick={handleLock}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0 1 9.9-1" />
        </svg>
        <span>锁定</span>
      </button>
    </div>
  </header>

  <div class="layout">
    <!-- 左侧:条目列表 -->
    <aside class="sidebar">
      <div class="sidebar-head">
        <label class="search">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <circle cx="11" cy="11" r="7" />
            <path d="m20 20-3.5-3.5" />
          </svg>
          <input id="searchInput" type="text" placeholder="搜索标题/分类…" bind:value={searchQuery} />
        </label>
        <button id="newBtn" class="btn btn-primary btn-sm" onclick={createNew}>＋ 新增</button>
      </div>
      <ul id="itemList" class="item-list">
        {#if !filteredItems.length}
          <li class="item-empty">
            {searchQuery ? "无匹配结果" : "暂无条目,点击\"新增\""}
          </li>
        {:else}
          {#each filteredItems as it, i (it.id)}
            <!-- 与 Go 版一致:列表项即点击目标 -->
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
            <li
              class="item"
              class:active={it.id === currentId && !creating}
              style="animation-delay: {i * 24}ms"
              tabindex="0"
              onkeydown={(e) => {
                if (e.key === "Enter") selectItem(it.id);
              }}
              onclick={() => selectItem(it.id)}
            >
              <div class="item-title">{it.title}</div>
              {#if it.category}<div class="item-cat">{it.category}</div>{/if}
              <div class="item-time">{formatTime(it.updated_at)} · v{it.version_count ?? 0}</div>
            </li>
          {/each}
        {/if}
      </ul>
    </aside>

    <!-- 右侧:编辑区 + 版本历史 -->
    <main class="content">
      {#if !detail}
        <div id="emptyHint" class="empty-hint">
          <span class="big">🗄</span>
          从左侧选择或新增一个条目
        </div>
      {:else}
        <div id="editor" class="editor">
          {#key currentId === null ? "new" : currentId}
            <div class="editor-card">
              <div class="editor-row">
                <label class="field-label" for="itemTitle">标题</label>
                <input id="itemTitle" type="text" placeholder="例:GitHub 密钥" bind:this={titleInput} bind:value={detail.title} />
              </div>
              <div class="editor-foot" style="margin-top: 16px;">
                <div class="custom-select" id="categorySelect" class:open={categoryOpen}>
                  <button
                    class="custom-select-trigger"
                    type="button"
                    onclick={() => (categoryOpen = !categoryOpen)}
                  >
                    <span class="custom-select-value">{detail.category || "通用"}</span>
                    <svg class="custom-select-arrow" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                      <polyline points="6 9 12 15 18 9" />
                    </svg>
                  </button>
                  <ul class="custom-select-dropdown">
                    {#each CATEGORIES as cat (cat)}
                      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                      <!-- svelte-ignore a11y_click_events_have_key_events -->
                      <li
                        class:selected={detail.category === cat}
                        onclick={() => setCategory(cat)}
                      >
                        {cat || "通用"}
                      </li>
                    {/each}
                  </ul>
                </div>
                <span id="itemMeta" class="item-meta">
                  {creating
                    ? "新条目"
                    : `创建 ${formatTime(detail.created_at)} · 修改 ${formatTime(detail.updated_at)}`}
                </span>
              </div>
            </div>

            <div class="editor-card">
              <div class="editor-row">
                <div class="field-header">
                  <label class="field-label" for="itemValue">保密内容</label>
                  <div class="value-toggle-bar">
                    <button id="toggleValueBtn" class="btn btn-ghost btn-sm" type="button" onclick={toggleValue}>
                      <svg class="eye-icon" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                        <circle cx="12" cy="12" r="3" />
                      </svg>
                      <span class="toggle-text">{valueVisible ? "点击隐藏" : "点击显示"}</span>
                    </button>
                  </div>
                </div>
                <textarea
                  id="itemValue"
                  class={valueVisible ? "value-visible" : "value-hidden"}
                  placeholder="点击「点击显示」查看保密内容…"
                  spellcheck="false"
                  use:autoResize
                  bind:value={detail.value}
                ></textarea>
              </div>
            </div>

            <div class="editor-card">
              <div class="editor-row">
                <label class="field-label" for="itemNote">备注</label>
                <textarea
                  id="itemNote"
                  class="note-textarea"
                  placeholder="添加备注信息(可选)…"
                  spellcheck="false"
                  use:autoResize
                  bind:value={detail.note}
                ></textarea>
              </div>
            </div>

            <div class="editor-actions">
              <button id="saveBtn" class="btn btn-primary" onclick={save}>💾 保存</button>
              <button id="deleteBtn" class="btn btn-danger" onclick={remove}>🗑 删除</button>
              <span class="spacer"></span>
              <span class="hint-hk">Ctrl + S 快速保存</span>
            </div>
          {/key}
        </div>

        {#if !creating && versions.length}
          <div id="historyPanel" class="history-panel">
            <h3>历史版本</h3>
            <div id="historyList" class="history-list">
              {#each versions as v (v.id)}
                <div class="history-item">
                  <!-- 点击版本号区域查看该版本内容 -->
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <div onclick={() => viewVersion(v.version)} title="点击查看该版本内容">
                    <span class="hver">v{v.version}</span>
                    <span class="htime">{formatTime(v.created_at)}</span>
                  </div>
                  <div class="history-actions">
                    <button class="restore-btn" type="button" onclick={() => restoreVersion(v.version)}>
                      还原
                    </button>
                    <button
                      class="delete-ver-btn"
                      type="button"
                      title="删除此版本"
                      onclick={() => deleteVersion(v.version)}
                    >
                      ✕
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      {/if}
    </main>
  </div>
</div>

{#if confirmAction}
  <ConfirmModal
    title={confirmAction.title}
    message={confirmAction.message}
    confirmText={confirmAction.confirmText}
    danger={confirmAction.danger ?? false}
    onConfirm={() => {
      const run = confirmAction?.run;
      confirmAction = null;
      run?.();
    }}
    onCancel={() => (confirmAction = null)}
  />
{/if}

{#if verifyAction}
  <VerifyModal
    hint={verifyAction.hint}
    onVerify={(password) => verifyAction!.run(password)}
    onCancel={() => (verifyAction = null)}
  />
{/if}

{#if showSettings}
  <SettingsModal
    {settings}
    onSave={saveSettings}
    onClose={() => (showSettings = false)}
  />
{/if}
