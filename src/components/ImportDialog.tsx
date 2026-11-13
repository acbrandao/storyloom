// "Add Media": either index a folder where it is (read-only, default) or copy into a
// managed library with a preview of every destination before anything is written.
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Loader2 } from "lucide-react";
import { api, errorText } from "../api";
import { bumpStructure } from "../store";
import type { IngestLayout, IngestPlan } from "../types";
import { formatBytes } from "../format";
import { Modal, toast } from "./ui";

type Mode = "index" | "copy";

export function ImportDialog({ onClose, initialPath }: { onClose: () => void; initialPath?: string }) {
  const [mode, setMode] = useState<Mode>("index");
  const [source, setSource] = useState(initialPath ?? "");
  const [library, setLibrary] = useState("");
  const [layout, setLayout] = useState<IngestLayout>("byCaptureDate");
  const [plan, setPlan] = useState<IngestPlan | null>(null);
  const [renameOn, setRenameOn] = useState(false);
  const [renameTemplate, setRenameTemplate] = useState("{capture_date}_{capture_time}_{original_name}");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.getSetting("libraryPath").then((p) => p && setLibrary(p)).catch(() => {});
  }, []);
  useEffect(() => setPlan(null), [mode, source, library, layout, renameOn, renameTemplate]);
  useEffect(() => {
    api.getSetting("importRenameTemplate").then((t) => { if (t) { setRenameOn(true); setRenameTemplate(t); } }).catch(() => {});
  }, []);

  const pick = async (setter: (p: string) => void, title: string) => {
    const p = await open({ directory: true, multiple: false, title });
    if (typeof p === "string") setter(p);
  };

  const addFolder = async () => {
    setBusy(true);
    try {
      await api.addSource(source);
      bumpStructure();
      toast("Indexing started — you can browse while it runs", "success");
      onClose();
    } catch (e) {
      toast(errorText(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const preview = async () => {
    setBusy(true);
    try {
      await api.setSetting("libraryPath", library);
      await api.setSetting("importRenameTemplate", renameOn ? renameTemplate : "");
      setPlan(await api.planIngest(source, library, layout, renameOn ? renameTemplate : null));
    } catch (e) {
      toast(errorText(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const startCopy = async () => {
    if (!plan) return;
    setBusy(true);
    try {
      await api.startIngest(plan);
      bumpStructure();
      toast(`Copying ${plan.toCopy.toLocaleString()} files into your library…`, "success");
      onClose();
    } catch (e) {
      toast(errorText(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const lowSpace = plan && plan.freeBytes != null && plan.freeBytes < plan.totalBytes * 1.05;

  return (
    <Modal
      title="Add Media"
      wide={!!plan}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>Cancel</button>
          {mode === "index" ? (
            <button className="btn primary" disabled={!source || busy} onClick={addFolder}>{busy && <Loader2 size={14} className="spin" />}Add Folder</button>
          ) : plan ? (
            <button className="btn primary" disabled={busy || plan.toCopy === 0 || !!lowSpace} onClick={startCopy}>Copy {plan.toCopy.toLocaleString()} Files</button>
          ) : (
            <button className="btn primary" disabled={!source || !library || busy} onClick={preview}>{busy && <Loader2 size={14} className="spin" />}Preview Import…</button>
          )}
        </>
      }
    >
      <label className={`choice ${mode === "index" ? "on" : ""}`}>
        <input type="radio" checked={mode === "index"} onChange={() => setMode("index")} />
        <div><b>Add a folder (recommended)</b><br />Catalog media where it already is. Files are only read — nothing is copied, moved or changed.</div>
      </label>
      <label className={`choice ${mode === "copy" ? "on" : ""}`}>
        <input type="radio" checked={mode === "copy"} onChange={() => setMode("copy")} />
        <div><b>Import into my library</b><br />Copy media from a card, phone or folder into an organized library folder. Every copy is verified, original dates are preserved, and the source is left untouched.</div>
      </label>

      <div className="field">
        <span>{mode === "index" ? "Folder to add" : "Import from"}</span>
        <div className="path-row">
          <div className="path" title={source}><bdi>{source || "No folder chosen"}</bdi></div>
          <button className="btn" onClick={() => pick(setSource, "Choose a folder with media")}>Choose…</button>
        </div>
      </div>

      {mode === "copy" && (
        <>
          <div className="field">
            <span>Library folder</span>
            <div className="path-row">
              <div className="path" title={library}><bdi>{library || "No library folder chosen"}</bdi></div>
              <button className="btn" onClick={() => pick(setLibrary, "Choose your library folder")}>Choose…</button>
            </div>
          </div>
          <div className="field">
            <span>Organize copies</span>
            <select className="select" value={layout} onChange={(e) => setLayout(e.target.value as IngestLayout)}>
              <option value="byCaptureDate">By date taken — Library/2026/2026-07-14/</option>
              <option value="keepFolderStructure">Keep the original folder structure</option>
            </select>
            <span className="hint">Files without a capture date go into “Unknown Date” — the import date is never used as the capture date.</span>
          </div>
          <div className="field">
            <label style={{ display: "flex", gap: 8, alignItems: "center" }}><input type="checkbox" checked={renameOn} onChange={(e) => setRenameOn(e.target.checked)} />Rename the copies</label>
            {renameOn && (
              <>
                <input className="input" value={renameTemplate} onChange={(e) => setRenameTemplate(e.target.value)} onKeyDown={(e) => e.stopPropagation()} style={{ fontFamily: "ui-monospace, SF Mono, Menlo, monospace" }} />
                <span className="hint">Fields: {"{capture_date} {capture_time} {year} {month} {day} {camera} {original_name} {sequence}"}. Originals keep their names; the original name is remembered in the catalog.</span>
              </>
            )}
          </div>
        </>
      )}

      {plan && (
        <div className="field">
          <span>Preview</span>
          <div className="row" style={{ gap: 16 }}>
            <span><b>{plan.toCopy.toLocaleString()}</b> to copy ({formatBytes(plan.totalBytes)})</span>
            {plan.alreadyPresent > 0 && <span>{plan.alreadyPresent.toLocaleString()} already in the library (identical — skipped)</span>}
            {plan.freeBytes != null && <span className="subtle">{formatBytes(plan.freeBytes)} free</span>}
          </div>
          {lowSpace && <div className="error-text"><AlertTriangle size={13} /> Not enough free space on the library drive.</div>}
          <div className="card" style={{ maxHeight: 260, overflow: "auto", padding: 0 }}>
            <table className="data">
              <thead><tr><th>From</th><th>To</th><th className="num">Size</th></tr></thead>
              <tbody>
                {plan.items.slice(0, 500).map((i, k) => (
                  <tr key={k} style={i.alreadyPresent ? { opacity: 0.5 } : undefined}>
                    <td style={{ fontSize: 11.5 }}>{i.source.slice(plan.sourceDir.length + 1)}</td>
                    <td style={{ fontSize: 11.5 }}>{i.alreadyPresent ? "(already in library)" : i.destination.slice(plan.libraryDir.length + 1)}</td>
                    <td className="num">{formatBytes(i.size)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {plan.items.length > 500 && <div className="subtle" style={{ padding: 8 }}>…and {(plan.items.length - 500).toLocaleString()} more</div>}
          </div>
        </div>
      )}
    </Modal>
  );
}
