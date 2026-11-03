<script setup lang="ts">
import {
	ref,
	reactive,
	computed,
	watch,
	onMounted,
	onUnmounted,
	onBeforeUnmount,
	nextTick,
} from "vue";
import { useI18n } from "vue-i18n";
import { call, on } from "@/lib/api";
import {
	useSettingsStore,
	type Settings,
	type MouflonKeysStore,
} from "@/stores/settings";
import { useLocalesStore } from "@/stores/locales";
import { useModuleLocaleStore } from "@/stores/moduleLocale";
import { loadLocaleFromServer } from "@/i18n";
import { useNotify } from "@/composables/useNotify";
import {
	appearance,
	setAppearance,
	type Appearance,
} from "@/composables/useAppearance";
import SettingsField from "@/components/SettingsField.vue";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
	FolderOpen,
	Eye,
	EyeOff,
	RefreshCw,
	Plus,
	Check,
	Loader2,
} from "@lucide/vue";
const store = useSettingsStore(),
	localesStore = useLocalesStore(),
	moduleLocaleStore = useModuleLocaleStore();
const { t, locale } = useI18n();
const { toast, confirm } = useNotify();
const pane = ref("recording");
const panes = ["general", "recording", "network", "platforms"];
const form = reactive<Settings>({ ...store.settings });
const ready = ref(false),
	loadError = ref(""),
	validationError = ref(""),
	choosing = ref(false),
	customDuration = ref(false);
const keyStore = ref<MouflonKeysStore>({
	keys: {},
	auto_synced_at: null,
	manual_updated_at: null,
});
const keyError = ref(""),
	keyBusy = ref(false),
	syncMessage = ref(""),
	newPkey = ref(""),
	newPdkey = ref("");
