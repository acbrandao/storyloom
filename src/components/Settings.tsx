import { useEffect, useState } from "react";
import { CheckCircle2, FolderSearch, XCircle } from "lucide-react";
import { api, errorText } from "../api";
import { setState, useStore } from "../store";
import type { AppInfo } from "../types";
import { formatBytes, formatMs } from "../format";
import { confirmDialog, toast } from "./ui";

const SHORTCUTS: [string, string][] = [
  ["0 – 5", "Set star rating (0 clears)"],
  ["P", "Keep (pick)"],
  ["X", "Reject"],
  ["U", "Clear keep / reject"],
  ["R", "Toggle needs review"],
  ["F", "Toggle favorite"],
  ["6 / 7 / 8 / 9", "Red / yellow / green / blue label"],
  ["Space or Enter", "Open the large viewer"],
  ["← → ↑ ↓", "Move between items (Shift extends the selection)"],
  ["⌘A", "Select all"],
  ["Esc", "Close viewer / clear selection"],
  ["⌘F", "Search"],
  ["F2", "Rename the selected files"],
  ["⌘I", "Toggle the details panel"],
  ["+ / −", "Bigger / smaller thumbnails"],
  ["Viewer: , and .", "Previous / next video frame"],
  ["Viewer: [ and ]", "Slower / faster playback"],
];

export function SettingsView() {
  const theme = useStore((s) => s.theme);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [backups, setBackups] = useState<{ path: string; size: number; createdMs: number }[]>([]);
  const refresh = () => {
    api.appInfo().then(setInfo).catch(() => {});
    api.listBackups().then(setBackups).catch(() => {});
  };
  useEffect(refresh, []);

  const tool = (name: string, path: string | null | undefined, version: string | null | undefined, purpose: string, install: string) => (
    <tr>
      <td>{path ? <CheckCircle2 size={14} color="var(--pick)" /> : <XCircle size={14} color="var(--reject)" />} {name}</td>
      <td className="subtle">{purpose}</td>
      <td style={{ fontSize: 11.5 }}>{path ? <>{path}{version ? ` (${version})` : ""}</> : <span>Not found — install with <span className="kbd">{install}</span></span>}</td>
    </tr>
  );

  return (
    <div className="page" style={{ maxWidth: 980 }}>
      <h2>Appearance</h2>
      <div className="card">
        <div className="seg">
          {(["dark", "light", "system"] as const).map((t) => <button key={t} className={theme === t ? "on" : ""} onClick={() => setState({ theme: t })}>{t[0].toUpperCase() + t.slice(1)}</button>)}
        </div>
      </div>

      <h2>Media tools</h2>
      <div className="card">
        <table className="data">
          <tbody>
            {tool("FFmpeg", info?.tools.ffmpeg, info?.tools.ffmpegVersion, "Video thumbnails, audio artwork", "brew install ffmpeg")}
            {tool("FFprobe", info?.tools.ffprobe, null, "Video & audio details", "brew install ffmpeg")}
            {tool("ExifTool", info?.tools.exiftool, info?.tools.exiftoolVersion, "Optional: richer photo metadata (Canon CR3, maker notes, IPTC)", "brew install exiftool")}
          </tbody>
        </table>
        <p className="subtle" style={{ marginBottom: 0 }}>Without ExifTool, photo details are read by Media Curator’s built-in reader (JPEG, HEIC, TIFF and most RAW formats). After installing a tool, restart the app and use “Re-read all file details”.</p>
        <div className="row" style={{ marginTop: 10 }}>
          <button className="btn small" onClick={async () => {
            const ids = await api.mediaIds({}, "captureDate", true);
            if (await confirmDialog({ title: "Re-read all file details?", message: `Details and thumbnails for ${ids.length.toLocaleString()} items will be read again in the background. Your ratings, tags and notes are kept.`, okLabel: "Re-read" })) {
              await api.reprocess(ids); toast("Re-reading in the background…");
            }
          }}>Re-read all file details</button>
          <button className="btn small" onClick={async () => { await api.clearThumbnailCache(); setState((s) => ({ revision: s.revision + 1 })); toast("Thumbnail cache cleared; thumbnails will be recreated as you browse"); }}>Clear thumbnail cache</button>
        </div>
      </div>

      <h2>Catalog & backups</h2>
      <div className="card">
        <dl className="kv" style={{ gridTemplateColumns: "140px 1fr" }}>
          <dt>Catalog</dt><dd>{info?.databasePath} <button className="icon-btn" style={{ width: 20, height: 20 }} onClick={() => info && api.revealPath(info.databasePath)}><FolderSearch size={12} /></button></dd>
          <dt>Cache</dt><dd>{info?.cacheDir}</dd>
          <dt>Version</dt><dd>{info?.version}</dd>
        </dl>
        <p className="subtle">The catalog is backed up automatically once a day (the 20 most recent backups are kept). Backups are consistent snapshots taken while the app runs. See “Backup and recovery” in the documentation to restore one.</p>
        <div className="row">
          <button className="btn small" onClick={async () => { try { const p = await api.backupDatabase(); toast(`Backup saved: ${p.split("/").pop()}`, "success"); refresh(); } catch (e) { toast(errorText(e), "error"); } }}>Back up now</button>
          <button className="btn small" onClick={async () => { const r = await api.integrityCheck(); toast(r === "ok" ? "Catalog is healthy" : `Problem found: ${r}`, r === "ok" ? "success" : "error"); }}>Check catalog integrity</button>
        </div>
        {backups.length > 0 && (
          <table className="data" style={{ marginTop: 10 }}>
            <thead><tr><th>Backup</th><th className="num">Size</th><th>Created</th><th /></tr></thead>
            <tbody>
              {backups.slice(0, 8).map((b) => (
                <tr key={b.path}><td>{b.path.split("/").pop()}</td><td className="num">{formatBytes(b.size)}</td><td>{formatMs(b.createdMs)}</td>
                  <td><button className="icon-btn" style={{ width: 22, height: 22 }} onClick={() => api.revealPath(b.path)}><FolderSearch size={13} /></button></td></tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      <h2>Keyboard shortcuts</h2>
      <div className="card">
        <div className="shortcuts">
          {SHORTCUTS.flatMap(([k, v]) => [<span key={k}><span className="kbd">{k}</span></span>, <span key={k + "-d"}>{v}</span>])}
        </div>
      </div>
    </div>
  );
}
