export type RoomPlatform = "stripchat" | "chaturbate";
export function roomIdentity(input: string, platform: RoomPlatform = "stripchat", mirror?: string | null): string | null {
  const value = input.trim().toLowerCase();
  let name = value, cb = platform === "chaturbate";
  if (value.includes("://")) {
    try {
      const url = new URL(value);
      const host = url.hostname;
      cb = host === "chaturbate.com" || host.endsWith(".chaturbate.com");
      const mirrorHost = mirror ? new URL(`https://${mirror.replace(/\/+$/, "")}/`).hostname : null;
      const sc = host === "stripchat.com" || host.endsWith(".stripchat.com") || host === mirrorHost;
      if (url.protocol !== "https:" || url.username || url.password || url.port || (!cb && !sc)) return null;
      name = url.pathname.replace(/^\/+|\/+$/g, "");
    } catch { return null; }
  } else if (value.startsWith("chaturbate~")) { cb = true; name = value.slice(11); }
  if (!(cb ? /^[a-z0-9_]+$/ : /^[a-z0-9_-]+(?:@xh)?$/).test(name)) return null;
  return cb ? `chaturbate~${name}` : name;
}
export const roomDisplayName = (name: string) => name.replace(/^chaturbate~/, "");
export const roomPlatform = (name: string) => name.startsWith("chaturbate~") ? "Chaturbate" : "Stripchat";
export type RoomFilter = "all" | "online" | "recording" | "watching";
export interface RoomRow { username: string; is_online: boolean; is_recording: boolean }
export function matchesRoom(room: RoomRow, query: string, filter: RoomFilter, watching: ReadonlySet<string>): boolean {
  const matches = `${roomDisplayName(room.username)} ${roomPlatform(room.username)}`.toLowerCase().includes(query.trim().toLowerCase());
  return matches && (filter === "all" || filter === "online" && room.is_online || filter === "recording" && room.is_recording || filter === "watching" && watching.has(room.username));
}
export function compareRooms(a: RoomRow, b: RoomRow, onlineFirst: boolean): number {
  return (onlineFirst ? Number(b.is_online) - Number(a.is_online) : 0) || roomDisplayName(a.username).localeCompare(roomDisplayName(b.username)) || a.username.localeCompare(b.username);
}
