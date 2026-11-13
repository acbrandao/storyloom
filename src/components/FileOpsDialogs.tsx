// Batch rename and folder organization. Both show a full preview computed by the Rust core,
// require explicit confirmation, run as journaled background jobs, and can be undone.
import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, CheckCircle2, Loader2 } from "lucide-react";
import { api, errorText } from "../api";
import { bumpStructure, setState, useStore } from "../store";
import type { ConflictPolicy, CoreEvent, OrganizeMode, OrganizePlan, PlanItem, RenamePlan } from "../types";
import { formatBytes } from "../format";
import { confirmDialog, Modal, toast } from "./ui";

const TOKENS: [string, string][] = [
  ["{capture_date}", "2026-07-14"], ["{capture_time}", "153245"], ["{year}", "2026"], ["{month}", "07"], ["{day}", "14"],
  ["{location}", "SandyHook"], ["{description}", "Windsurfing"], ["{camera}", "iPhone16Pro"], ["{original_name}", "IMG_0001"],
  ["{name}", "current name"], ["{rating}", "5"], ["{sequence}", "001"],
];

const TEMPLATE_PRESETS = [
  "{capture_date}_{capture_time}_{location}_{description}_{sequence}",
  "{capture_date}_{capture_time}_{original_name}",
  "{capture_date}_{location}_{sequence}",
  "{year}-{month}-{day}_{sequence:4}",
];

/** Resolve when a background job finishes; returns its final status. */
export function waitForJob(jobId: number): Promise<{ status: string; message: string | null }> {
  return new Promise((resolve) => {
    const un = listen<CoreEvent>("core-event", (e) => {
      const ev = e.payload;
      if (ev.kind === "jobProgress" && ev.jobId === jobId && ev.status !== "running") {
        un.then((f) => f());
        resolve({ status: ev.status, message: ev.message });
      }
    });
  });
}

function summarize(message: string | null) {
  try {
    const s = message ? JSON.parse(message) : null;
    return { done: s?.done ?? 0, failed: (s?.failed ?? []).length, warnings: (s?.timestampWarnings ?? []).length };
  } catch {
    return { done: 0, failed: 0, warnings: 0 };
  }
}

export async function runUndo(batchId: string) {
  try {
    const job = await api.undoBatch(batchId);
    const r = await waitForJob(job);
    let detail = "";
    try {
      const s = r.message ? JSON.parse(r.message) : null;
      detail = s ? ` — ${s.undone} restored${s.skipped?.length ? `, ${s.skipped.length} left as they are` : ""}` : "";
    } catch { /* ignore */ }
    toast(r.status === "completed" ? `Undo finished${detail}` : `Undo ${r.status}: ${r.message ?? ""}`, r.status === "completed" ? "success" : "error");
    bumpStructure();
  } catch (e) {
    toast(errorText(e), "error");
  }
}

