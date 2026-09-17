<template>
  <div class="mt-4px">
    <!-- 页面头部：标题与新增按钮 -->
    <PageHeader title="订阅管理" sub="管理各 AI 提供商的连接与 API 密钥">
      <template #actions>
        <button type="button" class="btn" :disabled="loading" @click="load">
          <el-icon :size="14"><Refresh /></el-icon>刷新
        </button>
        <router-link to="/subscriptions/new" class="btn">
          <el-icon :size="14"><Plus /></el-icon>添加提供方
        </router-link>
        <router-link to="/subscriptions/custom" class="btn primary">
          <el-icon :size="14"><Connection /></el-icon>添加自定义提供方
        </router-link>
      </template>
    </PageHeader>

    <div class="tabs">
      <button v-for="t in tabs" :key="t.key" class="tab" :class="{ active: tab === t.key }" @click="tab = t.key">{{ t.label }}</button>
    </div>

    <template v-if="tab === 'connections'">
    <!-- 加载中占位 -->
    <div v-if="loading" class="spin-wrap p-40px text-center text-ink-4">
      <el-icon class="spin" :size="18"><Loading /></el-icon>
    </div>

    <!-- 空态：引导用户添加第一个连接 -->
    <div v-else-if="connections.length === 0" class="card">
      <EmptyState title="还没有订阅" desc="从内置提供商列表中添加一个连接，或填写自定义端点接入 OpenAI 兼容服务">
        <div class="empty-actions flex gap-8px">
          <router-link to="/subscriptions/new" class="btn">添加提供方</router-link>
          <router-link to="/subscriptions/custom" class="btn primary">添加自定义提供方</router-link>
        </div>
      </EmptyState>
    </div>

    <!-- 订阅列表 -->
    <div v-else class="card">
      <!-- ToS 合规说明：解释提供商名称旁徽标的含义 -->
      <div class="tos-hint flex items-start gap-8px py-10px px-14px border-b border-line border-solid border-0 text-12px text-ink-3">
        <el-icon :size="13" class="shrink-0 mt-2px text-ink-4"><InfoFilled /></el-icon>
        <div class="leading-[1.6] min-w-0">
          <span class="text-ink-2 font-medium">ToS 合规：</span>提供商名称旁的徽标表示「通过本应用代理调用该提供商 API」是否符合其服务条款——
          <span class="tos-pill tos-ok mx-2px">可用</span> 允许代理使用；
          <span class="tos-pill tos-warn mx-2px">谨慎</span> 存在限制性条款；
          <span class="tos-pill tos-err mx-2px">避免</span> 明确禁止，违规可能封号。
          悬浮徽标可查看审查说明与日期。结论基于预置审查快照，提供商条款可能随时更新，请自行留意。
        </div>
      </div>

      <!-- 工具条：搜索 / 计数 / 批量测试 -->
      <div class="list-bar flex items-center gap-10px px-14px py-10px border-b border-line border-solid border-0">
        <el-input
          v-model="keyword"
          placeholder="搜索名称 / 提供方 / 模型"
          clearable
          size="small"
          class="list-search max-w-240px"
        />
        <span class="spacer flex-1" />
        <span class="list-count text-11.5px text-ink-4 shrink-0 tabular-nums">优先级数值越大优先使用，共 {{ filtered.length }} 个连接</span>
        <button type="button" class="btn sm shrink-0" :disabled="testingAll || filtered.length === 0" @click="testAll">
          <el-icon v-if="testingAll" class="spin" :size="12"><Loading /></el-icon>
          {{ testingAll ? `测试中 ${progress.done}/${progress.total}` : '全部测试' }}
        </button>
      </div>

      <table class="table">
        <colgroup>
          <col style="width: 96px" />
          <col style="width: 240px" />
          <col style="width: 220px" />
          <col style="width: 52px" />
          <col style="width: 92px" />
          <col style="width: 118px" />
        </colgroup>
        <thead>
          <tr>
            <th>状态</th>
            <th>提供商</th>
            <th>模型</th>
            <th class="num">优先级</th>
            <th class="num">更新于</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="conn in filtered" :key="conn.id" @click="$router.push(`/subscriptions/${conn.id}`)">
            <!-- 状态列：健康时直接显示响应耗时（绿色），失败/禁用/未测试给出文字状态 -->
            <td>
              <div class="status-cell flex items-center gap-6px whitespace-nowrap">
                <span v-if="isTesting(conn.id)" class="testing inline-flex items-center gap-4px text-11.5px text-ink-3">
                  <el-icon class="spin" :size="12"><Loading /></el-icon>测试中
                </span>
                <el-tooltip v-else :content="statusTip(conn)" placement="top" :show-after="200">
                  <span
                    class="lat-pill inline-flex items-center gap-5px max-w-full py-[1.5px] px-7px border border-solid border-transparent rounded-full text-11.5px tabular-nums whitespace-nowrap bg-surface-2 text-ink-3 cursor-default [&.ok]:bg-ok-bg [&.ok]:text-ok [&.err]:bg-err-bg [&.err]:text-err [&.warn]:bg-warn-bg [&.warn]:text-warn [&.neutral]:bg-surface-2 [&.neutral]:text-ink-4"
                    :class="statusTone(conn)"
                  >
                    <i class="lat-dot w-5px h-5px shrink-0 rounded-full bg-current" aria-hidden="true" />
                    <span class="lat-text overflow-hidden text-ellipsis">{{ statusLabel(conn) }}</span>
                  </span>
                </el-tooltip>
                <el-tooltip v-if="statusTone(conn) === 'err' && conn.lastError" :content="conn.lastError" placement="top">
                  <el-icon class="err-icon shrink-0 text-err cursor-help" :size="13"><WarningFilled /></el-icon>
                </el-tooltip>
              </div>
            </td>

            <!-- 提供商图标与名称 -->
            <td>
              <div class="prov-cell flex items-center gap-10px min-w-0 max-w-full">
                <ProviderLogo :name="conn.provider" :hint="`${conn.name} ${conn.baseUrl || ''}`" :color="provColor(conn.provider)" :size="24" />
                <div class="prov-meta min-w-0 max-w-180px">
                  <div class="prov-name flex items-center gap-6px text-13px font-medium text-ink whitespace-nowrap overflow-hidden text-ellipsis" :title="conn.name">
                    <span class="overflow-hidden text-ellipsis">{{ conn.name }}</span>
                    <el-tooltip v-if="tosProviderKey(conn)" :content="tosTip(tosProviderKey(conn))" placement="top">
                      <span
                        class="tos-pill inline-flex items-center shrink-0 text-10px leading-none py-2px px-5px rounded-full border border-solid border-transparent cursor-help"
                        :class="tosClass(tosProviderKey(conn))"
                      >{{ tosLabel(tosProviderKey(conn)) }}</span>
                    </el-tooltip>
                  </div>
                  <div class="prov-id mono text-11px text-ink-4 whitespace-nowrap overflow-hidden text-ellipsis" :title="conn.baseUrl || conn.provider">
                    {{ conn.baseUrl || conn.provider }}
                  </div>
                </div>
              </div>
            </td>

            <!-- 模型标签：超出 maxTags 折叠为 +N -->
            <td>
              <div v-if="conn.models && conn.models.length" class="model-tags flex flex-wrap gap-x-4px gap-y-3px items-center">
                <span
                  v-for="(m, i) in visibleModels(conn)"
                  :key="m.id"
                  class="model-tag inline-flex items-center gap-4px text-11px leading-[1.55] py-1px px-6px bg-surface-2 border border-solid border-line rounded-8px text-ink-2 whitespace-nowrap max-w-150px overflow-hidden text-ellipsis"
                  :title="modelTagTitle(m)"
                >
                  <i v-if="i === 0" class="def-dot w-5px h-5px rounded-full bg-accent shrink-0" aria-hidden="true" />
                  {{ modelTagLabel(m) }}
                </span>
                <el-tooltip
                  v-if="conn.models.length > maxTags"
                  :content="overflowModels(conn)"
                  placement="top"
                >
                  <span class="model-tag more inline-flex items-center text-11px leading-[1.55] py-1px px-6px bg-surface-2 border border-solid border-line rounded-8px text-ink-3 whitespace-nowrap cursor-default">+{{ conn.models.length - maxTags }}</span>
                </el-tooltip>
              </div>
              <!-- 无模型列表时回退展示默认模型 -->
              <div
                v-else
                class="model-cell text-12.5px text-ink-2 whitespace-nowrap overflow-hidden text-ellipsis tabular-nums [&.empty]:text-ink-4"
                :class="{ empty: !conn.defaultModel }"
              >
                <el-tooltip
                  v-if="conn.defaultModel"
                  :content="conn.defaultModel"
                  placement="top"
                >
                  <span>{{ conn.defaultModel }}</span>
                </el-tooltip>
                <span v-else>—</span>
              </div>
            </td>


            <!-- 优先级 -->
            <td class="num"><span class="pri-val text-12px tabular-nums text-ink-3">{{ conn.priority ?? 0 }}</span></td>

            <!-- 更新时间 -->
            <td class="num">{{ fmtTime(conn.updatedAt || conn.createdAt) }}</td>

            <!-- 行内操作：直接测试健康状态 + 进入详情 -->
            <td class="action-cell pl-8px pr-14px">
              <div class="action-btns flex items-center justify-end gap-6px [&_.btn.sm]:py-4px [&_.btn.sm]:px-8px [&_.btn.sm]:text-11.5px">
                <button
                  type="button"
                  class="btn sm"
                  :disabled="isTesting(conn.id) || testingAll"
                  @click.stop="testOne(conn)"
                >
                  {{ isTesting(conn.id) ? '测试中' : '测试' }}
                </button>
                <button type="button" class="btn sm" @click.stop="$router.push(`/subscriptions/${conn.id}`)">查看</button>
              </div>
            </td>
          </tr>

          <!-- 搜索无结果 -->
          <tr v-if="filtered.length === 0">
            <td colspan="6" class="text-center text-ink-4 py-24px text-12.5px">
              没有匹配「{{ keyword }}」的连接
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    </template>

    <div v-else-if="tab === 'rate-limits'" class="card">
      <div class="list-bar flex items-center gap-10px px-14px py-10px border-b border-line border-solid border-0">
        <span class="text-13px font-medium text-ink-2">速率限制配置</span>
        <span class="spacer flex-1" />
        <span class="text-11.5px text-ink-4 tabular-nums">共 {{ rateLimits.length }} 条</span>
        <button type="button" class="btn sm" :disabled="rlLoading" @click="loadRateLimits">
          <el-icon v-if="rlLoading" class="spin" :size="12"><Loading /></el-icon>刷新
        </button>
        <button type="button" class="btn sm primary" @click="openRateLimitCreate">
          <el-icon :size="12"><Plus /></el-icon>新建
        </button>
      </div>
      <table class="table">
        <colgroup>
          <col style="width: 160px" />
          <col style="width: 220px" />
          <col style="width: 80px" />
          <col style="width: 80px" />
          <col style="width: 90px" />
          <col style="width: 90px" />
          <col style="width: 80px" />
          <col style="width: 110px" />
        </colgroup>
        <thead>
          <tr>
            <th>提供商</th>
            <th>模型</th>
            <th class="num">RPM</th>
            <th class="num">RPD</th>
            <th class="num">TPM</th>
            <th class="num">TPD</th>
            <th>状态</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="cap in rateLimits" :key="cap.id">
            <td><span class="pill info mono">{{ cap.provider }}</span></td>
            <td><span class="font-mono text-12.5px text-ink-2">{{ cap.model }}</span></td>
            <td class="num">{{ cap.rpm ?? '—' }}</td>
            <td class="num">{{ cap.rpd ?? '—' }}</td>
            <td class="num">{{ cap.tpm ?? '—' }}</td>
            <td class="num">{{ cap.tpd ?? '—' }}</td>
            <td><span class="pill" :class="cap.isActive ? 'ok' : 'neutral'">{{ cap.isActive ? '启用' : '停用' }}</span></td>
            <td class="action-cell pl-8px pr-14px">
              <div class="action-btns flex items-center justify-end gap-6px">
                <button type="button" class="btn sm" @click="openRateLimitEdit(cap)">编辑</button>
                <button type="button" class="btn sm ghost" @click="removeRateLimit(cap)">删除</button>
              </div>
            </td>
          </tr>
          <tr v-if="rateLimits.length === 0">
            <td colspan="8" class="text-center text-ink-4 py-24px text-12.5px">暂无速率限制配置</td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-else-if="tab === 'catalog'" class="flex flex-col gap-12px">
      <!-- 统计概览 -->
      <div class="stats-row flex gap-12px">
        <div class="card flex-1 py-14px px-18px">
          <div class="text-11.5px text-ink-4">模型总数</div>
          <div class="text-20px font-semibold tabular-nums mt-2px">{{ catalog.length }}</div>
        </div>
        <div class="card flex-1 py-14px px-18px">
          <div class="text-11.5px text-ink-4">覆盖提供方</div>
          <div class="text-20px font-semibold tabular-nums mt-2px">{{ catGrouped.length }}</div>
        </div>
        <div class="card flex-[2] py-14px px-18px min-w-0">
          <div class="text-11.5px text-ink-4">最后同步</div>
          <div class="text-13px font-medium mt-4px truncate" :title="catLastSynced || ''">{{ catLastSynced ? fmtDateTime(catLastSynced) : '从未同步' }}</div>
        </div>
      </div>

      <!-- 加载中 -->
      <div v-if="catLoading" class="card py-40px text-center text-ink-4">
        <el-icon class="spin" :size="18"><Loading /></el-icon>
      </div>

      <!-- 空态：引导同步数据 -->
      <div v-else-if="catalog.length === 0" class="card">
        <EmptyState title="模型目录为空" desc="点击「同步」即可导入内置的官方模型参数快照（上下文窗口、限速、能力标签）；配置远程 feed 后可自动更新">
          <div class="text-12px text-ink-4 font-mono bg-surface-2 py-8px px-14px rounded-sm leading-[1.7]">
            可选：设置环境变量配置远程 feed 以自动更新<br />
            <span class="text-ink-2">VORTEX_CATALOG_FEED_URL=https://example.com/models.json</span><br />
            feed 格式：{ "models": [ { "provider": "openai", "model": "gpt-4o", ... } ] }
          </div>
          <div class="empty-actions flex gap-8px mt-12px">
            <button type="button" class="btn primary" :disabled="syncing" @click="syncCatalog">
              <el-icon v-if="syncing" class="spin" :size="14"><Loading /></el-icon>
              <el-icon v-else :size="14"><RefreshRight /></el-icon>
              {{ syncing ? '同步中…' : '立即同步' }}
            </button>
          </div>
        </EmptyState>
      </div>

      <!-- 目录列表 -->
      <template v-else>
        <div class="card">
          <div class="list-bar flex items-center gap-10px px-14px py-10px border-b border-line border-solid border-0">
            <el-input v-model="catKeyword" placeholder="搜索提供方 / 模型" clearable size="small" class="max-w-240px" />
            <span class="spacer flex-1" />
            <span class="text-11.5px text-ink-4 tabular-nums">共 {{ catalog.length }} 条</span>
            <button type="button" class="btn sm" @click="loadCatalog">
              <el-icon :size="12"><Refresh /></el-icon>刷新
            </button>
            <button type="button" class="btn sm primary" :disabled="syncing" @click="syncCatalog">
              <el-icon v-if="syncing" class="spin" :size="12"><Loading /></el-icon>
              <el-icon v-else :size="12"><RefreshRight /></el-icon>
              {{ syncing ? '同步中' : '同步' }}
            </button>
          </div>
        </div>

        <div v-for="g in catFilteredGroups" :key="g.provider" class="card">
          <div class="flex items-center gap-10px py-10px px-16px border-b border-line border-solid border-0">
            <ProviderLogo :name="g.provider" :size="20" />
            <span class="mono text-13.5px font-semibold">{{ g.provider }}</span>
            <span class="pill neutral text-11px">{{ g.models.length }} 个模型</span>
          </div>

          <div class="flex flex-col">
            <div
              v-for="m in g.models"
              :key="m.id"
              class="model-row flex items-center gap-14px py-10px px-16px border-b border-line border-solid border-0 last:border-b-0 hover:bg-surface-2 transition-colors"
            >
              <!-- 模型名 -->
              <div class="min-w-0 w-240px shrink-0">
                <div class="mono text-12.5px text-ink font-medium truncate" :title="m.model">{{ m.model }}</div>
                <div v-if="m.name" class="text-11px text-ink-4 truncate">{{ m.name }}</div>
              </div>

              <!-- 上下文窗口 -->
              <div class="w-90px shrink-0 text-11.5px text-ink-3">
                <span v-if="m.contextWindow" class="tabular-nums">{{ fmtContext(m.contextWindow) }}</span>
                <span v-else class="text-ink-5">—</span>
              </div>

              <!-- 能力标签 -->
              <div class="flex items-center gap-4px shrink-0">
                <span v-if="m.supportsTools" class="cap-tag" title="支持工具调用（Function Calling）">工具</span>
                <span v-if="m.supportsVision" class="cap-tag" title="支持图像输入（视觉）">视觉</span>
                <span v-if="m.supportsStreaming" class="cap-tag" title="支持流式输出">流式</span>
                <span v-if="!m.supportsTools && !m.supportsVision && !m.supportsStreaming" class="text-11px text-ink-5">—</span>
              </div>

              <!-- 官方限速 -->
              <div class="flex items-center gap-10px text-11px text-ink-4 min-w-0 flex-1 overflow-hidden">
                <span v-if="m.rpm" class="shrink-0" title="每分钟请求数">RPM {{ m.rpm }}</span>
                <span v-if="m.rpd" class="shrink-0" title="每日请求数">RPD {{ fmtNum(m.rpd) }}</span>
                <span v-if="m.tpm" class="shrink-0" title="每分钟 Token 数">TPM {{ fmtNum(m.tpm) }}</span>
                <span v-if="m.tpd" class="shrink-0" title="每日 Token 数">TPD {{ fmtNum(m.tpd) }}</span>
                <span v-if="m.freeQuota" class="shrink-0 pill ok text-10.5px" :title="m.freeQuota">{{ m.freeQuota }}</span>
                <span v-if="!m.rpm && !m.rpd && !m.tpm && !m.tpd && !m.freeQuota" class="text-ink-5">无限速信息</span>
              </div>

              <!-- 应用为限额 -->
              <el-tooltip content="把官方限速值写入速率限制规则，超限时路由自动跳到下一个候选" placement="top">
                <button
                  type="button"
                  class="btn sm shrink-0"
                  :disabled="!m.rpm && !m.rpd && !m.tpm && !m.tpd"
                  @click="applyAsCap(m)"
                >应用为限额</button>
              </el-tooltip>
            </div>
          </div>
        </div>

        <div v-if="catFilteredGroups.length === 0" class="card py-24px text-center text-12.5px text-ink-4">
          没有匹配「{{ catKeyword }}」的模型
        </div>
      </template>
    </div>

    <div v-else-if="tab === 'tos'" class="card">
      <div class="list-bar flex items-center gap-10px px-14px py-10px border-b border-line border-solid border-0">
        <span class="text-13px font-medium text-ink-2">ToS 审查记录</span>
        <span class="spacer flex-1" />
        <span class="text-11.5px text-ink-4 tabular-nums">共 {{ tosReviews.length }} 条</span>
        <button type="button" class="btn sm" :disabled="tosLoading" @click="loadTosReviews">
          <el-icon v-if="tosLoading" class="spin" :size="12"><Loading /></el-icon>刷新
        </button>
      </div>
      <table class="table">
        <colgroup>
          <col style="width: 160px" />
          <col style="width: 120px" />
          <col />
          <col style="width: 140px" />
        </colgroup>
        <thead>
          <tr>
            <th>提供商</th>
            <th>结论</th>
            <th>备注</th>
            <th class="num">审查日期</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="rv in tosReviews" :key="rv.id">
            <td><span class="pill info mono">{{ rv.provider }}</span></td>
            <td><span class="pill" :class="tosVerdictTone(rv.verdict)">{{ tosVerdictLabel(rv.verdict) }}</span></td>
            <td class="text-12.5px text-ink-3">{{ rv.notes ?? '—' }}</td>
            <td class="num">{{ fmtTime(rv.reviewDate || rv.updatedAt) }}</td>
          </tr>
          <tr v-if="tosReviews.length === 0">
            <td colspan="4" class="text-center text-ink-4 py-24px text-12.5px">暂无审查记录</td>
          </tr>
        </tbody>
      </table>
    </div>

    <el-dialog v-model="rlDialogOpen" :title="rlEditing ? '编辑速率限制' : '新建速率限制'" width="520">
      <div class="flex flex-col gap-16px">
        <div class="flex flex-col gap-6px">
          <label class="text-13px font-medium text-ink-2">提供商</label>
          <el-input v-model="rlForm.provider" placeholder="如 openai" class="font-mono" />
        </div>
        <div class="flex flex-col gap-6px">
          <label class="text-13px font-medium text-ink-2">模型</label>
          <el-input v-model="rlForm.model" placeholder="如 gpt-4o" class="font-mono" />
        </div>
        <div class="grid grid-cols-2 gap-12px">
          <div class="flex flex-col gap-6px">
            <label class="text-13px font-medium text-ink-2">RPM</label>
            <el-input-number v-model="rlForm.rpm" :min="0" controls-position="right" class="w-full" />
          </div>
          <div class="flex flex-col gap-6px">
            <label class="text-13px font-medium text-ink-2">RPD</label>
            <el-input-number v-model="rlForm.rpd" :min="0" controls-position="right" class="w-full" />
          </div>
          <div class="flex flex-col gap-6px">
            <label class="text-13px font-medium text-ink-2">TPM</label>
            <el-input-number v-model="rlForm.tpm" :min="0" controls-position="right" class="w-full" />
          </div>
          <div class="flex flex-col gap-6px">
            <label class="text-13px font-medium text-ink-2">TPD</label>
            <el-input-number v-model="rlForm.tpd" :min="0" controls-position="right" class="w-full" />
          </div>
        </div>
        <div class="flex items-center gap-8px">
          <el-switch v-model="rlForm.isActive" />
          <span class="text-13px text-ink-3">启用</span>
        </div>
      </div>
      <template #footer>
        <button class="btn" @click="rlDialogOpen = false">取消</button>
        <button class="btn accent" @click="saveRateLimit">{{ rlEditing ? '保存' : '创建' }}</button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
