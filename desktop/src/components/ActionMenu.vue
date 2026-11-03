<script setup lang="ts">
import { ref, nextTick } from "vue";
import { MoreHorizontal } from "@lucide/vue";
import { useI18n } from "vue-i18n";
import {
	DropdownMenuRoot,
	DropdownMenuTrigger,
	DropdownMenuPortal,
	DropdownMenuContent,
	DropdownMenuItem,
} from "reka-ui";
defineProps<{
	label?: string;
	items: { key: string; label: string; danger?: boolean; disabled?: boolean }[];
}>();
const emit = defineEmits<{ select: [key: string] }>();
const { t } = useI18n();
const open = ref(false);
async function select(key: string) {
	open.value = false;
	await nextTick();
	emit("select", key);
}
</script>
<template>
	<DropdownMenuRoot v-model:open="open"
		><DropdownMenuTrigger
			class="action-menu-trigger"
			:aria-label="label || t('ui.more')"
			:title="label || t('ui.more')"
			><MoreHorizontal class="size-4" /></DropdownMenuTrigger
		><DropdownMenuPortal
			><DropdownMenuContent
				class="action-menu-popup"
				align="end"
				:side-offset="5"
				:collision-padding="12"
				><DropdownMenuItem
					v-for="item in items"
					:key="item.key"
					:disabled="item.disabled"
					:class="{ 'text-destructive': item.danger }"
					@select="select(item.key)"
					>{{ item.label }}</DropdownMenuItem
				></DropdownMenuContent
			></DropdownMenuPortal
		></DropdownMenuRoot
	>
</template>
