import type { RecordingFile } from "../types/recordings";

export interface RecordingGroup {
	key: string;
	username: string;
	platform: "Stripchat" | "Chaturbate";
	started_at: string;
	files: RecordingFile[];
	totalSize: number;
	hasRecording: boolean;
}
export function recordingName(file: RecordingFile) {
	const stem = file.name.replace(/\.[^.]+$/, "");
	const current = /^(.*)_(\d{8})_(\d{6})_part(\d{2})$/.exec(stem);
	const legacy = /^(.*?)(?:_part\d+)?_(\d{8})_(\d{6})$/.exec(stem);
	const parsed = current || legacy;
	const stamp = parsed ? `${parsed[2]}_${parsed[3]}` : null;
	const username = parsed?.[1] ?? stem;
	return {
		username: username.replace(/^chaturbate~/, ""),
		stamp,
		part: file.part_index ?? (current ? Number(current[4]) : null),
	};
}
function filenameTime(stamp: string | null): string | null {
	if (!stamp) return null;
	const [d, t] = stamp.split("_");
	const date = new Date(
		Number(d.slice(0, 4)),
		Number(d.slice(4, 6)) - 1,
		Number(d.slice(6, 8)),
		Number(t.slice(0, 2)),
		Number(t.slice(2, 4)),
		Number(t.slice(4, 6)),
	);
	return Number.isFinite(date.getTime()) ? date.toISOString() : null;
}
export function recordingGroupInfo(file: RecordingFile) {
	const parsed = recordingName(file);
	const parent = file.path
		.replace(/\\/g, "/")
		.split("/")
		.slice(0, -1)
		.join("/");
	const platform =
		/(?:^|\/)chaturbate(?:\/|~)/i.test(parent) ||
		file.name.startsWith("chaturbate~")
			? ("Chaturbate" as const)
			: ("Stripchat" as const);
	const groupTime =
		file.group_started_at && Number.isFinite(Date.parse(file.group_started_at))
			? file.group_started_at
			: filenameTime(parsed.stamp) || file.started_at;
	// New names share a fixed timestamp; legacy names remain separate sessions.
	const groupToken =
		parsed.part != null
			? parsed.stamp || groupTime
			: file.name.replace(/\.[^.]+$/, "");
	return {
		key: `${parent}\n${parsed.username}\n${groupToken}`,
		username: parsed.username,
		platform,
		started_at: groupTime,
	};
}
export function compareParts(a: RecordingFile, b: RecordingFile) {
	const ap = recordingName(a).part,
		bp = recordingName(b).part;
	return (
		(ap != null && bp != null ? ap - bp : 0) ||
		(Date.parse(a.started_at) || 0) - (Date.parse(b.started_at) || 0) ||
		a.name.localeCompare(b.name)
	);
}
export function groupRecordings(files: RecordingFile[]): RecordingGroup[] {
	const groups = new Map<string, RecordingGroup>();
	for (const file of files) {
		const info = recordingGroupInfo(file);
		let group = groups.get(info.key);
		if (!group) {
			group = { ...info, files: [], totalSize: 0, hasRecording: false };
			groups.set(info.key, group);
		}
		group.files.push(file);
		group.totalSize += file.size_bytes;
		group.hasRecording ||= file.is_recording;
	}
	return [...groups.values()]
		.map((group) => ({ ...group, files: group.files.sort(compareParts) }))
		.sort(
			(a, b) =>
				(Date.parse(b.started_at) || 0) - (Date.parse(a.started_at) || 0) ||
				a.key.localeCompare(b.key),
		);
}
