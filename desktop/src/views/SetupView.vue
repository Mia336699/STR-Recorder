<!--
    首次启动配置向导 / First-launch Setup Wizard

    在 setup_done = false 时显示，引导用户完成基础配置：
    Step 1 — 语言选择
    Step 2 — 录制输出目录
    Step 3 — 网络代理（可选）
    完成后将 setup_done 置为 true 并跳转到主页。

    Shown when setup_done = false. Guides the user through basic configuration:
    Step 1 — Language
    Step 2 — Recording output directory
    Step 3 — Network proxies (optional)
    On finish, sets setup_done = true and navigates to the home page.
-->
<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { useSettingsStore } from "@/stores/settings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { loadLocaleFromServer } from "@/i18n";
import { useModuleLocaleStore } from "@/stores/moduleLocale";
import { useLocalesStore } from "@/stores/locales";

import { call } from "@/lib/api";
import appIcon from "@/assets/str-icon.png";
import { FolderOpen, Check, AlertCircle, RefreshCw } from "@lucide/vue";
interface ToolStatus {
	name: string;
	available: boolean;
	version?: string;
	error?: string;
}
const tools = ref<ToolStatus[]>([]),
	toolsLoading = ref(false),
	toolsError = ref(""),
	ready = ref(false),
	choosing = ref(false);
async function checkTools() {
	toolsLoading.value = true;
	toolsError.value = "";
	try {
		tools.value = await call<ToolStatus[]>("get_runtime_tools");
	} catch (e) {
		toolsError.value = String(e);
	} finally {
		toolsLoading.value = false;
	}
}
async function chooseDirectory() {
	choosing.value = true;
	try {
		const path = await call<string | null>("choose_recording_directory", {
			title: t("ui.chooseFolderTitle"),
		});
		if (path) outputDir.value = path;
	} catch (e) {
		error.value = String(e);
	} finally {
		choosing.value = false;
	}
}
const router = useRouter();
const { t, locale } = useI18n();
const store = useSettingsStore();
const moduleLocaleStore = useModuleLocaleStore();
const localesStore = useLocalesStore();

// ── 步骤控制 / Step control ──────────────────────────────────────────────────
const TOTAL_STEPS = 3;
const step = ref(1);
const saving = ref(false);
const error = ref("");

/** 步骤切换方向，用于决定滑动动画方向 / Step direction for slide animation */
const stepTransition = ref("step-forward");

// ── 表单字段 / Form fields ───────────────────────────────────────────────────
const language = ref("zh-CN");
const outputDir = ref("");
const apiProxy = ref("");
const scMirror = ref("");
const cdnProxy = ref("");

/** 可用语言列表（从共享 store 读取，由 App.vue 统一维护）
 * Available locales (from shared store, maintained by App.vue) */
// 直接使用 store 的 ref，保持响应式；不能赋给普通变量（会丢失响应式）
// Use the store's ref directly to keep reactivity; assigning to a plain variable breaks it

async function initialize() {
	try {
		void checkTools();
		// App.vue 可能还未完成 onMounted（例如首次配置路由直接到 /setup），
		// 确保语言列表已加载；若已加载则 refresh() 内部会快速返回已有结果。
		// App.vue may not have finished its onMounted yet (e.g. first-launch routing to /setup).
		// Ensure the locale list is loaded; if already loaded, refresh() returns quickly.
		if (!localesStore.loaded) {
			await localesStore.refresh();
		}
		await store.fetchSettings();
		const s = store.settings;
		language.value = s.language || "zh-CN";
		locale.value = language.value;
		outputDir.value = s.output_dir || "";
		apiProxy.value = s.api_proxy_url || "";
		scMirror.value = s.sc_mirror_url || "";
		cdnProxy.value = s.cdn_proxy_url || "";
		ready.value = true;
	} catch (e) {
		error.value = String(e);
	}
}
onMounted(initialize);

// ── 语言切换 / Language switch ───────────────────────────────────────────────
async function setLanguage(lang: string) {
	language.value = lang;
	// 先加载消息，再切换 locale，避免 vue-i18n 在消息就绪前以 fallback 语言渲染
	// Load messages first, then switch locale to avoid vue-i18n rendering with fallback
	const { modules: moduleLocales, warning } = await loadLocaleFromServer(lang);
	locale.value = lang;
	moduleLocaleStore.setLocales(lang, moduleLocales);
	if (warning) {
		// SetupView 没有 toast，用 error ref 展示
		error.value =
			t("settings.localeFileInvalid", { file: `${lang}.json` }) +
			": " +
			warning;
	}
}

// ── 步骤校验 / Step validation ───────────────────────────────────────────────
const canNext = computed(() => {
	if (!ready.value || saving.value) return false;
	if (step.value === 2) return outputDir.value.trim().startsWith("/");
	return true;
});

