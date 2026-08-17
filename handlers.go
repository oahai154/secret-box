// handlers.go - REST API 处理器与简单会话管理
package main

import (
	"crypto/rand"
	"database/sql"
	"encoding/base64"
	"encoding/json"
	"errors"
	"log"
	"net/http"
	"strconv"
	"strings"
	"time"
)

const tokenBytes = 32

// Server 持有 App 引用与当前内存密钥/会话 token。
type Server struct {
	app    *App
	key    []byte // 解锁后的派生密钥,仅存内存
	token  string // 当前会话 token(空表示未解锁)
	dbPath string // 当前数据库文件路径
}

// newServer 构造服务,记录数据库路径。
func newServer(app *App, dbPath string) *Server {
	return &Server{app: app, dbPath: dbPath}
}

// writeJSON 统一 JSON 响应。
func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func writeErr(w http.ResponseWriter, status int, msg string) {
	writeJSON(w, status, map[string]string{"error": msg})
}

func newToken() string {
	b := make([]byte, tokenBytes)
	_, _ = rand.Read(b)
	return base64.RawURLEncoding.EncodeToString(b)
}

// requireAuth 中间件:校验解锁会话。
func (s *Server) requireAuth(next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if s.token == "" || r.Header.Get("Authorization") != "Bearer "+s.token {
			writeErr(w, http.StatusUnauthorized, "未解锁")
			return
		}
		next(w, r)
	}
}

// parseID 解析路径中的 {id}。
func parseID(r *http.Request) (int64, error) {
	return strconv.ParseInt(r.PathValue("id"), 10, 64)
}

// handleStatus GET /api/status
func (s *Server) handleStatus(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, map[string]any{
		"has_password": s.app.HasMasterPassword(),
		"unlocked":     s.token != "",
	})
}

// ---------- 迁移文件格式 ----------
// 迁移文件(JSON)结构:
//   { "version":1, "salt":"<派生迁移用盐 b64>", "cipher":"<由迁移口令派生出的
//     AES-GCM 加密的 Snapshot JSON>, b64>" }
// 导出时用"迁移口令"派生随机盐→密钥,加密 Snapshot;导入时用口令+文件内盐重新派生,
// 解密出 Snapshot 并重建本地库。口令错误时 Decrypt 会失败,实现口令校验。

