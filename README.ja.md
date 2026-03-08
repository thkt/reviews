[English](README.md) | **日本語**

# reviews

設定されたスキル（デフォルト: `/review`）の実行前に静的解析ツールを走らせ、結果をエージェントにコンテキストとして渡す [Claude Code hook](https://docs.anthropic.com/en/docs/claude-code/hooks)。エージェントが手動でコードを読む代わりに、リンター出力・型エラーを事前に取得できる。

## 仕組み

```text
/review → PreToolUse hook 発火 → reviews バイナリ実行
  ├─ プロジェクト種別を検出（package.json, tsconfig.json, React）
  ├─ 該当ツールを OS スレッドで並列実行
  └─ ツール出力を additionalContext として JSON 返却
        → 監査エージェントが実際の静的解析結果を参照
```

hook は**アドバイザリー専用**：常にツール呼び出しを承認し、スキルをブロックしない。ツールの失敗や未インストールは静かにスキップされる。

## 特徴

- **並列実行**: 有効な全ツールを OS スレッドで同時実行
- **フェイルオープン設計**: エラーがスキルをブロックしない
- **自動検出**: プロジェクトに該当するツールのみ実行（package.json, tsconfig.json, React）
- **バイナリ解決**: ツールを `node_modules/.bin` から `.git` 境界まで探索

## 必要なツール

使いたいツールをインストール：

| ツール                                                    | インストール                                |
| --------------------------------------------------------- | ------------------------------------------- |
| [oxlint](https://oxc.rs)                                  | `npm i -g oxlint`                           |
| [knip](https://knip.dev)                                  | `npm i -D knip`（プロジェクトローカル推奨） |
| [tsgo](https://github.com/microsoft/typescript-go)        | `npm i -g @typescript/native-preview`       |
| [react-doctor](https://github.com/millionco/react-doctor) | `npm i -g react-doctor`                     |

未インストールのツールは静かにスキップされる。

## インストール

### Claude Code Plugin（推奨）

バイナリのインストールと hook の登録を自動で行います:

```bash
claude plugins marketplace add github:thkt/reviews
claude plugins install reviews
```

バイナリが未インストールの場合、同梱のインストーラを実行:

```bash
~/.claude/plugins/cache/reviews/reviews/*/hooks/install.sh
```

### Homebrew

```bash
brew install thkt/tap/reviews
```

### リリースバイナリ

[Releases](https://github.com/thkt/reviews/releases) から最新バイナリをダウンロード：

```bash
# macOS (Apple Silicon)
curl -L https://github.com/thkt/reviews/releases/latest/download/reviews-aarch64-apple-darwin.tar.gz | tar xz
mv reviews ~/.local/bin/
```

### ソースから

```bash
cd /tmp
git clone https://github.com/thkt/reviews.git
cd reviews
cargo build --release
cp target/release/reviews ~/.local/bin/
cd .. && rm -rf reviews
```

## 使い方

プラグインとしてインストールした場合、hook は自動で登録されます。手動で設定する場合は `~/.claude/settings.json` に追加：

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "reviews",
            "timeout": 45000
          }
        ],
        "matcher": "Skill"
      }
    ]
  }
}
```

設定されたスキルが呼ばれると（デフォルト: `/review`）、hook は以下を実行する：

1. stdin から Skill ツール入力を読み取り
2. スキル名が `skills` リストに含まれるか確認（非該当は無出力で終了）
3. プロジェクト種別を検出し、該当ツールを並列実行
4. ツール結果を `additionalContext` として JSON 出力

## ツール

| ツール                                                    | 条件                   | 引数                             |
| --------------------------------------------------------- | ---------------------- | -------------------------------- |
| [knip](https://knip.dev)                                  | `package.json` あり    | `--reporter json --no-exit-code` |
| [oxlint](https://oxc.rs)                                  | `package.json` あり    | `--format json .`                |
| [tsgo](https://github.com/microsoft/typescript-go)        | `tsconfig.json` あり   | `--noEmit`                       |
| [react-doctor](https://github.com/millionco/react-doctor) | React が依存関係に存在 | `. --verbose`                    |

ツールはまず `node_modules/.bin` から解決し、見つからなければ `$PATH` にフォールバック。

## 設定

プロジェクトルートの `.claude/tools.json` に `reviews` キーを追加。全フィールド省略可 — 上書きしたい項目のみ指定。

> **移行**: プロジェクトルートの `.claude-reviews.json` はレガシーフォールバックとして引き続きサポート。両方存在する場合は `.claude/tools.json` が優先。

**デフォルト**（設定ファイル不要）: 全ツール有効、`/review` で発動。

```json
{
  "reviews": {
    "enabled": true,
    "skills": ["review"],
    "tools": {
      "knip": true,
      "oxlint": true,
      "tsgo": true,
      "react_doctor": true
    }
  }
}
```

### 例

**`/audit` で発動させる：**

```json
{
  "reviews": {
    "skills": ["audit"]
  }
}
```

**複数スキルで発動：**

```json
{
  "reviews": {
    "skills": ["review", "audit"]
  }
}
```

**特定ツールを無効化：**

```json
{
  "reviews": {
    "tools": {
      "tsgo": false
    }
  }
}
```

**プロジェクト単位で無効化：**

```json
{
  "reviews": {
    "enabled": false
  }
}
```

### 設定ファイルの解決

設定ファイルは `$CWD` から最も近い `.git` ディレクトリまで上方向に探索される。`.claude/tools.json` に `reviews` キーがあればデフォルトとマージされる。

## 既存リンターとの併用

lefthook、husky、lint-staged でコミット時に oxlint を実行している場合、reviews のチェックと重複する可能性がある。両者は目的が異なる：

| ツール           | タイミング             | 目的                                     |
| ---------------- | ---------------------- | ---------------------------------------- |
| reviews (hook)   | 設定されたスキル実行時 | エージェントに静的解析コンテキストを提供 |
| lefthook / husky | コミット時             | コードが履歴に入る前の最終ゲート         |

重複するツールを reviews 側で無効化するには：

```json
{
  "reviews": {
    "tools": {
      "oxlint": false
    }
  }
}
```

## ライセンス

MIT
