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
}

export type Settings = Record<string, string>;

export interface SecretboxIpc {
  getStatus(): Promise<Status>;
  unlock(password: string): Promise<void>;
  lock(): Promise<void>;
  setupPassword(password: string): Promise<void>;
  verifyPassword(password: string): Promise<void>;
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
    unlock: async (password: string) => {
      await invoke("unlock", { password });
    },
    lock: async () => {
      await invoke("lock");
    },
    setupPassword: async (password: string) => {
      await invoke("setup_password", { password });
    },
    verifyPassword: async (password: string) => {
      await invoke("verify_password", { password });
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
  };
}

declare global {
  interface Window {
    /** 测试注入点：Playwright 在页面脚本运行前设置，应用启动时采用。 */
    __SECRETBOX_IPC__?: SecretboxIpc;
  }
}

export const ipc: SecretboxIpc = window.__SECRETBOX_IPC__ ?? createTauriIpc();
