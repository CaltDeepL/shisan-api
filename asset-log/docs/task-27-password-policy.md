# タスク #27: パスワード要件の強化

## 概要

登録時のパスワード検証を、単純な「12文字以上」から次の要件へ拡張する。

| 項目 | 要件 |
|---|---|
| 最小長 | 12 Unicode 文字 |
| 最大長 | 256 Unicode 文字 |
| byte 上限 | UTF-8 で 512 bytes |
| common password | denylist に一致する値を拒否 |
| login | 登録要件の validation は行わない |

文字数は `String::len()` ではなく `chars().count()` で判定する。一方、Argon2 に渡る実データ量を制限するため byte 上限も別に持つ。

## 構成

### 登録時 validation

```rust
const MIN_PASSWORD_CHARS: usize = 12;
const MAX_PASSWORD_CHARS: usize = 256;
const MAX_PASSWORD_BYTES: usize = 512;
```

validation の順序は次の通り。

```text
12文字未満
  ↓
256文字超
  ↓
UTF-8 512 bytes超
  ↓
common password
```

### common password

小規模な denylist をコード内に固定する。比較は ASCII の大文字小文字を区別しない。

パスワード本体の Unicode normalization や lower-case 化は行わない。hash へ渡す文字列はユーザーが入力した値そのもの。

### login では policy を再検証しない

既存方針を維持する。login で登録時要件を返さず、email normalization と hash verification だけを行う。

### テスト共通ユーザー

既存の `tests/common/register_user()` は `password1234` を使用していたため、denylist 導入後は `integration-passphrase-123` へ変更する。

## フロントエンド

登録画面の hint を次へ変更する。

```text
12〜256文字・一般的なパスワードは使用不可
```

最終判定はバックエンドを正本とし、422 の `ProblemDetails.errors` を既存 UI で表示する。

## 検証項目

| # | 確認 | 期待 |
|---|---|---|
| 1 | 11文字 | 422 |
| 2 | 12文字の Unicode password | 201 |
| 3 | 257文字 | 422 |
| 4 | 256文字以内だが UTF-8 512 bytes 超 | 422 |
| 5 | `password1234` | 422 |
| 6 | 正常な passphrase | 201 |
| 7 | login | policy validation を通さず従来どおり認証 |

## 残課題

denylist は意図的に小さく始める。大規模漏洩パスワード corpus を導入する場合は、バイナリサイズ・更新方法・検索コストを別タスクで設計する。