/**
 * 订阅列表页面。
 *
 * 职责：展示所有 AI 提供商连接的列表，包括状态、提供商、模型、优先级与更新时间，
 * 并提供新增内置/自定义提供方的入口。点击行可跳转到连接详情。
 *
 * 与旧版的差异：状态不再只是「进去详情页才能测」——列表每行都有「测试」按钮，
 * 顶部可一键批量测试，测试完成后就地回写该行的状态徽标与错误信息，不必刷新页面。
 */
import { computed, onMounted, ref, reactive, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Loading, Plus, Connection, Refresh, RefreshRight, WarningFilled, InfoFilled } from '@element-plus/icons-vue'
import PageHeader from '@/components/ui/PageHeader.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import ProviderLogo from '@/components/ui/ProviderLogo.vue'
import { listProviders, testProvider, type ProviderDef, type ProviderConnection } from '@/api/providers'
import { tosReviewApi, type TosReview } from '@/api/tosReview'
import { rateLimitsApi, type RateLimitCap, type UpsertRateLimitCap } from '@/api/rateLimits'
import { modelCatalogApi, type CatalogEntry } from '@/api/modelCatalog'

const tabs = [
  { key: 'connections', label: '连接管理' },
  { key: 'rate-limits', label: '速率限制' },
  { key: 'catalog', label: '模型目录' },
  { key: 'tos', label: 'ToS 审查' },
] as const
const tab = ref<typeof tabs[number]['key']>('connections')

