<!--
    录制文件管理页面 / Recording File Management View

    展示所有录制文件，按主播分组，支持：
    - 实时录制时长计时和录制速度显示
    - 磁盘空间监控
    - 文件合并进度跟踪
    - 后处理流水线触发和进度显示
    - Contact Sheet 预览图查看（带缩放/平移）
    - 单文件和批量删除
    - 多列排序和分组折叠

    Displays all recording files grouped by streamer, supporting:
    - Real-time recording duration timer and recording speed display
    - Disk space monitoring
    - File merge progress tracking
    - Post-processing pipeline triggering and progress display
    - Contact Sheet preview image viewing (with zoom/pan)
    - Single and batch file deletion
    - Multi-column sorting and group collapsing
-->
<script setup lang="ts">
	import { onMounted, onUnmounted, computed, ref, watchEffect } from "vue";
	import { call, on } from "@/lib/api";
	import { useNotify } from "../composables/useNotify";
	import { usePostprocessStore } from "@/stores/postprocess";
	import { useRecordings } from "@/composables/useRecordings";
	import { usePostprocess, makePpProgress } from "@/composables/usePostprocess";
	import type { RecordingFile } from "@/types/recordings";
	import { recordingName, type RecordingGroup } from "@/lib/recordingGroups";
	import RecordingPlayer from "@/components/RecordingPlayer.vue";
	import RecordingDetails from "@/components/RecordingDetails.vue";
	import ActionMenu from "@/components/ActionMenu.vue";
	import { useImagePreview } from "@/composables/useImagePreview";
	import { Button } from "@/components/ui/button";
	import { Badge } from "@/components/ui/badge";
	import { Checkbox } from "@/components/ui/checkbox";
	import { Loader2, Image, Search, Play, FolderOpen, ChevronDown, ChevronRight } from "@lucide/vue";
	import { Progress } from "@/components/ui/progress";
	import {
		Dialog,
		DialogContent,
		DialogHeader,
		DialogTitle,
	} from "@/components/ui/dialog";
	import {
		Table,
		TableBody,
		TableCell,
		TableHead,
		TableHeader,
		TableRow,
	} from "@/components/ui/table";
	import { formatSize, formatDuration } from "@/utils/format";
	import { useI18n } from "vue-i18n";

	const { toast, confirm } = useNotify();
	const { t, locale } = useI18n();
	const ppStore = usePostprocessStore();
	/** 事件取消订阅函数列表 / Event unsubscribe function list */
	const unlisteners: (() => void)[] = [];
	/** 本地已发起删除的文件路径集合（用于过滤 recording-deleted 事件通知）/ Locally deleted paths (to filter recording-deleted notifications) */
	const localDeletedPaths = new Set<string>();
	/**
	 * 删除时正在后处理的文件路径集合。
	 * 与 localDeletedPaths 不同，此集合不在 recording-deleted 时清除，
	 * 而是等到 postprocess-done 事件处理完后才清除，确保能正确抑制后处理失败 toast。
	 *
	 * Paths that were being post-processed when deleted.
	 * Unlike localDeletedPaths, this set is NOT cleared on recording-deleted;
	 * it is cleared after postprocess-done is handled, so the failure toast is correctly suppressed.
	 */
	const ppCancelledByDelete = new Set<string>();

	/** 磁盘空间信息 / Disk space information */
	interface DiskSpace {
		total_bytes: number;
		available_bytes: number;
		used_bytes: number;
	}
	const diskSpace = ref<DiskSpace | null>(null);

	/**
	 * 从后端刷新磁盘空间信息。
	 * Refresh disk space information from the backend.
	 */
	async function refreshDiskSpace() {
		try {
			diskSpace.value = await call<DiskSpace>("get_disk_space");
		} catch {}
	}

	/** 各文件的实时录制速度（字节/秒）/ Real-time recording speed per file (bytes/second) */
	const recordingSpeed = ref<Record<string, number>>({});

	/** 合并进度（video_path -> {out_bytes, total_bytes}）/ Merge progress per video path */
	const mergeProgress = ref<Record<string, { out_bytes: number; total_bytes: number }>>({});

	/** 分片下载统计（video_path -> {downloaded, failed}）/ Segment download stats per video path */
	const segmentStats = ref<Record<string, { downloaded: number; failed: number }>>({});

	const rec = useRecordings();
	const {
		files,
		loading,
		elapsed,
		selected,
		selectedCount,
		collapsedGroups,
		groups,
		load,
		startTick,
		stopTick,
		scheduleDirRefresh,
		cleanup: recCleanup,
		toggleSort,
		sortIcon,
		toggleGroup,
		getFileChecked,
		setFileChecked,
		getGroupChecked,
		setGroupChecked,
		getAllChecked,
		setAllChecked,
	} = rec;

	const pp = usePostprocess();
	const {
		ppStatus,
		ppProgress,
		moduleOutputs,
		runPostprocess,
		restoreFromBackend,
		handlePostprocessDone,
		removeFile: ppRemoveFile,
	} = pp;

	/**
	 * 从当前 files.value 列表中同步模块输出路径到 moduleOutputs。
	 * 仅补充缺失的条目，不覆盖已有的（如推断值或实时更新值）。
	 * 用于 load() 之后确保 contact_sheet 等预览图按钮能正确显示。
	 *
	 * Sync module output paths from the current files.value list into moduleOutputs.
	 * Only fills in missing entries; does not overwrite existing ones (e.g. inferred or live-updated values).
	 * Called after load() to ensure preview buttons (e.g. contact_sheet) are correctly shown.
	 */
	function syncModuleOutputsFromFiles() {
		for (const f of files.value) {
			if (f.is_recording) continue;
			if (f.module_outputs && Object.keys(f.module_outputs).length > 0) {
				// meta 是持久化真相来源，直接覆盖写入（与 onMounted 初始化行为一致）
				// Meta is the persistent source of truth; overwrite directly (consistent with onMounted init)
				moduleOutputs.value[f.path] = {
					...moduleOutputs.value[f.path],
					...f.module_outputs,
				};
			}
		}
	}

	const preview = useImagePreview();
	const {
		previewOpen,
		previewUrl,
		previewTitle,
		previewScale,
		previewTranslate,
		previewViewportRef,
		previewImageRef,
		isDragging,
		viewportSize,
		resetPreviewTransform,
		onPreviewImageLoad,
		onPreviewWheel,
		onPreviewMousedown,
		onDocMousemove,
		onDocMouseup,
		openPreview,
	} = preview;

	/**
	 * 用系统默认程序打开录制文件。
	 * Open a recording file with the system default application.
	 */
	async function openFile(path: string) {
        const file=files.value.find(f=>f.path===path);
        if(file && canPlay(file)) playbackFiles.value=[file];
    }

	/**
	 * 打开模块输出文件（使用预览弹窗）。
	 * Tauri 版：通过 invoke 读取文件为 base64 data URL 后显示。
	 *
	 * Open module output file (preview dialog).
	 * Tauri version: reads file as base64 data URL via invoke.
	 */
	async function openModuleOutput(filePath: string, moduleId: string) {
		const outputPath = moduleOutputs.value[filePath]?.[moduleId];
		if (!outputPath) return;
		try {
			const result = await call<{ data: string }>("read_output_file", { path: outputPath });
			openPreview(result.data, outputPath.split(/[\\/]/).pop() ?? "预览图");
		} catch (e) {
			toast(String(e), "error");
		}
	}

	/**
	 * 删除单个录制文件（需要用户确认）。
	 * Delete a single recording file (requires user confirmation).
	 */
	async function deleteFile(f: {
		name: string;
		path: string;
		is_recording: boolean;
	}) {
		const ok = await confirm({
			title: t("recordings.delete.title"),
			message: `${t("recordings.delete.message", { name: f.name })}\n${t("ui.deleteRelated")}`,
			confirmText: t("recordings.delete.confirm"),
			danger: true,
		});
		if (!ok) return;
		try {
			if (ppStatus.value[f.path] === "running") {
				ppCancelledByDelete.add(f.path);
				await call("cancel_postprocess", { path: f.path }).catch(() => {});
			}
			localDeletedPaths.add(f.path);
			await call("delete_recording", { path: f.path });
			files.value = files.value.filter((r) => r.path !== f.path);
			delete elapsed.value[f.path];
			ppRemoveFile(f.path);
			selected.value.delete(f.path);
			toast(t("recordings.delete.done", { name: f.name }), "success");
		} catch (e) {
			localDeletedPaths.delete(f.path);
			toast(String(e), "error");
		}
	}

	/**
	 * 批量删除已选中的文件（需要用户确认）。
	 * Batch delete selected files (requires user confirmation).
	 */
	async function deleteSelected() {
		const paths = [...selected.value];
		const count = paths.length;
		const ok = await confirm({
			title: t("recordings.delete.batchTitle"),
			message: `${t("recordings.delete.batchMessage", { count })}\n${paths.map(p=>p.split(/[\\/]/).pop()).slice(0,12).join("\n")}\n${t("ui.deleteRelated")}`,
			confirmText: t("recordings.delete.confirm"),
			danger: true,
		});
		if (!ok) return;
		await Promise.all(
			paths
				.filter((p) => ppStatus.value[p] === "running")
				.map((p) => {
					ppCancelledByDelete.add(p);
					return call("cancel_postprocess", { path: p }).catch(() => {});
				}),
		);
		let failed = 0;
		for (const path of paths) {
			try {
				localDeletedPaths.add(path);
				await call("delete_recording", { path });
				files.value = files.value.filter((r) => r.path !== path);
				delete elapsed.value[path];
				ppRemoveFile(path);
				selected.value.delete(path);
			} catch {
				localDeletedPaths.delete(path);
				failed++;
			}
		}
		if (failed > 0)
			toast(t("recordings.delete.batchFailed", { count: failed }), "error");
		else toast(t("recordings.delete.batchDone", { count }), "success");
	}

	/**
	 * 对所有已选中且符合条件的文件批量触发后处理。
	 * 按录制开始时间排序，确保处理顺序一致。
	 *
	 * Trigger post-processing for all selected eligible files in batch.
	 * Sorted by recording start time to ensure consistent processing order.
	 */
	async function postProcessSelected() {
		const paths = [...selected.value].filter(
			(p) =>
				ppStatus.value[p] !== "running" &&
				ppStatus.value[p] !== "waiting" &&
				!files.value.find((f) => f.path === p)?.is_recording &&
				!files.value.find((f) => f.path === p)?.status?.startsWith("merging"),
		);
		if (paths.length === 0) return;
		selected.value.clear();
		paths.sort((a, b) => {
			const fa = files.value.find((f) => f.path === a);
			const fb = files.value.find((f) => f.path === b);
			return (
				new Date(fa?.started_at ?? 0).getTime() -
				new Date(fb?.started_at ?? 0).getTime()
			);
		});
		for (const path of paths) {
			await call("run_postprocess_cmd", { path }).catch((e) => {
				toast(String(e), "error");
			});
		}
	}

	/** 已选中文件中可触发后处理的数量 / Number of selected files eligible for post-processing */
	const ppSelectableCount = computed(
		() =>
			[...selected.value].filter(
				(p) =>
					ppStatus.value[p] !== "running" &&
					ppStatus.value[p] !== "waiting" &&
					!files.value.find((f) => f.path === p)?.is_recording &&
					!files.value.find((f) => f.path === p)?.status?.startsWith("merging"),
			).length,
	);

	/** 所有正在录制文件的总录制速度（字节/秒）/ Total recording speed (bytes/second) */
	const totalRecordingSpeed = computed(() =>
		Object.values(recordingSpeed.value).reduce((sum, s) => sum + s, 0),
	);

	/** 正在录制的文件数量 / Number of files currently recording */
	const recordingCount = computed(
		() => files.value.filter((f) => f.is_recording).length,
	);

	/** 磁盘使用率百分比 / Disk usage percentage */
	const diskUsedPct = computed(() => {
		if (!diskSpace.value || diskSpace.value.total_bytes === 0) return 0;
		return Math.min(
			100,
			(diskSpace.value.used_bytes / diskSpace.value.total_bytes) * 100,
		);
	});
	onMounted(async () => {
		document.addEventListener("mousemove", onDocMousemove);
		document.addEventListener("mouseup", onDocMouseup);

		await load();
		startTick();
		await refreshDiskSpace();
		const diskTimer = setInterval(refreshDiskSpace, 30_000);
		unlisteners.push(() => clearInterval(diskTimer));
		if (!ppStore.pipeline?.nodes?.length) await ppStore.fetchPipeline();

		// 先恢复运行中/等待中的后处理任务状态（来自内存，不依赖 meta）
		// First restore running/waiting post-processing task states (from memory, independent of meta)
		await restoreFromBackend();

		// 再从文件列表的 meta status 字段初始化 done/error 状态和模块输出路径。
		// meta 是持久化的真相来源，优先级高于推断值，直接覆盖写入。
		//
		// Then initialize status and module output paths from meta status fields in the file list.
		// Meta is the persistent source of truth and takes priority over inferred values.
		for (const f of files.value) {
			if (f.is_recording) continue;
			if (f.status === "finish") {
				ppStatus.value[f.path] = "done";
			} else if (f.status === "pp_error") {
				ppStatus.value[f.path] = "error";
			} else if (f.status === "pp_waiting") {
				if (ppStatus.value[f.path] !== "running") ppStatus.value[f.path] = "waiting";
			} else if (f.status === "pp_running") {
				ppStatus.value[f.path] = "running";
			}
			// 从 meta pp_results 恢复各模块执行结果，用于 done/error 状态下的详情展示
			// Restore per-module results from meta pp_results for detail display in done/error state
			if (f.pp_results && f.pp_results.length > 0 &&
				(f.status === "finish" || f.status === "pp_error")) {
				const results = f.pp_results.map((r) => ({
					moduleId: r.module_id,
					success: r.success,
					message: r.message,
				}));
				const allOk = results.every((r) => r.success);
				ppProgress.value[f.path] = {
					...makePpProgress(
						allOk ? results.length : 0,
						results.length,
						0, 0, "", allOk ? 100 : 0, "", 0,
						{ processing: t("usePostprocess.processing"), waiting: t("usePostprocess.waitingProgress") },
					),
					moduleResults: results,
				};
			}
			if (f.module_outputs && Object.keys(f.module_outputs).length > 0) {
				moduleOutputs.value[f.path] = f.module_outputs;
			}
		}

		unlisteners.push(
			await on("recordings-dir-changed", () => scheduleDirRefresh(syncModuleOutputsFromFiles)),
		);

		unlisteners.push(
			await on("sse-lagged", async () => {
				// SSE 广播队列溢出，事件已丢失，重新从后端恢复完整状态
				// SSE broadcast queue overflowed, events lost; restore full state from backend
				await load();
				// 先恢复运行中任务，再用 meta 覆盖 done/error 状态
				// First restore running tasks, then overwrite done/error status from meta
				await restoreFromBackend();
				for (const f of files.value) {
					if (f.is_recording) continue;
					if (f.status === "finish") {
						ppStatus.value[f.path] = "done";
					} else if (f.status === "pp_error") {
						ppStatus.value[f.path] = "error";
					} else if (f.status === "pp_waiting") {
						if (ppStatus.value[f.path] !== "running") ppStatus.value[f.path] = "waiting";
					} else if (f.status === "pp_running") {
						ppStatus.value[f.path] = "running";
					}
					if (f.pp_results && f.pp_results.length > 0 &&
						(f.status === "finish" || f.status === "pp_error")) {
						const results = f.pp_results.map((r) => ({
							moduleId: r.module_id,
							success: r.success,
							message: r.message,
						}));
						const allOk = results.every((r) => r.success);
						ppProgress.value[f.path] = {
							...makePpProgress(
								allOk ? results.length : 0,
								results.length,
								0, 0, "", allOk ? 100 : 0, "", 0,
								{ processing: t("usePostprocess.processing"), waiting: t("usePostprocess.waitingProgress") },
							),
							moduleResults: results,
						};
					}
					if (f.module_outputs && Object.keys(f.module_outputs).length > 0) {
						moduleOutputs.value[f.path] = f.module_outputs;
					}
				}
			}),
		);

		unlisteners.push(
			await on("recording-deleted", (payload) => {
				const p = payload as { path: string };
				const isLocal = localDeletedPaths.has(p.path);
				localDeletedPaths.delete(p.path);
				files.value = files.value.filter((r) => r.path !== p.path);
				delete elapsed.value[p.path];
				ppRemoveFile(p.path);
				selected.value.delete(p.path);
				if (!files.value.some((f) => f.is_recording)) stopTick();
				if (!isLocal) {
					const name = p.path.split(/[\\/]/).pop() ?? p.path;
					toast(t("recordings.otherClientDeleted", { name }), "info");
				}
			}),
		);

		unlisteners.push(
			await on("recording-file-update", async (payload) => {
				const p = payload as {
					path: string;
					size_bytes: number;
					speed_bps?: number;
					segments_downloaded?: number;
					segments_failed?: number;
				};
				// path is the video file path (from meta)
				const f = files.value.find((r) => r.path === p.path);
				if (f) {
					if (p.speed_bps != null && f.is_recording) {
						recordingSpeed.value = {
							...recordingSpeed.value,
							[p.path]: p.speed_bps,
						};
					} else if (!f.is_recording) {
						delete recordingSpeed.value[p.path];
					}
					f.size_bytes = p.size_bytes;
					// 更新分片统计 / Update segment stats
					if (f.is_recording && (p.segments_downloaded != null || p.segments_failed != null)) {
						segmentStats.value = {
							...segmentStats.value,
							[p.path]: {
								downloaded: p.segments_downloaded ?? segmentStats.value[p.path]?.downloaded ?? 0,
								failed: p.segments_failed ?? segmentStats.value[p.path]?.failed ?? 0,
							},
						};
					}
				} else {
					await load();
					startTick();
					syncModuleOutputsFromFiles();
				}
			}),
		);

		unlisteners.push(
			await on("recording-started", async () => {
				await load();
				startTick();
				syncModuleOutputsFromFiles();
			}),
		);

		unlisteners.push(
			await on("recording-stopped", async (payload) => {
				const p = payload as { video_path?: string };
				await load();
				syncModuleOutputsFromFiles();
				// 录制结束时清理速度数据 / Clean up recording speed when recording stops
				if (p.video_path) {
					const nextSpeed = { ...recordingSpeed.value };
					delete nextSpeed[p.video_path];
					recordingSpeed.value = nextSpeed;
				}
				// 合并完成后清理进度数据 / Clean up merge progress after merge completes
				if (p.video_path) {
					const next = { ...mergeProgress.value };
					delete next[p.video_path];
					mergeProgress.value = next;
				}
				// 录制结束时清理分片统计 / Clean up segment stats when recording stops
				if (p.video_path) {
					const nextStats = { ...segmentStats.value };
					delete nextStats[p.video_path];
					segmentStats.value = nextStats;
				}
			}),
		);

		unlisteners.push(
			await on("merge-progress", (payload) => {
				const p = payload as {
					video_path: string;
					out_bytes: number;
					total_bytes: number;
				};
				if (p.video_path) {
					mergeProgress.value = {
						...mergeProgress.value,
						[p.video_path]: { out_bytes: p.out_bytes, total_bytes: p.total_bytes },
					};
				}
			}),
		);

		unlisteners.push(
			await on("postprocess-waiting", (payload) => {
				const p = payload as { path: string };
				ppStatus.value[p.path] = "waiting";
			}),
		);

		unlisteners.push(
			await on("postprocess-started", (payload) => {
				const p = payload as { path: string };
				ppStatus.value[p.path] = "running";
				ppProgress.value[p.path] = makePpProgress(0, 0, 0, 0, "", 0, "", 0, {
					processing: t("usePostprocess.processing"),
					waiting: t("usePostprocess.waitingProgress"),
				});
			}),
		);

		unlisteners.push(
			await on("postprocess-progress", (payload) => {
				const p = payload as {
					path: string;
					done: number;
					total: number;
					pct: number;
					modDone: number;
					modTotal: number;
					moduleName: string;
				};
				const prev = ppProgress.value[p.path];
				ppProgress.value[p.path] = makePpProgress(
					p.done,
					p.total,
					p.modDone,
					p.modTotal,
					p.moduleName ?? "",
					p.pct,
					prev?.moduleName ?? "",
					prev?.modulePct ?? 0,
					{
						processing: t("usePostprocess.processing"),
						waiting: t("usePostprocess.waitingProgress"),
					},
				);
			}),
		);

		unlisteners.push(
			await on("postprocess-done", async (payload) => {
				const p = payload as {
					path: string;
					results: { moduleId: string; success: boolean; message: string }[];
				};
				const wasCancelledByDelete = ppCancelledByDelete.has(p.path);
				ppCancelledByDelete.delete(p.path);
				handlePostprocessDone(
					p,
					async () => {
						await load();
						syncModuleOutputsFromFiles();
					},
					() => wasCancelledByDelete,
				);
			}),
		);
	});

	onUnmounted(() => {
		document.removeEventListener("mousemove", onDocMousemove);
		document.removeEventListener("mouseup", onDocMouseup);
		recCleanup();
		unlisteners.forEach((fn) => fn());
	});

    const query=ref("");
    const fileFilter=ref("all");
    const playbackFiles=ref<RecordingFile[]>([]);
    const detailPath=ref<string|null>(null);
    const detailFile=computed(()=>files.value.find(f=>f.path===detailPath.value));
    const detailGroup=computed(()=>groups.value.find(g=>g.files.some(f=>f.path===detailPath.value)));
    function canPlay(f:RecordingFile){return !f.is_recording && !f.status?.startsWith('merging');}
    function canProcess(f:RecordingFile){return canPlay(f) && ppStatus.value[f.path] !== 'running' && ppStatus.value[f.path] !== 'waiting';}
    function processing(f:RecordingFile){return ['pp_waiting','pp_running'].includes(f.status || '') || ['waiting','running'].includes(ppStatus.value[f.path] || '');}
    const processingCount=computed(()=>files.value.filter(processing).length);
    const visibleGroups=computed(()=>groups.value.map(group=>{
        const list=group.files.filter(f=> (fileFilter.value === 'all' || fileFilter.value === 'recording' && f.is_recording || fileFilter.value === 'processing' && processing(f)) && `${group.platform} ${group.username} ${f.name}`.toLowerCase().includes(query.value.trim().toLowerCase()));
        return {...group,files:list,totalSize:list.reduce((sum,f)=>sum+f.size_bytes,0),hasRecording:list.some(f=>f.is_recording)};
    }).filter(g=>g.files.length));
    function playGroup(group:RecordingGroup){playbackFiles.value=group.files.filter(canPlay);}
    function playSelected(){playbackFiles.value=groups.value.flatMap(g=>g.files).filter(f=>selected.value.has(f.path)&&canPlay(f));}
    function date(value:string,timeOnly=false){const d=new Date(value);return Number.isFinite(d.getTime()) ? new Intl.DateTimeFormat(locale.value,timeOnly ? {hour:'2-digit',minute:'2-digit',second:'2-digit',hour12:false} : {year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hour12:false}).format(d) : t('ui.noData');}
    function status(f:RecordingFile){
        if(f.is_recording)return {label:t('recordings.status.recording'),tone:'recording'};
        if(f.status==='merging_waiting')return {label:t('recordings.status.waitingMerge'),tone:'warning'};
        if(f.status==='merging') {const p=mergeProgress.value[f.path];return {label:t('recordings.status.merging')+(p?.total_bytes ? ` ${Math.min(100,Math.round(p.out_bytes/p.total_bytes*100))}%` : ''),tone:'warning'};}
        const state=ppStatus.value[f.path];
        if(state==='running'||state==='waiting'||f.status==='pp_waiting'||f.status==='pp_running')return {label:ppProgress.value[f.path]?.overallLabel || t('ui.processing'),tone:'warning'};
        if(state==='error'||f.status==='pp_error')return {label:t('recordings.status.failed'),tone:'error'};
        if(f.status==='finish'||state==='done')return {label:t('recordings.status.done'),tone:'done'};
        return {label:t('ui.noData'),tone:'muted'};
    }
    function fileActions(f:RecordingFile){return [
        {key:'details',label:t('ui.details')},
        ...(moduleOutputs.value[f.path]?.contact_sheet ? [{key:'preview',label:t('ui.previewImage')}] : []),
        {key:'process',label:t('recordings.actions.postprocess'),disabled:!canProcess(f)},
        {key:'external',label:t('ui.external'),disabled:!canPlay(f)},
        {key:'reveal',label:t('ui.reveal'),disabled:!canPlay(f)},
        {key:'delete',label:t('recordings.actions.delete'),danger:true,disabled:!canPlay(f)},
    ];}
    async function fileAction(key:string,f:RecordingFile){
        if(key==='details')detailPath.value=f.path;
        else if(key==='preview')void openModuleOutput(f.path,'contact_sheet');
        else if(key==='process')void runPostprocess(f.path);
        else if(key==='delete')void deleteFile(f);
        else try {await call(key==='reveal'?'reveal_recording':'open_recording',{path:f.path});}catch(e){toast(String(e),'error');}
    }
    async function openDirectory(){try{await call('open_output_dir');}catch(e){toast(String(e),'error');}}