function PreviewTable({ items, base }: { items: PlanItem[]; base?: string }) {
  const show = (p: string) => (base && p.startsWith(base) ? p.slice(base.length + 1) : p.split("/").pop());
  return (
    <div className="card" style={{ maxHeight: 300, overflow: "auto", padding: 0 }}>
      <table className="data">
        <thead><tr><th>Current</th><th>New</th><th>Status</th></tr></thead>
        <tbody>
          {items.slice(0, 1000).map((i) => (
            <tr key={i.mediaId} style={i.status === "ready" ? undefined : { opacity: 0.6 }}>
              <td style={{ fontSize: 11.5, wordBreak: "break-all" }}>{i.from.split("/").pop()}</td>
              <td style={{ fontSize: 11.5, wordBreak: "break-all" }}>{i.status === "ready" || i.status === "conflict" ? show(i.to) : "—"}</td>
              <td style={{ fontSize: 11.5, whiteSpace: "nowrap" }}>
                {i.status === "ready" ? (i.message ? <span className="warn">{i.message}</span> : <span style={{ color: "var(--pick)" }}><CheckCircle2 size={11} /> ready</span>)
                  : i.status === "unchanged" ? <span className="subtle">{i.message ?? "no change"}</span>
                  : <span className="error-text" title={i.message ?? ""}><AlertTriangle size={11} /> {i.message ?? i.status}</span>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {items.length > 1000 && <div className="subtle" style={{ padding: 8 }}>…and {(items.length - 1000).toLocaleString()} more</div>}
    </div>
  );
}

export function RenameDialog({ ids, onClose }: { ids: number[]; onClose: () => void }) {
  const [template, setTemplate] = useState(() => localStorage.getItem("mc-rename-template") ?? TEMPLATE_PRESETS[0]);
  const [start, setStart] = useState(1);
  const [policy, setPolicy] = useState<ConflictPolicy>("skip");
  const [plan, setPlan] = useState<RenamePlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const t = setTimeout(async () => {
      try {
        setPlan(await api.planRename(ids, template, start, policy));
        setError(null);
      } catch (e) {
        setPlan(null);
        setError(errorText(e));
      }
    }, 250);
    return () => clearTimeout(t);
  }, [ids, template, start, policy]);

  const insert = (tok: string) => {
    const el = inputRef.current;
    if (!el) return setTemplate((t) => t + tok);
    const [a, b] = [el.selectionStart ?? template.length, el.selectionEnd ?? template.length];
    setTemplate(template.slice(0, a) + tok + template.slice(b));
    requestAnimationFrame(() => { el.focus(); el.setSelectionRange(a + tok.length, a + tok.length); });
  };

  const apply = async () => {
    if (!plan) return;
    const ok = await confirmDialog({
      title: `Rename ${plan.ready.toLocaleString()} file${plan.ready === 1 ? "" : "s"} on disk?`,
      message: <>The files themselves are renamed. Their content, capture dates and file dates are kept, the original names stay in the catalog, and you can undo this afterwards.{plan.conflicts > 0 && <><br /><br />{plan.conflicts} file(s) with conflicting names will be left unchanged.</>}</>,
      okLabel: "Rename Files",
    });
    if (!ok) return;
    setBusy(true);
    try {
      localStorage.setItem("mc-rename-template", template);
      const [job, batch] = await api.startRename(ids, template, start, policy);
      onClose();
      const r = await waitForJob(job);
      const s = summarize(r.message);
      bumpStructure();
      if (r.status === "completed") {
        toast(`Renamed ${s.done} file${s.done === 1 ? "" : "s"}${s.failed ? `; ${s.failed} could not be renamed` : ""}`, s.failed ? "error" : "success", { label: "Undo", onClick: () => runUndo(batch) });
      } else toast(`Rename ${r.status}: ${r.message ?? ""}`, "error");
    } catch (e) {
      toast(errorText(e), "error");
      setBusy(false);
    }
  };

  return (
    <Modal title={`Rename ${ids.length.toLocaleString()} item${ids.length === 1 ? "" : "s"}`} wide onClose={onClose} footer={
      <>
        <span className="subtle" style={{ marginRight: "auto" }}>{plan && `${plan.ready} ready · ${plan.unchanged} unchanged · ${plan.conflicts} conflicts${plan.missing ? ` · ${plan.missing} missing` : ""}`}</span>
        <button className="btn" onClick={onClose}>Cancel</button>
        <button className="btn primary" disabled={!plan || plan.ready === 0 || busy} onClick={apply}>{busy && <Loader2 size={14} className="spin" />}Rename…</button>
      </>
    }>
      <label className="field">
        <span>Name template</span>
        <input ref={inputRef} className="input" value={template} onChange={(e) => setTemplate(e.target.value)} onKeyDown={(e) => e.stopPropagation()} spellCheck={false} style={{ fontFamily: "ui-monospace, SF Mono, Menlo, monospace" }} />
        {error ? <span className="hint error-text">{error}</span> : <span className="hint">The file extension is kept automatically. Click a field to insert it. Numbers follow capture order.</span>}
      </label>
      <div className="row" style={{ gap: 6 }}>
        {TOKENS.map(([t, ex]) => <button key={t} className="btn small" title={`e.g. ${ex}`} onClick={() => insert(t)}>{t.slice(1, -1).replace("_", " ")}</button>)}
      </div>
      <div className="row" style={{ gap: 14 }}>
        <label className="field" style={{ flexDirection: "row", alignItems: "center", gap: 8 }}><span>Presets</span>
          <select className="select" value="" onChange={(e) => e.target.value && setTemplate(e.target.value)}>
            <option value="">Choose…</option>
            {TEMPLATE_PRESETS.map((p) => <option key={p} value={p}>{p}</option>)}
          </select>
        </label>
        <label className="field" style={{ flexDirection: "row", alignItems: "center", gap: 8 }}><span>Start numbers at</span>
          <input className="input" type="number" min={0} value={start} onChange={(e) => setStart(Math.max(0, Number(e.target.value) || 0))} onKeyDown={(e) => e.stopPropagation()} style={{ width: 80 }} />
        </label>
        <label className="field" style={{ flexDirection: "row", alignItems: "center", gap: 8 }}><span>If a name is taken</span>
          <select className="select" value={policy} onChange={(e) => setPolicy(e.target.value as ConflictPolicy)}>
            <option value="skip">Leave that file unchanged</option>
            <option value="addSuffix">Add a number (-2, -3…)</option>
          </select>
        </label>
      </div>
      {plan && <PreviewTable items={plan.items} />}
    </Modal>
  );
}

const SUBFOLDER_PRESETS: [string, string][] = [
  ["", "No subfolders"],
  ["{year}", "By year — 2026/"],
  ["{year}/{capture_date}", "By day — 2026/2026-07-14/"],
  ["{year}/{location}", "By place — 2026/SandyHook/"],
  ["{type}", "By type — photo/, video/"],
];

export function OrganizeDialog({ ids, onClose }: { ids: number[]; onClose: () => void }) {
  const [destination, setDestination] = useState(() => localStorage.getItem("mc-organize-dest") ?? "");
  const [subfolders, setSubfolders] = useState(() => localStorage.getItem("mc-organize-sub") ?? "{year}/{capture_date}");
  const [mode, setMode] = useState<OrganizeMode>("copy");
  const [policy, setPolicy] = useState<ConflictPolicy>("skip");
  const [plan, setPlan] = useState<OrganizePlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!destination) { setPlan(null); return; }
    const t = setTimeout(async () => {
      try {
        setPlan(await api.planOrganize(ids, destination, subfolders || null, mode, policy));
        setError(null);
      } catch (e) {
        setPlan(null);
        setError(errorText(e));
      }
    }, 250);
    return () => clearTimeout(t);
  }, [ids, destination, subfolders, mode, policy]);

  const lowSpace = plan && mode === "copy" && plan.freeBytes != null && plan.freeBytes < plan.totalBytes * 1.05;
  const crossDrive = useMemo(() => plan?.items.filter((i) => i.message?.includes("different drive")).length ?? 0, [plan]);

  const apply = async () => {
    if (!plan) return;
    const n = plan.ready;
    const ok = await confirmDialog(mode === "move" ? {
      title: `Move ${n.toLocaleString()} file${n === 1 ? "" : "s"}?`,
      message: <>The files will be moved into <b>{destination}</b>. Nothing is copied or deleted: each file is moved on the same drive, keeping its content and dates. You can undo this afterwards.</>,
      okLabel: "Move Files",
    } : {
      title: `Copy ${n.toLocaleString()} file${n === 1 ? "" : "s"} (${formatBytes(plan.totalBytes)})?`,
      message: <>Verified copies will be created in <b>{destination}</b> with their original dates. Originals are not changed. Ratings, tags and notes are carried over to the copies.</>,
      okLabel: "Copy Files",
    });
    if (!ok) return;
    setBusy(true);
    try {
      localStorage.setItem("mc-organize-dest", destination);
      localStorage.setItem("mc-organize-sub", subfolders);
      const [job, batch] = await api.startOrganize(ids, destination, subfolders || null, mode, policy);
      onClose();
      const r = await waitForJob(job);
      const s = summarize(r.message);
      bumpStructure();
      if (r.status === "completed") {
        const verb = mode === "move" ? "Moved" : "Copied";
        toast(`${verb} ${s.done} file${s.done === 1 ? "" : "s"}${s.failed ? `; ${s.failed} failed` : ""}${s.warnings ? `; ${s.warnings} date warning(s) — see Activity` : ""}`, s.failed ? "error" : "success", { label: "Undo", onClick: () => runUndo(batch) });
      } else toast(`${mode === "move" ? "Move" : "Copy"} ${r.status}: ${r.message ?? ""}`, "error");
    } catch (e) {
      toast(errorText(e), "error");
      setBusy(false);
    }
  };

  return (
    <Modal title={`Copy or move ${ids.length.toLocaleString()} item${ids.length === 1 ? "" : "s"} to a folder`} wide onClose={onClose} footer={
      <>
        <span className="subtle" style={{ marginRight: "auto" }}>{plan && `${plan.ready} ready · ${plan.skipped} skipped${mode === "copy" ? ` · ${formatBytes(plan.totalBytes)}` : ""}${plan.freeBytes != null ? ` · ${formatBytes(plan.freeBytes)} free` : ""}`}</span>
        <button className="btn" onClick={onClose}>Cancel</button>
        <button className="btn primary" disabled={!plan || plan.ready === 0 || busy || !!lowSpace} onClick={apply}>{busy && <Loader2 size={14} className="spin" />}{mode === "move" ? "Move…" : "Copy…"}</button>
      </>
    }>
      <div className="row" style={{ gap: 10 }}>
        <label className={`choice ${mode === "copy" ? "on" : ""}`} style={{ flex: 1 }}>
          <input type="radio" checked={mode === "copy"} onChange={() => setMode("copy")} />
          <div><b>Copy</b><br />Make verified copies; originals stay where they are.</div>
        </label>
        <label className={`choice ${mode === "move" ? "on" : ""}`} style={{ flex: 1 }}>
          <input type="radio" checked={mode === "move"} onChange={() => setMode("move")} />
          <div><b>Move</b><br />Relocate files on the same drive. Nothing is deleted.</div>
        </label>
      </div>
      <div className="field">
        <span>Destination folder</span>
        <div className="path-row">
          <div className="path" title={destination}><bdi>{destination || "No folder chosen"}</bdi></div>
          <button className="btn" onClick={async () => { const p = await open({ directory: true, title: "Choose destination folder" }); if (typeof p === "string") setDestination(p); }}>Choose…</button>
        </div>
      </div>
      <div className="row" style={{ gap: 14, alignItems: "flex-end" }}>
        <label className="field" style={{ flex: 1 }}>
          <span>Subfolders</span>
          <input className="input" value={subfolders} onChange={(e) => setSubfolders(e.target.value)} onKeyDown={(e) => e.stopPropagation()} placeholder="none" style={{ fontFamily: "ui-monospace, SF Mono, Menlo, monospace" }} />
        </label>
        <select className="select" value={SUBFOLDER_PRESETS.some(([v]) => v === subfolders) ? subfolders : "custom"} onChange={(e) => e.target.value !== "custom" && setSubfolders(e.target.value)}>
          {SUBFOLDER_PRESETS.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
          <option value="custom" disabled>Custom</option>
        </select>
        <select className="select" value={policy} onChange={(e) => setPolicy(e.target.value as ConflictPolicy)} title="If a file with the same name exists">
          <option value="skip">Skip existing names</option>
          <option value="addSuffix">Add a number to the name</option>
        </select>
      </div>
      {error && <div className="error-text">{error}</div>}
      {crossDrive > 0 && mode === "move" && <div className="warn"><AlertTriangle size={13} /> {crossDrive} file(s) are on a different drive and can only be copied.</div>}
      {lowSpace && <div className="error-text"><AlertTriangle size={13} /> Not enough free space on the destination drive.</div>}
      {plan && <PreviewTable items={plan.items} base={plan.destination} />}
    </Modal>
  );
}

/** Mounted once in App; opens whichever file-operation dialog the store requests. */
export function FileOpsDialogHost() {
  const dialog = useStore((s) => s.dialog);
  if (!dialog) return null;
  const close = () => setState({ dialog: null });
  return dialog.kind === "rename" ? <RenameDialog ids={dialog.ids} onClose={close} /> : <OrganizeDialog ids={dialog.ids} onClose={close} />;
}