// 当前连接列表
const connections = ref<ProviderConnection[]>([])
// 内置提供商定义列表
const providers = ref<ProviderDef[]>([])
// 提供商 ToS 合规状态映射（provider → review）
const tosMap = ref<Record<string, TosReview>>({})
// 是否正在加载
const loading = ref(true)
// 搜索关键字
const keyword = ref('')
// 正在测试中的连接 ID → true
const testingMap = ref<Record<string, boolean>>({})
// 是否正在批量测试
const testingAll = ref(false)
// 批量测试进度
const progress = ref({ done: 0, total: 0 })

/**
 * 根据提供商 ID 查询其主题色。
 * @param id 提供商标识
 * @returns 颜色字符串，未找到则返回空串
 */
function provColor(id: string): string {
  return providers.value.find((p) => p.id === id)?.color ?? ''
}

/** 根据 provider、baseUrl、name 推断 ToS 审查的 provider key */
function tosProviderKey(conn: ProviderConnection): string {
  if (tosMap.value[conn.provider]) return conn.provider
  const url = (conn.baseUrl || '').toLowerCase()
  const name = (conn.name || '').toLowerCase()
  const patterns: [string, string][] = [
    ['nvidia.com', 'nvidia'], ['openrouter.ai', 'openrouter'], ['z.ai', 'zai'],
    ['volces.com', 'volcengine-code'], ['sensenova', 'sensenova'], ['siliconflow', 'siliconflow'],
    ['deepseek.com', 'deepseek'], ['groq.com', 'groq'], ['mistral.ai', 'mistral'],
    ['anthropic.com', 'anthropic'], ['openai.com', 'openai'], ['cloudflare.com', 'cloudflare'],
    ['huggingface.co', 'huggingface'], ['minimax', 'minimax'], ['cohere.com', 'cohere'],
    ['cerebras.ai', 'cerebras'], ['qwen', 'qwen'], ['ollama', 'ollama'], ['xai', 'xai'],
    ['gemini', 'gemini'], ['google', 'gemini'],
  ]
  for (const [pat, key] of patterns) {
    if ((url.includes(pat) || name.includes(pat)) && tosMap.value[key]) return key
  }
  return ''
}

