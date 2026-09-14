# Vortex — Release Makefile
#
# 常用命令:
#   make release                    # 自动 bump patch 并发布（最常用）
#   make release '修复xxx问题'       # 同上，说明文字作为本次提交消息
#   make release VERSION=0.2.0      # 发布指定版本
#   make minor                      # bump minor 并发布
#   make major                      # bump major 并发布
#   make current-version            # 查看当前版本与最新标签
#   make sync-version VERSION=0.1.4 # 仅同步版本号到所有配置文件（不提交不推送）
#   make check                      # 前端 + Rust 构建检查
#   make dry-run                    # 预览发布步骤（不执行）
#   make clean                      # 清理构建产物
#
# 环境变量:
#   SKIP_CHECK=1                    # 跳过构建检查（快速发布）
#   NO_PUSH=1                       # 不推送到远程（仅本地提交+标签）

CURRENT_VERSION := $(shell node -p "require('./package.json').version")
NEXT_PATCH     := $(shell node -p "const v=require('./package.json').version.split('.');v[2]++;v.join('.')")
RELEASE_VERSION := $(if $(VERSION),$(VERSION),$(NEXT_PATCH))
GIT_REMOTE     := origin
GIT_BRANCH     := $(shell git branch --show-current)
REPO_URL       := https://github.com/ht-shaipe/vortex

# ── 提交消息（位置参数）──────────────────────────────────
# 用法: make release '修复xxx问题' / make minor '新功能说明'
# release/patch/minor/major 后面的文字会作为"提交未提交改动"的 commit message；
# 未提供时使用默认消息。注意: 文字会被 make 当作目标，若与已有目标重名会被优先执行。
MSG_WORDS  := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
COMMIT_MSG := $(if $(RELEASE_MSG),$(RELEASE_MSG),$(strip $(MSG_WORDS)))
SYNC_MSG   := $(if $(COMMIT_MSG),$(COMMIT_MSG),chore: sync changes before v$(RELEASE_VERSION))

# 让位置参数不触发 "No rule to make target" 错误
ifneq ($(words $(MSG_WORDS)),0)
$(MSG_WORDS):
	@:
endif

