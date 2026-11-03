<!--
    后处理流水线配置页面 / Post-processing Pipeline Configuration View

    提供可视化的流水线编辑器，支持：
    - 拖拽排序节点
    - 启用/禁用节点
    - 配置每个节点的参数（字符串、数字、布尔、下拉选择）
    - 从模块选择器添加新节点
    - 自动保存（防抖 600ms）

    Provides a visual pipeline editor supporting:
    - Drag-and-drop node reordering
    - Enable/disable nodes
    - Configure per-node parameters (string, number, boolean, select)
    - Add new nodes from a module picker
    - Auto-save with 600ms debounce
-->
<script setup lang="ts">
	import { onMounted, onBeforeUnmount, computed, ref, watch } from "vue";
	import { usePostprocessStore, type PipelineNode } from "@/stores/postprocess";
	import { useNotify } from "@/composables/useNotify";
	import { Button } from "@/components/ui/button";
	import { GripVertical, ChevronUp, ChevronDown, X } from "@lucide/vue";
	import { Switch } from "@/components/ui/switch";
	import { Input } from "@/components/ui/input";
	import { Label } from "@/components/ui/label";
	import { Badge } from "@/components/ui/badge";
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogHeader,
		DialogTitle,
	} from "@/components/ui/dialog";
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from "@/components/ui/select";
	import {
		NumberField,
		NumberFieldContent,
		NumberFieldDecrement,
		NumberFieldIncrement,
		NumberFieldInput,
	} from "@/components/ui/number-field";
	import PipelineNodeBox from "@/components/PipelineNodeBox.vue";
	import { useI18n } from "vue-i18n";

	const store = usePostprocessStore();
	const { toast } = useNotify();
	const { t } = useI18n();

	const selectedId = ref<string | null>(null);
	const ready = ref(false), loadError = ref("");
	async function initialize() {
		ready.value = false; loadError.value = "";
		try {
			await Promise.all([store.fetchModules(), store.saving || store.saveError ? Promise.resolve() : store.fetchPipeline()]);
			await store.initModuleWatcher(() => toast(t("postprocess.updatedByOther"), "info"));
			ready.value = true;
		} catch (error) { loadError.value = String(error); }
	}
	onMounted(initialize);
	onBeforeUnmount(() => store.flushPendingSave());
	watch(() => store.pipeline.nodes.map(node => node.nodeId), ids => {
		if (!selectedId.value || !ids.includes(selectedId.value)) selectedId.value = ids[0] ?? null;
	}, { immediate: true });
	const selected = computed(() => nodesWithInfo.value.filter(item => item.node.nodeId === selectedId.value));
	async function retrySave() { try { await store.savePipeline(); } catch {} }

	/** 当前可用模块 ID 集合 / Set of currently available module IDs */
	const availableModuleIds = computed(
		() => new Set(store.modules.map((m) => m.id)),
	);

	/**
	 * 判断节点对应的模块是否已不存在（模块文件被删除）。
	 * Check if a node's module is missing (module file was deleted).
	 */
	function isModuleMissing(node: PipelineNode) {
		return !availableModuleIds.value.has(node.moduleId);
	}

	// 拖拽排序状态 / Drag-and-drop sorting state
	/** 正在拖拽的节点索引 / Index of the node being dragged */
	const dragIndex = ref<number | null>(null);
	/** 拖拽悬停的目标节点索引 / Index of the node being dragged over */
	const dragOverIndex = ref<number | null>(null);
	/** 是否正在拖拽手柄（防止整个卡片触发拖拽）/ Whether dragging via handle (prevents card from triggering drag) */
	const isDraggingHandle = ref(false);

	/**
	 * 拖拽开始：记录源节点索引。
	 * Drag start: record source node index.
	 */
	function onDragStart(e: DragEvent, idx: number) {
		e.dataTransfer?.setData("text/plain", String(idx));
		dragIndex.value = idx;
	}

	/**
	 * 拖拽经过：更新悬停目标索引。
	 * Drag over: update hover target index.
	 */
	function onDragOver(e: DragEvent, idx: number) {
		e.preventDefault();
		dragOverIndex.value = idx;
	}

	/**
	 * 放置：执行节点位置交换。
	 * Drop: perform node position swap.
	 */
	function onDrop(e: DragEvent, idx: number) {
		e.preventDefault();
		if (dragIndex.value === null || dragIndex.value === idx) {
			dragIndex.value = null;
			dragOverIndex.value = null;
			return;
		}
		const nodes = [...store.pipeline.nodes];
		const [moved] = nodes.splice(dragIndex.value, 1);
		nodes.splice(idx, 0, moved);
		store.pipeline.nodes = nodes;
		dragIndex.value = null;
		dragOverIndex.value = null;
	}
	/**
	 * 拖拽结束：清理拖拽状态。
	 * Drag end: clean up drag state.
	 */
	function onDragEnd() {
		dragIndex.value = null;
		dragOverIndex.value = null;
		isDraggingHandle.value = false;
	}

	/** 是否显示模块选择器对话框 / Whether to show the module picker dialog */
	const showPicker = ref(false);

	/**
	 * 打开模块选择器（先刷新模块列表）。
	 * Open the module picker (refresh module list first).
	 */
	async function openPicker() {
		await store.fetchModules();
		showPicker.value = true;
	}

	/** 已在流水线中使用的模块 ID 集合（每个模块只能添加一次）/ Set of module IDs already in the pipeline (each module can only be added once) */
	const usedModuleIds = computed(
		() => new Set(store.pipeline.nodes.map((n) => n.moduleId)),
	);

	/** 尚未添加到流水线的可用模块列表 / Available modules not yet added to the pipeline */
	const availableModules = computed(() =>
		store.modules.filter((m) => !usedModuleIds.value.has(m.id)),
	);

	/**
	 * 将指定模块添加到流水线并关闭选择器。
	 * Add the specified module to the pipeline and close the picker.
	 *
	 * @param id - 模块 ID / Module ID
	 */
	function addModule(id: string) {
		store.addNode(id);
		selectedId.value = store.pipeline.nodes[store.pipeline.nodes.length - 1]?.nodeId ?? null;
		showPicker.value = false;
	}

	/** 已启用的节点列表（用于底部统计显示）/ List of enabled nodes (for bottom stats display) */
	const enabledNodes = computed(() =>
		store.pipeline.nodes.filter((n) => n.enabled),
	);

	/** 节点列表附带对应模块信息（用于模板渲染）/ Node list with corresponding module info (for template rendering) */
	const nodesWithInfo = computed(() =>
		store.pipeline.nodes.map((node) => ({
			node,
			info: store.modules.find((m) => m.id === node.moduleId),
		})),
	);
