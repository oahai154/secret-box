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

/** 应用版本：与真实后端一致地取自 package.json（版本号以 tauri.conf.json 为准同步）。 */
function readAppVersion(): string {
  const pkgPath = path.join(import.meta.dirname, "../../package.json");
  return (JSON.parse(fs.readFileSync(pkgPath, "utf-8")) as { version: string }).version;
}

/**
 * 解锁页登录：填入主密码后点"解锁"按钮，并等待登录视图卸载。
 * 输入停顿 250ms 会触发自动登录，可能先于点击解锁；两条路径都会卸载
 * 登录视图，因此等待卸载是确定性的（点击短超时兜底，自动登录已卸载时忽略）。
 */
export async function login(page: Page, password: string): Promise<void> {
  await page.fill("#authPassword", password);
  await page
    .locator("#authBtn")
    .click({ timeout: 400 })
    .catch(() => {
      /* 已被自动登录卸载 */
    });
  await page.waitForSelector("#authBtn", { state: "detached" });
}

/**
 * 在页面脚本运行前注入 mock IPC（应用启动时检测 window.__SECRETBOX_IPC__）。
 */
export function injectMockIpc(page: Page): void {  const fix = loadFixture();
  const appVersion = readAppVersion();
  const code = `
    window.__SECRETBOX_IPC__ = (() => {
      const fix = ${JSON.stringify(fix)};
      const appVersion = ${JSON.stringify(appVersion)};
      const settings = {
        auto_lock_seconds: "120",
        delete_requires_password: "true",
        delete_version_requires_password: "true",
      };
      let nextId = Math.max(...fix.items.map((i) => i.id)) + 1;
      const store = new Map(fix.items.map((it) => [it.id, JSON.parse(JSON.stringify(it))]));
      const nowIso = () => new Date().toISOString();
      let mockRecoveryKey = "K7MQ-4XTA-9PLW-2RDN-6VHC-3XBT-8YQE-5ZJS";
      return {
        getStatus: async () => ({ has_password: true, unlocked: false, legacy: false }),
        // 与后端 get_app_info 一致：名称固定、版本与安装包（package.json 同步值）一致
        getAppInfo: async () => ({ name: "SecretBox", version: appVersion }),
        // 与后端 open_url 一致：仅放行 http/https，测试中无需真的打开
        openExternal: async () => {},
        upgradeV1: async (password) => {
          // 与后端一致：主密码不变，数据迁移后返回新生成的恢复密钥
          if (password !== fix.password) throw "主密码不正确";
          return { ok: true, recovery_key: "K7MQ-4XTA-9PLW-2RDN-6VHC-3XBT-8YQE-5ZJS" };
        },
        unlock: async (password) => {
          if (password !== fix.password) throw "主密码错误";
        },
        lock: async () => {},
        setupPassword: async (password) => {
          if (!password || password.trim().length < 4) throw "主密码至少 4 个字符";
          // 与后端 setup_password 一致：同时返回生成的恢复密钥（见 ADR-0003）
          return { ok: true, recovery_key: "K7MQ-4XTA-9PLW-2RDN-6VHC-3XBT-8YQE-5ZJS" };
        },
        verifyPassword: async (password) => {
          if (password !== fix.password) throw "密码不正确";
        },
        changePassword: async (oldPassword, newPassword) => {
          if (oldPassword !== fix.password) throw "解密失败(主密码可能不正确)";
          if (!newPassword || newPassword.length < 4) throw "新密码至少 4 位";
          fix.password = newPassword;
        },
        recoverPassword: async (recoveryKey, newPassword) => {
          // 与后端一致：归一化后比对恢复密钥；成功后新主密码生效
          const normalized = (recoveryKey || "").toUpperCase().replace(/[^0-9A-Z]/g, "");
          if (normalized !== mockRecoveryKey.replace(/-/g, "")) throw "恢复密钥不正确";
          if (!newPassword || newPassword.length < 4) throw "新密码至少 4 位";
          fix.password = newPassword;
        },
        getRecoveryKey: async () => {
          // 与后端一致：仅解锁后可见；mock 中调用即处于解锁态
          return mockRecoveryKey;
        },
        regenerateRecoveryKey: async (masterPassword) => {
          if (masterPassword !== fix.password) throw "解密失败(主密码可能不正确)";
          // 生成 32 字符的 8 组新码
          const alphabet = "23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
          let code = "";
          for (let i = 0; i < 32; i++) {
            code += alphabet[Math.floor(Math.random() * alphabet.length)];
          }
          mockRecoveryKey = code.replace(/(.{4})(?=.)/g, "$1-");
          fix.recoveryKey = mockRecoveryKey;
          return mockRecoveryKey;
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
