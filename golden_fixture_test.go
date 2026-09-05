// golden_fixture_test.go - 跨语言黄金样本生成与读回验证工具。
//
// 用法（都通过环境变量门控，普通 go test 不触发）：
//
//	生成样本: SECRETBOX_GOLDEN_OUT=<输出目录> go test -run TestGenerateGoldenFixture
//	读回验证: SECRETBOX_VERIFY_DB=<db路径> SECRETBOX_VERIFY_PASSWORD=<主密码> \
//	          SECRETBOX_VERIFY_OUT=<输出json路径> go test -run TestGoldenVerifyDump
//
// 生成的样本提交进仓库，供 Rust 侧 cargo test 断言逐字节兼容（见 ADR-0002）。
// 样本只使用测试密码，绝不接触真实 secretbox.db。
package main

import (
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"testing"
)

// goldenPassword 黄金样本的测试主密码（公开的测试数据，非真实密码）。
const goldenPassword = "golden-test-password"

// goldenFixedSalt KDF 差分向量用的固定盐（0x00..0x1f），使派生密钥可预先算好并断言。
var goldenFixedSalt = []byte{
	0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
	0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
	0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
	0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
}

// GoldenExpected 期望明文 JSON 的顶层结构（Rust 测试按此解析断言）。
type GoldenExpected struct {
	// Password 测试主密码，明文写入（测试数据）。
	Password string `json:"password"`
	// Salt 主密码派生盐值(base64)，与 golden.db meta 表一致。
	Salt string `json:"salt"`
	// KdfVector 固定盐 KDF 向量：Rust 必须派生出同样的密钥。
	KdfVector GoldenKdfVector `json:"kdf_vector"`
	// DecryptVectors Go 版 Encrypt 产生的密文（随机 nonce）+ 期望明文，
	// Rust 必须能解密；同时 Rust 自行加密的输出由读回工具在 Go 侧验证。
	DecryptVectors []GoldenDecryptVector `json:"decrypt_vectors"`
	// Items golden.db 内全部条目及历史版本的期望明文。
	Items []GoldenItem `json:"items"`
}

// GoldenKdfVector 固定盐 + 主密码 → 期望密钥。
type GoldenKdfVector struct {
	Password string `json:"password"`
	SaltHex  string `json:"salt_hex"`
	KeyHex   string `json:"key_hex"`
}

// GoldenDecryptVector 单条解密向量。
type GoldenDecryptVector struct {
	Plaintext string `json:"plaintext"`
	// Cipher base64(nonce || ciphertext+tag)，由 Go 版 Encrypt 生成。
	Cipher string `json:"cipher"`
}

// GoldenItem 单个条目的期望数据。
type GoldenItem struct {
	ID           int64             `json:"id"`
	Title        string            `json:"title"`
	Category     string            `json:"category"`
	Note         string            `json:"note"`
	CreatedAt    string            `json:"created_at"`
	UpdatedAt    string            `json:"updated_at"`
	Value        string            `json:"value"`
	VersionCount int               `json:"version_count"`
	Versions     []GoldenVersion   `json:"versions"`
	Extra        map[string]string `json:"extra,omitempty"`
}

// GoldenVersion 单个历史版本的期望数据。
type GoldenVersion struct {
	Version   int    `json:"version"`
	CreatedAt string `json:"created_at"`
	Snapshot  string `json:"snapshot"`
}

