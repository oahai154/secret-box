// 测试助手：把黄金样本 fixture 变成注入浏览器的 mock IPC。
// mock 的行为与 Rust 后端语义一致（错误密码拒绝、条目按 updated_at 倒序等）；
// 后端本身的语义由 Rust 侧单元测试与黄金样本测试保证。
import * as fs from "fs";
import * as path from "path";
import type { Page } from "@playwright/test";

export interface FixtureVersion {
  version: number;
  created_at: string;
  snapshot: string;
}

export interface FixtureItem {
  id: number;
  title: string;
  category: string;
  note: string;
  created_at: string;
  updated_at: string;
  value: string;
  version_count: number;
  versions: FixtureVersion[];
}

export interface Fixture {
  password: string;
  salt: string;
  items: FixtureItem[];
}

export function loadFixture(): Fixture {
  const fixturePath = path.join(
    import.meta.dirname,
    "../../crates/secretbox-core/tests/fixtures/expected.json",
  );
  return JSON.parse(fs.readFileSync(fixturePath, "utf-8")) as Fixture;
}

/**
 * 在页面脚本运行前注入 mock IPC（应用启动时检测 window.__SECRETBOX_IPC__）。
 */
export function injectMockIpc(page: Page): void {
  const fix = loadFixture();
  const code = `
    window.__SECRETBOX_IPC__ = (() => {
      const fix = ${JSON.stringify(fix)};
      const settings = {
        auto_lock_seconds: "120",
        delete_requires_password: "true",
        delete_version_requires_password: "true",
      };
      let nextId = Math.max(...fix.items.map((i) => i.id)) + 1;
      const store = new Map(fix.items.map((it) => [it.id, JSON.parse(JSON.stringify(it))]));
      const nowIso = () => new Date().toISOString();
      return {
        getStatus: async () => ({ has_password: true, unlocked: false }),
        unlock: async (password) => {
          if (password !== fix.password) throw "主密码错误";
        },
        lock: async () => {},
        setupPassword: async (password) => {
          if (!password || password.trim().length < 4) throw "主密码至少 4 个字符";
        },
        verifyPassword: async (password) => {
          if (password !== fix.password) throw "密码不正确";
        },
        changePassword: async (oldPassword, newPassword) => {
          if (oldPassword !== fix.password) throw "解密失败(主密码可能不正确)";
          if (!newPassword || newPassword.length < 4) throw "新密码至少 4 位";
          fix.password = newPassword;
        },
        listItems: async () =>
          [...store.values()].map((it) => ({
            id: it.id,
            title: it.title,
            category: it.category,
            note: it.note,
            created_at: it.created_at,
            updated_at: it.updated_at,
            version_count: it.version_count,
          })),
        getItem: async (id) => {
          const it = store.get(id);
          if (!it) throw "条目不存在";
          return { ...it };
        },
        createItem: async (input) => {
          const id = nextId++;
          const now = nowIso();
          store.set(id, {
            id,
            ...input,
            created_at: now,
            updated_at: now,
            version_count: 1,
            versions: [{ version: 1, created_at: now, snapshot: input.value }],
          });
          return id;
        },
        updateItem: async (id, input) => {
          const it = store.get(id);
          if (!it) throw "条目不存在";
          Object.assign(it, input);
          it.updated_at = nowIso();
          it.version_count = (it.version_count || 0) + 1;
          it.versions = it.versions || [];
          it.versions.push({ version: it.version_count, created_at: it.updated_at, snapshot: input.value });
          return { ...it };
        },
        deleteItem: async (id) => {
          store.delete(id);
        },
        listVersions: async (id) => {
          const it = store.get(id);
          if (!it) throw "条目不存在";
          return (it.versions || []).map((v) => ({
            id: v.version,
            secret_id: id,
            version: v.version,
            created_at: v.created_at,
          }));
        },
        getVersionSnapshot: async (id, version) => {
          const it = store.get(id);
          if (!it) throw "条目不存在";
          const v = (it.versions || []).find((x) => x.version === version);
          if (!v) throw "版本不存在";
          return v.snapshot;
        },
        restoreVersion: async (id, version) => {
          const it = store.get(id);
          if (!it) throw "条目不存在";
          const v = (it.versions || []).find((x) => x.version === version);
          if (!v) throw "版本不存在";
          it.value = v.snapshot;
          it.updated_at = nowIso();
          it.version_count = (it.version_count || 0) + 1;
          it.versions = it.versions || [];
          it.versions.push({ version: it.version_count, created_at: it.updated_at, snapshot: v.snapshot });
          return { ...it };
        },
        deleteVersion: async (id, version) => {
          const it = store.get(id);
          if (!it) throw "条目不存在";
          it.versions = (it.versions || []).filter((x) => x.version !== version);
        },
        getSettings: async () => ({ ...settings }),
        updateSettings: async (updates) => {
          Object.assign(settings, updates);
        },
        exportSnapshot: async (password) => {
          if (!password || password.trim().length < 4) throw "迁移口令至少 4 个字符";
          const snap = {
            has_password: true,
            salt: fix.salt,
            items: [...store.values()].map((it) => ({
              title: it.title,
              category: it.category,
              note: it.note,
              value: "cipher:" + it.value,
              created: it.created_at,
              updated: it.updated_at,
              versions: (it.versions || []).map((v) => ({
                version: v.version,
                snapshot: "cipher:" + v.snapshot,
                created: v.created_at,
              })),
            })),
          };
          const file = { version: 1, salt: "bW9ja0V4cG9ydFNhbHQ=", cipher: btoa(unescape(encodeURIComponent(JSON.stringify(snap)))) };
          return {
            filename: "secretbox-backup-20260905-120000.secretbox",
            content: btoa(unescape(encodeURIComponent(JSON.stringify(file)))),
          };
        },
        saveSnapshotFile: async (filename) => "C:\\\\mock\\\\exports\\\\" + filename,
        applyWindowTheme: async () => {},
        importSnapshot: async (password, content) => {
          if (!content) throw "迁移文件无法解析(损坏?)";
          if (password !== "mock-import-pass") throw "迁移口令错误或文件已损坏";
          // 导入成功：用黄金样本数据覆盖本地，需用原主密码重新解锁
          store.clear();
          for (const it of fix.items) store.set(it.id, JSON.parse(JSON.stringify(it)));
          return { imported: true, has_password: true, items: fix.items.length };
        },
        wipe: async () => {
          store.clear();
          fix.password = "";
        },
      };
    })();
  `;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  void page.addInitScript(code as any);
}
