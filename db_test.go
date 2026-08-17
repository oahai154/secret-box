package main

import (
	"encoding/base64"
	"encoding/json"
	"path/filepath"
	"testing"
)

func TestDBVersionHistory(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "test.db")
	a, err := NewApp(dbPath)
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()

	key, salt, _ := DeriveKey("master", nil)
	if err := a.SetSalt(salt); err != nil {
		t.Fatal(err)
	}

	id, err := a.CreateItem(key, "GH", "API密钥", "", "original_001")
	if err != nil {
		t.Fatal(err)
	}

	// 第一次更新 -> v2
	if _, err := a.UpdateItem(key, id, "GH", "API密钥", "", "changed_002"); err != nil {
		t.Fatal(err)
	}
	// 第二次更新 -> v3
	if _, err := a.UpdateItem(key, id, "GH", "API密钥", "", "changed_003"); err != nil {
		t.Fatal(err)
	}

	vs, err := a.ListVersions(id)
	if err != nil {
		t.Fatal(err)
	}
	if len(vs) != 3 {
		t.Fatalf("version count = %d, want 3", len(vs))
	}
	if vs[0].Version != 3 {
		t.Fatalf("latest version = %d, want 3", vs[0].Version)
	}

	// 还原到 v1
	snap, err := a.GetVersionSnapshot(key, id, 1)
	if err != nil {
		t.Fatal(err)
	}
	if snap != "original_001" {
		t.Fatalf("snapshot v1 = %q, want 'original_001'", snap)
	}
	item, err := a.UpdateItem(key, id, "GH", "API密钥", "", snap)
	if err != nil {
		t.Fatal(err)
	}
	if item.Value != "original_001" {
		t.Fatalf("after restore value = %q, want 'original_001'", item.Value)
	}
	vs2, _ := a.ListVersions(id)
	if len(vs2) != 4 {
		t.Fatalf("version count after restore = %d, want 4", len(vs2))
	}

	// 删除条目后版本一并删除
	if err := a.DeleteItem(id); err != nil {
		t.Fatal(err)
	}
	vs3, _ := a.ListVersions(id)
	if len(vs3) != 0 {
		t.Fatalf("versions after delete = %d, want 0", len(vs3))
	}
}

// TestMigrationRoundTrip 验证:导出快照 -> 清空(wire/wipe) -> 还原 后数据与主密码盐保持一致。
func TestMigrationRoundTrip(t *testing.T) {
	dir := t.TempDir()
	a, err := NewApp(filepath.Join(dir, "src.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	key, salt, _ := DeriveKey("master", nil)
	if err := a.SetSalt(salt); err != nil {
		t.Fatal(err)
	}
	id, _ := a.CreateItem(key, "GH", "API密钥", "", "v1")
	if _, err := a.UpdateItem(key, id, "GH", "API密钥", "", "v2-after-two-updates"); err != nil {
		t.Fatal(err)
	}

	// 导出快照
	snap, err := a.GetSnapshot()
	if err != nil {
		t.Fatal(err)
	}
	if !snap.HasPassword {
		t.Fatal("snapshot should report has_password")
	}
	if len(snap.Items) != 1 || len(snap.Items[0].Versions) != 2 {
		t.Fatalf("snapshot items=%d versions=%d, want 1/2", len(snap.Items), len(snap.Items[0].Versions))
	}
	// 模拟迁移文件完整往返:加密 -> 解密
	mKey, mSalt, _ := DeriveKey("migrate-pass", nil)
	raw, _ := json.Marshal(snap)
	c, _ := Encrypt(mKey, string(raw))
	envelope, _ := json.Marshal(map[string]any{
		"version": 1,
		"salt":    base64.StdEncoding.EncodeToString(mSalt),
		"cipher":  c,
	})
	// 解密
	var f struct {
		Salt   string `json:"salt"`
		Cipher string `json:"cipher"`
	}
	_ = json.Unmarshal(envelope, &f)
	fsalt, _ := base64.StdEncoding.DecodeString(f.Salt)
	dKey, _, _ := DeriveKey("migrate-pass", fsalt)
	plain, err := Decrypt(dKey, f.Cipher)
	if err != nil {
		t.Fatal("decrypt envelope failed:", err)
	}
	var restored Snapshot
	_ = json.Unmarshal([]byte(plain), &restored)

	// 把快照还原到一个全新库(模拟迁移到新位置)
	b, err := NewApp(filepath.Join(dir, "dst.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer b.Close()
	if err := b.RestoreFromSnapshot(&restored); err != nil {
		t.Fatal(err)
	}
	if !b.HasMasterPassword() {
		t.Fatal("restored db should have master password")
	}
	// 用原主密码解锁后读取条目与历史
	dkey2, _, _ := DeriveKey("master", b.salt)
	it, err := b.GetItem(dkey2, 1)
	if err != nil {
		t.Fatal("read restored item failed:", err)
	}
	if it.Value != "v2-after-two-updates" {
		t.Fatalf("restored value=%q", it.Value)
	}
	vs, _ := b.ListVersions(1)
	if len(vs) != 2 {
		t.Fatalf("restored versions=%d want 2", len(vs))
	}

	// Wipe 清空
	if err := b.Wipe(); err != nil {
		t.Fatal(err)
	}
	if b.HasMasterPassword() {
		t.Fatal("after wipe has_password should be false")
	}
	list, _ := b.ListItems()
	if len(list) != 0 {
		t.Fatalf("after wipe items=%d want 0", len(list))
	}
}