/** ToS 结论对应的样式类 */
function tosClass(provider: string): string {
  const v = tosMap.value[provider]?.verdict
  return v === 'ok' ? 'tos-ok' : v === 'caution' ? 'tos-warn' : v === 'avoid' ? 'tos-err' : 'tos-neutral'
}
/** ToS 结论对应的中文标签 */
function tosLabel(provider: string): string {
  const v = tosMap.value[provider]?.verdict
  return v === 'ok' ? '可用' : v === 'caution' ? '谨慎' : v === 'avoid' ? '避免' : ''
}
/** ToS 悬浮说明：结论 + 备注 + 审查日期 */
function tosTip(provider: string): string {
  const r = tosMap.value[provider]
  if (!r) return ''
  const bits: string[] = []
  bits.push(`ToS 合规：${tosLabel(provider)}`)
  if (r.notes) bits.push(r.notes)
  if (r.reviewDate) bits.push(`审查于 ${r.reviewDate}`)
  return bits.join('\n')
}

/**
 * 按关键字过滤连接：匹配名称、显示名、提供方、分组与模型 ID/别名。
 */
const filtered = computed(() => {
  const k = keyword.value.trim().toLowerCase()
  if (!k) return connections.value
  return connections.value.filter((c) => {
    const parts = [
      c.name,
      c.displayName ?? '',
      c.provider,
      c.groupName ?? '',
      c.defaultModel ?? '',
      c.baseUrl ?? '',
      ...(c.models ?? []).map((m) => `${m.id} ${m.name ?? ''}`),
    ]
    return parts.join(' ').toLowerCase().includes(k)
  })
})

