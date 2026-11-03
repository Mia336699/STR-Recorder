<!--
    主播列表页面 / Streamer List View

    展示所有被追踪主播的卡片网格，支持添加/移除主播、手动开始/停止录制、切换自动录制。
    页面挂载时初始化事件监听器并从后端加载主播列表。

    Displays a card grid of all tracked streamers, supporting add/remove streamers,
    manual start/stop recording, and toggling auto-record.
    Initializes event listeners and loads the streamer list from backend on mount.
-->
<script setup lang="ts">
import { onBeforeUnmount } from "vue";
import { livePreviews } from "@/composables/useLivePreviews";
onBeforeUnmount(() => { void livePreviews.stopAll(); });
	import { onMounted, ref, computed } from "vue";
	import { useStreamersStore } from "../stores/streamers";
	import type { StreamerEntry } from "../stores/streamers";
	import { useNotify } from "../composables/useNotify";
	import { useMergingStore } from "../stores/merging";
	import { usePpStatusStore } from "../stores/ppStatus";
	import { Search, LayoutGrid, Plus } from "@lucide/vue";
	import { matchesRoom, compareRooms, type RoomFilter } from "@/lib/rooms";
	import StreamerCard from "../components/StreamerCard.vue";
	import AddStreamerDialog from "../components/AddStreamerDialog.vue";
	import { Button } from "@/components/ui/button";
	import { useI18n } from "vue-i18n";

	const store = useStreamersStore();
	const mergingStore = useMergingStore();
	const ppStatusStore = usePpStatusStore();
	const { toast, confirm } = useNotify();
	const { t } = useI18n();
	/** 是否显示添加主播对话框 / Whether to show the add streamer dialog */
	const showAdd = ref(false);
    const query = ref("");
    const filter = ref<RoomFilter>("all");
    const order = ref("online");
    const compact = ref(false);
    const watching = computed(() => new Set([...livePreviews.entries].filter(([, entry]) => entry.state !== "error").map(([name]) => name)));
    const counts = computed(() => ({ all: store.streamers.length, online: store.streamers.filter(s => s.is_online).length, recording: store.streamers.filter(s => s.is_recording).length, watching: watching.value.size }));
    const filters: RoomFilter[] = ["all", "online", "recording", "watching"];
    const sorted = computed(() => [...store.streamers].sort((a,b) => compareRooms(a,b,order.value === "online")));
    const visible = (s: StreamerEntry) => matchesRoom(s, query.value, filter.value, watching.value);
    const hasMatches = computed(() => store.streamers.some(visible));
    function clearFilters() { query.value = ""; filter.value = "all"; }

	onMounted(async () => {
		store.initListeners();
		await store.fetchStreamers();
	});

	/**
	 * 处理移除主播操作，先弹出确认对话框。
	 * 删除前取消该主播所有正在进行的后处理任务，并清理合并队列状态。
	 *
	 * Handle remove streamer action with confirmation dialog.
	 * Cancels all in-progress post-processing tasks and clears merge queue state before removal.
	 *
	 * @param username - 主播用户名 / Streamer username
	 */
	async function handleRemove(username: string) {
		const ok = await confirm({
			title: t("home.remove.title"),
			message: `${t("home.remove.message", { username })}\n${t("ui.removeHint")}`,
			confirmText: t("home.remove.confirm"),
			danger: true,
		});
		if (!ok) return;
		try {
			// 取消并清理该主播的后处理任务 / Cancel and clear post-processing tasks for this streamer
			await ppStatusStore.cancelAndClearForUsername(username);
			// 清理该主播的合并队列状态 / Clear merge queue state for this streamer
			mergingStore.clearMergingForUsername(username);
			await store.removeStreamer(username);
			toast(t("home.remove.done", { username }), "success");
		} catch (e) {
			toast(String(e), "error");
		}
	}

	/**
	 * 处理手动开始录制操作。
	 * Handle manual start recording action.
	 *
	 * @param username - 主播用户名 / Streamer username
	 */
	async function handleStart(username: string) {
		try {
			await store.startRecording(username);
			toast(t("home.start.done", { username }), "success");
		} catch (e) {
			toast(String(e), "error");
		}
	}

	/**
	 * 处理自动录制开关切换。
	 * 若开启自动录制且主播当前可录制但未在录制，则立即开始录制。
	 *
	 * Handle auto-record toggle.
	 * If enabled and streamer is currently recordable but not recording, start recording immediately.
	 *
	 * @param username - 主播用户名 / Streamer username
	 * @param streamer - 主播数据对象 / Streamer data object
	 * @param enabled - 是否开启自动录制 / Whether to enable auto-record
	 */
	async function handleToggleAuto(
		username: string,
		streamer: StreamerEntry,
		enabled: boolean,
	) {
        if (enabled && streamer.is_recordable && !streamer.is_recording) {
            if (!await confirm({title:t("ui.enableAuto"), message:t("ui.enableAutoHint"), confirmText:t("ui.auto")})) return;
        }
		try {
			await store.setAutoRecord(username, enabled);
			if (enabled && streamer.is_recordable && !streamer.is_recording) {
				await store.startRecording(username);
				toast(t("home.start.autoStarted", { username }), "success");
			}
		} catch (e) {
			toast(String(e), "error");
		}
	}

	/**
	 * 处理停止录制操作，先弹出确认对话框。
	 * Handle stop recording action with confirmation dialog.
	 *
	 * @param username - 主播用户名 / Streamer username
	 */
	async function handleStop(username: string) {
		const ok = await confirm({
			title: t("home.stop.title"),
			message: t("home.stop.message", { username }),
			confirmText: t("home.stop.confirm"),
			danger: true,
		});
		if (!ok) return;
		try {
			await store.stopRecording(username);
			toast(t("home.stop.done", { username }), "info");
		} catch (e) {
			toast(String(e), "error");
		}
	}
