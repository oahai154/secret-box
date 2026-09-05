// IPC 抽象层：前端只依赖这个接口，不直接调用 Tauri API。
// 好处：Playwright 测试与视觉对照时可以在浏览器里注入 mock 实现。
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

export interface Status {
  has_password: boolean;
}

export interface SecretboxIpc {
  getStatus(): Promise<Status>;
  unlock(password: string): Promise<{ key_len: number }>;
  listItems(): Promise<Item[]>;
}

function createTauriIpc(): SecretboxIpc {
  return {
    getStatus: () => invoke<Status>("get_status"),
    unlock: (password: string) =>
      invoke<{ key_len: number }>("unlock", { password }),
    listItems: () => invoke<Item[]>("list_items"),
  };
}

export const ipc: SecretboxIpc = createTauriIpc();
