[English](README.md) | **日本語**

# reviews

設定されたスキル（デフォルト: `/review`）の実行前に静的解析ツールを走らせ、結果をエージェントにコンテキストとして渡す[Claude Code hook](https://docs.anthropic.com/en/docs/claude-code/hooks)です。エージェントが手動でコードを読む代わりに、リンター出力・型エラーを事前に取得できます。

## 仕組み

```text
/review → PreToolUse hook 発火 → reviews バイナリ実行
  ├─ プロジェクト種別を検出（package.json, tsconfig.json, React）
  ├─ 該当ツールを OS スレッドで並列実行
  └─ ツール出力を additionalContext として JSON 返却
        → 監査エージェントが実際の静的解析結果を参照
```

hookは**アドバイザリー専用**で、常にツール呼び出しを承認しスキルをブロックしません。ツールの失敗や未インストールは静かにスキップされます。

## 特徴

| 機能                 | 説明                                                                       |
| -------------------- | -------------------------------------------------------------------------- |
| 並列実行             | 有効な全ツールをOSスレッドで同時実行                                       |
| フェイルオープン設計 | エラーがスキルをブロックしない                                             |
| 自動検出             | プロジェクトに該当するツールのみ実行（package.json, tsconfig.json, React） |
| バイナリ解決         | `node_modules/.bin`から`.git`境界まで探索                                  |

## 必要なツール

使いたいツールをインストールしてください。

| ツール                                                    | インストール                                |
| --------------------------------------------------------- | ------------------------------------------- |
| [oxlint](https://oxc.rs)                                  | `npm i -g oxlint`                           |
| [knip](https://knip.dev)                                  | `npm i -D knip`（プロジェクトローカル推奨） |
| [tsgo](https://github.com/microsoft/typescript-go)        | `npm i -g @typescript/native-preview`       |
| [react-doctor](https://github.com/millionco/react-doctor) | `npm i -g react-doctor`                     |

未インストールのツールは静かにスキップされます。

## インストール

### Claude Code Plugin（推奨）

バイナリのインストールとhookの登録が自動で行われます。

```bash
claude plugins marketplace add thkt/sentinels
claude plugins install reviews
```

バイナリが未インストールの場合、同梱のインストーラを実行してください。

```bash
~/.claude/plugins/cache/reviews/reviews/*/hooks/install.sh
```

### Homebrew

```bash
brew install thkt/tap/reviews
```

### リリースバイナリ

[Releases](https://github.com/thkt/reviews/releases)から最新バイナリをダウンロードしてください。

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

プラグインとしてインストールした場合、hookは自動で登録されます。手動で設定する場合は `~/.claude/settings.json` に追加してください。

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

設定されたスキルが呼ばれると（デフォルト: `/review`）、hookは以下を実行します。

1. stdinからSkillツール入力を読み取り
2. スキル名が `skills` リストに含まれるか確認（非該当は無出力で終了）
3. プロジェクト種別を検出し、該当ツールを並列実行
4. ツール結果を `additionalContext` としてJSON出力

## ツール

| ツール                                                    | 条件                   | 引数                                          |
| --------------------------------------------------------- | ---------------------- | --------------------------------------------- |
| [knip](https://knip.dev)                                  | `package.json` あり    | `--reporter json --no-exit-code`              |
| [oxlint](https://oxc.rs)                                  | `package.json` あり    | `--format json --ignore-pattern node_modules` |
| [tsgo](https://github.com/microsoft/typescript-go)        | `tsconfig.json` あり   | `--noEmit`                                    |
| [react-doctor](https://github.com/millionco/react-doctor) | React が依存関係に存在 | `. --verbose`                                 |

ツールはまず `node_modules/.bin` から解決し、見つからなければ `$PATH` にフォールバックします。

## 設定

プロジェクトルートの `.claude/tools.json` に `reviews` キーを追加します。すべてのフィールドはオプションで、オーバーライドしたいもののみ指定してください。

> **移行**: プロジェクトルートの `.claude-reviews.json` もレガシーフォールバックとしてサポートされています。両方存在する場合、`.claude/tools.json` が優先されます。

設定ファイルがない場合のデフォルト構成です。すべてのツールが有効で、`/review` で発動します。

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

`/audit`で発動させる設定です。

```json
{
  "reviews": {
    "skills": ["audit"]
  }
}
```

複数スキルで発動する設定です。

```json
{
  "reviews": {
    "skills": ["review", "audit"]
  }
}
```

特定ツールを無効化する設定です。

```json
{
  "reviews": {
    "tools": {
      "tsgo": false
    }
  }
}
```

プロジェクト単位で無効化できます。

```json
{
  "reviews": {
    "enabled": false
  }
}
```

### 設定ファイルの解決

設定ファイルは `$CWD` からもっとも近い `.git` ディレクトリまで上方向に探索されます。`.claude/tools.json` に `reviews` キーがあればデフォルトとマージされます。

## 既存リンターとの併用

lefthook、husky、lint-stagedでコミット時にoxlintを実行している場合、reviewsのチェックと重複する可能性がありますが、両者の目的は異なります。

| ツール           | タイミング             | 目的                                     |
| ---------------- | ---------------------- | ---------------------------------------- |
| reviews (hook)   | 設定されたスキル実行時 | エージェントに静的解析コンテキストを提供 |
| lefthook / husky | コミット時             | コードが履歴に入る前の最終ゲート         |

重複するツールをreviews側で無効化する場合の設定です。

```json
{
  "reviews": {
    "tools": {
      "oxlint": false
    }
  }
}
```

## 関連ツール

| ツール                                           | Hook        | タイミング              | 役割                          |
| ------------------------------------------------ | ----------- | ----------------------- | ----------------------------- |
| [guardrails](https://github.com/thkt/guardrails) | PreToolUse  | Write/Edit 前           | リント + セキュリティチェック |
| [formatter](https://github.com/thkt/formatter)   | PostToolUse | Write/Edit 後           | 自動コード整形                |
| **reviews**                                      | PreToolUse  | レビュー系 Skill 実行時 | 静的解析コンテキスト提供      |
| [gates](https://github.com/thkt/gates)           | Stop        | エージェント完了時      | 品質ゲート (knip/tsgo/madge)  |

## ライセンス

MIT
