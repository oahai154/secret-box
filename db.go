// db.go - 数据层:SQLite 初始化 + 条目 CRUD + 版本历史管理
package main

import (
	"database/sql"
	"encoding/base64"
	"errors"
	"time"

	_ "modernc.org/sqlite"
)

// Item 条目(读列表时不含明文;单独读取时填充 Value)。
type Item struct {
	ID        int64  `json:"id"`
	Title     string `json:"title"`
	Category  string `json:"category"`
	CreatedAt string `json:"created_at"`
	UpdatedAt string `json:"updated_at"`
	Value     string `json:"value,omitempty"`
	// 可选的计数统计(列表附带版本数量)
	VersionCount int `json:"version_count,omitempty"`
}

// Version 单个历史版本。
type Version struct {
	ID        int64  `json:"id"`
	SecretID  int64  `json:"secret_id"`
	Version   int    `json:"version"`
	CreatedAt string `json:"created_at"`
}

// App 持有数据库句柄与运行时元信息盐值。
type App struct {
	db   *sql.DB
	salt []byte // 主密码派生密钥所用盐值,从 meta 表读取
}

// NewApp 打开或创建数据库文件,必要时初始化表与 meta 盐值。
func NewApp(dbPath string) (*App, error) {
	a := &App{}
	db, err := sql.Open("sqlite", dbPath+"?_pragma=journal_mode(WAL)&_pragma=foreign_keys(1)")
	if err != nil {
		return nil, err
	}
	a.db = db

	queries := []string{
		`CREATE TABLE IF NOT EXISTS meta (
			key TEXT PRIMARY KEY,
			value TEXT NOT NULL
		)`,
		`CREATE TABLE IF NOT EXISTS secret_items (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			title TEXT NOT NULL,
			category TEXT NOT NULL DEFAULT '',
			encrypted_value TEXT NOT NULL,
			created_at TEXT NOT NULL,
			updated_at TEXT NOT NULL
		)`,
		`CREATE TABLE IF NOT EXISTS secret_versions (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			secret_id INTEGER NOT NULL REFERENCES secret_items(id) ON DELETE CASCADE,
			version INTEGER NOT NULL,
			encrypted_snapshot TEXT NOT NULL,
			created_at TEXT NOT NULL
		)`,
		`CREATE INDEX IF NOT EXISTS idx_versions_secret ON secret_versions(secret_id, version DESC)`,
	}
	for _, q := range queries {
		if _, err := db.Exec(q); err != nil {
			return nil, err
		}
	}
	// 读取或生成盐值
	row := db.QueryRow(`SELECT value FROM meta WHERE key='salt'`)
	var enc string
	switch err := row.Scan(&enc); err {
	case sql.ErrNoRows:
		// 生成盐值交给加密层;这里先占位,由设置主密码时写入
		a.salt = nil
	case nil:
		a.salt, err = base64.StdEncoding.DecodeString(enc)
		if err != nil {
			return nil, err
		}
	default:
		return nil, err
	}
	return a, nil
}

func nowISO() string { return time.Now().Format(time.RFC3339) }

// SetSalt 持久化盐值(设置主密码时调用)。
func (a *App) SetSalt(salt []byte) error {
	_, err := a.db.Exec(
		`INSERT INTO meta(key,value) VALUES('salt',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value`,
		base64.StdEncoding.EncodeToString(salt))
	return err
}

// HasMasterPassword 是否已设置主密码(依据盐值是否已写)。
func (a *App) HasMasterPassword() bool {
	var n int
	_ = a.db.QueryRow(`SELECT COUNT(*) FROM meta WHERE key='salt'`).Scan(&n)
	return n > 0
}

// CreateItem 新增条目并写入首个版本。
func (a *App) CreateItem(key []byte, title, category, value string) (int64, error) {
	enc, err := Encrypt(key, value)
	if err != nil {
		return 0, err
	}
	now := nowISO()
	tx, err := a.db.Begin()
	if err != nil {
		return 0, err
	}
	defer tx.Rollback()
	res, err := tx.Exec(
		`INSERT INTO secret_items(title,category,encrypted_value,created_at,updated_at) VALUES(?,?,?,?,?)`,
		title, category, enc, now, now)
	if err != nil {
		return 0, err
	}
	id, _ := res.LastInsertId()
	if _, err := tx.Exec(
		`INSERT INTO secret_versions(secret_id,version,encrypted_snapshot,created_at) VALUES(?,1,?,?)`,
		id, enc, now); err != nil {
		return 0, err
	}
	return id, tx.Commit()
}