// TestGenerateGoldenFixture 生成 golden.db + expected.json（需 SECRETBOX_GOLDEN_OUT）。
func TestGenerateGoldenFixture(t *testing.T) {
	outDir := os.Getenv("SECRETBOX_GOLDEN_OUT")
	if outDir == "" {
		t.Skip("未设置 SECRETBOX_GOLDEN_OUT，跳过黄金样本生成")
	}

	dbPath := filepath.Join(outDir, "golden.db")
	_ = os.Remove(dbPath)
	_ = os.Remove(dbPath + "-wal")
	_ = os.Remove(dbPath + "-shm")

	app, err := NewApp(dbPath)
	if err != nil {
		t.Fatalf("NewApp: %v", err)
	}
	defer app.Close()

	// 模拟设置主密码
	key, salt, err := DeriveKey(goldenPassword, nil)
	if err != nil {
		t.Fatalf("DeriveKey: %v", err)
	}
	if err := app.SetSalt(salt); err != nil {
		t.Fatalf("SetSalt: %v", err)
	}
	app.salt = salt

	// 条目1：含中文、emoji、换行的常规条目
	id1, err := app.CreateItem(key,
		"GitHub",
		"开发",
		"主账号，已开启两步验证\n备用码放在保险柜",
		"gh_user_2026@example.com: Hunter2-Overflow! 🦀")
	if err != nil {
		t.Fatalf("CreateItem 1: %v", err)
	}

	// 条目2：更新两次，形成 3 个历史版本
	id2, err := app.CreateItem(key,
		"公司邮箱",
		"工作",
		"",
		"zhang.san@corp.example.cn: Mail-Pass-2026-#315")
	if err != nil {
		t.Fatalf("CreateItem 2: %v", err)
	}
	if _, err := app.UpdateItem(key, id2, "公司邮箱", "工作", "季度轮换",
		"zhang.san@corp.example.cn: Mail-Pass-2026-Q2-#777"); err != nil {
		t.Fatalf("UpdateItem 2a: %v", err)
	}
	if _, err := app.UpdateItem(key, id2, "公司邮箱", "工作", "季度轮换（已交由主管备份）",
		"zhang.san@corp.example.cn: Mail-Pass-2026-Q3-@888"); err != nil {
		t.Fatalf("UpdateItem 2b: %v", err)
	}

	// 条目3：空密码 + 特殊字符备注的边界情况
	id3, err := app.CreateItem(key,
		"家里 Wi-Fi & <路由器>",
		"生活",
		`备注含引号 "双引号" 与单引号 'single' 及 %s %d 占位符`,
		"")
	if err != nil {
		t.Fatalf("CreateItem 3: %v", err)
	}

	_ = id1
	_ = id3

	// 关闭后重新打开，从磁盘读回作为期望数据（保证期望值来自持久化结果）
	if err := app.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
	app2, err := NewApp(dbPath)
	if err != nil {
		t.Fatalf("reopen: %v", err)
	}
	defer app2.Close()

	// 固定盐 KDF 向量
	fixedKey, _, err := DeriveKey(goldenPassword, goldenFixedSalt)
	if err != nil {
		t.Fatalf("DeriveKey fixed: %v", err)
	}

	// 解密向量：Go 版 Encrypt 若干典型明文（nonce 随机，Rust 只需能解）
	plainTexts := []string{
		"",
		"hello",
		"中文明文测试 🦀",
		"line1\nline2\r\nline3",
		"引号\"单引号'反斜杠\\斜杠/制表\t含NUL\u0000字符",
		"长文本 " + repeat("x", 5000),
	}
	vectors := make([]GoldenDecryptVector, 0, len(plainTexts))
	for _, p := range plainTexts {
		cipher, err := Encrypt(key, p)
		if err != nil {
			t.Fatalf("Encrypt vector: %v", err)
		}
		vectors = append(vectors, GoldenDecryptVector{Plaintext: p, Cipher: cipher})
	}

	// 从库中读出全部条目与版本
	items, err := app2.ListItems()
	if err != nil {
		t.Fatalf("ListItems: %v", err)
	}
	expected := GoldenExpected{
		Password: goldenPassword,
		KdfVector: GoldenKdfVector{
			Password: goldenPassword,
			SaltHex:  hex.EncodeToString(goldenFixedSalt),
			KeyHex:   hex.EncodeToString(fixedKey),
		},
		DecryptVectors: vectors,
		Items:          []GoldenItem{},
	}
	// 盐值直接读库（base64）
	var saltB64 string
	if err := app2.db.QueryRow(`SELECT value FROM meta WHERE key='salt'`).Scan(&saltB64); err != nil {
		t.Fatalf("read salt: %v", err)
	}
	expected.Salt = saltB64
	if base64.StdEncoding.EncodeToString(salt) != saltB64 {
		t.Fatalf("盐值读回不一致")
	}

	for _, it := range items {
		full, err := app2.GetItem(key, it.ID)
		if err != nil {
			t.Fatalf("GetItem %d: %v", it.ID, err)
		}
		gi := GoldenItem{
			ID:           full.ID,
			Title:        full.Title,
			Category:     full.Category,
			Note:         full.Note,
			CreatedAt:    full.CreatedAt,
			UpdatedAt:    full.UpdatedAt,
			Value:        full.Value,
			VersionCount: it.VersionCount,
			Versions:     []GoldenVersion{},
		}
		versions, err := app2.ListVersions(it.ID)
		if err != nil {
			t.Fatalf("ListVersions %d: %v", it.ID, err)
		}
		for _, v := range versions {
			snap, err := app2.GetVersionSnapshot(key, it.ID, int64(v.Version))
			if err != nil {
				t.Fatalf("GetVersionSnapshot %d/%d: %v", it.ID, v.Version, err)
			}
			gi.Versions = append(gi.Versions, GoldenVersion{
				Version:   v.Version,
				CreatedAt: v.CreatedAt,
				Snapshot:  snap,
			})
		}
		expected.Items = append(expected.Items, gi)
	}

	data, err := json.MarshalIndent(&expected, "", "  ")
	if err != nil {
		t.Fatalf("MarshalIndent: %v", err)
	}
	if err := os.WriteFile(filepath.Join(outDir, "expected.json"), data, 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	fmt.Printf("黄金样本已生成: %s (%d 个条目)\n", outDir, len(expected.Items))
}

func repeat(s string, n int) string {
	b := make([]byte, 0, len(s)*n)
	for i := 0; i < n; i++ {
		b = append(b, s...)
	}
	return string(b)
}

// TestGoldenVerifyDump 把指定数据库的全部明文导出为 JSON，供跨语言回环验证
// （Rust 写入后由 Go 读回比对）。需同时设置 SECRETBOX_VERIFY_DB / SECRETBOX_VERIFY_PASSWORD /
// SECRETBOX_VERIFY_OUT 三个环境变量。
func TestGoldenVerifyDump(t *testing.T) {
	dbPath := os.Getenv("SECRETBOX_VERIFY_DB")
	password := os.Getenv("SECRETBOX_VERIFY_PASSWORD")
	outPath := os.Getenv("SECRETBOX_VERIFY_OUT")
	if dbPath == "" || outPath == "" {
		t.Skip("未设置 SECRETBOX_VERIFY_DB/SECRETBOX_VERIFY_OUT，跳过读回验证")
	}

	app, err := NewApp(dbPath)
	if err != nil {
		t.Fatalf("NewApp: %v", err)
	}
	defer app.Close()

	dump := struct {
		Salt  string       `json:"salt"`
		Items []GoldenItem `json:"items"`
	}{Items: []GoldenItem{}}

	var saltB64 string
	if err := app.db.QueryRow(`SELECT value FROM meta WHERE key='salt'`).Scan(&saltB64); err == nil {
		dump.Salt = saltB64
	}

	salt, err := base64.StdEncoding.DecodeString(dump.Salt)
	if err != nil {
		t.Fatalf("decode salt: %v", err)
	}
	key, _, err := DeriveKey(password, salt)
	if err != nil {
		t.Fatalf("DeriveKey: %v", err)
	}

	items, err := app.ListItems()
	if err != nil {
		t.Fatalf("ListItems: %v", err)
	}
	for _, it := range items {
		full, err := app.GetItem(key, it.ID)
		if err != nil {
			t.Fatalf("GetItem %d: %v", it.ID, err)
		}
		gi := GoldenItem{
			ID:           full.ID,
			Title:        full.Title,
			Category:     full.Category,
			Note:         full.Note,
			CreatedAt:    full.CreatedAt,
			UpdatedAt:    full.UpdatedAt,
			Value:        full.Value,
			VersionCount: it.VersionCount,
			Versions:     []GoldenVersion{},
		}
		versions, err := app.ListVersions(it.ID)
		if err != nil {
			t.Fatalf("ListVersions %d: %v", it.ID, err)
		}
		for _, v := range versions {
			snap, err := app.GetVersionSnapshot(key, it.ID, int64(v.Version))
			if err != nil {
				t.Fatalf("GetVersionSnapshot: %v", err)
			}
			gi.Versions = append(gi.Versions, GoldenVersion{
				Version:   v.Version,
				CreatedAt: v.CreatedAt,
				Snapshot:  snap,
			})
		}
		dump.Items = append(dump.Items, gi)
	}

	data, err := json.MarshalIndent(&dump, "", "  ")
	if err != nil {
		t.Fatalf("MarshalIndent: %v", err)
	}
	if err := os.WriteFile(outPath, data, 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	fmt.Printf("读回验证已写出: %s\n", outPath)

	// 可选：与期望文件逐字段比对（Rust 写路径的跨语言回环验收）
	if expectPath := os.Getenv("SECRETBOX_VERIFY_EXPECT"); expectPath != "" {
		expectData, err := os.ReadFile(expectPath)
		if err != nil {
			t.Fatalf("读取期望文件: %v", err)
		}
		var expected struct {
			Salt  string       `json:"salt"`
			Items []GoldenItem `json:"items"`
		}
		if err := json.Unmarshal(expectData, &expected); err != nil {
			t.Fatalf("期望文件格式无效: %v", err)
		}
		if dump.Salt != expected.Salt {
			t.Fatalf("盐值不一致: got %q want %q", dump.Salt, expected.Salt)
		}
		canonical := func(items []GoldenItem) []GoldenItem {
			cp := append([]GoldenItem(nil), items...)
			sort.Slice(cp, func(i, j int) bool { return cp[i].ID < cp[j].ID })
			for k := range cp {
				vs := append([]GoldenVersion(nil), cp[k].Versions...)
				sort.Slice(vs, func(i, j int) bool { return vs[i].Version < vs[j].Version })
				cp[k].Versions = vs
			}
			return cp
		}
		got := canonical(dump.Items)
		want := canonical(expected.Items)
		if len(got) != len(want) {
			t.Fatalf("条目数量不一致: got %d want %d", len(got), len(want))
		}
		for i := range got {
			if !reflect.DeepEqual(got[i], want[i]) {
				t.Fatalf("条目 %d 数据不一致:\n got: %+v\nwant: %+v", got[i].ID, got[i], want[i])
			}
		}
		fmt.Printf("期望比对通过: %d 个条目逐字段一致\n", len(got))
	}
}

// TestGoldenVerifyRustVectors 用 Go 版 Decrypt 解密 Rust 侧产生的加密向量，
// 完成加密原语的差分闭环（Rust 加密 → Go 解密）。
// 需设置 SECRETBOX_RUST_VECTORS_IN 指向向量 JSON 文件。
func TestGoldenVerifyRustVectors(t *testing.T) {
	inPath := os.Getenv("SECRETBOX_RUST_VECTORS_IN")
	if inPath == "" {
		t.Skip("未设置 SECRETBOX_RUST_VECTORS_IN，跳过 Rust 加密向量读回")
	}
	data, err := os.ReadFile(inPath)
	if err != nil {
		t.Fatalf("读取向量文件: %v", err)
	}
	var payload struct {
		Vectors []GoldenDecryptVector `json:"vectors"`
	}
	if err := json.Unmarshal(data, &payload); err != nil {
		t.Fatalf("向量文件格式无效: %v", err)
	}

	app, err := NewApp(filepath.Join(t.TempDir(), "unused.db"))
	if err != nil {
		t.Fatalf("NewApp: %v", err)
	}
	defer app.Close()

	salt, err := base64.StdEncoding.DecodeString("1s3wnY4LRO5eflwzcF5woAPgH8zJQAWHKqL6dV7tAho=")
	if err != nil {
		t.Fatalf("decode salt: %v", err)
	}
	key, _, err := DeriveKey(goldenPassword, salt)
	if err != nil {
		t.Fatalf("DeriveKey: %v", err)
	}

	for i, v := range payload.Vectors {
		plain, err := Decrypt(key, v.Cipher)
		if err != nil {
			t.Fatalf("向量 %d 解密失败: %v", i, err)
		}
		if plain != v.Plaintext {
			t.Fatalf("向量 %d 明文不一致: got %q want %q", i, plain, v.Plaintext)
		}
	}
	fmt.Printf("Rust 加密向量读回验证通过: %d 条\n", len(payload.Vectors))
}

// buildMigrationFile 用与 handlers.go handleExport 相同的格式构建迁移文件内容
// （base64( JSON{version:1, salt, cipher} )），供快照跨语言互通测试使用。
func buildMigrationFile(t *testing.T, app *App, passphrase string) string {
	t.Helper()
	snap, err := app.GetSnapshot()
	if err != nil {
		t.Fatalf("GetSnapshot: %v", err)
	}
	snapJSON, err := json.Marshal(snap)
	if err != nil {
		t.Fatalf("Marshal snapshot: %v", err)
	}
	key, salt, err := DeriveKey(passphrase, nil)
	if err != nil {
		t.Fatalf("DeriveKey: %v", err)
	}
	cipherText, err := Encrypt(key, string(snapJSON))
	if err != nil {
		t.Fatalf("Encrypt: %v", err)
	}
	file := map[string]any{
		"version": 1,
		"salt":    base64.StdEncoding.EncodeToString(salt),
		"cipher":  cipherText,
	}
	fileJSON, _ := json.Marshal(file)
	return base64.StdEncoding.EncodeToString(fileJSON)
}

// TestSnapshotExportTool 把指定数据库导出为迁移文件（Go 格式），供 Rust 导入验证。
// 需设置 SECRETBOX_SNAPSHOT_EXPORT_DB / SECRETBOX_SNAPSHOT_EXPORT_PASSWORD /
// SECRETBOX_SNAPSHOT_EXPORT_OUT。
func TestSnapshotExportTool(t *testing.T) {
	dbPath := os.Getenv("SECRETBOX_SNAPSHOT_EXPORT_DB")
	password := os.Getenv("SECRETBOX_SNAPSHOT_EXPORT_PASSWORD")
	outPath := os.Getenv("SECRETBOX_SNAPSHOT_EXPORT_OUT")
	if dbPath == "" || outPath == "" {
		t.Skip("未设置 SECRETBOX_SNAPSHOT_EXPORT_DB/SECRETBOX_SNAPSHOT_EXPORT_OUT，跳过快照导出")
	}

	app, err := NewApp(dbPath)
	if err != nil {
		t.Fatalf("NewApp: %v", err)
	}
	defer app.Close()

	content := buildMigrationFile(t, app, password)
	if err := os.WriteFile(outPath, []byte(content), 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	fmt.Printf("Go 快照导出完成: %s\n", outPath)
}

// TestSnapshotImportTool 用 Go 版格式解析迁移文件（Rust 导出）并还原到新数据库，
// 再导出全部明文供比对。需设置 SECRETBOX_SNAPSHOT_IMPORT_FILE /
// SECRETBOX_SNAPSHOT_IMPORT_PASSWORD / SECRETBOX_SNAPSHOT_IMPORT_DB，
// 可选 SECRETBOX_SNAPSHOT_IMPORT_OUT / SECRETBOX_SNAPSHOT_IMPORT_EXPECT 逐字段比对。
func TestSnapshotImportTool(t *testing.T) {
	filePath := os.Getenv("SECRETBOX_SNAPSHOT_IMPORT_FILE")
	password := os.Getenv("SECRETBOX_SNAPSHOT_IMPORT_PASSWORD")
	dbPath := os.Getenv("SECRETBOX_SNAPSHOT_IMPORT_DB")
	if filePath == "" || dbPath == "" {
		t.Skip("未设置 SECRETBOX_SNAPSHOT_IMPORT_FILE/SECRETBOX_SNAPSHOT_IMPORT_DB，跳过快照导入")
	}

	raw, err := os.ReadFile(filePath)
	if err != nil {
		t.Fatalf("读取迁移文件: %v", err)
	}
	var file struct {
		Version int    `json:"version"`
		Salt    string `json:"salt"`
		Cipher  string `json:"cipher"`
	}
	content, err := base64.StdEncoding.DecodeString(string(raw))
	if err != nil {
		t.Fatalf("迁移文件无法解析(损坏?): %v", err)
	}
	if err := json.Unmarshal(content, &file); err != nil || file.Version != 1 || file.Salt == "" {
		t.Fatalf("迁移文件格式无效")
	}
	salt, err := base64.StdEncoding.DecodeString(file.Salt)
	if err != nil {
		t.Fatalf("迁移文件格式无效: %v", err)
	}
	key, _, err := DeriveKey(password, salt)
	if err != nil {
		t.Fatalf("DeriveKey: %v", err)
	}
	plain, err := Decrypt(key, file.Cipher)
	if err != nil {
		t.Fatalf("迁移口令错误或文件已损坏: %v", err)
	}
	var snap Snapshot
	if err := json.Unmarshal([]byte(plain), &snap); err != nil {
		t.Fatalf("迁移文件内容无效: %v", err)
	}

	_ = os.Remove(dbPath)
	_ = os.Remove(dbPath + "-wal")
	_ = os.Remove(dbPath + "-shm")
	app, err := NewApp(dbPath)
	if err != nil {
		t.Fatalf("NewApp: %v", err)
	}
	defer app.Close()
	if err := app.RestoreFromSnapshot(&snap); err != nil {
		t.Fatalf("RestoreFromSnapshot: %v", err)
	}
	fmt.Printf("Go 快照导入完成: %d 个条目\n", len(snap.Items))

	// 导出全部明文并可选比对（与 TestGoldenVerifyDump 相同的比对逻辑）
	outPath := os.Getenv("SECRETBOX_SNAPSHOT_IMPORT_OUT")
	if outPath == "" {
		return
	}
	dumpSalt, err := base64.StdEncoding.DecodeString(snap.SaltB64)
	if err != nil {
		t.Fatalf("decode snapshot salt: %v", err)
	}
	dumpKey, _, err := DeriveKey(os.Getenv("SECRETBOX_SNAPSHOT_DB_PASSWORD"), dumpSalt)
	if err != nil {
		t.Fatalf("DeriveKey: %v", err)
	}
	items, err := app.ListItems()
	if err != nil {
		t.Fatalf("ListItems: %v", err)
	}
	dump := struct {
		Salt  string       `json:"salt"`
		Items []GoldenItem `json:"items"`
	}{Salt: snap.SaltB64, Items: []GoldenItem{}}
	for _, it := range items {
		full, err := app.GetItem(dumpKey, it.ID)
		if err != nil {
			t.Fatalf("GetItem %d: %v", it.ID, err)
		}
		gi := GoldenItem{
			ID: full.ID, Title: full.Title, Category: full.Category, Note: full.Note,
			CreatedAt: full.CreatedAt, UpdatedAt: full.UpdatedAt, Value: full.Value,
			VersionCount: it.VersionCount, Versions: []GoldenVersion{},
		}
		versions, err := app.ListVersions(it.ID)
		if err != nil {
			t.Fatalf("ListVersions %d: %v", it.ID, err)
		}
		for _, v := range versions {
			snapPlain, err := app.GetVersionSnapshot(dumpKey, it.ID, int64(v.Version))
			if err != nil {
				t.Fatalf("GetVersionSnapshot: %v", err)
			}
			gi.Versions = append(gi.Versions, GoldenVersion{Version: v.Version, CreatedAt: v.CreatedAt, Snapshot: snapPlain})
		}
		dump.Items = append(dump.Items, gi)
	}
	data, err := json.MarshalIndent(&dump, "", "  ")
	if err != nil {
		t.Fatalf("MarshalIndent: %v", err)
	}
	if err := os.WriteFile(outPath, data, 0o644); err != nil {
		t.Fatalf("WriteFile: %v", err)
	}
	fmt.Printf("导入后明文已写出: %s\n", outPath)

	if expectPath := os.Getenv("SECRETBOX_SNAPSHOT_IMPORT_EXPECT"); expectPath != "" {
		expectData, err := os.ReadFile(expectPath)
		if err != nil {
			t.Fatalf("读取期望文件: %v", err)
		}
		var expected struct {
			Salt  string       `json:"salt"`
			Items []GoldenItem `json:"items"`
		}
		if err := json.Unmarshal(expectData, &expected); err != nil {
			t.Fatalf("期望文件格式无效: %v", err)
		}
		if dump.Salt != expected.Salt {
			t.Fatalf("盐值不一致: got %q want %q", dump.Salt, expected.Salt)
		}
		canonical := func(items []GoldenItem) []GoldenItem {
			cp := append([]GoldenItem(nil), items...)
			sort.Slice(cp, func(i, j int) bool { return cp[i].ID < cp[j].ID })
			for k := range cp {
				vs := append([]GoldenVersion(nil), cp[k].Versions...)
				sort.Slice(vs, func(i, j int) bool { return vs[i].Version < vs[j].Version })
				cp[k].Versions = vs
			}
			return cp
		}
		got := canonical(dump.Items)
		want := canonical(expected.Items)
		if len(got) != len(want) {
			t.Fatalf("条目数量不一致: got %d want %d", len(got), len(want))
		}
		for i := range got {
			if !reflect.DeepEqual(got[i], want[i]) {
				t.Fatalf("条目 %d 数据不一致:\n got: %+v\nwant: %+v", got[i].ID, got[i], want[i])
			}
		}
		fmt.Printf("期望比对通过: %d 个条目逐字段一致\n", len(got))
	}
}