/**
 * 判断某连接是否正在测试中。
 * @param id 连接 ID
 */
function isTesting(id: string): boolean {
  return !!testingMap.value[id]
}

/**
 * 状态列展示文本。
 *
 * 健康态不再显示「健康」二字，而是直接显示最近一次测试的响应耗时（如 `128ms`），
 * 让用户一眼看出快慢；其余状态仍用文字。
 *
 * @param conn 提供商连接
 */
function statusLabel(conn: ProviderConnection): string {
  if (!conn.isActive) return '已禁用'
  const s = conn.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success', 'active'].includes(s)) {
    return typeof conn.lastLatencyMs === 'number' ? fmtLatency(conn.lastLatencyMs) : '可用'
  }
  if (['failed', 'error', 'invalid'].includes(s)) return '失败'
  return '未测试'
}

/**
 * 格式化耗时：小于 1000ms 用毫秒，否则折算为秒并保留一位小数。
 * @param ms 毫秒
 */
function fmtLatency(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return '可用'
  return ms < 1000 ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`
}

/**
 * 根据连接状态计算徽标色调。
 * @param conn 提供商连接
 */
function statusTone(conn: ProviderConnection): 'ok' | 'warn' | 'err' | 'neutral' {
  if (!conn.isActive) return 'neutral'
  const s = conn.testStatus?.toLowerCase() ?? ''
  if (['ok', 'healthy', 'success', 'active'].includes(s)) return 'ok'
  if (['failed', 'error', 'invalid'].includes(s)) return 'err'
  return 'neutral'
}

/**
 * 状态徽标的悬浮说明：最近测试时间 + 失败原因，未测试时给出操作引导。
 * @param conn 提供商连接
 */
function statusTip(conn: ProviderConnection): string {
  if (!conn.isActive) return '该连接已禁用，不参与路由'
  const bits: string[] = []
  bits.push(conn.lastTestedAt ? `最近测试：${fmtDateTime(conn.lastTestedAt)}` : '尚未测试，点击「测试」检查连通性')
  if (typeof conn.lastLatencyMs === 'number') bits.push(`响应耗时：${fmtLatency(conn.lastLatencyMs)}`)
  if (conn.lastError) bits.push(`错误：${conn.lastError}`)
  if (!conn.hasApiKey && !conn.hasAccessToken) bits.push('提示：该连接未配置密钥')
  return bits.join('\n')
}

/**
 * 把测试结果就地写回列表项，避免刷新整页。
 * @param conn 被测试的连接
 * @param r 测试结果
 */
function applyResult(conn: ProviderConnection, r: { status: string; error?: string; latencyMs?: number }) {
  conn.testStatus = r.status
  conn.lastError = r.error || undefined
  conn.lastTestedAt = new Date().toISOString()
  // 耗时随之持久化到数据库（last_latency_ms），刷新后列表仍能显示
  conn.lastLatencyMs = typeof r.latencyMs === 'number' ? r.latencyMs : undefined
}

/**
 * 测试单个连接的健康状态（行内按钮）。
 * @param conn 待测试的连接
 */
async function testOne(conn: ProviderConnection) {
  if (isTesting(conn.id)) return
  testingMap.value[conn.id] = true
  try {
    const r = await testProvider(conn.id)
    applyResult(conn, r)
    if (r.status === 'ok') {
      ElMessage.success(`${conn.name}：可用${r.latencyMs ? ` · ${fmtLatency(r.latencyMs)}` : ''}`)
    } else {
      ElMessage.error(`${conn.name}：${r.error || '连接失败'}`)
    }
  } catch (e: any) {
    const msg = e?.response?.data?.error || e?.message || '请求未到达网关'
    applyResult(conn, { status: 'error', error: msg })
    ElMessage.error(`${conn.name}：测试请求失败`)
  } finally {
    delete testingMap.value[conn.id]
  }
}

/**
 * 批量测试当前列表中的所有连接。
 *
 * 串行执行而非并发：后端每个测试自带 15 秒超时，并发打爆上游容易被限流，
 * 串行也能让进度条读数准确。
 */
async function testAll() {
  const list = filtered.value
  if (!list.length) return
  testingAll.value = true
  progress.value = { done: 0, total: list.length }
  let ok = 0
  let fail = 0
  try {
    for (const conn of list) {
      try {
        const r = await testProvider(conn.id)
        applyResult(conn, r)
        if (r.status === 'ok') ok++
        else fail++
      } catch {
        applyResult(conn, { status: 'error', error: '测试请求失败' })
        fail++
      }
      progress.value.done++
    }
    if (fail === 0) ElMessage.success(`全部 ${ok} 个连接均可用`)
    else ElMessage.warning(`测试完成：${ok} 个可用，${fail} 个失败（悬浮状态徽标可看原因）`)
  } finally {
    testingAll.value = false
  }
}

/**
 * 格式化 ISO 时间为 M/D HH:mm。
 * @param iso ISO 时间字符串
 */
function fmtTime(iso: string): string {
  if (!iso) return '—'
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

/**
 * 格式化 ISO 时间为 M/D HH:mm:ss（用于 tooltip 里的精确时间）。
 * @param iso ISO 时间字符串
 */
function fmtDateTime(iso: string): string {
  if (!iso) return '—'
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`
}

