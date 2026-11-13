export function formatBytes(n: number): string {
  if (!n) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(1000)));
  const v = n / Math.pow(1000, i);
  return `${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function formatDuration(sec: number | null | undefined, long = false): string {
  if (sec == null || !isFinite(sec)) return "";
  const s = Math.floor(sec % 60);
  const m = Math.floor((sec / 60) % 60);
  const h = Math.floor(sec / 3600);
  if (long && h > 0) return `${h} h ${m} min`;
  if (long && m > 0) return `${m} min ${s} s`;
  if (long) return `${s} s`;
  const pad = (x: number) => String(x).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

/** Long total durations for the dashboard ("12 h 30 min"). */
export function formatHours(sec: number): string {
  if (sec < 60) return `${Math.round(sec)} s`;
  const h = Math.floor(sec / 3600);
  const m = Math.round((sec % 3600) / 60);
  return h > 0 ? `${h} h ${m} min` : `${m} min`;
}

export function formatTimecode(sec: number, fps = 0): string {
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec / 60) % 60);
  const s = Math.floor(sec % 60);
  const pad = (x: number) => String(x).padStart(2, "0");
  const frac = fps > 0 ? `:${pad(Math.floor((sec % 1) * fps))}` : `.${String(Math.floor((sec % 1) * 1000)).padStart(3, "0")}`;
  return `${pad(h)}:${pad(m)}:${pad(s)}${frac}`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** Catalog local date-time "YYYY-MM-DDTHH:MM:SS[.fff]" → "14 Jul 2026, 15:32". Never re-zoned. */
export function formatLocal(dt: string | null | undefined, withSeconds = false): string {
  if (!dt) return "";
  const m = dt.match(/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})/);
  if (!m) return dt;
  const [, y, mo, d, h, mi, s] = m;
  return `${Number(d)} ${MONTHS[Number(mo) - 1]} ${y}, ${h}:${mi}${withSeconds ? `:${s}` : ""}`;
}

export function formatShortDate(dt: string | null | undefined): string {
  if (!dt) return "";
  const m = dt.match(/^(\d{4})-(\d{2})-(\d{2})/);
  if (!m) return dt;
  return `${Number(m[3])} ${MONTHS[Number(m[2]) - 1]} ${m[1]}`;
}

export function formatMs(ms: number | null | undefined): string {
  if (ms == null) return "—";
  return new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "medium" });
}

export function formatCount(n: number): string {
  return n.toLocaleString();
}

export function plural(n: number, word: string, pluralWord = word + "s") {
  return `${n.toLocaleString()} ${n === 1 ? word : pluralWord}`;
}

export const CAPTURE_SOURCE_LABEL: Record<string, string> = {
  exif_original: "Camera (when the photo was taken)",
  exif_digitized: "Camera (digitized time)",
  exif_datetime: "Camera (file date)",
  quicktime_creationdate: "Device local time",
  container_utc: "Video file (UTC, shown in your time zone)",
  container_local: "Video file (camera local time)",
  filename: "Read from the file name",
  derived_clip: "Clip: original's capture time + in-point",
};
