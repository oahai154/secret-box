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
  listItems(): Promise<Item[]>;
  getItem(id: number): Promise<Item>;
  listVersions(id: number): Promise<Version[]>;
  getSettings(): Promise<Settings>;
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
    listItems: () => invoke<Item[]>("list_items"),
    getItem: (id: number) => invoke<Item>("get_item", { id }),
    listVersions: (id: number) => invoke<Version[]>("list_versions", { id }),
    getSettings: () => invoke<Settings>("get_settings"),
  };
}

declare global {
  interface Window {
    /** 测试注入点：Playwright 在页面脚本运行前设置，应用启动时采用。 */
    __SECRETBOX_IPC__?: SecretboxIpc;
  }
}

export const ipc: SecretboxIpc = window.__SECRETBOX_IPC__ ?? createTauriIpc();
