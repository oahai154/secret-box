// IPC 抽象层：前端只依赖这个接口，不直接调用 Tauri API。
// 好处：Playwright 测试与视觉对照时可以在浏览器里注入 mock 实现
// （测试通过 window.__SECRETBOX_IPC__ 注入，见 tests/e2e/helpers.ts）。
import { invoke } from "@tauri-apps/api/core";

// 条目（与 Rust 侧 secretbox_core::Item 的 JSON 字段一致）
export interface Item {
  id: number;
  title: string;
  category: string;
  note?: string;
  created_at: string;
  updated_at: string;
  value?: string;
  version_count?: number;
}

// 历史版本（仅元数据）
export interface Version {
  id: number;
  secret_id: number;
  version: number;
  created_at: string;
}

export interface Status {
  has_password: boolean;
  unlocked: boolean;
  /** v1 旧库待升级标记（见 ADR-0003），为 true 时前端强制进入升级向导。 */
  legacy?: boolean;
}

// 应用信息（解锁页页脚与设置"关于"区展示）
export interface AppInfo {
  name: string;
  version: string;
}

// 首次设置主密码的结果：同时返回生成的恢复密钥（见 ADR-0003）
export interface SetupResult {
  ok: boolean;
  recovery_key: string;
}

export type Settings = Record<string, string>;

// 导出的结果：文件名 + 文件内容。快照为 base64 文本（.secretbox 文件内容），
// 明文 CSV 为原始文本（带 BOM）。
export interface ExportResult {
  filename: string;
  content: string;
}

// 导入快照的结果（与 Go 版响应一致）
export interface ImportResult {
  imported: boolean;
  has_password: boolean;
  items: number;
}

export interface SecretboxIpc {
  getStatus(): Promise<Status>;
  /** 应用名称与版本，后端取自 tauri.conf.json，与安装包一致。 */
  getAppInfo(): Promise<AppInfo>;
  /** 用系统默认浏览器打开外部网页链接（后端仅放行 http/https）。 */
  openExternal(url: string): Promise<void>;
  unlock(password: string): Promise<void>;
  lock(): Promise<void>;
  setupPassword(password: string): Promise<SetupResult>;
  /** v1 旧库升级：验证主密码后建新 v2 库并迁移数据，返回生成的恢复密钥。 */
  upgradeV1(password: string): Promise<SetupResult>;
  verifyPassword(password: string): Promise<void>;
  changePassword(oldPassword: string, newPassword: string): Promise<void>;
  /** 忘记主密码的救援：用恢复密钥重设主密码并直接进入解锁态（见 ADR-0003）。 */
  recoverPassword(recoveryKey: string, newPassword: string): Promise<void>;
  /** 查看当前恢复密钥（仅解锁后），未设置返回 null。 */
  getRecoveryKey(): Promise<string | null>;
  /** 重新生成恢复密钥（需验证主密码），返回新的规范分组码，旧码立即作废。 */
  regenerateRecoveryKey(masterPassword: string): Promise<string>;
  listItems(): Promise<Item[]>;
  getItem(id: number): Promise<Item>;
  createItem(input: ItemInput): Promise<number>;
  updateItem(id: number, input: ItemInput): Promise<Item>;
  deleteItem(id: number): Promise<void>;
  listVersions(id: number): Promise<Version[]>;
  getVersionSnapshot(id: number, version: number): Promise<string>;
  restoreVersion(id: number, version: number): Promise<Item>;
  deleteVersion(id: number, version: number): Promise<void>;
  getSettings(): Promise<Settings>;
  updateSettings(updates: Settings): Promise<void>;
  exportSnapshot(password: string): Promise<ExportResult>;
  /** 明文导出（ADR-0004）：一键生成不加密 CSV（标题/分类/内容/备注，不含历史版本）。锁定态后端拒绝。 */
  exportCsv(): Promise<ExportResult>;
  importSnapshot(password: string, content: string): Promise<ImportResult>;
  wipe(): Promise<void>;
  /** 弹原生"另存为"对话框保存导出内容，返回保存路径；取消返回空串。 */
  saveSnapshotFile(filename: string, content: string): Promise<string>;
  /** 把原生窗口边框/标题栏颜色设为指定主题（"light" | "dark"）。 */
  applyWindowTheme(theme: "light" | "dark"): Promise<void>;
}

