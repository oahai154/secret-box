// browser_other.go - 非 Windows 平台打开默认浏览器。
//go:build !windows

package main

import (
	"fmt"
	"os/exec"
	"runtime"
)

func openBrowser(urlStr string) error {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		cmd = exec.Command("open", urlStr)
	default:
		cmd = exec.Command("xdg-open", urlStr)
	}
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("启动 %s 失败: %w", cmd.Path, err)
	}
	return nil
}