</script>

<template>
  <div>
    <header class="page-heading"><div><div class="eyebrow">YOUR CHANNELS</div><h1>{{ t('home.title') }}</h1><p>{{ t('ui.homeCounts', {total:counts.all,online:counts.online,recording:counts.recording,watching:counts.watching}) }}</p></div><Button @click="showAdd = true"><Plus class="size-4" />{{ t('home.addStreamer').replace(/^\+\s*/, '') }}</Button></header>
    <div v-if="store.streamers.length" class="view-toolbar">
      <div class="filter-tabs" role="tablist" :aria-label="t('ui.roomState')"><button v-for="key in filters" :key="key" type="button" role="tab" :aria-selected="filter === key" class="filter-tab" :class="{active:filter === key}" @click="filter = key">{{ t(`ui.${key}`) }}<small>{{ counts[key] }}</small></button></div>
      <label class="toolbar-search"><Search class="size-4" /><input v-model="query" :aria-label="t('ui.search')" :placeholder="t('ui.search')" type="search" /></label>
      <select v-model="order" :aria-label="t('ui.sort')"><option value="online">{{ t('ui.onlineFirst') }}</option><option value="name">{{ t('ui.nameOrder') }}</option></select>
      <Button variant="outline" size="sm" :aria-pressed="compact" @click="compact = !compact"><LayoutGrid class="size-4" />{{ compact ? t('ui.standard') : t('ui.compact') }}</Button>
    </div>
    <div v-if="store.error" role="alert" class="view-error">{{ store.error }} <button class="underline ml-2" @click="store.fetchStreamers()">{{ t('ui.retry') }}</button></div>
    <div v-if="store.loading && !store.streamers.length" class="loading-grid" role="status" :aria-label="t('home.loadingStreamers')"><div v-for="i in 8" :key="i" class="loading-card"><div /><span /></div></div>
    <div v-else-if="!store.streamers.length" class="empty-state"><LayoutGrid /><h2>{{ t('home.noStreamers') }}</h2><Button @click="showAdd = true">{{ t('home.addFirst') }}</Button></div>
    <template v-else>
      <div class="room-grid" :class="{compact, 'watch-grid':filter === 'watching' && !compact}">
        <!-- Keep filtered cards mounted: filtering must not stop active previews. -->
        <StreamerCard v-for="s in sorted" v-show="visible(s)" :key="s.username" :streamer="s" @remove="handleRemove(s.username)" @toggle-auto="handleToggleAuto(s.username,s,$event)" @start="handleStart(s.username)" @stop="handleStop(s.username)" />
      </div>
      <div v-if="!hasMatches" class="empty-state"><Search /><h2>{{ t('ui.noMatch') }}</h2><p>{{ t('ui.noMatchHint') }}</p><Button variant="outline" @click="clearFilters">{{ t('ui.clearFilters') }}</Button></div>
      <footer class="page-note"><span>{{ t('ui.stale') }}</span><span>{{ t('ui.previewHint') }}</span></footer>
    </template>
    <AddStreamerDialog v-if="showAdd" @close="showAdd = false" @added="showAdd = false" />
  </div>
</template>
