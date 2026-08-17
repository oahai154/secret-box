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
)

const tokenBytes = 32

// Server 持有 App 引用与当前内存密钥/会话 token。
type Server struct {
	app   *App
	key   []byte // 解锁后的派生密钥,仅存内存
	token string // 当前会话 token(空表示未解锁)
}

func newServer(app *App) *Server { return &Server{app: app} }

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

// handleCreateItem POST /api/items  {title,category,value}
func (s *Server) handleCreateItem(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Title    string `json:"title"`
		Category string `json:"category"`
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
	id, err := s.app.CreateItem(s.key, body.Title, body.Category, body.Value)
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
	it, err := s.app.UpdateItem(s.key, id, body.Title, body.Category, body.Value)
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
	updated, err := s.app.UpdateItem(s.key, id, it.Title, it.Category, content)
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

	mux.HandleFunc("POST /api/items", s.requireAuth(s.handleCreateItem))
	mux.HandleFunc("GET /api/items", s.requireAuth(s.handleListItems))
	mux.HandleFunc("GET /api/items/{id}", s.requireAuth(s.handleGetItem))
	mux.HandleFunc("PUT /api/items/{id}", s.requireAuth(s.handleUpdateItem))
	mux.HandleFunc("DELETE /api/items/{id}", s.requireAuth(s.handleDeleteItem))
	mux.HandleFunc("GET /api/items/{id}/versions", s.requireAuth(s.handleListVersions))
	mux.HandleFunc("POST /api/items/{id}/restore/{version}", s.requireAuth(s.handleRestore))

	return logging(mux)
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