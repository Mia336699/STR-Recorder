/**
 * 录制文件管理 Composable / Recording File Management Composable
 *
 * 管理录制文件列表的加载、分组、排序、选择和计时功能。
 * 文件按主播用户名分组，支持多列排序，并为正在录制的文件提供实时计时器。
 * 所有状态（录制中/合并中/后处理等）均来自后端 meta 文件的 `status` 字段。
 *
 * Manages loading, grouping, sorting, selection, and timing of recording files.
 * Files are grouped by streamer username, support multi-column sorting,
 * and provide a real-time timer for actively recording files.
 * All status (recording/merging/post-processing etc.) comes from the `status` field in backend meta files.
 */

import { ref, computed } from "vue";
import { call } from "@/lib/api";
import type { RecordingFile } from "@/types/recordings";
import { groupRecordings, recordingName, type RecordingGroup } from "@/lib/recordingGroups";
import { ArrowUpDown, ArrowUp, ArrowDown } from "@lucide/vue";

/** 支持的排序字段 / Supported sort keys */
export type SortKey = "started_at" | "size_bytes" | "video_duration_secs";
/** 排序方向 / Sort direction */
export type SortDir = "asc" | "desc";

/** 按主播分组的录制文件组 / Recording file group by streamer */
export type Group = RecordingGroup;

/**
 * 从录制文件名中提取主播用户名。
 * 文件名格式为 `{username}_{YYYYMMDD}_{HHmmss}.ext`。
 *
 * Extract the streamer username from a recording filename.
 * Filename format: `{username}_{YYYYMMDD}_{HHmmss}.ext`
 */
export function usernameFromFile(f: RecordingFile): string {
    return recordingName(f).username;
}

/**
 * 录制文件列表状态与操作。
 */