// ListItems 返回全部条目(不含明文)及版本数量。
func (a *App) ListItems() ([]Item, error) {
	rows, err := a.db.Query(`
		SELECT s.id,s.title,s.category,s.created_at,s.updated_at,
		       (SELECT COUNT(*) FROM secret_versions v WHERE v.secret_id=s.id) AS vcount
		FROM secret_items s ORDER BY s.updated_at DESC`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	items := []Item{}
	for rows.Next() {
		var it Item
		if err := rows.Scan(&it.ID, &it.Title, &it.Category, &it.CreatedAt, &it.UpdatedAt, &it.VersionCount); err != nil {
			return nil, err
		}
		items = append(items, it)
	}
	return items, rows.Err()
}

// GetItem 读取单条并返回解密明文。
func (a *App) GetItem(key []byte, id int64) (*Item, error) {
	var it Item
	var enc string
	row := a.db.QueryRow(
		`SELECT id,title,category,encrypted_value,created_at,updated_at FROM secret_items WHERE id=?`, id)
	if err := row.Scan(&it.ID, &it.Title, &it.Category, &enc, &it.CreatedAt, &it.UpdatedAt); err != nil {
		return nil, err
	}
	val, err := Decrypt(key, enc)
	if err != nil {
		return nil, err
	}
	it.Value = val
	return &it, nil
}

// UpdateItem 更新条目标题/分类/内容,并创建新版本。
func (a *App) UpdateItem(key []byte, id int64, title, category, value string) (*Item, error) {
	enc, err := Encrypt(key, value)
	if err != nil {
		return nil, err
	}
	now := nowISO()
	tx, err := a.db.Begin()
	if err != nil {
		return nil, err
	}
	defer tx.Rollback()
	// 计算下一版本号
	var nextV int
	if err := tx.QueryRow(`SELECT COALESCE(MAX(version),0)+1 FROM secret_versions WHERE secret_id=?`, id).Scan(&nextV); err != nil {
		return nil, err
	}
	if _, err := tx.Exec(
		`INSERT INTO secret_versions(secret_id,version,encrypted_snapshot,created_at) VALUES(?,?,?,?)`,
		id, nextV, enc, now); err != nil {
		return nil, err
	}
	res, err := tx.Exec(
		`UPDATE secret_items SET title=?,category=?,encrypted_value=?,updated_at=? WHERE id=?`,
		title, category, enc, now, id)
	if err != nil {
		return nil, err
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return nil, errors.New("条目不存在")
	}
	item, err := a.getFromTx(tx, key, id)
	if err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return item, nil
}

// DeleteItem 删除条目(版本因 ON DELETE CASCADE 一并删除)。
func (a *App) DeleteItem(id int64) error {
	_, err := a.db.Exec(`DELETE FROM secret_items WHERE id=?`, id)
	return err
}

// ListVersions 返回某条目全部版本(仅元数据)。
func (a *App) ListVersions(id int64) ([]Version, error) {
	rows, err := a.db.Query(
		`SELECT id,secret_id,version,created_at FROM secret_versions WHERE secret_id=? ORDER BY version DESC`, id)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	vs := []Version{}
	for rows.Next() {
		var v Version
		if err := rows.Scan(&v.ID, &v.SecretID, &v.Version, &v.CreatedAt); err != nil {
			return nil, err
		}
		vs = append(vs, v)
	}
	return vs, rows.Err()
}

// GetVersionSnapshot 读取某历史版本的加密快照解密,其内容。
func (a *App) GetVersionSnapshot(key []byte, id, version int64) (string, error) {
	var enc string
	row := a.db.QueryRow(
		`SELECT encrypted_snapshot FROM secret_versions WHERE secret_id=? AND version=?`, id, version)
	if err := row.Scan(&enc); err != nil {
		return "", err
	}
	return Decrypt(key, enc)
}

// getFromTx 在事务内读取条目(供 UpdateItem 复用)。
func (a *App) getFromTx(tx *sql.Tx, key []byte, id int64) (*Item, error) {
	var it Item
	var enc string
	row := tx.QueryRow(
		`SELECT id,title,category,encrypted_value,created_at,updated_at FROM secret_items WHERE id=?`, id)
	if err := row.Scan(&it.ID, &it.Title, &it.Category, &enc, &it.CreatedAt, &it.UpdatedAt); err != nil {
		return nil, err
	}
	val, err := Decrypt(key, enc)
	if err != nil {
		return nil, err
	}
	it.Value = val
	return &it, nil
}

// getAnyEncrypted 返回任一条目的密文,用于解锁时校验主密码;无数据返回空串。
func (a *App) getAnyEncrypted() string {
	var enc string
	// 优先取 secret_items,若空则取任意历史快照
	err := a.db.QueryRow(`SELECT encrypted_value FROM secret_items LIMIT 1`).Scan(&enc)
	if err != nil {
		_ = a.db.QueryRow(`SELECT encrypted_snapshot FROM secret_versions LIMIT 1`).Scan(&enc)
	}
	return enc
}

// Close 关闭数据库。
func (a *App) Close() error { return a.db.Close() }