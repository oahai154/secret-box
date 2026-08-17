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
	port := flag.Int("port", 0, "监听端口(0=自动探测空闲端口,默认从 8080 起)")
	dbFlag := flag.String("db", "", "数据库文件路径(默认:用户数据目录 SecretBox/secretbox.db)")
	noOpen := flag.Bool("no-open", false, "不自动打开浏览器")
	flag.Parse()

	// 数据库路径:默认放到用户数据目录,保证 exe 无论放哪都能找到数据
	dbPath := *dbFlag
	if dbPath == "" {
		dbPath = defaultDBPath()
	}

	// 确保数据目录存在后再打开数据库
	if err := os.MkdirAll(filepath.Dir(dbPath), 0o700); err != nil {
		log.Fatalf("创建数据目录失败: %v", err)
	}

	app, err := NewApp(dbPath)
	if err != nil {
		log.Fatalf("数据库初始化失败: %v", err)
	}
	defer app.Close()

	srv := newServer(app, dbPath)
	handler := staticHandler(srv.routes())

	// 端口:0 表示自动探测。先尝试指定端口(默认从 8080 起),占用则向上递增到空闲。
	ln, err := listen(port)
	if err != nil {
		log.Fatalf("无可用端口: %v", err)
	}
	addr := ln.Addr().(*net.TCPAddr)
	webURL := fmt.Sprintf("http://127.0.0.1:%d", addr.Port)
	log.Printf("SecretBox 已启动: %s   (数据库: %s)", webURL, dbPath)

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

// defaultDBPath 返回跨平台的用户数据目录下的数据库路径。
// Windows: %LOCALAPPDATA%\SecretBox; macOS: ~/Library/Application Support;
// Linux: ~/.local/share。保证无论 exe 放哪,数据始终落在用户固定位置。
func defaultDBPath() string {
	dir := ""
	for _, p := range []string{os.Getenv("LOCALAPPDATA"), os.Getenv("XDG_DATA_HOME"), os.Getenv("APPDATA")} {
		if p != "" {
			dir = p
			break
		}
	}
	if dir == "" {
		if home, err := os.UserHomeDir(); err == nil {
			switch runtime.GOOS {
			case "darwin":
				dir = filepath.Join(home, "Library", "Application Support")
			default:
				dir = filepath.Join(home, ".local", "share")
			}
		}
	}
	if dir == "" {
		dir = "."
	}
	return filepath.Join(dir, "SecretBox", "secretbox.db")
}

// listen 自动选择一个空闲端口监听 127.0.0.1。
// port <= 0 时从 8080 起向上探测;port > 0 时先尝试指定端口,被占用则递增。
func listen(port *int) (net.Listener, error) {
	start := *port
	if start <= 0 {
		start = 8080
	}
	for p := start; p < start+200; p++ {
		ln, err := net.Listen("tcp", fmt.Sprintf("127.0.0.1:%d", p))
		if err == nil {
			return ln, nil
		}
	}
	return nil, fmt.Errorf("端口 %d-%d 均不可用", start, start+199)
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