package main

import (
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

	id, err := a.CreateItem(key, "GH", "API密钥", "original_001")
	if err != nil {
		t.Fatal(err)
	}

	// 第一次更新 -> v2
	if _, err := a.UpdateItem(key, id, "GH", "API密钥", "changed_002"); err != nil {
		t.Fatal(err)
	}
	// 第二次更新 -> v3
	if _, err := a.UpdateItem(key, id, "GH", "API密钥", "changed_003"); err != nil {
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
	item, err := a.UpdateItem(key, id, "GH", "API密钥", snap)
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