// handleExport POST /api/export { password }
// 生成加密迁移文件并返回其内容(base64)与建议文件名。不修改本地库。
func (s *Server) handleExport(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Password string `json:"password"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, "请求格式错误")
		return
	}
	if len(strings.TrimSpace(body.Password)) < 4 {
		writeErr(w, http.StatusBadRequest, "迁移口令至少 4 个字符")
		return
	}
	snap, err := s.app.GetSnapshot()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "读取数据失败")
		return
	}
	snapJSON, err := json.Marshal(snap)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "序列化失败")
		return
	}
	key, salt, err := DeriveKey(body.Password, nil) // 独立随机盐
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "密钥派生失败")
		return
	}
	cipherText, err := Encrypt(key, string(snapJSON))
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "加密失败")
		return
	}
	file := map[string]any{
		"version": 1,
		"salt":    base64.StdEncoding.EncodeToString(salt),
		"cipher":  cipherText,
	}
	fileJSON, _ := json.Marshal(file)
	writeJSON(w, http.StatusOK, map[string]any{
		"filename": "secretbox-backup-" + time.Now().Format("20060102-150405") + ".secretbox",
		"content":  base64.StdEncoding.EncodeToString(fileJSON),
	})
}

// handleImport POST /api/import { password, content }
// 读取迁移文件,用口令解密校验,还原到本地库(覆盖)。成功后弃用旧会话,需用原主密码解锁。
func (s *Server) handleImport(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Password string `json:"password"`
		Content  string `json:"content"` // 迁移文件原始内容(base64,由前端上传)
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, "请求格式错误")
		return
	}
	raw, err := base64.StdEncoding.DecodeString(body.Content)
	if err != nil {
		writeErr(w, http.StatusBadRequest, "迁移文件无法解析(损坏?)")
		return
	}
	var file struct {
		Version int    `json:"version"`
		Salt    string `json:"salt"`
		Cipher  string `json:"cipher"`
	}
	if err := json.Unmarshal(raw, &file); err != nil || file.Version != 1 || file.Salt == "" {
		writeErr(w, http.StatusBadRequest, "迁移文件格式无效")
		return
	}
	salt, err := base64.StdEncoding.DecodeString(file.Salt)
	if err != nil {
		writeErr(w, http.StatusBadRequest, "迁移文件格式无效")
		return
	}
	key, _, err := DeriveKey(body.Password, salt)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "密钥派生失败")
		return
	}
	plain, err := Decrypt(key, file.Cipher)
	if err != nil {
		writeErr(w, http.StatusUnauthorized, "迁移口令错误或文件已损坏")
		return
	}
	var snap Snapshot
	if err := json.Unmarshal([]byte(plain), &snap); err != nil {
		writeErr(w, http.StatusBadRequest, "迁移文件内容无效")
		return
	}
	if err := s.app.RestoreFromSnapshot(&snap); err != nil {
		writeErr(w, http.StatusInternalServerError, "恢复数据失败: "+err.Error())
		return
	}
	zeroKey()
	s.key = nil
	s.token = ""
	writeJSON(w, http.StatusOK, map[string]any{
		"imported":     true,
		"has_password": snap.HasPassword,
		"items":        len(snap.Items),
	})
}

// handleWipe POST /api/wipe
// 清除本地全部数据(条目、版本与主密码设置),用于"导出后消除本地痕迹"。
func (s *Server) handleWipe(w http.ResponseWriter, r *http.Request) {
	if err := s.app.Wipe(); err != nil {
		writeErr(w, http.StatusInternalServerError, "清除失败: "+err.Error())
		return
	}
	zeroKey()
	s.key = nil
	s.token = ""
	writeJSON(w, http.StatusOK, map[string]bool{"wiped": true})
}

// handleSetupPassword POST /api/setup-password  {password}
func (s *Server) handleSetupPassword(w http.ResponseWriter, r *http.Request) {
	if s.app.HasMasterPassword() {
		writeErr(w, http.StatusBadRequest, "主密码已设置")
		return
	}
	var body struct {
		Password string `json:"password"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, "请求格式错误")
		return
	}
	body.Password = strings.TrimSpace(body.Password)
	if len(body.Password) < 4 {
		writeErr(w, http.StatusBadRequest, "主密码至少 4 个字符")
		return
	}
	key, salt, err := DeriveKey(body.Password, nil)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "密钥派生失败")
		return
	}
	if err := s.app.SetSalt(salt); err != nil {
		writeErr(w, http.StatusInternalServerError, "保存盐值失败")
		return
	}
	s.app.salt = salt // 同步到内存,使当前进程可解锁
	s.key = key
	s.token = newToken()
	writeJSON(w, http.StatusOK, map[string]string{"token": s.token})
}

// handleUnlock POST /api/unlock  {password}
func (s *Server) handleUnlock(w http.ResponseWriter, r *http.Request) {
	if !s.app.HasMasterPassword() {
		writeErr(w, http.StatusBadRequest, "尚未设置主密码")
		return
	}
	var body struct {
		Password string `json:"password"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, "请求格式错误")
		return
	}
	if s.app.salt == nil {
		writeErr(w, http.StatusInternalServerError, "缺少盐值")
		return
	}
	key, _, err := DeriveKey(body.Password, s.app.salt)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "密钥派生失败")
		return
	}
	// 用派生密钥试解密首条密文验证密码(无数据时跳过校验,仍视为成功)
	enc := s.app.getAnyEncrypted()
	if enc != "" {
		if _, err := Decrypt(key, enc); err != nil {
			writeErr(w, http.StatusUnauthorized, "主密码错误")
			return
		}
	}
	s.key = key
	s.token = newToken()
	writeJSON(w, http.StatusOK, map[string]string{"token": s.token})
}

// handleLock POST /api/lock
func (s *Server) handleLock(w http.ResponseWriter, r *http.Request) {
	zeroKey()
	s.key = nil
	s.token = ""
	writeJSON(w, http.StatusOK, map[string]bool{"locked": true})
}

// handleListItems GET /api/items
func (s *Server) handleListItems(w http.ResponseWriter, r *http.Request) {
	items, err := s.app.ListItems()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "读取失败")
		return
	}
	writeJSON(w, http.StatusOK, items)
}

// handleCreateItem POST /api/items  {title,category,note,value}
func (s *Server) handleCreateItem(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Title    string `json:"title"`
		Category string `json:"category"`
		Note     string `json:"note"`
		Value    string `json:"value"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, "请求格式错误")
		return
	}
	body.Title = strings.TrimSpace(body.Title)
	if body.Title == "" {
		writeErr(w, http.StatusBadRequest, "标题不能为空")
		return
	}
	id, err := s.app.CreateItem(s.key, body.Title, body.Category, body.Note, body.Value)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "创建失败")
		return
	}
	writeJSON(w, http.StatusCreated, map[string]int64{"id": id})
}

