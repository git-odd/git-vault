# git-vault

[English](README.md) | **简体中文**

---

**面向开源独立开发者的无痕、Git SHA 锚定带外私有资产同步 CLI。**

`git-vault` 将你的设计规格（`SPEC*.md`）、待办规划（`TODO*.md`）、AI 协作提示词（`AGENTS*.md`）以及本地私钥（`.env`）以明文形式独立保存在私有 Git 仓库中，并通过公开代码的 Commit SHA 自动实现版本时空对齐。

安装后可直接作为原生 Git 子命令调用：`git vault <command>`。

---

### 💡 核心设计哲学

* **带外物理级隔离（Out-of-band Isolation）**：公开仓库（MIT/AGPL 等）只存放纯净开源代码与测试用例。设计文档、TODO 与密钥存放在独立金库，物理级杜绝误提交、公开泄露与爬虫抓取。
* **时空版本对齐（Git-Anchored Time Travel）**：公开代码库完全不知晓金库的存在。金库以公开 Commit SHA 为锚点，通过**第一父链祖先回溯算法**实现“代码切到旧版本，私有资产自动时空倒流”。
* **零本地额外状态（Zero Local State）**：公开仓库内部不留 `.git/vault-state.json` 等多余元数据文件，通过 `.git/info/exclude` 实现隐形忽略。
* **强硬命令合同（Explicit Intent）**：
  * `push` / `pull` 强制要求公开仓库处于干净提交状态（`git diff --quiet` 与 `git diff --cached --quiet`），杜绝半提交与锚点错位。
  * 对同一个 Commit SHA 重复 `push` 直接覆盖更新槽位。
  * `pull` 发现本地私有资产与目标快照存在未保存差异时，坚决拦截报错，杜绝意外覆盖。

---

### 📦 安装方式

```bash
cargo install git-vault
```

---

### 🚀 快速上手

#### 1. 配置并初始化

在公开代码仓库根目录下执行：

```bash
# 初始化当前项目（可指定私有中心金库的远端 Git 地址）
git vault init git@github.com:yourname/vault.git
```

命令将自动：
* 从 `origin` 解析出项目唯一标识（如 `github.com/alice/my-project`）；
* 检查候选私有文件是否已被公开 Git 跟踪（若有则报警拦截）；
* 将默认 Pattern 写入当前仓库的 `.git/info/exclude`；
* 准备本地金库缓存 `~/.vault/repo/`。

#### 2. 推送私有资产快照

编辑或创建私有文件（`SPEC.md`, `.env`, `docs/private/note.md`），提交公开代码后执行：

```bash
git vault push
```

#### 3. 查看状态

```bash
git vault status
```

输出示例：
```text
Project:         github.com/alice/my-project
Public Worktree: clean
HEAD:            c045fa260b2b0d471d6327898a4267708b728a6b
Target Snapshot: c045fa260b2b (exact match)
Private Assets:  aligned (2 file(s))
  • SPEC.md
  • .env
```

#### 4. 一键脱水（开源演示 / 录屏模式）

录制视频或公开演示前，一键彻底清理工作区中的所有私有文件：

```bash
git vault clean
```

#### 5. 在新设备上瞬间复水

新设备 `git clone` 代码后，一键取回对齐的私有资产：

```bash
git clone https://github.com/alice/my-project.git
cd my-project
git vault pull
```

---

### 🔍 默认追踪规则

`git-vault` 默认自动捕获并隐形保护以下私有文件：

```text
SPEC*.md          # 设计规格与算法文档
TODO*.md          # 内部规划与待办备忘
AGENTS*.md        # AI 协作指令与提示词规范
.env              # 本地私有环境变量与密钥
*.local.*         # 本地私有覆盖配置文件
docs/private/**   # 专属私有文档目录
```

> **注**：`.env.example`、`.env.sample`、`.env.template` 等示例模板天然放行，无需复杂过滤规则。

---

### 📂 中心金库存储拓扑

所有私有资产均保存在本地 `~/.vault/repo/`（并自动同步到私有远端 Git 库）：

```text
~/.vault/repo/ (私有 Git 仓库)
└── projects/
    └── <host>/<username>/<repo>/         # 例: projects/github.com/alice/my-project/
        └── snapshots/
            ├── <commit_sha_1>/
            │   ├── .vault-manifest.json
            │   ├── .env
            │   ├── SPEC.md
            │   └── docs/private/
            │       └── note.md
            └── <commit_sha_2>/
                └── .vault-manifest.json  # {"files": []} (空快照，作为回溯停止标记)
```

---

### ⚖️ 现有方案对比 (Why Not Analysis)

| 现有方案 | 运作机制 | 致命缺陷（为什么不用它） |
| :--- | :--- | :--- |
| **`git-crypt` / `transcrypt`** | 用 GPG 加密敏感文件，**将密文 Commit 进公开代码库**。 | 1. 公开仓库仍留有文件存在痕迹与体积；<br>2. 密钥未来一旦泄露，整个历史全盘暴露；<br>3. 无法做到“物理级不存在”。 |
| **`git submodule` / `subtree`** | 将私有仓库作为子模块挂载。 | 1. 根目录必须保留 `.gitmodules`，明晃晃暴露私有库 URL；<br>2. 跨分支合并极度繁琐、容易 detached HEAD。 |
| **`git-notes`** | 在 Git 对象库带外附加元数据。 | 1. 面向附着在 Commit 上的零散元数据，不适合作为多文件目录树的工作区投影机制；<br>2. `refs/notes` 默认不随常规 fetch/push 自动同步。 |
| **`dotenv-vault` / `Doppler`** | 商业云端密钥服务。 | 1. 仅支持 Key-Value，**完全不支持 Markdown 设计文档、TODO、算法图纸**；<br>2. 依赖第三方商业账号和网络 API。 |
| **`git-vault`（本项目）** | **完全带外存储 + Git SHA 锚点对齐 + 本地隐形注入**。 | **公开仓库 0 痕迹、支持全量多级文件树、支持时空回溯、单一私有库全项目通用、单二进制 0 依赖。** |

---

### 📄 开源许可 (License)

MIT License.