/** 列表中最多展示的模型标签数；超出折叠为 +N */
const maxTags = 3

/**
 * 计算模型标签显示文本：优先用自定义名称，回退到模型 ID。
 * @param m 模型对象
 */
function modelTagLabel(m: { id: string; name?: string }): string {
  const trimmed = m.name?.trim()
  return trimmed && trimmed.length > 0 ? trimmed : m.id
}

/**
 * 计算模型标签的 tooltip 文本：形如「id（名称）」。
 * @param m 模型对象
 */
function modelTagTitle(m: { id: string; name?: string }): string {
  const trimmed = m.name?.trim()
  return trimmed && trimmed.length > 0 ? `${m.id}（${trimmed}）` : m.id
}

/**
 * 取前 maxTags 个模型用于列表展示。
 * @param conn 提供商连接
 */
function visibleModels(conn: ProviderConnection) {
  return (conn.models ?? []).slice(0, maxTags)
}

/**
 * 将超出 maxTags 的模型拼接为 tooltip 内容。
 * @param conn 提供商连接
 */
function overflowModels(conn: ProviderConnection): string {
  const rest = (conn.models ?? []).slice(maxTags)
  return rest.map((m) => modelTagTitle(m)).join('\n')
}

/**
 * 加载内置提供商定义与连接列表。
 */