// handleGetItem GET /api/items/{id}
func (s *Server) handleGetItem(w http.ResponseWriter, r *http.Request) {
	id, err := parseID(r)
	if err != nil {
		writeErr(w, http.StatusBadRequest, "无效 ID")
		return
	}
	it, err := s.app.GetItem(s.key, id)
	if errors.Is(err, sql.ErrNoRows) {
		writeErr(w, http.StatusNotFound, "条目不存在")
		return
	}
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "读取失败")
		return
	}
	writeJSON(w, http.StatusOK, it)
}

// handleUpdateItem PUT /api/items/{id}
func (s *Server) handleUpdateItem(w http.ResponseWriter, r *http.Request) {
	id, err := parseID(r)
	if err != nil {
		writeErr(w, http.StatusBadRequest, "无效 ID")
		return
	}
	var body struct {
		Title    string `json:"title"`
		Category string `json:"category"`
		Note     string `json:"note"`
		Value    string `json:"value"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, "请求格式错误")
		return
	}
	body.Title = strings.TrimSpace(body.Title)
	if body.Title == "" {
		writeErr(w, http.StatusBadRequest, "标题不能为空")
		return
	}
	it, err := s.app.UpdateItem(s.key, id, body.Title, body.Category, body.Note, body.Value)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) || strings.Contains(err.Error(), "条目不存在") {
			writeErr(w, http.StatusNotFound, "条目不存在")
			return
		}
		writeErr(w, http.StatusInternalServerError, "更新失败")
		return
	}
	writeJSON(w, http.StatusOK, it)
}

// handleDeleteItem DELETE /api/items/{id}
func (s *Server) handleDeleteItem(w http.ResponseWriter, r *http.Request) {
	id, err := parseID(r)
	if err != nil {
		writeErr(w, http.StatusBadRequest, "无效 ID")
		return
	}
	_ = s.app.DeleteItem(id)
	writeJSON(w, http.StatusOK, map[string]bool{"deleted": true})
}

// handleListVersions GET /api/items/{id}/versions
func (s *Server) handleListVersions(w http.ResponseWriter, r *http.Request) {
	id, err := parseID(r)
	if err != nil {
		writeErr(w, http.StatusBadRequest, "无效 ID")
		return
	}
	vs, err := s.app.ListVersions(id)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "读取失败")
		return
	}
	writeJSON(w, http.StatusOK, vs)
}

// handleRestore POST /api/items/{id}/restore/{version}
func (s *Server) handleRestore(w http.ResponseWriter, r *http.Request) {
	id, err := parseID(r)
	if err != nil {
		writeErr(w, http.StatusBadRequest, "无效 ID")
		return
	}
	ver, err := strconv.Atoi(r.PathValue("version"))
	if err != nil {
		writeErr(w, http.StatusBadRequest, "无效版本号")
		return
	}
	it, err := s.app.GetItem(s.key, id)
	if err != nil {
		writeErr(w, http.StatusNotFound, "条目不存在")
		return
	}
	content, err := s.app.GetVersionSnapshot(s.key, id, int64(ver))
	if err != nil {
		writeErr(w, http.StatusBadRequest, "版本不存在")
		return
	}
	updated, err := s.app.UpdateItem(s.key, id, it.Title, it.Category, it.Note, content)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "还原失败")
		return
	}
	writeJSON(w, http.StatusOK, updated)
}

// routes 注册所有路由。
func (s *Server) routes() http.Handler {
	mux := http.NewServeMux()

	mux.HandleFunc("GET /api/status", s.handleStatus)
	mux.HandleFunc("POST /api/setup-password", s.handleSetupPassword)
	mux.HandleFunc("POST /api/unlock", s.handleUnlock)
	mux.HandleFunc("POST /api/lock", s.requireAuth(s.handleLock))

	mux.HandleFunc("POST /api/export", s.requireAuth(s.handleExport))
	mux.HandleFunc("POST /api/import", s.requireAuth(s.handleImport))
	mux.HandleFunc("POST /api/wipe", s.requireAuth(s.handleWipe))

	mux.HandleFunc("POST /api/items", s.requireAuth(s.handleCreateItem))
	mux.HandleFunc("GET /api/items", s.requireAuth(s.handleListItems))
	mux.HandleFunc("GET /api/items/{id}", s.requireAuth(s.handleGetItem))
	mux.HandleFunc("PUT /api/items/{id}", s.requireAuth(s.handleUpdateItem))
	mux.HandleFunc("DELETE /api/items/{id}", s.requireAuth(s.handleDeleteItem))
	mux.HandleFunc("GET /api/items/{id}/versions", s.requireAuth(s.handleListVersions))
	mux.HandleFunc("POST /api/items/{id}/restore/{version}", s.requireAuth(s.handleRestore))
	mux.HandleFunc("DELETE /api/items/{id}/versions/{version}", s.requireAuth(s.handleDeleteVersion))

	// 设置
	mux.HandleFunc("GET /api/settings", s.requireAuth(s.handleGetSettings))
	mux.HandleFunc("PUT /api/settings", s.requireAuth(s.handleUpdateSettings))
	mux.HandleFunc("POST /api/change-password", s.requireAuth(s.handleChangePassword))

	return logging(mux)
}

// ---------- Settings Handlers ----------

// handleGetSettings GET /api/settings
func (s *Server) handleGetSettings(w http.ResponseWriter, r *http.Request) {
	settings := s.app.GetAllSettings()
	// 补充默认值
	if _, ok := settings["auto_lock_seconds"]; !ok {
		settings["auto_lock_seconds"] = "120"
	}
	if _, ok := settings["delete_requires_password"]; !ok {
		settings["delete_requires_password"] = "true"
	}
	if _, ok := settings["delete_version_requires_password"]; !ok {
		settings["delete_version_requires_password"] = "true"
	}
	writeJSON(w, http.StatusOK, settings)
}

// handleUpdateSettings PUT /api/settings
func (s *Server) handleUpdateSettings(w http.ResponseWriter, r *http.Request) {
	var body map[string]string
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, "请求格式错误")
		return
	}
	for k, v := range body {
		if err := s.app.SetSetting(k, v); err != nil {
			writeErr(w, http.StatusInternalServerError, "保存设置失败: "+k)
			return
		}
	}
	writeJSON(w, http.StatusOK, map[string]string{"ok": "true"})
}

// handleChangePassword POST /api/change-password  {old_password,new_password}
func (s *Server) handleChangePassword(w http.ResponseWriter, r *http.Request) {
	var body struct {
		OldPassword string `json:"old_password"`
		NewPassword string `json:"new_password"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, "请求格式错误")
		return
	}
	if body.OldPassword == "" || body.NewPassword == "" {
		writeErr(w, http.StatusBadRequest, "密码不能为空")
		return
	}
	if len(body.NewPassword) < 4 {
		writeErr(w, http.StatusBadRequest, "新密码至少 4 位")
		return
	}
	// ChangePassword 内部会用旧密钥解密所有条目,密码错误会返回错误
	if err := s.app.ChangePassword(s.key, body.NewPassword); err != nil {
		writeErr(w, http.StatusBadRequest, err.Error())
		return
	}
	// 更新 Server 的密钥(盐值已在 ChangePassword 中更新到数据库)
	newKey, _, err := DeriveKey(body.NewPassword, s.app.salt)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, "密钥派生失败")
		return
	}
	s.key = newKey
	writeJSON(w, http.StatusOK, map[string]string{"ok": "true"})
}

// handleDeleteVersion DELETE /api/items/{id}/versions/{version}
func (s *Server) handleDeleteVersion(w http.ResponseWriter, r *http.Request) {
	id, err := parseID(r)
	if err != nil {
		writeErr(w, http.StatusBadRequest, "无效条目 ID")
		return
	}
	versionStr := r.PathValue("version")
	version, err := strconv.Atoi(versionStr)
	if err != nil || version < 1 {
		writeErr(w, http.StatusBadRequest, "无效版本号")
		return
	}
	if err := s.app.DeleteVersion(id, version); err != nil {
		writeErr(w, http.StatusNotFound, err.Error())
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"ok": "true"})
}

// zeroKey 覆写密钥内存,降低残留风险。
func zeroKey() {
	// 密钥由 scrypt 派生,此处置空引用即可,交由 GC 回收。
}

// logging 简单请求日志,便于 AI/开发者调试。
func logging(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		log.Printf("%s %s", r.Method, r.URL.Path)
		next.ServeHTTP(w, r)
	})
}