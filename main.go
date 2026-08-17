// main.go - SecretBox 入口:数据库启动 + 静态资源 embed + HTTP 服务
package main

import (
	"embed"
	"flag"
	"fmt"
	"io/fs"
	"log"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"time"
)

//go:embed web
var webFS embed.FS

func main() {
	port := flag.Int("port", 8080, "监听端口")
	dbPath := flag.String("db", "", "数据库文件路径(默认:可执行文件同目录 secretbox.db)")
	noOpen := flag.Bool("no-open", false, "不自动打开浏览器")
	flag.Parse()

	// 数据库路径:默认放在可执行文件同目录,便于单文件分发
	if *dbPath == "" {
		exe, err := os.Executable()
		if err != nil {
			exe = "."
		}
		*dbPath = filepath.Join(filepath.Dir(exe), "secretbox.db")
	}

	app, err := NewApp(*dbPath)
	if err != nil {
		log.Fatalf("数据库初始化失败: %v", err)
	}
	defer app.Close()

	srv := newServer(app)
	handler := staticHandler(srv.routes())

	// 监听 127.0.0.1,仅本机访问
	ln, err := net.Listen("tcp", fmt.Sprintf("127.0.0.1:%d", *port))
	if err != nil {
		log.Fatalf("监听端口失败: %v", err)
	}
	addr := ln.Addr().(*net.TCPAddr)
	webURL := fmt.Sprintf("http://127.0.0.1:%d", addr.Port)
	log.Printf("SecretBox 已启动: %s   (数据库: %s)", webURL, *dbPath)

	if !*noOpen {
		go func() {
			time.Sleep(300 * time.Millisecond)
			openBrowser(webURL)
		}()
	}

	httpServer := &http.Server{Handler: handler}
	if err := httpServer.Serve(ln); err != nil {
		log.Fatalf("服务异常: %v", err)
	}
}

// staticHandler 将 /api 交给业务路由,其余路径从 embed 的 web 目录提供。
func staticHandler(api http.Handler) http.Handler {
	sub, err := fs.Sub(webFS, "web")
	if err != nil {
		log.Fatalf("静态资源加载失败: %v", err)
	}
	fileServer := http.FileServer(http.FS(sub))
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if len(r.URL.Path) >= 4 && r.URL.Path[:4] == "/api" {
			api.ServeHTTP(w, r)
			return
		}
		fileServer.ServeHTTP(w, r)
	})
}

// openBrowser 跨平台打开默认浏览器。
func openBrowser(urlStr string) {
	u, _ := url.Parse(urlStr)
	_ = u
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		cmd = exec.Command("open", urlStr)
	case "windows":
		cmd = exec.Command("rundll32", "url.dll,FileProtocolHandler", urlStr)
	default:
		cmd = exec.Command("xdg-open", urlStr)
	}
	if err := cmd.Start(); err != nil {
		log.Printf("打开浏览器失败: %v", err)
	}
}