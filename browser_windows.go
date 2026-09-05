// browser_windows.go - Windows 下打开默认浏览器。
// 优先使用 shell32!ShellExecuteW(系统默认协议处理器,最可靠),
// 失败时依次回退 rundll32 与 cmd start,并把错误带 URL 记录到日志。
//go:build windows

package main

import (
	"fmt"
	"os/exec"

	"golang.org/x/sys/windows"
)

func openBrowser(urlStr string) error {
	// 首选:ShellExecuteW,走系统默认的 http/https 协议关联
	err := windows.ShellExecute(0, windows.StringToUTF16Ptr("open"), windows.StringToUTF16Ptr(urlStr), nil, nil, windows.SW_SHOWNORMAL)
	if err == nil {
		return nil
	}
	firstErr := err

	// 回退 1:rundll32 FileProtocolHandler
	if err := exec.Command("rundll32", "url.dll,FileProtocolHandler", urlStr).Start(); err == nil {
		return nil
	}

	// 回退 2:cmd start(空 title 参数防止 URL 被当作窗口标题)
	if err := exec.Command("cmd", "/c", "start", "", urlStr).Start(); err == nil {
		return nil
	}

	return fmt.Errorf("ShellExecute 失败: %w; rundll32/cmd start 回退均失败", firstErr)
}
