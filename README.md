# git-vault

**English** | [简体中文](README_zh-CN.md)

---

**Zero-trace, Git-anchored out-of-band private asset manager for open source developers.**

`git-vault` keeps your private design specs (`SPEC*.md`), roadmaps (`TODO*.md`), AI agent guidelines (`AGENTS*.md`), and local secrets (`.env`) safely stored in a separate, private Git repository, while automatically aligning them with your public repository's commit history.

Installed as `git-vault`, it works seamlessly as a native Git subcommand: `git vault <command>`.

---

### 💡 Core Design Philosophy

* **Out-of-band Isolation**: Keep public repositories (MIT/AGPL) purely for code. Private specs and credentials live in your personal vault repo—preventing accidental commits, public exposure, and unwanted crawler scraping.
* **Git-Anchored Time Travel**: The public repository has zero knowledge of the vault. The vault uses the public `HEAD` commit SHA as an anchor. When you check out an older commit, `git-vault` retrieves the corresponding private assets using **first-parent ancestor lookup**.
* **Zero Local State**: No extra metadata files like `.git/vault-state.json` inside your repository. Private files are invisibly ignored via `.git/info/exclude`.
* **Explicit & Robust Contract**:
  * `push` and `pull` strictly require the public repository to be clean (`git diff --quiet` & `git diff --cached --quiet`), ensuring your assets are bound to reproducible commit states.
  * Re-pushing to the same commit SHA cleanly overwrites the slot.
  * If local private assets diverge from the target snapshot during `pull`, the operation is aborted to protect unsaved edits.

---

### 📦 Installation

```bash
cargo install git-vault
```

---

### 🚀 Quick Start

#### 1. Configure and Initialize

In your public Git repository:

```bash
# Initialize for the current project (optionally provide your central private vault remote)
git vault init git@github.com:yourname/vault.git
```

This will:
* Detect project identity from `origin` (e.g. `github.com/alice/my-project`).
* Verify that no candidate private files are currently tracked by public Git.
* Register default private patterns in `.git/info/exclude`.
* Ensure `~/.vault/repo/` is ready.

#### 2. Push Private Assets

Create or edit your private files (`SPEC.md`, `.env`, `docs/private/note.md`), commit your public code, then:

```bash
git vault push
```

#### 3. Inspect Status

```bash
git vault status
```

Example output:
```text
Project:         github.com/alice/my-project
Public Worktree: clean
HEAD:            c045fa260b2b0d471d6327898a4267708b728a6b
Target Snapshot: c045fa260b2b (exact match)
Private Assets:  aligned (2 file(s))
  • SPEC.md
  • .env
```

#### 4. Dehydrate (Clean for Open-Source Demos)

Before recording a screencast or sharing your screen, purge all private assets from the workspace with a single command:

```bash
git vault clean
```

#### 5. Rehydrate on Any Machine

Clone your public repository on a new machine and pull your private assets in one step:

```bash
git clone https://github.com/alice/my-project.git
cd my-project
git vault pull
```

---

### 🔍 Default Tracking Patterns

`git-vault` automatically manages the following private file patterns:

```text
SPEC*.md          # Design specs and algorithm documentation
TODO*.md          # Internal roadmaps and task lists
AGENTS*.md        # AI agent instructions and collaboration guidelines
.env              # Local environment variables and secrets
*.local.*         # Local override configuration files
docs/private/**   # Dedicated private documentation directory
```

> **Note**: Public templates like `.env.example`, `.env.sample`, and `.env.template` are explicitly untouched and safe for public commits.

---

### 📂 Central Vault Storage Topology

All assets are organized in your private vault repository (`~/.vault/repo/`) under a clean hierarchical structure:

```text
~/.vault/repo/ (Private Git Repository)
└── projects/
    └── <host>/<username>/<repo>/         # e.g., projects/github.com/alice/my-project/
        └── snapshots/
            ├── <commit_sha_1>/
            │   ├── .vault-manifest.json
            │   ├── .env
            │   ├── SPEC.md
            │   └── docs/private/
            │       └── note.md
            └── <commit_sha_2>/
                └── .vault-manifest.json  # {"files": []} (empty snapshot)
```

---

### ⚖️ Comparison with Existing Solutions

| Solution | Mechanism | Why Not? |
| :--- | :--- | :--- |
| **`git-crypt` / `transcrypt`** | Encrypts files with GPG, commits ciphertext into public repo. | 1. Public history retains encrypted blobs and size.<br>2. If key leaks later, entire history is exposed.<br>3. Cannot achieve true "zero trace". |
| **`git submodule` / `subtree`** | Mounts a private repo inside the project. | 1. Leaves `.gitmodules` in repo root.<br>2. Cumbersome branching and detached HEAD issues. |
| **`git-notes`** | Attaches metadata to Git objects out-of-band. | 1. Intended for small text notes, not multi-file directory trees.<br>2. `refs/notes` do not sync automatically by default. |
| **`dotenv-vault` / `Doppler`** | Commercial cloud key-value vaults. | 1. KV-only, no support for markdown specs, TODOs, or design docs.<br>2. Requires external SaaS accounts and network APIs. |
| **`git-vault`** | **Out-of-band Git backend + SHA anchor alignment + Invisible local injection**. | **Zero public footprint, multi-level file trees, automatic time-travel, zero external dependencies.** |

---

### 📄 License

MIT License.