async function load() {
  loading.value = true
  try {
    const data = await listProviders()
    providers.value = data.providers ?? []
    connections.value = data.connections ?? []
  } finally {
    loading.value = false
  }
  // 并行加载 ToS 合规状态（失败不阻断主流程）
  try {
    const reviews = await tosReviewApi.list()
    tosMap.value = Object.fromEntries(reviews.map((r) => [r.provider, r]))
  } catch { /* 后端未就绪时静默 */ }
}

const rateLimits = ref<RateLimitCap[]>([])
const rlLoading = ref(false)
const rlDialogOpen = ref(false)
const rlEditing = ref<string | null>(null)
const rlForm = reactive<UpsertRateLimitCap & { isActive: boolean }>({
  provider: '',
  model: '',
  rpm: undefined,
  rpd: undefined,
  tpm: undefined,
  tpd: undefined,
  isActive: true,
})

async function loadRateLimits() {
  rlLoading.value = true
  try {
    rateLimits.value = await rateLimitsApi.list()
  } catch {
    ElMessage.error('加载速率限制失败')
  } finally {
    rlLoading.value = false
  }
}

function openRateLimitCreate() {
  rlEditing.value = null
  rlForm.provider = ''
  rlForm.model = ''
  rlForm.rpm = undefined
  rlForm.rpd = undefined
  rlForm.tpm = undefined
  rlForm.tpd = undefined
  rlForm.isActive = true
  rlDialogOpen.value = true
}

function openRateLimitEdit(cap: RateLimitCap) {
  rlEditing.value = cap.id
  rlForm.provider = cap.provider
  rlForm.model = cap.model
  rlForm.rpm = cap.rpm
  rlForm.rpd = cap.rpd
  rlForm.tpm = cap.tpm
  rlForm.tpd = cap.tpd
  rlForm.isActive = cap.isActive
  rlDialogOpen.value = true
}

async function saveRateLimit() {
  if (!rlForm.provider.trim() || !rlForm.model.trim()) {
    ElMessage.warning('请填写提供商与模型')
    return
  }
  try {
    await rateLimitsApi.upsert({
      provider: rlForm.provider.trim(),
      model: rlForm.model.trim(),
      rpm: rlForm.rpm,
      rpd: rlForm.rpd,
      tpm: rlForm.tpm,
      tpd: rlForm.tpd,
      isActive: rlForm.isActive,
    })
    ElMessage.success(rlEditing.value ? '已保存' : '已创建')
    rlDialogOpen.value = false
    await loadRateLimits()
  } catch {
    ElMessage.error('操作失败')
  }
}

async function removeRateLimit(cap: RateLimitCap) {
  try {
    await ElMessageBox.confirm(`确定删除「${cap.provider}/${cap.model}」的速率限制？`, '删除确认', { type: 'warning' })
    await rateLimitsApi.delete(cap.id)
    ElMessage.success('已删除')
    await loadRateLimits()
  } catch {
    /* cancelled */
  }
}

const catalog = ref<CatalogEntry[]>([])
const catLoading = ref(false)
const syncing = ref(false)
// 目录搜索关键字（与连接列表的 keyword 独立）
const catKeyword = ref('')