</script>

<template>
 <div class="pipeline-view">
  <header class="page-heading"><div><div class="eyebrow">WORKFLOW</div><h1>{{ t('postprocess.title') }}</h1><p>{{ t('postprocess.description') }}</p></div><span class="settings-save" role="status" :class="{'text-destructive':store.saveError}">{{ store.saveError ? t('ui.saveFailed') : store.saving ? t('ui.saving') : t('ui.saved') }}</span></header>
  <div v-if="loadError" class="view-error" role="alert">{{ loadError }} <Button variant="outline" size="sm" @click="initialize">{{ t('ui.retry') }}</Button></div>
  <div v-if="store.saveError" class="view-error" role="alert">{{ store.saveError }} <Button variant="outline" size="sm" @click="retrySave">{{ t('ui.retry') }}</Button></div>
  <div v-if="!ready && !loadError" class="empty-state" role="status">{{ t('common.loading') }}</div>
  <div v-if="ready" class="pipeline-workspace">
   <section class="pipeline-flow" :aria-label="t('postprocess.title')">
    <div class="pipeline-input"><span class="pipeline-step">↓</span><div><strong>{{ t('postprocess.input.label') }}</strong><p>{{ t('postprocess.input.description') }}</p></div></div>
    <div v-for="({node,info},idx) in nodesWithInfo" :key="node.nodeId" class="pipeline-node" :class="{'is-selected': selectedId===node.nodeId,'is-skipped':!node.enabled,'is-missing':isModuleMissing(node),'is-drop-target':dragOverIndex===idx && dragIndex!==idx}" :draggable="isDraggingHandle" @dragstart="onDragStart($event,idx)" @dragover="onDragOver($event,idx)" @drop="onDrop($event,idx)" @dragend="onDragEnd">
     <div class="pipeline-node-main"><span class="pipeline-grip" @mousedown="isDraggingHandle=true" @mouseup="isDraggingHandle=false"><GripVertical class="size-4" /></span><span class="pipeline-step">{{ String(idx+1).padStart(2,'0') }}</span><button class="pipeline-node-select" :aria-pressed="selectedId===node.nodeId" @click="selectedId=node.nodeId"><strong>{{ info?.name??node.moduleId }}</strong><small>{{ isModuleMissing(node)?t('postprocess.node.missing'):!node.enabled?t('ui.skipped'):t('ui.enabled') }}</small></button><Switch :model-value="node.enabled" :aria-label="`${t('ui.enabled')} ${info?.name??node.moduleId}`" @update:model-value="node.enabled=$event" /></div>
     <p>{{ info?.description }}</p>
     <div class="pipeline-node-actions"><Button variant="ghost" size="icon" :aria-label="t('ui.moveUp')" :disabled="idx===0" @click="store.moveNode(node.nodeId,'up')"><ChevronUp class="size-4" /></Button><Button variant="ghost" size="icon" :aria-label="t('ui.moveDown')" :disabled="idx===store.pipeline.nodes.length-1" @click="store.moveNode(node.nodeId,'down')"><ChevronDown class="size-4" /></Button><Button variant="ghost" size="icon" class="text-destructive" :aria-label="t('common.delete')" @click="store.removeNode(node.nodeId)"><X class="size-4" /></Button></div>
    </div>
    <p v-if="!store.pipeline.nodes.length" class="empty-state">{{ t('postprocess.empty') }}</p>
    <Button variant="outline" class="pipeline-add" @click="openPicker">{{ t('postprocess.addModule') }}</Button>
    <p class="pipeline-footnote">{{ t('postprocess.stats',{enabled:enabledNodes.length,total:store.pipeline.nodes.length}) }} · {{ t('ui.pipelineHint') }}</p>
   </section>
   <aside class="pipeline-inspector" :aria-label="t('ui.moduleParameters')">
    <header><div class="eyebrow">INSPECTOR</div><h2>{{ t('ui.moduleParameters') }}</h2></header>
    <template v-for="{node,info} in selected" :key="node.nodeId"><h3>{{ info?.name??node.moduleId }}</h3><p class="pipeline-description">{{ info?.description }}</p><p v-if="isModuleMissing(node)" class="view-error" role="alert">{{ t('postprocess.node.missing') }}</p>
					<div
						v-if="info?.params.length"
						class="pipeline-parameters"
					>
						<template
							v-for="param in info.params"
							:key="`${node.nodeId}__${param.key}`"
						>
							<div class="flex flex-col gap-1.5">
								<Label class="text-xs" :for="`${node.nodeId}__${param.key}`">{{ param.label }}</Label>

								<Switch
									v-if="param.type === 'boolean'"
 :id="`${node.nodeId}__${param.key}`" :aria-label="param.label"
									:model-value="
										node.params[param.key] === true ||
										node.params[param.key] === 'true'
									"
									@update:model-value="node.params[param.key] = $event"
								/>

								<Select
									v-else-if="param.type === 'select'"
									:model-value="String(node.params[param.key] ?? param.default)"
									@update:model-value="
										node.params[param.key] = String($event ?? param.default)
									"
								>
									<SelectTrigger :id="`${node.nodeId}__${param.key}`" :aria-label="param.label" size="sm" class="w-full">
										<SelectValue />
									</SelectTrigger>
									<SelectContent>
										<SelectItem
											v-for="opt in param.options"
											:key="opt"
											:value="opt"
										>
											{{ opt }}
										</SelectItem>
									</SelectContent>
								</Select>

								<NumberField
									v-else-if="param.type === 'number'"
									:model-value="Number(node.params[param.key] ?? param.default)"
									@update:model-value="node.params[param.key] = $event ?? 0"
								>
									<NumberFieldContent>
										<NumberFieldDecrement />
										<NumberFieldInput :id="`${node.nodeId}__${param.key}`" :aria-label="param.label" />
										<NumberFieldIncrement />
									</NumberFieldContent>
								</NumberField>

								<Input :id="`${node.nodeId}__${param.key}`" :aria-label="param.label"
									v-else
									:model-value="String(node.params[param.key] ?? param.default)"
									class="h-8 text-sm"
									@update:model-value="node.params[param.key] = String($event)"
								/>
							</div>
						</template>
					</div>
    <p v-if="info && !info.params.length" class="pipeline-description">{{ t('ui.noModuleParameters') }}</p></template>
    <p v-if="!selected.length" class="empty-state">{{ t('ui.selectModule') }}</p>
   </aside>
  </div>
		<Dialog :open="showPicker" @update:open="showPicker = $event">
			<DialogContent class="max-w-sm">
				<DialogHeader>
					<DialogTitle>{{ t("postprocess.picker.title") }}</DialogTitle>
					<DialogDescription class="sr-only">{{ t("postprocess.picker.description") }}</DialogDescription>
				</DialogHeader>
				<div class="flex flex-col gap-1 mt-1">
					<div
						v-if="availableModules.length === 0"
						class="text-sm text-muted-foreground px-2 py-4 text-center"
					>
						<template v-if="store.modules.length === 0">
							{{ t("postprocess.picker.noModules") }}<br />
							<span class="text-xs"
								>{{ t("postprocess.picker.noModulesHint") }}</span
							>
						</template>
						<template v-else> {{ t("postprocess.picker.allAdded") }} </template>
					</div>
					<Button
						v-for="mod in availableModules"
						:key="mod.id"
						variant="ghost"
						class="flex flex-col items-start px-3 py-2.5 rounded-lg text-left h-auto w-full whitespace-normal"
						@click="addModule(mod.id)"
					>
						<span class="text-sm font-medium">{{ mod.name }}</span>
						<span class="text-xs text-muted-foreground">{{
							mod.description
						}}</span>
					</Button>
				</div>
			</DialogContent>
		</Dialog>

 </div>
</template>
