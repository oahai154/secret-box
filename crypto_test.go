package main

import "testing"

func TestDeriveAndEncryptDecrypt(t *testing.T) {
	key, salt, err := DeriveKey("my-master-password", nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(key) != 32 {
		t.Fatalf("key length = %d, want 32", len(key))
	}
	if len(salt) != saltSize {
		t.Fatalf("salt length = %d, want %d", len(salt), saltSize)
	}

	plain := "ghp_1234567890secret-token"
	enc, err := Encrypt(key, plain)
	if err != nil {
		t.Fatal(err)
	}
	if enc == plain {
		t.Fatal("ciphertext should not equal plaintext")
	}

	dec, err := Decrypt(key, enc)
	if err != nil {
		t.Fatal(err)
	}
	if dec != plain {
		t.Fatalf("roundtrip mismatch: %q != %q", dec, plain)
	}
}

func TestDecryptWrongKey(t *testing.T) {
	key, _, err := DeriveKey("correct-password", nil)
	if err != nil {
		t.Fatal(err)
	}
	enc, err := Encrypt(key, "secret")
	if err != nil {
		t.Fatal(err)
	}

	wrongKey, _, err := DeriveKey("wrong-password", nil)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := Decrypt(wrongKey, enc); err == nil {
		t.Fatal("expected error decrypting with wrong key")
	}
}

func TestSaltDeterminism(t *testing.T) {
	// 相同密码+相同盐 => 相同密钥(用于重启后解锁)
	_, salt, _ := DeriveKey("password", nil)
	k1, _, _ := DeriveKey("password", salt)
	k2, _, _ := DeriveKey("password", salt)
	if string(k1) != string(k2) {
		t.Fatal("same salt + password must derive same key")
	}
}