/** 目录最后同步时间（取所有条目的最大 lastSynced） */
const catLastSynced = computed(() => {
  const times = catalog.value.map((e) => e.lastSynced).filter(Boolean) as string[]
  if (times.length === 0) return ''
  return times.sort()[times.length - 1] ?? ''
})

/** 目录按提供方分组 */
const catGrouped = computed(() => {
  const map = new Map<string, CatalogEntry[]>()
  for (const e of catalog.value) {
    const list = map.get(e.provider) ?? []
    list.push(e)
    map.set(e.provider, list)
  }
  return [...map.entries()]
    .map(([provider, models]) => ({ provider, models: models.sort((a, b) => a.model.localeCompare(b.model)) }))
    .sort((a, b) => a.provider.localeCompare(b.provider))
})

/** 搜索过滤后的目录分组 */
const catFilteredGroups = computed(() => {
  const k = catKeyword.value.trim().toLowerCase()
  if (!k) return catGrouped.value
  return catGrouped.value
    .map((g) => ({
      provider: g.provider,
      models: g.models.filter((m) => `${g.provider} ${m.model} ${m.name ?? ''}`.toLowerCase().includes(k)),
    }))
    .filter((g) => g.models.length > 0 || g.provider.toLowerCase().includes(k))
})

/**
 * 格式化上下文窗口：按 k/M token 展示。
 * @param n token 数
 */
function fmtContext(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n % 1_000_000 === 0 ? 0 : 1)}M ctx`
  // 2 的幂（如 65536、131072）按 1024 进制展示，与官方「64K/128K」口径一致
  if (n > 0 && (n & (n - 1)) === 0) return `${n / 1024}k ctx`
  if (n >= 1000) return `${Math.round(n / 1000)}k ctx`
  return `${n} ctx`
}

/** 千分位格式化数字。 */
function fmtNum(n?: number): string {
  return typeof n === 'number' ? n.toLocaleString('en-US') : ''
}

/**
 * 把官方限速值应用为速率限制规则。
 * @param m 目录条目
 */
async function applyAsCap(m: CatalogEntry) {
  try {
    await rateLimitsApi.upsert({
      provider: m.provider,
      model: m.model,
      rpm: m.rpm || undefined,
      rpd: m.rpd || undefined,
      tpm: m.tpm || undefined,
      tpd: m.tpd || undefined,
    })
    ElMessage.success(`已把 ${m.provider}/${m.model} 的官方限速写入速率限制`)
    // 限额列表可能已加载，同步刷新使新规则立即可见
    if (rateLimits.value.length > 0) loadRateLimits()
  } catch {
    ElMessage.error('应用限额失败')
  }
}

async function loadCatalog() {
  catLoading.value = true
  try {
    catalog.value = await modelCatalogApi.list()
  } catch {
    ElMessage.error('加载模型目录失败')
  } finally {
    catLoading.value = false
  }
}

/** 同步：导入内置快照，若配置了 feed 再拉远程。 */
async function syncCatalog() {
  syncing.value = true
  try {
    const r = await modelCatalogApi.sync()
    if (r.warning) {
      ElMessage.warning(`${r.warning}（内置快照已导入）`)
    } else {
      ElMessage.success(`同步完成，共 ${r.synced} 条模型`)
    }
    await loadCatalog()
  } catch (e: any) {
    const msg = e?.response?.data?.error || e?.message || '同步失败'
    ElMessage.error(`同步失败：${msg}`)
  } finally {
    syncing.value = false
  }
}

const tosReviews = ref<TosReview[]>([])
const tosLoading = ref(false)

async function loadTosReviews() {
  tosLoading.value = true
  try {
    tosReviews.value = await tosReviewApi.list()
  } catch {
    ElMessage.error('加载审查记录失败')
  } finally {
    tosLoading.value = false
  }
}

function tosVerdictTone(verdict: string): 'ok' | 'warn' | 'err' | 'neutral' {
  if (verdict === 'ok') return 'ok'
  if (verdict === 'caution') return 'warn'
  if (verdict === 'avoid') return 'err'
  return 'neutral'
}

function tosVerdictLabel(verdict: string): string {
  if (verdict === 'ok') return '可用'
  if (verdict === 'caution') return '谨慎'
  if (verdict === 'avoid') return '避免'
  return verdict || '未知'
}

onMounted(load)

watch(tab, (t) => {
  if (t === 'rate-limits' && rateLimits.value.length === 0) loadRateLimits()
  else if (t === 'catalog' && catalog.value.length === 0) loadCatalog()
  else if (t === 'tos' && tosReviews.value.length === 0) loadTosReviews()
})
</script>

<style scoped>
/* ToS 合规徽标色调 */
.tos-pill.tos-ok { background: var(--ok-bg); color: var(--ok); }
.tos-pill.tos-warn { background: var(--warn-bg); color: var(--warn); }
.tos-pill.tos-err { background: var(--err-bg); color: var(--err); }
.tos-pill.tos-neutral { background: var(--surface-3); color: var(--ink-4); }

/* 模型目录能力标签 */
.cap-tag {
  display: inline-flex;
  align-items: center;
  font-size: 10.5px;
  line-height: 1;
  padding: 3px 6px;
  border-radius: 999px;
  background: var(--accent-bg);
  color: var(--accent);
  cursor: default;
}
</style>
