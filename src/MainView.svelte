<script lang="ts">
  import { tick } from "svelte";
  import { ipc, type Item, type Settings, type Version } from "./ipc";
  import ConfirmModal from "./ConfirmModal.svelte";
  import VerifyModal from "./VerifyModal.svelte";
  import SettingsModal from "./SettingsModal.svelte";
  import ChangePasswordModal from "./ChangePasswordModal.svelte";
  import DbModal from "./DbModal.svelte";
  import PromptModal from "./PromptModal.svelte";

  let {
    settings,
    themeMode,
    applyTheme,
    onLock,
    onToast,
    onSessionReset,
  }: {
    settings: Settings;
    themeMode: string;
    applyTheme: (theme: string) => void;
    onLock: () => void;
    onToast: (message: string, type?: string) => void;
    /** 导入快照/清除痕迹后重置会话回到解锁页（参数为数据是否仍设有主密码） */
    onSessionReset: (hasPassword: boolean) => void;
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

  /** 编辑器可改动字段的快照，用于判断"有没有真的改动" */
  type Draft = { title: string; category: string; note: string; value: string };

  // 标题按 trim 后比对：后端只接收 trim 过的标题，仅补空格不算改动
  function draftOf(it: Item | null): Draft {
    return {
      title: (it?.title ?? "").trim(),
      category: it?.category ?? "",
      note: it?.note ?? "",
      value: it?.value ?? "",
    };
  }

  /** 载入/保存时的内容基线：与当前内容一致即"无改动"，不保存（保存幂等，不刷历史版本） */
  let baseline = $state<Draft | null>(null);

  const dirty = $derived.by(() => {
    if (!detail || !baseline) return false;
    const now = draftOf(detail);
    return (
      now.title !== baseline.title ||
      now.category !== baseline.category ||
      now.note !== baseline.note ||
      now.value !== baseline.value
    );
  });

  // 标题为空或内容无改动时保存按钮置灰（置灰的理由由 Ctrl+S 的提示兜底说明）
  const canSave = $derived(!!detail && dirty && detail.title.trim().length > 0);

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
  let showChangePassword = $state(false);
  let showDb = $state(false);
  let dbHasPassword = $state(true);

  // 单行口令输入弹窗（Promise 化，供导出/导入流程等待口令）
  let promptRequest = $state<{
    title: string;
    hint: string;
    resolve: (value: string | null) => void;
  } | null>(null);

  function askPassphrase(title: string, hint = ""): Promise<string | null> {
    return new Promise((resolve) => {
      promptRequest = { title, hint, resolve };
    });
  }

  function settlePrompt(value: string | null) {
    const req = promptRequest;
    promptRequest = null;
    req?.resolve(value);
  }

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
      baseline = draftOf(it);
      versions = vs;
    } catch (e) {
      onToast(typeof e === "string" ? e : String(e), "err");
    }
  }

  function clearEditor() {
    currentId = null;
    creating = false;
    detail = null;
    baseline = null;
    versions = [];
  }

  // ---------- 新增 / 保存 / 删除（与 Go 版行为一致） ----------
  function createNew() {
    // 清空编辑区，进入"新增"模式
    creating = true;
    currentId = null;
    // 新条目还没有任何秘密可藏，直接以"显示"态开场：否则点「点击显示」看不出任何变化
    // （空内容在两种状态下渲染完全一样），用户会以为按钮坏了、也搞不清能不能输入。
    valueVisible = true;
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
    baseline = draftOf(detail);
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
    // 内容没有改动就不写库：按钮已置灰，这里兜住 Ctrl+S 等所有路径
    if (!dirty) {
      onToast("内容未变化,无需保存");
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
      // 落库后的内容成为新基线：按钮回到"无改动"的置灰态
      baseline = draftOf(detail);
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
          valueVisible = true;
          viewingVersion = null;
          items = await ipc.listItems();
          versions = await ipc.listVersions(currentId!);
          // 还原后的内容已是库中内容：基线跟着走，否则保存按钮会一直亮着并谎报"已记录新版本"
          baseline = draftOf(detail);
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
  // 隐藏态不把明文放进 DOM（保持原有约定），改用掩码提示"这里有内容且被锁着"。
  // 掩码长度固定、不随真实长度变化，避免泄露口令长度。
  const VALUE_MASK = "●●●●●●●●";

  // 占位文案随状态变化：原先两态共用一句"点击「点击显示」查看保密内容…"，
  // 内容为空时两种状态渲染完全一样，切了也看不出变化，正是"点了像没点"的根源。
  const valuePlaceholder = $derived(
    valueVisible
      ? "输入保密内容…"
      : detail?.value
        ? `${VALUE_MASK} 已隐藏,点击这里或「点击显示」查看并编辑`
        : "点击这里或「点击显示」后可输入…",
  );

  function toggleValue() {
    valueVisible = !valueVisible;
  }

  // 点击隐藏态的保密内容区就展开并聚焦。隐藏态此前是"看着像输入框、其实点不动"的死区
  // （pointer-events:none + tabindex=-1）：既不接受输入，也不给任何反馈，最误导人。
  async function revealValue() {
    if (valueVisible) return;
    valueVisible = true;
    // 等只读态真正切成可编辑后再聚焦，否则可能聚焦到旧节点、输入落空
    await tick();
    valueEl?.focus();
  }

  // 文本框按内容自动调整高度（配合 CSS 的 min-height）。
  // 用 $effect 而非 use: action：$effect 在模板渲染效果（value 赋值、绑定）之后运行，
  // 量高时 DOM 已是最新；action 的 update 与属性赋值同批无序，会量到旧值导致塌陷。
  function resizeTextarea(node: HTMLTextAreaElement) {
    node.style.height = "auto";
    node.style.height = node.scrollHeight + "px";
  }

  let valueEl = $state<HTMLTextAreaElement>();
  let noteEl = $state<HTMLTextAreaElement>();

  $effect(() => {
    valueVisible;
    void (detail?.value ?? "");
    void (detail?.note ?? "");
    if (valueEl) resizeTextarea(valueEl);
    if (noteEl) resizeTextarea(noteEl);
  });

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

  // ---------- 数据备份 / 迁移 / 清除痕迹（与 Go 版行为一致） ----------
  async function openDbModal() {
    try {
      const st = await ipc.getStatus();
      dbHasPassword = st.has_password;
    } catch {
      dbHasPassword = true;
    }
    showDb = true;
  }

  // 明文导出（ADR-0004）：一键生成不加密 CSV，不重输主密码（已解锁会话本就可见全部明文），
  // 但必须先如实警示。先关数据弹窗再弹确认，避免确认弹窗被遮住（与 wipe 同一处理）。
  function exportCsv() {
    showDb = false;
    confirmAction = {
      title: "导出明文 CSV",
      message:
        "即将生成不加密的 CSV 文件,包含全部条目的标题、分类、内容与备注(不含历史版本)。任何拿到此文件的人都能读取全部内容,请妥善保管、用完即删。继续导出吗?",
      confirmText: "继续导出",
      danger: true,
      run: async () => {
        try {
          const data = await ipc.exportCsv();
          const saved = await ipc.saveSnapshotFile(data.filename, data.content);
          if (!saved) {
            onToast("未选择保存位置,导出已取消", "err");
            return;
          }
          onToast("已导出明文文件: " + data.filename);
        } catch (e) {
          onToast("导出失败: " + (typeof e === "string" ? e : String(e)), "err");
        }
      },
    };
  }

  async function exportBackup(): Promise<boolean> {
    const pw = await askPassphrase("设置迁移口令", "该口令用于加密迁移文件,请务必牢记");
    if (pw === null) return false;
    if (pw.trim().length < 4) {
      onToast("迁移口令至少 4 个字符", "err");
      return false;
    }
    try {
      const data = await ipc.exportSnapshot(pw);
      const saved = await ipc.saveSnapshotFile(data.filename, data.content);
      if (!saved) {
        onToast("未选择保存位置,导出已取消", "err");
        return false;
      }
      onToast("已导出迁移文件: " + data.filename);
      return true;
    } catch (e) {
      onToast("导出失败: " + (typeof e === "string" ? e : String(e)), "err");
      return false;
    }
  }

  async function handleImportFile(file: File) {
    const text = await file.text();
    const pw = await askPassphrase("输入迁移文件的口令", file.name);
    if (pw === null) return;
    try {
      const res = await ipc.importSnapshot(pw, text);
      onToast(`导入成功: ${res.items} 条数据`, "ok");
      // 导入覆盖了本地数据，弃用当前会话，需用原主密码重新解锁
      onSessionReset(res.has_password);
    } catch (e) {
      onToast("导入失败: " + (typeof e === "string" ? e : String(e)), "err");
    }
  }

  function wipe() {
    // 先关闭数据备份弹窗,避免它遮住后续的密码验证弹窗
    showDb = false;
    verifyAction = {
      hint: "清除本地全部数据需要验证主密码,验证后还需先导出迁移文件",
      run: async (password) => {
        await ipc.verifyPassword(password);
        await doWipe();
      },
    };
  }

  async function doWipe() {
    // 1) 强制导出,导出失败/取消则中止
    onToast("请先完成迁移文件导出");
    const exported = await exportBackup();
    if (!exported) {
      onToast("未完成导出,已取消清除", "err");
      return;
    }
    // 2) 导出成功后再做一次最终确认
    confirmAction = {
      title: "清除确认",
      message: "迁移文件已保存。确定要彻底清除本地全部数据吗?此操作不可恢复!",
      confirmText: "确定清除",
      danger: true,
      run: async () => {
        try {
          await ipc.wipe();
          onToast("已清除本地全部数据", "ok");
          onSessionReset(false);
        } catch (e) {
          onToast("清除失败: " + (typeof e === "string" ? e : String(e)), "err");
        }
      },
    };
  }

  // ---------- 修改主密码（与 Go 版 confirmChangePassword 行为一致） ----------
  async function changePassword(oldPassword: string, newPassword: string) {
    await ipc.changePassword(oldPassword, newPassword);
    showChangePassword = false;
    onToast("密码修改成功", "ok");
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
      <button id="dbBtn" class="btn btn-ghost btn-sm" title="数据库位置与状态" onclick={openDbModal}>
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
              <div class="editor-foot">
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

              <div class="editor-divider"></div>

              <div class="editor-row">
                <div class="field-header">
                  <label class="field-label" for="itemValue">保密内容</label>
                  <div class="value-toggle-bar">
                    <button id="toggleValueBtn" class="btn btn-ghost btn-sm" type="button" onclick={toggleValue}>
                      <svg class="eye-icon" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        {#if valueVisible}
                          <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                          <circle cx="12" cy="12" r="3" />
                        {:else}
                          <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94" />
                          <path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24" />
                          <line x1="1" y1="1" x2="23" y2="23" />
                        {/if}
                      </svg>
                      <span class="toggle-text">{valueVisible ? "点击隐藏" : "点击显示"}</span>
                    </button>
                  </div>
                </div>
                <textarea
                  id="itemValue"
                  rows={1}
                  bind:this={valueEl}
                  readonly={!valueVisible}
                  class={valueVisible ? "value-visible" : "value-hidden"}
                  placeholder={valuePlaceholder}
                  spellcheck="false"
                  tabindex={valueVisible ? 0 : -1}
                  value={valueVisible ? (detail?.value ?? "") : ""}
                  onclick={revealValue}
                  oninput={(e) => {
                    if (detail) detail.value = e.currentTarget.value;
                  }}
                ></textarea>
              </div>

              <div class="editor-divider"></div>

              <div class="editor-row">
                <label class="field-label" for="itemNote">备注</label>
                <textarea
                  id="itemNote"
                  rows={1}
                  bind:this={noteEl}
                  class="note-textarea"
                  placeholder="添加备注信息(可选)…"
                  spellcheck="false"
                  bind:value={detail.note}
                ></textarea>
              </div>

              <div class="editor-divider"></div>

              <div class="editor-actions">
                <button id="saveBtn" class="btn btn-primary" disabled={!canSave} onclick={save}>💾 保存</button>
                <button id="deleteBtn" class="btn btn-danger" onclick={remove}>🗑 删除</button>
                <span class="spacer"></span>
                <span class="hint-hk">Ctrl + S 快速保存</span>
              </div>
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
    onChangePassword={() => {
      showSettings = false;
      showChangePassword = true;
    }}
    onToast={onToast}
  />
{/if}

{#if showChangePassword}
  <ChangePasswordModal
    onSubmit={changePassword}
    onCancel={() => (showChangePassword = false)}
  />
{/if}

{#if showDb}
  <DbModal
    {items}
    hasPassword={dbHasPassword}
    onClose={() => (showDb = false)}
    onExport={exportBackup}
    onExportCsv={exportCsv}
    onImportFile={handleImportFile}
    onWipe={wipe}
  />
{/if}

{#if promptRequest}
  <PromptModal
    title={promptRequest.title}
    hint={promptRequest.hint}
    placeholder="输入口令"
    onSubmit={async (value) => settlePrompt(value)}
    onCancel={() => settlePrompt(null)}
  />
{/if}