</script>

<template>
 <div>
		<Dialog :open="previewOpen" @update:open="previewOpen = $event">
			<DialogContent
				class="p-0 overflow-hidden flex flex-col w-fit"
				style="max-width: 90vw; max-height: 90vh"
			>
				<DialogHeader class="px-4 pt-4 pb-2 shrink-0">
					<DialogTitle class="text-sm font-mono truncate">{{
						previewTitle
					}}</DialogTitle>
				</DialogHeader>
				<div
					ref="previewViewportRef"
					class="relative overflow-hidden flex items-center justify-center bg-black/5 px-4 pb-4"
					:style="{
						width: viewportSize.width,
						height: viewportSize.height,
						cursor: isDragging
							? 'grabbing'
							: previewScale > 1
								? 'grab'
								: 'default',
					}"
					@wheel.prevent="onPreviewWheel"
					@mousedown="onPreviewMousedown"
				>
					<img
						ref="previewImageRef"
						:src="previewUrl"
						:alt="previewTitle"
						class="rounded select-none pointer-events-none"
						@load="onPreviewImageLoad"
						:style="{
							maxWidth: '100%',
							maxHeight: '100%',
							transform: `translate(${previewTranslate.x}px, ${previewTranslate.y}px) scale(${previewScale})`,
							transformOrigin: 'center center',
							transition: isDragging ? 'none' : 'transform 0.1s',
						}"
					/>
					<Transition name="fade">
						<Button
							v-if="previewScale !== 1"
							variant="secondary"
							size="sm"
							class="absolute bottom-5 left-1/2 -translate-x-1/2 z-10 rounded-full bg-black/60 hover:bg-black/80 text-white text-xs px-3 py-1.5 backdrop-blur-sm"
							@click="resetPreviewTransform"
						>
							{{
								t("recordings.resetZoom", {
									pct: Math.round(previewScale * 100),
								})
							}}
						</Button>
					</Transition>
				</div>
			</DialogContent>
		</Dialog>
  <header class="page-heading"><div><div class="eyebrow">RECORDING LIBRARY</div><h1>{{ t('recordings.title') }}</h1><p>{{ t('recordings.subtitle.total',{count:files.length}) }} · {{ t('ui.groups',{count:groups.length}) }}<span v-if="totalRecordingSpeed > 0"> · {{ formatSize(totalRecordingSpeed) }}/s</span></p></div><Button variant="outline" @click="openDirectory"><FolderOpen class="size-4" />{{ t('ui.openFolder') }}</Button></header>
  <div v-if="diskSpace && diskSpace.available_bytes < 5 * 1024 ** 3" class="detail-warning mb-4" role="status">{{ t('ui.available',{size:formatSize(diskSpace.available_bytes)}) }}</div>
  <div class="view-toolbar"><div class="filter-tabs" role="tablist" :aria-label="t('recordings.title')"><button v-for="key in ['all','recording','processing']" :key="key" type="button" role="tab" :aria-selected="fileFilter===key" class="filter-tab" :class="{active:fileFilter===key}" @click="fileFilter=key">{{ t(`ui.${key}`) }}<small v-if="key==='recording'">{{ recordingCount }}</small><small v-if="key==='processing'">{{ processingCount }}</small></button></div><label class="toolbar-search"><Search class="size-4" /><input v-model="query" type="search" :placeholder="t('ui.filesSearch')" :aria-label="t('ui.filesSearch')" /></label><span class="subtle-tag">{{ t('ui.fileOrder') }}</span></div>
  <div v-if="selectedCount" class="file-selection"><span>{{ t('recordings.subtitle.selected',{count:selectedCount}) }}</span><Button variant="ghost" size="sm" @click="selected.clear()">{{ t('ui.clearSelection') }}</Button><span class="flex-1" /><Button variant="outline" size="sm" @click="playSelected"><Play class="size-4" />{{ t('ui.playSelected') }}</Button><Button variant="outline" size="sm" :disabled="!ppSelectableCount" @click="postProcessSelected">{{ t('recordings.actions.postprocess') }}</Button><Button variant="destructive" size="sm" @click="deleteSelected">{{ t('recordings.actions.delete') }}</Button></div>
  <div v-if="loading && !files.length" class="empty-state" role="status"><Loader2 class="animate-spin" />{{ t('recordings.loading') }}</div>
  <div v-else-if="!files.length" class="empty-state"><FolderOpen /><h2>{{ t('recordings.empty') }}</h2></div>
  <template v-else>
   <section v-for="group in visibleGroups" :key="group.key" class="recording-group">
    <header class="recording-group-head"><Checkbox :aria-label="`${t('ui.selectGroup')} ${group.username}`" :model-value="getGroupChecked(group)" :disabled="!group.files.some(canPlay)" @update:model-value="setGroupChecked(group)" /><button class="collapse-control" :aria-expanded="!collapsedGroups.has(group.key)" :aria-label="`${t('ui.toggleGroup')} ${group.username}`" @click="toggleGroup(group.key)"><ChevronRight v-if="collapsedGroups.has(group.key)" class="size-4" /><ChevronDown v-else class="size-4" /></button><div class="recording-group-title"><strong>{{ group.username }} <span class="platform-mark" :class="{cb:group.platform==='Chaturbate'}">{{ group.platform[0] }}</span><span v-if="group.hasRecording" class="recording-pill">● {{ t('ui.recording') }}</span></strong><small>{{ t('ui.groupStarted',{time:date(group.started_at)}) }}</small></div><span class="recording-group-size">{{ t('ui.groupSize',{count:group.files.length,size:formatSize(group.totalSize)}) }}</span><Button variant="outline" size="sm" :disabled="!group.files.some(canPlay)" @click="playGroup(group)"><Play class="size-4" />{{ t('ui.sequential') }}</Button></header>
    <table v-if="!collapsedGroups.has(group.key)" class="files-table"><colgroup><col style="width:32px" /><col /><col style="width:84px" /><col style="width:64px" /><col style="width:72px" /><col style="width:100px" /><col style="width:80px" /></colgroup><thead><tr><th /><th>{{ t('recordings.table.filename') }}</th><th>{{ t('ui.actualStart') }}</th><th>{{ t('ui.durationShort') }}</th><th>{{ t('recordings.table.size') }}</th><th>{{ t('ui.fileState') }}</th><th>{{ t('recordings.table.actions') }}</th></tr></thead><tbody>
     <tr v-for="f in group.files" :key="f.path"><td><Checkbox :aria-label="`${t('ui.selectFile')} ${f.name}`" :model-value="getFileChecked(f.path)" :disabled="!canPlay(f)" @update:model-value="setFileChecked(f.path)" /></td><td><button class="file-name-button" :title="f.name" @click="detailPath=f.path"><span class="media-file-icon">{{ f.name.split('.').pop()?.toUpperCase() }}</span><span><strong>{{ f.name }}</strong><small v-if="recordingName(f).part != null">{{ t('ui.part',{n:String(recordingName(f).part).padStart(2,'0')}) }}</small><small v-else>{{ group.platform }}</small></span></button></td><td :title="date(f.started_at)">{{ date(f.started_at,true) }}<small v-if="new Date(f.started_at).toDateString() !== new Date(group.started_at).toDateString()" class="block text-[10px] text-muted-foreground">{{ new Date(f.started_at).toLocaleDateString(locale) }}</small></td><td>{{ f.is_recording ? formatDuration(elapsed[f.path] ?? f.record_duration_secs ?? 0) : f.video_duration_secs != null ? formatDuration(f.video_duration_secs) : '—' }}</td><td>{{ formatSize(f.size_bytes) }}</td><td><span class="file-status" :class="`tone-${status(f).tone}`">{{ status(f).label }}</span></td><td><div class="file-row-actions"><button type="button" :disabled="!canPlay(f)" :aria-label="`${t('recordings.actions.play')} ${f.name}`" :title="t('recordings.actions.play')" @click="openFile(f.path)"><Play class="size-3.5" /></button><ActionMenu :label="`${f.name} · ${t('ui.more')}`" :items="fileActions(f)" @select="fileAction($event,f)" /></div></td></tr>
    </tbody></table>
   </section>
   <div v-if="!visibleGroups.length" class="empty-state"><Search /><h2>{{ t('ui.noFilesMatch') }}</h2><Button variant="outline" @click="query='';fileFilter='all'">{{ t('ui.clearFilters') }}</Button></div>
   <footer class="page-note"><span>{{ t('ui.fileOrder') }}</span><span>{{ t('ui.detailsHint') }}</span></footer>
  </template>
  <RecordingDetails v-if="detailFile && detailGroup" :file="detailFile" :group="detailGroup" :stats="segmentStats[detailFile.path]" :speed="recordingSpeed[detailFile.path]" :outputs="moduleOutputs[detailFile.path]" :processing="status(detailFile).label" @close="detailPath=null" @play="playbackFiles=[$event];detailPath=null" @preview="openModuleOutput(detailFile.path,'contact_sheet')" />
  <RecordingPlayer v-if="playbackFiles.length" :files="playbackFiles" @close="playbackFiles=[]" />
 </div>
</template>