# 颜色
C_RESET  := \033[0m
C_GREEN  := \033[32m
C_YELLOW := \033[33m
C_RED    := \033[31m
C_CYAN   := \033[36m
C_BOLD   := \033[1m

.PHONY: help current-version sync-version check release minor major dry-run clean

# ── 默认目标 ─────────────────────────────────────────────

help:
	@echo "$(C_BOLD)Vortex Release Makefile$(C_RESET)"
	@echo ""
	@echo "$(C_CYAN)常用命令:$(C_RESET)"
	@echo "  make release                    自动 bump patch 并发布（提交所有改动→bump→推送）"
	@echo "  make release '修复xxx问题'       同上，说明文字作为本次提交消息"
	@echo "  make release VERSION=0.2.0      发布指定版本"
	@echo "  make minor                      bump minor 并发布"
	@echo "  make major                      bump major 并发布"
	@echo "  make current-version            查看当前版本与最新标签"
	@echo "  make sync-version VERSION=0.1.4  仅同步版本号（不提交不推送）"
	@echo "  make check                      构建检查"
	@echo "  make dry-run                    预览发布步骤"
	@echo "  make clean                      清理构建产物"
	@echo ""
	@echo "$(C_CYAN)环境变量:$(C_RESET)"
	@echo "  SKIP_CHECK=1                     跳过构建检查"
	@echo "  NO_PUSH=1                        不推送到远程"

current-version:
	@echo "$(C_GREEN)当前版本:$(C_RESET)   $(CURRENT_VERSION)"
	@echo "$(C_GREEN)下一版本:$(C_RESET)   $(NEXT_PATCH)"
	@echo "$(C_GREEN)最新标签:$(C_RESET)   $$(git describe --tags --abbrev=0 2>/dev/null || echo '无')"
	@echo "$(C_GREEN)分支:$(C_RESET)       $(GIT_BRANCH)"
	@echo "$(C_GREEN)远程:$(C_RESET)       $(REPO_URL)"

# ── 版本号同步 ───────────────────────────────────────────
# 更新 4 个文件: package.json, Cargo.toml, tauri.conf.json, docs/index.html

sync-version:
	@test -n "$(VERSION)" || (echo "$(C_RED)错误: 请指定版本号。用法: make sync-version VERSION=0.1.4$(C_RESET)" && exit 1)
	@test "$(VERSION)" != "$(CURRENT_VERSION)" || (echo "$(C_YELLOW)版本号未变更（当前已是 $(CURRENT_VERSION)）$(C_RESET)" && exit 0)
	@echo "$(C_CYAN)同步版本号: $(CURRENT_VERSION) → $(VERSION)$(C_RESET)"
	@perl -i -pe 's/"version": "\Q$(CURRENT_VERSION)\E"/"version": "$(VERSION)"/' package.json
	@perl -i -pe 's/^version = "\Q$(CURRENT_VERSION)\E"/version = "$(VERSION)"/' src-tauri/Cargo.toml
	@perl -i -pe 's/"version": "\Q$(CURRENT_VERSION)\E"/"version": "$(VERSION)"/' src-tauri/tauri.conf.json
	@perl -i -pe 's/v\Q$(CURRENT_VERSION)\E/v$(VERSION)/' docs/index.html
	@echo "$(C_GREEN)✓ 已更新:$(C_RESET)"
	@echo "    package.json"
	@echo "    src-tauri/Cargo.toml"
	@echo "    src-tauri/tauri.conf.json"
	@echo "    docs/index.html"

# ── 构建检查 ─────────────────────────────────────────────

check:
	@echo "$(C_CYAN)▶ 前端构建检查...$(C_RESET)"
	@npm run build
	@echo "$(C_CYAN)▶ Rust 构建检查...$(C_RESET)"
	@cd src-tauri && cargo check
	@echo "$(C_GREEN)✓ 构建检查通过$(C_RESET)"

# ── 发布流程 ─────────────────────────────────────────────
# make release              → 自动 bump patch（0.1.4 → 0.1.5）
# make release VERSION=x.y.z → 发布指定版本
# 流程: 校验 → (可选)构建检查 → 提交未提交改动 → 同步版本号 → 更新 Cargo.lock → 提交 → 标签 → 推送

release:
	@test "$(RELEASE_VERSION)" != "$(CURRENT_VERSION)" || (echo "$(C_RED)错误: 版本号未变更（当前已是 $(CURRENT_VERSION)）$(C_RESET)" && exit 1)
	@! git tag -l "v$(RELEASE_VERSION)" | grep -q . || (echo "$(C_RED)错误: 标签 v$(RELEASE_VERSION) 已存在$(C_RESET)" && exit 1)
	@echo "$(C_BOLD)═══ 发布 v$(RELEASE_VERSION) ═══$(C_RESET)"
	@echo "  $(C_GREEN)当前版本:$(C_RESET) $(CURRENT_VERSION)"
	@echo "  $(C_GREEN)目标版本:$(C_RESET) $(RELEASE_VERSION)"
	@echo "  $(C_GREEN)分支:$(C_RESET)     $(GIT_BRANCH)"
	@echo ""
	@printf "确认发布？[y/N] " && read ans && test "$$ans" = "y" || (echo "$(C_YELLOW)已取消$(C_RESET)" && exit 1)
	@if [ "$(SKIP_CHECK)" != "1" ]; then $(MAKE) check; fi
	@if ! git diff --quiet || ! git diff --cached --quiet; then \
		echo "$(C_CYAN)▶ 提交未提交的改动（消息: $(SYNC_MSG)）...$(C_RESET)"; \
		git add -A; \
		git commit -m "$(SYNC_MSG)"; \
		echo "$(C_GREEN)✓ 已提交未暂存改动$(C_RESET)"; \
	fi
	@$(MAKE) sync-version VERSION=$(RELEASE_VERSION)
	@echo "$(C_CYAN)▶ 更新 Cargo.lock...$(C_RESET)"
	@cd src-tauri && cargo check 2>/dev/null
	@echo "$(C_CYAN)▶ 提交版本变更...$(C_RESET)"
	@git add package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json docs/index.html
	@git commit -m "v$(RELEASE_VERSION): bump version"
	@git tag "v$(RELEASE_VERSION)"
	@echo "$(C_GREEN)✓ 已提交并打标签 v$(RELEASE_VERSION)$(C_RESET)"
	@if [ "$(NO_PUSH)" != "1" ]; then \
		echo "$(C_CYAN)▶ 推送到远程（触发 CI 构建）...$(C_RESET)"; \
		git push $(GIT_REMOTE) $(GIT_BRANCH); \
		git push $(GIT_REMOTE) "v$(RELEASE_VERSION)"; \
		echo "$(C_GREEN)✓ 已推送$(C_RESET)"; \
		echo ""; \
		echo "$(C_BOLD)CI 构建进度:$(C_RESET) $(REPO_URL)/actions"; \
		echo "$(C_BOLD)Release 页面:$(C_RESET) $(REPO_URL)/releases/tag/v$(RELEASE_VERSION)"; \
	else \
		echo "$(C_YELLOW)⚠ NO_PUSH=1，未推送到远程$(C_RESET)"; \
		echo "  手动推送: git push $(GIT_REMOTE) $(GIT_BRANCH) && git push $(GIT_REMOTE) v$(RELEASE_VERSION)"; \
	fi

# ── 快捷命令 ─────────────────────────────────────────────

minor:
	@$(MAKE) release VERSION=$(shell node -p "const v=require('./package.json').version.split('.');v[1]++;v[2]=0;v.join('.')") $(if $(COMMIT_MSG),RELEASE_MSG="$(COMMIT_MSG)")

major:
	@$(MAKE) release VERSION=$(shell node -p "const v=require('./package.json').version.split('.');v[0]++;v[1]=0;v[2]=0;v.join('.')") $(if $(COMMIT_MSG),RELEASE_MSG="$(COMMIT_MSG)")

# ── 辅助命令 ─────────────────────────────────────────────

dry-run:
	@echo "$(C_BOLD)═══ Dry Run: v$(RELEASE_VERSION) ═══$(C_RESET)"
	@echo "  $(C_GREEN)当前版本:$(C_RESET)    $(CURRENT_VERSION)"
	@echo "  $(C_GREEN)目标版本:$(C_RESET)    $(RELEASE_VERSION)"
	@echo "  $(C_GREEN)分支:$(C_RESET)        $(GIT_BRANCH)"
	@echo ""
	@echo "$(C_CYAN)将更新文件:$(C_RESET)"
	@echo "    package.json              \"version\": \"$(RELEASE_VERSION)\""
	@echo "    src-tauri/Cargo.toml      version = \"$(RELEASE_VERSION)\""
	@echo "    src-tauri/tauri.conf.json \"version\": \"$(RELEASE_VERSION)\""
	@echo "    docs/index.html           v$(RELEASE_VERSION) · MIT 开源"
	@echo ""
	@echo "$(C_CYAN)将执行:$(C_RESET)"
	@echo "    1. 构建检查 (vite build + cargo check)"
	@echo "    2. 提交未提交的改动 (git add -A && git commit)"
	@echo "    3. 同步版本号到 4 个文件"
	@echo "    4. 更新 Cargo.lock"
	@echo "    5. git commit -m \"v$(RELEASE_VERSION): bump version\""
	@echo "    6. git tag v$(RELEASE_VERSION)"
	@echo "    7. git push origin $(GIT_BRANCH)"
	@echo "    8. git push origin v$(RELEASE_VERSION)"
	@echo ""
	@echo "$(C_CYAN)CI 将在以下地址构建:$(C_RESET)"
	@echo "    $(REPO_URL)/actions"

clean:
	@echo "$(C_CYAN)清理构建产物...$(C_RESET)"
	@rm -rf dist
	@cd src-tauri && cargo clean
	@echo "$(C_GREEN)✓ 已清理$(C_RESET)"
