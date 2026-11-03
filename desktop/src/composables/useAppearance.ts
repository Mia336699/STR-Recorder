import { ref } from "vue";
export type Appearance = "system" | "dark" | "light";
let initial: Appearance = "system";
try {
	const value = localStorage.getItem("str:appearance");
	if (value === "dark" || value === "light") initial = value;
} catch {}
export const appearance = ref<Appearance>(initial);
export function applyAppearance(systemDark: boolean) {
	document.documentElement.classList.toggle(
		"dark",
		appearance.value === "dark" ||
			(appearance.value === "system" && systemDark),
	);
}
export function setAppearance(value: Appearance) {
	localStorage.setItem("str:appearance", value);
	appearance.value = value;
	applyAppearance(matchMedia("(prefers-color-scheme: dark)").matches);
}
