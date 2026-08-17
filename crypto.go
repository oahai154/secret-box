// crypto.go - 加密层:scrypt 密钥派生 + AES-256-GCM 加解密
package main

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"encoding/base64"
	"errors"
	"io"

	"golang.org/x/crypto/scrypt"
)

const (
	saltSize    = 32
	nonceSize   = 12
	scryptN     = 1 << 15
	scryptR     = 8
	scryptP     = 1
	keyLen      = 32 // AES-256
)

// DeriveKey 从主密码派生 AES-256 密钥。
// 若传入 salt 为空则生成新随机盐;返回派生后的明文密钥(仅存于内存)与盐值。
func DeriveKey(password string, salt []byte) (key, saltOut []byte, err error) {
	if len(salt) == 0 {
		salt = make([]byte, saltSize)
		if _, err = io.ReadFull(rand.Reader, salt); err != nil {
			return nil, nil, err
		}
	}
	key, err = scrypt.Key([]byte(password), salt, scryptN, scryptR, scryptP, keyLen)
	if err != nil {
		return nil, nil, err
	}
	return key, salt, nil
}

// Encrypt AES-256-GCM 加密,返回 base64 编码字符串。
func Encrypt(key []byte, plaintext string) (string, error) {
	block, err := aes.NewCipher(key)
	if err != nil {
		return "", err
	}
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return "", err
	}
	nonce := make([]byte, gcm.NonceSize())
	if _, err = io.ReadFull(rand.Reader, nonce); err != nil {
		return "", err
	}
	// 输出 = base64( nonce || ciphertext+tag )
	out := gcm.Seal(nonce, nonce, []byte(plaintext), nil)
	return base64.StdEncoding.EncodeToString(out), nil
}

// Decrypt 解密 base64 编码的 GCM 密文。密钥错误将返回错误。
func Decrypt(key []byte, encoded string) (string, error) {
	raw, err := base64.StdEncoding.DecodeString(encoded)
	if err != nil {
		return "", err
	}
	block, err := aes.NewCipher(key)
	if err != nil {
		return "", err
	}
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return "", err
	}
	if len(raw) < gcm.NonceSize() {
		return "", errors.New("密文长度不足")
	}
	nonce, ciphertext := raw[:gcm.NonceSize()], raw[gcm.NonceSize():]
	plain, err := gcm.Open(nil, nonce, ciphertext, nil)
	if err != nil {
		return "", errors.New("解密失败(主密码可能不正确)")
	}
	return string(plain), nil
}