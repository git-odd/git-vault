# git-vault

[English](README.md) | **简体中文**

---

**面向开源独立开发者的无痕、Git SHA 锚定带外私有资产同步与 Hook 自动化 CLI。**

`git-vault` 将你的设计规格（`SPEC*.md`）、待办规划（`TODO*.md`）、AI 协作提示词（`AGENTS*.md`）以及本地私钥（`.env`）以明文形式独立保存在私有 Git 仓库中，并通过公开代码的 Commit SHA 自动实现版本时空对齐。

支持项目级 `.vaultignore` 自定义规则与 Git Hooks 原生联动：**一次 `git vault init`，日常只用原生 `git push` / `git checkout` 即可无感同步与对齐！**

安装后可直接作为原生 Git 子命令调用：`git vault <command>`。

---

### 💡 核心设计哲学

* **带外物理级隔离（Out-of-band Isolation）**：公开仓库（MIT/AGPL 等）只存放纯净开源代码与测试用例。设计文档、TODO 与密钥存放在独立金库，物理级杜绝误提交、公开泄露与爬虫抓取。
* **时空版本对齐（Git-Anchored Time Travel）**：公开代码库完全不知晓金库的存在。金库以公开 Commit SHA 为锚点，通过**第一父链祖先回溯算法**实现“代码切到旧版本，私有资产自动时空倒流”。
* **自动化无感闭环（Zero-Friction & Hook Automation）**：
  * 初始化时自动装配 Git 原生钩子（`pre-push`, `post-checkout`, `post-merge`）；
  * 执行 `git push` 自动触发私有金库快照推送；
  * 执行 `git checkout` 切换分支自动拉取快照（若本地有未保存的私有冲突，自动回滚拦截，坚决阻止带脏切分支）。
* **灵活规则与零本地额外状态（.vaultignore & Managed Block）**：
  * 项目根目录 `.vaultignore` 支持标准通配符与 `!` 白名单规则；
  * 规则自动以受控标记块（Managed Block）幂等同步至 `.git/info/exclude`，零污染公开历史。
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

#### 1. 配置并初始化（一键装配）

在公开代码仓库根目录下执行：

```bash
# 初始化当前项目（可指定私有中心金库的远端 Git 地址）
git vault init git@github.com:yourname/vault.git
```

命令将自动：
* 从 `origin` 解析出项目唯一标识（如 `github.com/alice/my-project`）；
* 在项目根目录生成 `.vaultignore` 规则文件（若不存在）；
* 检查候选私有文件是否已被公开 Git 跟踪（若有则报警拦截）；
* 将规则以受控块方式同步至 `.git/info/exclude`；
* 自动在 `.git/hooks/` 安装 `pre-push`、`post-checkout` 和 `post-merge` 钩子；
* 准备本地金库缓存 `~/.vault/repo/`。

#### 2. 无感开发流（推荐）

初始化完成后，你**无需手动执行任何 `git vault` 命令**：
1. 像平常一样编写公开代码与私有资产（如 `SPEC.md`, `.env`）；
2. 提交公开代码：`git commit -m "feat: add feature"`；
3. 推送公开代码：`git push origin main`（`pre-push` 钩子会自动将私有资产快照推到私有金库）；
4. 切换分支：`git checkout dev`（`post-checkout` 钩子会自动从金库拉取对应版本的私有资产）。

#### 3. 手动模式与常用命令

若需手动维护或排查，可直接运行子命令：

```bash
# 手动将当前私有资产快照推送至金库
git vault push

# 查看当前工作区、快照锚点与资产对齐状态
git vault status

# 一键脱水（录屏/公开演示前物理清除所有私有资产）
git vault clean

# 重新拉取并复水当前分支对应的私有资产
git vault pull

# 查看私有资产行级差异（自动调用 delta 语法高亮）
git vault diff
git vault diff -s             # 强制开启双栏对比
git vault diff -P             # 免分页直接输出至终端（无需按回车，直接滚轮翻看）
git vault diff -s -P          # 双栏且免分页直接输出
git vault diff --stat         # 仅查看文件变更与行数统计
git vault diff HEAD~1         # 与指定历史提交快照对比
git vault diff SPEC.md        # 仅对比指定文件或路径

# 手动管理 Git 钩子
git vault hook install
git vault hook uninstall
```

---

### 🔍 规则配置 (`.vaultignore`)

项目根目录的 `.vaultignore` 采用类似 `.gitignore` 的语法标准：

```text
# 私有资产规则
SPEC*.md          # 设计规格与算法文档
TODO*.md          # 内部规划与待办备忘
AGENTS*.md        # AI 协作指令与提示词规范
.env              # 本地私有环境变量与密钥
*.local.*         # 本地私有覆盖配置文件
docs/private/**   # 专属私有文档目录

# 支持 ! 前缀白名单反向放行公开文件
!SPEC_public.md
```

> **注**：`.vaultignore` 本身也会随每次快照一同备份到私有金库中，在全新机器上脱水复水时可 100% 完整自愈。

---

### 📄 开源许可证

本项目基于 [MIT License](LICENSE) 协议开源。