const visibleKeys = ref(new Set<string>());
const themeOptions: Appearance[] = ["system", "dark", "light"];
const keys = computed(() => Object.entries(keyStore.value.keys));
const errorMessage = computed(
	() => validationError.value || store.saveError || "",
);
const customShown = computed(
	() =>
		customDuration.value ||
		![0, 900, 1800, 3600].includes(form.max_recording_duration_secs),
);
const qualities = computed(() =>
	[
		...new Set([
			0,
			2160,
			1440,
			1080,
			720,
			480,
			360,
			240,
			160,
			form.preferred_resolution,
		]),
	].sort((a, b) => b - a),
);
let initialized = false;
let saveTimer: ReturnType<typeof setTimeout> | undefined;
let unlisten: (() => void) | undefined;
async function initialize() {
	loadError.value = "";
	ready.value = false;
	initialized = false;
	try {
		await store.initListeners();
		await store.fetchSettings();
		Object.assign(form, store.failedDraft ?? store.settings);
		if (!localesStore.loaded) await localesStore.refresh();
		await nextTick();
		ready.value = true;
		initialized = true;
		await loadKeys();
	} catch (e) {
		loadError.value = String(e);
	}
}
function flushBeforeExit() {
	if (saveTimer) {
		clearTimeout(saveTimer);
		saveTimer = undefined;
		void persist();
	}
}
onMounted(async () => {
	window.addEventListener("str-before-exit", flushBeforeExit);
	await initialize();
	unlisten = await on("mouflon-keys-updated", (payload) => {
		keyStore.value = payload as MouflonKeysStore;
		visibleKeys.value.clear();
	});
});
onBeforeUnmount(() => {
	if (saveTimer) {
		clearTimeout(saveTimer);
		saveTimer = undefined;
		void persist();
	}
});
onUnmounted(() => {
	window.removeEventListener("str-before-exit", flushBeforeExit);
	unlisten?.();
});
watch(pane, () => visibleKeys.value.clear());
watch(
	() => store.settings,
	async (value) => {
		if (!initialized || store.isSavingLocally) return;
		initialized = false;
		Object.assign(form, value);
		await nextTick();
		initialized = true;
	},
);
watch(
	() => [
		form.poll_interval_secs,
		form.auto_record,
		form.max_concurrent,
		form.max_recording_duration_secs,
		form.merge_format,
		form.preferred_resolution,
		form.resolution_preference,
		form.max_tmp_dir_gb,
	],
	() => {
		if (initialized) {
			clearTimeout(saveTimer);
			saveTimer = setTimeout(() => {
				saveTimer = undefined;
				void persist();
			}, 250);
		}
	},
);
function validatedSnapshot(): Settings | null {
	const value = { ...form };
	for (const key of [
		"api_proxy_url",
		"cdn_proxy_url",
		"sc_mirror_url",
		"mouflon_sync_url",
		"mouflon_sync_token",
	] as const)
		value[key] = value[key]?.trim() || null;
	for (const [key, min] of [
		["poll_interval_secs", 1],
		["max_concurrent", 0],
		["max_recording_duration_secs", 0],
		["preferred_resolution", 0],
		["max_tmp_dir_gb", 0],
	] as const) {
		if (!Number.isSafeInteger(value[key]) || value[key] < min) {
			validationError.value = t("ui.invalidSetting");
			return null;
		}
	}
	if (!value.output_dir.startsWith("/")) {
		validationError.value = t("ui.absoluteDirectory");
		return null;
	}
	for (const key of [
		"api_proxy_url",
		"cdn_proxy_url",
		"mouflon_sync_url",
	] as const) {
		if (!value[key]) continue;
		try {
			const url = new URL(value[key]);
			const protocols =
				key === "mouflon_sync_url"
					? ["http:", "https:"]
					: ["http:", "https:", "socks5:", "socks5h:"];
			if (!url.hostname || !protocols.includes(url.protocol)) throw new Error();
		} catch {
			validationError.value = t("ui.invalidNetwork");
			return null;
		}
	}
	if (
		value.sc_mirror_url &&
		!/^[a-z0-9.-]+(?::\d+)?$/i.test(value.sc_mirror_url)
	) {
		validationError.value = t("ui.invalidMirror");
		return null;
	}
	validationError.value = "";
	return value;
}
async function persist() {
	if (!initialized) return false;
	clearTimeout(saveTimer);
	const value = validatedSnapshot();
	if (!value) return false;
	try {
		await store.saveSettings(value);
		return true;
	} catch {
		return false;
	}
}
async function saveDirectory() {
	if (!ready.value || form.output_dir === store.settings.output_dir) return;
	const previous = store.settings.output_dir;
	const ok = await confirm({
		title: t("settings.outputDir.changeTitle"),
		message: t("settings.outputDir.changeMessage", { dir: form.output_dir }),
		confirmText: t("settings.outputDir.changeConfirm"),
	});
	if (ok) await persist();
	else form.output_dir = previous;
}
async function chooseDirectory() {
	choosing.value = true;
	try {
		const path = await call<string | null>("choose_recording_directory", {
			title: t("ui.chooseFolderTitle"),
		});
		if (path) {
			form.output_dir = path;
			await saveDirectory();
		}
	} catch (e) {
		validationError.value = String(e);
	} finally {
		choosing.value = false;
	}
}
async function changeLocale() {
	if (!(await persist())) return;
	try {
		const result = await loadLocaleFromServer(form.language);
		locale.value = form.language;
		moduleLocaleStore.setLocales(form.language, result.modules);
		if (result.warning)
			toast(
				t("settings.localeFileInvalid", { file: `${form.language}.json` }) +
					": " +
					result.warning,
				"warning",
			);
	} catch (e) {
		validationError.value = String(e);
	}
}
function changeAppearance(value: Appearance) {
	try {
		setAppearance(value);
	} catch (e) {
		validationError.value = String(e);
	}
}
function setDuration(seconds: number) {
	customDuration.value = false;
	form.max_recording_duration_secs = seconds;
}
async function loadKeys() {
	try {
		keyStore.value = await call<MouflonKeysStore>("list_mouflon_keys");
	} catch (e) {
		keyError.value = String(e);
	}
}
function reveal(key: string) {
	if (visibleKeys.value.has(key)) visibleKeys.value.delete(key);
	else visibleKeys.value.add(key);
}
async function addKey() {
	keyError.value = "";
	if (!newPkey.value.trim() || !newPdkey.value.trim()) {
		keyError.value = t("settings.keyError.empty");
		return;
	}
	keyBusy.value = true;
	try {
		await call("add_mouflon_key", {
			pkey: newPkey.value.trim(),
			pdkey: newPdkey.value.trim(),
		});
		newPkey.value = "";
		newPdkey.value = "";
		await loadKeys();
	} catch (e) {
		keyError.value = String(e);
	} finally {
		keyBusy.value = false;
	}
}
async function removeKey(pkey: string) {
	if (
		!(await confirm({
			title: t("common.delete"),
			message: t("ui.removeKeyHint"),
			confirmText: t("common.delete"),
			danger: true,
		}))
	)
		return;
	keyBusy.value = true;
	try {
		await call("remove_mouflon_key", { pkey });
		visibleKeys.value.delete(pkey);
		await loadKeys();
	} catch (e) {
		keyError.value = String(e);
	} finally {
		keyBusy.value = false;
	}
}
async function syncKeys() {
	if (!(await persist())) return;
	keyBusy.value = true;
	keyError.value = "";
	syncMessage.value = "";
	try {
		const changed = await call<boolean>("sync_mouflon_keys");
		await loadKeys();
		syncMessage.value = t(
			changed ? "settings.mouflonSyncDone" : "settings.mouflonSyncUpToDate",
		);
	} catch (e) {
		keyError.value = t("settings.mouflonSyncFailed", { error: String(e) });
	} finally {
		keyBusy.value = false;
	}
}
function date(value: string | null) {
	return value
		? new Date(value).toLocaleString(locale.value)
		: t("settings.mouflonNever");
}
</script>
<template>
	<div>
		<header class="page-heading">
			<div>
				<div class="eyebrow">PREFERENCES</div>
				<h1>{{ t("settings.title") }}</h1>
				<p>{{ t("ui.settingsHint") }}</p>
			</div>
			<span
				class="settings-save"
				:class="{ 'text-destructive': !!errorMessage }"
				role="status"
				><Loader2
					v-if="store.saveStatus === 'saving'"
					class="size-3.5 animate-spin"
				/><Check v-else-if="!errorMessage" class="size-3.5" />{{
					errorMessage
						? t("ui.saveFailed")
						: store.saveStatus === "saving"
							? t("ui.saving")
							: t("ui.saved")
				}}</span
			>
		</header>
		<div v-if="loadError" class="view-error" role="alert">
			{{ loadError }}
			<Button variant="outline" size="sm" @click="initialize">{{
				t("ui.retry")
			}}</Button>
		</div>
		<div v-if="errorMessage" class="view-error" role="alert">
			{{ errorMessage }}
			<Button variant="outline" size="sm" @click="persist">{{
				t("ui.retry")
			}}</Button>
		</div>
		<div v-if="!ready && !loadError" class="empty-state" role="status">
			{{ t("common.loading") }}
		</div>
		<div v-if="ready" class="settings-layout">
			<nav class="settings-navigation" :aria-label="t('settings.title')">
				<button
					v-for="key in panes"
					:key="key"
					type="button"
					:class="{ active: pane === key }"
					:aria-current="pane === key ? 'page' : undefined"
					@click="pane = key"
				>
					{{ t(`ui.${key === "recording" ? "recordingSettings" : key}`) }}
				</button>
			</nav>
			<div class="settings-content">
				<section v-if="pane === 'general'" class="settings-section">
					<h2>{{ t("ui.general") }}</h2>
					<SettingsField id="language" :label="t('settings.language.label')"
						><select
							id="language"
							v-model="form.language"
							@change="changeLocale"
						>
							<option
								v-for="item in localesStore.locales"
								:key="item.code"
								:value="item.code"
							>
								{{ item.name }}
							</option>
						</select></SettingsField
					><SettingsField :label="t('ui.appearance')"
						><div class="radio-choices">
							<button
								v-for="key in themeOptions"
								:key="key"
								type="button"
								:aria-pressed="appearance === key"
								:class="{ active: appearance === key }"
								@click="changeAppearance(key)"
							>
								{{ t(`ui.theme${key}`) }}
							</button>
						</div></SettingsField
					>
				</section>
				<section v-else-if="pane === 'recording'" class="settings-section">
					<h2>{{ t("ui.recordingSettings") }}</h2>
					<SettingsField
						id="output-dir"
						:label="t('settings.outputDir.label')"
						:hint="t('ui.directoryHint')"
						><div class="flex gap-2">
							<input
								id="output-dir"
								v-model="form.output_dir"
								class="min-w-0 flex-1"
								@blur="saveDirectory"
								@keydown.enter.prevent="saveDirectory"
							/><Button
								variant="outline"
								:disabled="choosing"
								@click="chooseDirectory"
								><FolderOpen class="size-4" />{{ t("ui.chooseFolder") }}</Button
							>
						</div></SettingsField
					>
					<SettingsField
						:label="t('ui.duration')"
						:hint="t('settings.recordingDuration.hint')"
						><div class="radio-choices">
							<button
								v-for="seconds in [0, 900, 1800, 3600]"
								:key="seconds"
								:class="{
									active:
										form.max_recording_duration_secs === seconds &&
										!customDuration,
								}"
								:aria-pressed="
									form.max_recording_duration_secs === seconds &&
									!customDuration
								"
								@click="setDuration(seconds)"
							>
								{{
									seconds
										? t("ui.minutes", { n: seconds / 60 })
										: t("ui.unlimited")
								}}</button
							><button
								:class="{ active: customShown }"
								:aria-pressed="customShown"
								@click="customDuration = true"
							>
								{{ t("ui.custom") }}
							</button>
						</div></SettingsField
					>
					<SettingsField
						v-if="customShown"
						id="duration-seconds"
						:label="t('settings.recordingDuration.label')"
						><input
							id="duration-seconds"
							v-model.number="form.max_recording_duration_secs"
							type="number"
							min="0"
							step="1"
					/></SettingsField>
					<div class="filename-example">
						<small>{{ t("ui.namePreview") }}</small
						><code
							>username_YYYYMMDD_HHMMSS_part01.mp4<br />username_YYYYMMDD_HHMMSS_part02.mp4</code
						>
					</div>
					<div class="settings-row">
						<SettingsField
							id="quality"
							:label="t('settings.preferredResolution.label')"
							><select id="quality" v-model.number="form.preferred_resolution">
								<option v-for="value in qualities" :key="value" :value="value">
									{{ value ? `${value}p` : t("ui.highestQuality") }}
								</option>
							</select></SettingsField
						><SettingsField
							id="merge-format"
							:label="t('settings.mergeFormat.label')"
							><select id="merge-format" v-model="form.merge_format">
								<option
									v-for="value in ['mp4', 'mkv', 'ts']"
									:key="value"
									:value="value"
								>
									{{ value.toUpperCase() }}
								</option>
							</select></SettingsField
						>
					</div>
					<SettingsField
						:label="t('ui.qualityFallback')"
						:hint="t('ui.qualityFallbackHint')"
						><div class="radio-choices">
							<button
								v-for="direction in ['lower', 'higher'] as const"
								:key="direction"
								:class="{ active: form.resolution_preference === direction }"
								:aria-pressed="form.resolution_preference === direction"
								@click="form.resolution_preference = direction"
							>
								{{
									t(
										direction === "lower"
											? "ui.lowerQuality"
											: "ui.higherQuality",
									)
								}}
							</button>
						</div></SettingsField
					>
					<div class="settings-row">
						<SettingsField
							id="max-concurrent"
							:label="t('settings.maxConcurrent.label')"
							:hint="t('settings.maxConcurrent.hint')"
							><input
								id="max-concurrent"
								v-model.number="form.max_concurrent"
								type="number"
								min="0"
								step="1" /></SettingsField
						><SettingsField
							id="poll-interval"
							:label="t('settings.pollInterval.label')"
							><input
								id="poll-interval"
								v-model.number="form.poll_interval_secs"
								type="number"
								min="1"
								step="1"
						/></SettingsField>
					</div>
					<SettingsField
						id="temp-limit"
						:label="t('settings.maxTmpDirGb.label')"
						:hint="t('settings.maxTmpDirGb.hint')"
						><input
							id="temp-limit"
							v-model.number="form.max_tmp_dir_gb"
							type="number"
							min="0"
							step="1"
					/></SettingsField>
					<div class="settings-switch-row">
						<div>
							<label for="global-auto">{{ t("ui.globalAuto") }}</label>
							<p>{{ t("ui.globalAutoHint") }}</p>
						</div>
						<Switch id="global-auto" v-model="form.auto_record" />
					</div>
					<div class="settings-hint">{{ t("ui.previewIndependent") }}</div>
				</section>
				<section v-else-if="pane === 'network'" class="settings-section">
					<h2>{{ t("ui.network") }}</h2>
					<SettingsField
						id="api-proxy"
						:label="t('ui.apiProxy')"
						:hint="t('ui.apiProxyHint')"
						><input
							id="api-proxy"
							v-model="form.api_proxy_url"
							placeholder="socks5://127.0.0.1:10808"
							@blur="persist"
							@keydown.enter.prevent="persist" /></SettingsField
					><SettingsField
						id="cdn-proxy"
						:label="t('ui.cdnProxy')"
						:hint="t('ui.cdnProxyHint')"
						><input
							id="cdn-proxy"
							v-model="form.cdn_proxy_url"
							placeholder="socks5://127.0.0.1:10808"
							@blur="persist"
							@keydown.enter.prevent="persist" /></SettingsField
					><SettingsField
						id="sc-mirror"
						:label="t('settings.scMirror.label')"
						:hint="t('settings.scMirror.hint')"
						><input
							id="sc-mirror"
							v-model="form.sc_mirror_url"
							placeholder="stripchat.example.com"
							@blur="persist"
							@keydown.enter.prevent="persist"
					/></SettingsField>
				</section>
				<section v-else class="settings-section">
					<h2>{{ t("ui.platforms") }}</h2>
					<div class="platform-summary">
						<span class="platform-mark">S</span><strong>Stripchat</strong
						><span class="subtle-tag ml-auto">{{
							t("ui.keysConfigured", { count: keys.length })
						}}</span>
					</div>
					<p class="text-xs text-muted-foreground leading-relaxed">
						{{ t("ui.keyHint") }}
					</p>
					<SettingsField
						id="sync-url"
						:label="t('settings.mouflonSyncUrl.label')"
						><input
							id="sync-url"
							v-model="form.mouflon_sync_url"
							placeholder="https://mouflon.chantrail.com"
							@blur="persist"
							@keydown.enter.prevent="persist" /></SettingsField
					><SettingsField
						id="sync-token"
						:label="t('settings.mouflonSyncToken.label')"
						><input
							id="sync-token"
							v-model="form.mouflon_sync_token"
							type="password"
							autocomplete="off"
							:placeholder="t('settings.mouflonSyncToken.placeholder')"
							@blur="persist"
							@keydown.enter.prevent="persist"
					/></SettingsField>
					<div class="flex items-center justify-between gap-3">
						<div class="text-xs text-muted-foreground space-y-1">
							<p>
								{{ t("settings.mouflonAutoSyncedAt")
								}}{{ date(keyStore.auto_synced_at) }}
							</p>
							<p>
								{{ t("settings.mouflonManualUpdatedAt")
								}}{{ date(keyStore.manual_updated_at) }}
							</p>
						</div>
						<Button
							variant="outline"
							:disabled="keyBusy || !form.mouflon_sync_url"
							@click="syncKeys"
							><RefreshCw
								class="size-4"
								:class="{ 'animate-spin': keyBusy }"
							/>{{
								t(keyBusy ? "settings.mouflonSyncing" : "settings.mouflonSync")
							}}</Button
						>
					</div>
					<p
						v-if="syncMessage"
						class="text-xs text-[var(--live)]"
						role="status"
					>
						{{ syncMessage }}
					</p>
					<p class="text-xs text-muted-foreground pt-3">
						{{ t("ui.keysHidden") }}
					</p>
					<div
						v-for="([pkey, pdkey], index) in keys"
						:key="pkey"
						class="key-configuration-row"
					>
						<span class="key-number">{{ index + 1 }}</span>
						<div class="key-values">
							<code>{{ visibleKeys.has(pkey) ? pkey : "•••• •••• ••••" }}</code
							><code>{{
								visibleKeys.has(pkey) ? pdkey : "•••• •••• ••••"
							}}</code>
						</div>
						<Button
							variant="ghost"
							size="icon"
							:aria-label="`${t(visibleKeys.has(pkey) ? 'ui.hideKey' : 'ui.revealKey')} ${index + 1}`"
							@click="reveal(pkey)"
							><EyeOff v-if="visibleKeys.has(pkey)" class="size-4" /><Eye
								v-else
								class="size-4" /></Button
						><Button
							variant="ghost"
							size="sm"
							class="text-destructive"
							:disabled="keyBusy"
							@click="removeKey(pkey)"
							>{{ t("common.delete") }}</Button
						>
					</div>
					<p v-if="!keys.length" class="text-xs text-muted-foreground">
						{{ t("settings.noKeys") }}
					</p>
					<div class="settings-row">
						<SettingsField id="new-pkey" label="pkey"
							><input
								id="new-pkey"
								v-model="newPkey"
								autocomplete="off" /></SettingsField
						><SettingsField id="new-pdkey" label="pdkey"
							><input
								id="new-pdkey"
								v-model="newPdkey"
								type="password"
								autocomplete="off"
						/></SettingsField>
					</div>
					<Button variant="outline" :disabled="keyBusy" @click="addKey"
						><Plus class="size-4" />{{ t("settings.addKey") }}</Button
					>
					<p v-if="keyError" class="text-xs text-destructive" role="alert">
						{{ keyError }}
					</p>
					<div class="platform-summary mt-5">
						<span class="platform-mark cb">C</span>
						<div>
							<strong>Chaturbate</strong>
							<p class="text-xs text-muted-foreground mt-1">
								{{ t("ui.chaturbateConfig") }}
							</p>
						</div>
					</div>
				</section>
			</div>
		</div>
	</div>
</template>