export function useRecordings() {
	/** 所有录制文件列表 / All recording files */
	const files = ref<RecordingFile[]>([]);
	/** 是否正在加载 / Whether loading */
	const loading = ref(false);
	/** 各文件的已录制时长（秒，实时递增）/ Elapsed recording duration per file (seconds, increments in real-time) */
	const elapsed = ref<Record<string, number>>({});
	/** 已选中的文件路径集合 / Set of selected file paths */
	const selected = ref<Set<string>>(new Set());
	/** 已折叠的分组用户名集合 / Set of collapsed group usernames */
	const collapsedGroups = ref<Set<string>>(new Set());
	/** 当前排序字段 / Current sort key */
	const sortKey = ref<SortKey>("started_at");
	/** 当前排序方向 / Current sort direction */
	const sortDir = ref<SortDir>("desc");

	/** 计时器句柄：每秒递增录制时长 / Timer handle: increments recording duration every second */
	let tickTimer: ReturnType<typeof setInterval> | null = null;
	let groupedInitially = false;
	/** 计时器句柄：防抖刷新文件列表 / Timer handle: debounced file list refresh */
	let dirRefreshTimer: ReturnType<typeof setTimeout> | null = null;

	function toggleSort(key: SortKey) {
		if (sortKey.value === key) {
			sortDir.value = sortDir.value === "desc" ? "asc" : "desc";
		} else {
			sortKey.value = key;
			sortDir.value = "desc";
		}
	}

	function sortIcon(key: SortKey) {
		if (sortKey.value !== key) return ArrowUpDown;
		return sortDir.value === "desc" ? ArrowDown : ArrowUp;
	}

	/**
	 * 按主播分组的文件列表（计算属性）。
	 * 直接按文件名提取用户名分组，无需额外的合并状态过滤。
	 *
	 * Files grouped by streamer (computed property).
	 * Groups directly by username extracted from filename, no merging state filtering needed.
	 */
	const groups = computed<Group[]>(() => {
        const result = groupRecordings(files.value);
        if (sortKey.value !== "started_at") {
            result.sort((a,b) => {
                const value = (g: Group) => sortKey.value === "size_bytes" ? g.totalSize : g.files.reduce((sum,f) => sum + (f.video_duration_secs || 0),0);
                return sortDir.value === "desc" ? value(b)-value(a) : value(a)-value(b);
            });
        } else if (sortDir.value === "asc") result.reverse();
        return result;
    });

	const allSelectableFiles = computed(() =>
		files.value.filter(
			(f) =>
				!f.is_recording &&
				f.status !== "merging_waiting" &&
				f.status !== "merging",
		),
	);
	const selectedCount = computed(() => selected.value.size);

	function getFileChecked(path: string) {
		return selected.value.has(path);
	}

	function setFileChecked(path: string) {
		if (selected.value.has(path)) selected.value.delete(path);
		else selected.value.add(path);
	}

	function getGroupChecked(group: Group): boolean | "indeterminate" {
		const selectable = group.files.filter(
			(f) =>
				!f.is_recording &&
				f.status !== "merging_waiting" &&
				f.status !== "merging",
		);
		if (selectable.length === 0) return false;
		const n = selectable.filter((f) => selected.value.has(f.path)).length;
		if (n === 0) return false;
		if (n === selectable.length) return true;
		return "indeterminate";
	}

	function setGroupChecked(group: Group) {
		const selectable = group.files.filter(
			(f) =>
				!f.is_recording &&
				f.status !== "merging_waiting" &&
				f.status !== "merging",
		);
		const allSel = selectable.every((f) => selected.value.has(f.path));
		if (allSel) selectable.forEach((f) => selected.value.delete(f.path));
		else selectable.forEach((f) => selected.value.add(f.path));
	}

	function getAllChecked(): boolean | "indeterminate" {
		const selectable = allSelectableFiles.value;
		if (selectable.length === 0) return false;
		const n = selectable.filter((f) => selected.value.has(f.path)).length;
		if (n === 0) return false;
		if (n === selectable.length) return true;
		return "indeterminate";
	}

	function setAllChecked() {
		const selectable = allSelectableFiles.value;
		const allSel = selectable.every((f) => selected.value.has(f.path));
		if (allSel) selectable.forEach((f) => selected.value.delete(f.path));
		else selectable.forEach((f) => selected.value.add(f.path));
	}

	function toggleGroup(username: string) {
		if (collapsedGroups.value.has(username))
			collapsedGroups.value.delete(username);
		else collapsedGroups.value.add(username);
	}

	/**
	 * 从后端加载录制文件列表，并重建计时器状态。
	 * Load recording file list from backend and rebuild timer state.
	 */
	async function load() {
		loading.value = true;
		try {
			files.value = await call<RecordingFile[]>("list_recordings");
			if (!groupedInitially && files.value.length) { groupedInitially = true; collapsedGroups.value = new Set(groups.value.slice(1).filter(g => !g.hasRecording).map(g=>g.key)); }
			rebuildElapsed();
			const paths = new Set(files.value.map((f) => f.path));
			for (const p of selected.value) {
				if (!paths.has(p)) selected.value.delete(p);
			}
		} finally {
			loading.value = false;
		}
	}

	function rebuildElapsed() {
		const next: Record<string, number> = {};
		for (const f of files.value) {
			if (f.is_recording) {
				const current = elapsed.value[f.path] ?? 0;
				next[f.path] = Math.max(current, f.record_duration_secs ?? 0);
			}
		}
		elapsed.value = next;
	}

	function startTick() {
		if (tickTimer) return;
		tickTimer = setInterval(() => {
			for (const path of Object.keys(elapsed.value)) elapsed.value[path]++;
		}, 1000);
	}

	function stopTick() {
		if (tickTimer) {
			clearInterval(tickTimer);
			tickTimer = null;
		}
	}

	function scheduleDirRefresh(afterLoad?: () => void) {
		if (dirRefreshTimer) clearTimeout(dirRefreshTimer);
		dirRefreshTimer = setTimeout(async () => {
			dirRefreshTimer = null;
			await load();
			if (files.value.some((f) => f.is_recording)) startTick();
			else stopTick();
			afterLoad?.();
		}, 300);
	}

	function cleanup() {
		stopTick();
		if (dirRefreshTimer) {
			clearTimeout(dirRefreshTimer);
			dirRefreshTimer = null;
		}
	}

	return {
		files,
		loading,
		elapsed,
		selected,
		selectedCount,
		collapsedGroups,
		groups,
		load,
		rebuildElapsed,
		startTick,
		stopTick,
		scheduleDirRefresh,
		cleanup,
		toggleSort,
		sortIcon,
		toggleGroup,
		getFileChecked,
		setFileChecked,
		getGroupChecked,
		setGroupChecked,
		getAllChecked,
		setAllChecked,
	};
}
