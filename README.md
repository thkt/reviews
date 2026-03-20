**English** | [日本語](README.ja.md)

# reviews

A [Claude Code hook](https://docs.anthropic.com/en/docs/claude-code/hooks) that runs static analysis tools before configured skills (default: `/review`) and feeds the results to the agent as context. Instead of the agent scanning code manually, it gets real linter output and type errors upfront.

## How it works

```text
/review → PreToolUse hook fires → reviews binary runs
  ├─ Detects project type (package.json, tsconfig.json, React)
  ├─ Runs applicable tools in parallel (OS threads)
  └─ Returns JSON with tool output as additionalContext
        → Audit agent sees real static analysis results
```

The hook is **advisory-only**: it always approves the tool call and never blocks the skill. Tool failures or missing tools are silently skipped.

## Features

- **Parallel execution**: All enabled tools run simultaneously via OS threads
- **Fail-open design**: Errors never block the parent skill command
- **Auto-detection**: Only runs tools relevant to the project (package.json, tsconfig.json, React)
- **Binary resolution**: Finds tools in `node_modules/.bin` with `.git` boundary

## Requirements

Install the tools you want to use:

| Tool                                                      | Install                                     |
| --------------------------------------------------------- | ------------------------------------------- |
| [oxlint](https://oxc.rs)                                  | `npm i -g oxlint`                           |
| [knip](https://knip.dev)                                  | `npm i -D knip` (project-local recommended) |
| [tsgo](https://github.com/microsoft/typescript-go)        | `npm i -g @typescript/native-preview`       |
| [react-doctor](https://github.com/millionco/react-doctor) | `npm i -g react-doctor`                     |

If a tool is not installed, it is silently skipped.

## Installation

### Claude Code Plugin (Recommended)

Installs the binary and registers the hook automatically:

```bash
claude plugins marketplace add thkt/sentinels
claude plugins install reviews
```

If the binary is not yet installed, run the bundled installer:

```bash
~/.claude/plugins/cache/reviews/reviews/*/hooks/install.sh
```

### Homebrew

```bash
brew install thkt/tap/reviews
```

### From Release

Download the latest binary from [Releases](https://github.com/thkt/reviews/releases):

```bash
# macOS (Apple Silicon)
curl -L https://github.com/thkt/reviews/releases/latest/download/reviews-aarch64-apple-darwin.tar.gz | tar xz
mv reviews ~/.local/bin/
```

### From Source

```bash
cd /tmp
git clone https://github.com/thkt/reviews.git
cd reviews
cargo build --release
cp target/release/reviews ~/.local/bin/
cd .. && rm -rf reviews
```

## Usage

When installed as a plugin, hooks are registered automatically. For manual setup, add to `~/.claude/settings.json`:

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

When a configured skill is invoked (default: `/review`), the hook:

1. Reads the Skill tool input from stdin
2. Checks if the skill name matches the `skills` list (exits silently for non-matching skills)
3. Detects project type and runs applicable tools in parallel
4. Outputs JSON with `additionalContext` containing tool results

## Tools

| Tool                                                      | Condition              | Arguments                                     |
| --------------------------------------------------------- | ---------------------- | --------------------------------------------- |
| [knip](https://knip.dev)                                  | `package.json` exists  | `--reporter json --no-exit-code`              |
| [oxlint](https://oxc.rs)                                  | `package.json` exists  | `--format json --ignore-pattern node_modules` |
| [tsgo](https://github.com/microsoft/typescript-go)        | `tsconfig.json` exists | `--noEmit`                                    |
| [react-doctor](https://github.com/millionco/react-doctor) | React in dependencies  | `. --verbose`                                 |

Tools are resolved from `node_modules/.bin` first, falling back to `$PATH`.

## Configuration

Add a `reviews` key to `.claude/tools.json` at your project root. All fields are optional — only specify what you want to override.

> **Migration**: `.claude-reviews.json` at the project root is still supported as a legacy fallback. If both exist, `.claude/tools.json` takes priority.

**Defaults** (no config file needed): all tools enabled, activates on `/review`.

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

### Examples

**Activate on `/audit` instead of `/review`:**

```json
{
  "reviews": {
    "skills": ["audit"]
  }
}
```

**Activate on multiple skills:**

```json
{
  "reviews": {
    "skills": ["review", "audit"]
  }
}
```

**Disable a specific tool:**

```json
{
  "reviews": {
    "tools": {
      "tsgo": false
    }
  }
}
```

**Disable reviews for a project:**

```json
{
  "reviews": {
    "enabled": false
  }
}
```

### Config Resolution

The config file is found by walking up from `$CWD` to the nearest `.git` directory. If `.claude/tools.json` exists there and contains a `reviews` key, it is loaded and merged with defaults.

## Using with Existing Linters

If you already run oxlint via lefthook, husky, or lint-staged on commit, reviews' checks may overlap. The two serve different purposes:

| Tool             | When                | Purpose                                      |
| ---------------- | ------------------- | -------------------------------------------- |
| reviews (hook)   | On configured skill | Provide static analysis context to the agent |
| lefthook / husky | On commit           | Final gate before code enters history        |

To disable overlapping tools in reviews and rely on your commit hook instead:

```json
{
  "reviews": {
    "tools": {
      "oxlint": false
    }
  }
}
```

## Companion Tools

This tool is part of a 4-tool quality pipeline for Claude Code. Each covers a
different phase — install the full suite for comprehensive coverage:

```bash
brew install thkt/tap/guardrails thkt/tap/formatter thkt/tap/reviews thkt/tap/gates
```

| Tool                                             | Hook        | Timing            | Role                              |
| ------------------------------------------------ | ----------- | ----------------- | --------------------------------- |
| [guardrails](https://github.com/thkt/guardrails) | PreToolUse  | Before Write/Edit | Lint + security checks            |
| [formatter](https://github.com/thkt/formatter)   | PostToolUse | After Write/Edit  | Auto code formatting              |
| **reviews**                                      | PreToolUse  | Before Skill      | Static analysis context injection |
| [gates](https://github.com/thkt/gates)           | Stop        | Agent completion  | Quality gates (knip, tsgo, madge) |

See [thkt/tap](https://github.com/thkt/homebrew-tap) for setup details.

## License

MIT
