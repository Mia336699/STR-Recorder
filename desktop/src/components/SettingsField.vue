<script setup lang="ts">
defineProps<{ id?: string; label: string; hint?: string }>();
</script>
<template>
	<div class="settings-field">
		<label :for="id">{{ label }}</label
		><slot /><small v-if="hint">{{ hint }}</small>
	</div>
</template>