function next() {
	error.value = "";
	if (step.value < TOTAL_STEPS) {
		stepTransition.value = "step-forward";
		step.value++;
	}
}

function back() {
	error.value = "";
	if (step.value > 1) {
		stepTransition.value = "step-back";
		step.value--;
	}
}

// ── 完成向导 / Finish wizard ─────────────────────────────────────────────────
async function finish() {
	if (!outputDir.value.trim().startsWith("/")) {
		error.value = t("ui.absoluteDirectory");
		stepTransition.value = "step-back";
		step.value = 2;
		return;
	}
	for (const proxy of [apiProxy.value, cdnProxy.value]) {
		if (!proxy.trim()) continue;
		try {
			const url = new URL(proxy.trim());
			if (
				!url.hostname ||
				!["http:", "https:", "socks5:", "socks5h:"].includes(url.protocol)
			)
				throw new Error();
		} catch {
			error.value = t("ui.invalidNetwork");
			return;
		}
	}
	if (
		scMirror.value.trim() &&
		!/^[a-z0-9.-]+(?::\d+)?$/i.test(scMirror.value.trim())
	) {
		error.value = t("ui.invalidMirror");
		return;
	}
	saving.value = true;
	error.value = "";
	try {
		await store.saveSettings({
			...store.settings,
			language: language.value,
			output_dir: outputDir.value.trim(),
			api_proxy_url: apiProxy.value.trim() || null,
			sc_mirror_url: scMirror.value.trim() || null,
			cdn_proxy_url: cdnProxy.value.trim() || null,
			setup_done: true,
		});
		await router.replace("/");
	} catch (e: unknown) {
		error.value = String(e);
	} finally {
		saving.value = false;
	}
}
</script>