export interface ItemInput {
  title: string;
  category: string;
  note: string;
  value: string;
}

function createTauriIpc(): SecretboxIpc {
  return {
    getStatus: () => invoke<Status>("get_status"),
    getAppInfo: () => invoke<AppInfo>("get_app_info"),
    openExternal: (url: string) => invoke<void>("open_url", { url }),
    unlock: async (password: string) => {
      await invoke("unlock", { password });
    },
    lock: async () => {
      await invoke("lock");
    },
    setupPassword: async (password: string) => {
      return invoke<SetupResult>("setup_password", { password });
    },
    upgradeV1: async (password: string) => {
      return invoke<SetupResult>("upgrade_v1", { password });
    },
    verifyPassword: async (password: string) => {
      await invoke("verify_password", { password });
    },
    changePassword: async (oldPassword: string, newPassword: string) => {
      await invoke("change_password", { oldPassword, newPassword });
    },
    recoverPassword: async (recoveryKey: string, newPassword: string) => {
      await invoke("recover_password", { recoveryKey, newPassword });
    },
    getRecoveryKey: async () => {
      const result = await invoke<{ recovery_key: string | null }>("get_recovery_key");
      return result.recovery_key;
    },
    regenerateRecoveryKey: async (masterPassword: string) => {
      const result = await invoke<{ recovery_key: string }>("regenerate_recovery_key", {
        masterPassword,
      });
      return result.recovery_key;
    },
    listItems: () => invoke<Item[]>("list_items"),
    getItem: (id: number) => invoke<Item>("get_item", { id }),
    createItem: (input: ItemInput) =>
      invoke<number>("create_item", {
        title: input.title,
        category: input.category,
        note: input.note,
        value: input.value,
      }),
    updateItem: (id: number, input: ItemInput) =>
      invoke<Item>("update_item", {
        id,
        title: input.title,
        category: input.category,
        note: input.note,
        value: input.value,
      }),
    deleteItem: (id: number) => invoke<void>("delete_item", { id }),
    listVersions: (id: number) => invoke<Version[]>("list_versions", { id }),
    getVersionSnapshot: (id: number, version: number) =>
      invoke<string>("get_version_snapshot", { id, version }),
    restoreVersion: (id: number, version: number) =>
      invoke<Item>("restore_version", { id, version }),
    deleteVersion: (id: number, version: number) =>
      invoke<void>("delete_version", { id, version }),
    getSettings: () => invoke<Settings>("get_settings"),
    updateSettings: (updates: Settings) => invoke<void>("update_settings", { settings: updates }),
    exportSnapshot: (password: string) => invoke<ExportResult>("export_snapshot", { password }),
    exportCsv: () => invoke<ExportResult>("export_csv"),
    importSnapshot: (password: string, content: string) =>
      invoke<ImportResult>("import_snapshot", { password, content }),
    wipe: () => invoke<void>("wipe"),
    saveSnapshotFile: (filename: string, content: string) =>
      invoke<string>("save_snapshot_file", { filename, content }),
    applyWindowTheme: (theme: "light" | "dark") =>
      invoke<void>("apply_window_theme", { theme }),
  };
}

declare global {
  interface Window {
    /** 测试注入点：Playwright 在页面脚本运行前设置，应用启动时采用。 */
    __SECRETBOX_IPC__?: SecretboxIpc;
  }
}

export const ipc: SecretboxIpc = window.__SECRETBOX_IPC__ ?? createTauriIpc();