<template>
	<div class="setup-workspace">
		<aside class="setup-intro">
			<img :src="appIcon" alt="STR Recorder" />
			<div class="eyebrow">STR RECORDER · MACOS</div>
			<h1>{{ t("ui.setupTitle") }}</h1>
			<p>{{ t("ui.setupHint") }}</p>
			<ul>
				<li>Stripchat · Chaturbate</li>
				<li>{{ t("ui.previewIndependent") }}</li>
				<li>{{ t("ui.fileOrder") }}</li>
			</ul>
			<section class="setup-tools">
				<div class="flex items-center justify-between">
					<h2>{{ t("ui.stepTools") }}</h2>
					<Button
						variant="ghost"
						size="icon"
						:disabled="toolsLoading"
						:aria-label="t('ui.retry')"
						@click="checkTools"
						><RefreshCw
							class="size-4"
							:class="{ 'animate-spin': toolsLoading }"
					/></Button>
				</div>
				<p v-if="toolsLoading" role="status">{{ t("common.loading") }}</p>
				<p v-else-if="toolsError" role="alert">{{ toolsError }}</p>
				<div
					v-else
					v-for="tool in tools"
					:key="tool.name"
					class="setup-tool"
					:title="tool.version || tool.error"
				>
					<Check
						v-if="tool.available"
						class="size-4 text-[var(--live)]"
					/><AlertCircle v-else class="size-4 text-destructive" /><strong>{{
						tool.name
					}}</strong
					><span>{{
						tool.available ? t("ui.toolAvailable") : t("ui.toolUnavailable")
					}}</span>
				</div>
			</section>
		</aside>
		<div class="setup-form flex flex-col gap-6">
			<!-- 标题 / Title -->
			<div class="flex flex-col gap-1.5">
				<div class="flex items-center gap-2.5">
					<img :src="appIcon" alt="" class="size-7 rounded-lg" />
					<span class="text-lg font-bold">STR Recorder</span>
				</div>
				<h2 class="text-xl font-bold mt-1">{{ t("setup.title") }}</h2>
				<p class="text-sm text-muted-foreground">{{ t("setup.subtitle") }}</p>
			</div>

			<!-- 步骤指示器 / Step indicator -->
			<div class="flex items-center gap-2">
				<template v-for="i in TOTAL_STEPS" :key="i">
					<div
						class="flex items-center justify-center w-7 h-7 rounded-full text-xs font-semibold transition-colors duration-200"
						:class="
							i < step
								? 'bg-primary text-primary-foreground'
								: i === step
									? 'bg-primary text-primary-foreground ring-2 ring-primary/30'
									: 'bg-muted text-muted-foreground'
						"
					>
						<svg
							v-if="i < step"
							class="w-3.5 h-3.5"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="3"
						>
							<polyline points="20 6 9 17 4 12" />
						</svg>
						<span v-else>{{ i }}</span>
					</div>
					<div
						v-if="i < TOTAL_STEPS"
						class="flex-1 h-px transition-colors duration-200"
						:class="i < step ? 'bg-primary' : 'bg-border'"
					/>
				</template>
			</div>

			<!-- 步骤内容（带方向感知的滑动过渡）/ Step content with directional slide transition -->
			<div class="overflow-hidden min-h-52">
				<Transition :name="stepTransition" mode="out-in">
					<div :key="step" class="flex flex-col gap-6">
						<!-- Step 1: 语言 / Language -->
						<template v-if="step === 1">
							<div class="flex flex-col gap-1.5">
								<h2 class="text-base font-semibold">
									{{ t("setup.step1.title") }}
								</h2>
								<p class="text-sm text-muted-foreground">
									{{ t("setup.step1.desc") }}
								</p>
							</div>
							<RadioGroup
								:model-value="language"
								class="flex flex-col gap-3"
								@update:model-value="(v) => v && setLanguage(String(v))"
							>
								<label
									v-for="loc in localesStore.locales"
									:key="loc.code"
									class="flex items-center gap-3 rounded-lg border p-4 cursor-pointer transition-colors"
									:class="
										language === loc.code
											? 'border-primary bg-primary/5'
											: 'border-border hover:border-primary/50'
									"
								>
									<RadioGroupItem :value="loc.code" />
									<span class="font-medium">{{ loc.name }}</span>
								</label>
							</RadioGroup>
						</template>

						<!-- Step 2: 输出目录 / Output directory -->
						<template v-if="step === 2">
							<div class="flex flex-col gap-1.5">
								<h2 class="text-base font-semibold">
									{{ t("setup.step2.title") }}
								</h2>
								<p class="text-sm text-muted-foreground">
									{{ t("setup.step2.desc") }}
								</p>
							</div>
							<div class="flex flex-col gap-2">
								<Label for="setup-directory">{{
									t("settings.outputDir.label")
								}}</Label>
								<div class="flex gap-2">
									<Input
										id="setup-directory"
										v-model="outputDir"
										:placeholder="t('setup.step2.placeholder')"
										autocomplete="off"
										autofocus
									/><Button
										variant="outline"
										:disabled="choosing"
										@click="chooseDirectory"
										><FolderOpen class="size-4" />{{
											t("ui.chooseFolder")
										}}</Button
									>
								</div>
								<p class="text-xs text-muted-foreground">
									{{ t("setup.step2.hint") }}
								</p>
							</div>
						</template>

						<!-- Step 3: 网络代理（可选）/ Network proxies (optional) -->
						<template v-if="step === 3">
							<div class="flex flex-col gap-1.5">
								<h2 class="text-base font-semibold">
									{{ t("setup.step3.title") }}
								</h2>
								<p class="text-sm text-muted-foreground">
									{{ t("setup.step3.desc") }}
								</p>
							</div>
							<div class="flex flex-col gap-4">
								<div class="flex flex-col gap-1.5">
									<Label for="setup-apiProxy">{{
										t("settings.apiProxy.label")
									}}</Label>
									<Input
										id="setup-apiProxy"
										v-model="apiProxy"
										:placeholder="t('settings.apiProxy.placeholder')"
										autocomplete="off"
									/>
								</div>
								<div class="flex flex-col gap-1.5">
									<Label for="setup-scMirror">{{
										t("settings.scMirror.label")
									}}</Label>
									<Input
										id="setup-scMirror"
										v-model="scMirror"
										:placeholder="t('settings.scMirror.placeholder')"
										autocomplete="off"
									/>
								</div>
								<div class="flex flex-col gap-1.5">
									<Label for="setup-cdnProxy">{{
										t("settings.cdnProxy.label")
									}}</Label>
									<Input
										id="setup-cdnProxy"
										v-model="cdnProxy"
										:placeholder="t('settings.cdnProxy.placeholder')"
										autocomplete="off"
									/>
								</div>
							</div>
						</template>
					</div>
				</Transition>
			</div>

			<!-- 错误提示 / Error message -->
			<div v-if="error" class="view-error" role="alert">
				{{ error }}
				<Button v-if="!ready" variant="outline" size="sm" @click="initialize">{{
					t("ui.retry")
				}}</Button>
			</div>

			<!-- 导航按钮 / Navigation buttons -->
			<div class="flex items-center justify-between">
				<Button variant="ghost" :disabled="step === 1 || saving" @click="back">
					{{ t("setup.back") }}
				</Button>

				<div class="flex items-center gap-3">
					<Button
						v-if="step === 3"
						variant="ghost"
						:disabled="saving || !ready"
						@click="finish"
					>
						{{ t("setup.skip") }}
					</Button>
					<Button v-if="step < TOTAL_STEPS" :disabled="!canNext" @click="next">
						{{ t("setup.next") }}
					</Button>
					<Button
						v-if="step === TOTAL_STEPS"
						:disabled="saving || !ready"
						@click="finish"
					>
						{{ saving ? t("setup.saving") : t("setup.finish") }}
					</Button>
				</div>
			</div>
		</div>
	</div>
</template>
