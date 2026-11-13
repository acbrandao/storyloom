import { useEffect, useState } from "react";
import { X, CheckCircle2, AlertTriangle, FolderSearch, Undo2 } from "lucide-react";
import { api, errorText } from "../api";
import { useStore } from "../store";
import type { BackgroundJob, BatchSummary, FileOperation, ScanIssue } from "../types";
import { runUndo } from "./FileOpsDialogs";
import { confirmDialog } from "./ui";
import { formatBytes, formatMs } from "../format";
import { toast } from "./ui";

function jobTitle(jobType: string) {
  if (jobType.startsWith("index")) return "Indexing";
  if (jobType === "ingest") return "Importing";
  if (jobType === "reprocess") return "Re-reading details";
  if (jobType === "rename") return "Renaming";
  if (jobType === "organize") return "Organizing";
  if (jobType === "undo") return "Undoing";
  if (jobType === "clip-export") return "Exporting clips";
  return jobType;
}

export function JobsPanel({ compact }: { compact?: boolean }) {
  const jobs = useStore((s) => s.jobs);
  if (!jobs.length) return compact ? <div className="subtle" style={{ padding: "4px 6px", display: "flex", gap: 6, alignItems: "center" }}><CheckCircle2 size={13} /> Library up to date</div> : null;
  return (
    <div className="jobs">
      {jobs.map((j) => {
        const pct = j.total > 0 ? Math.min(100, (j.done / j.total) * 100) : null;
        return (
          <div className="job" key={j.id}>
            <div className="row">
              <span className="phase">{jobTitle(j.jobType)} · {j.phase}</span>
              <button className="icon-btn" style={{ width: 20, height: 20 }} title="Cancel" onClick={() => api.cancelJob(j.id)}><X size={12} /></button>
            </div>
            <div className={`progress ${pct == null ? "indeterminate" : ""}`}><div style={pct != null ? { width: `${pct}%` } : undefined} /></div>
            <span className="msg">{j.message ?? (j.total > 0 ? `${j.done.toLocaleString()} of ${j.total.toLocaleString()}` : "Working…")}</span>
          </div>
        );
      })}
    </div>
  );
}

const SUMMARY_LABELS: Record<string, string> = {
  found: "files found", new: "new", changed: "changed", unchanged: "unchanged", missing: "missing", unsupported: "unsupported",
  inaccessible: "inaccessible", empty: "empty", metadataRead: "details read", metadataErrors: "unreadable", thumbnails: "thumbnails",
  thumbnailErrors: "thumbnail errors", hashed: "checked for duplicates", done: "done", undone: "restored", skipped: "skipped", copied: "copied", skippedIdentical: "already in library",
  failed: "failed", bytesCopied: "copied", timestampWarnings: "date warnings",
};

function summaryText(s: Record<string, unknown> | null) {
  if (!s) return "";
  return Object.entries(s)
    .filter(([k, v]) => typeof v === "number" && v > 0 && SUMMARY_LABELS[k])
    .map(([k, v]) => (k === "bytesCopied" ? `${formatBytes(v as number)} copied` : `${(v as number).toLocaleString()} ${SUMMARY_LABELS[k]}`))
    .join(" · ");
}

export function ActivityView() {
  const revision = useStore((s) => s.revision);
  const [jobs, setJobs] = useState<BackgroundJob[]>([]);
  const [ops, setOps] = useState<FileOperation[]>([]);
  const [batches, setBatches] = useState<BatchSummary[]>([]);
  const [issues, setIssues] = useState<{ jobId: number; list: ScanIssue[] } | null>(null);
  useEffect(() => {
    api.recentJobs(50).then(setJobs).catch(() => {});
    api.fileOperations(200).then(setOps).catch(() => {});
    api.operationBatches(50).then(setBatches).catch(() => {});
  }, [revision]);
  const showIssues = async (id: number) => {
    try { setIssues({ jobId: id, list: await api.jobIssues(id) }); } catch (e) { toast(errorText(e), "error"); }
  };
  return (
    <div className="page">
      <h2>Running now</h2>
      <JobsPanel />
      <h2>Recent activity</h2>
      <div className="card">
        <table className="data">
          <thead><tr><th>When</th><th>Task</th><th>Status</th><th>Result</th><th /></tr></thead>
          <tbody>
            {jobs.map((j) => (
              <tr key={j.id}>
                <td style={{ whiteSpace: "nowrap" }}>{formatMs(j.createdAt)}</td>
                <td>{jobTitle(j.jobType)}</td>
                <td><span className={`status-pill ${j.status}`}>{j.status}</span></td>
                <td style={{ color: "var(--text-2)" }}>{j.errorMessage ? <span className="error-text">{j.errorMessage}</span> : summaryText(j.summary)}</td>
                <td>{j.summary && ((j.summary.unsupported as number) > 0 || (j.summary.inaccessible as number) > 0 || (j.summary.empty as number) > 0) && <button className="btn small" onClick={() => showIssues(j.id)}>Problems</button>}</td>
              </tr>
            ))}
            {!jobs.length && <tr><td colSpan={5} className="subtle">Nothing yet.</td></tr>}
          </tbody>
        </table>
      </div>
      {issues && (
        <>
          <h2>Files that could not be added (job {issues.jobId})</h2>
          <div className="card">
            <table className="data">
              <thead><tr><th>File</th><th>Problem</th><th>Detail</th></tr></thead>
              <tbody>{issues.list.map((i, k) => <tr key={k}><td style={{ wordBreak: "break-all" }}>{i.path}</td><td>{i.issue}</td><td className="subtle">{i.detail}</td></tr>)}</tbody>
            </table>
          </div>
        </>
      )}
      <h2>File changes you can undo</h2>
      <div className="card">
        <table className="data">
          <thead><tr><th>When</th><th>Change</th><th className="num">Files</th><th>Status</th><th /></tr></thead>
          <tbody>
            {batches.map((b) => (
              <tr key={b.batchId}>
                <td style={{ whiteSpace: "nowrap" }}>{formatMs(b.createdAt)}</td>
                <td>{b.operationType === "rename" ? "Renamed files" : b.operationType === "move" ? "Moved files" : "Copied files"}</td>
                <td className="num">{(b.completed + b.undone + b.failed).toLocaleString()}</td>
                <td>
                  {b.completed > 0 && <span className="status-pill completed">{b.completed} applied</span>}{" "}
                  {b.undone > 0 && <span className="status-pill cancelled">{b.undone} undone</span>}{" "}
                  {b.failed > 0 && <span className="status-pill failed">{b.failed} failed</span>}
                </td>
                <td>{b.undoable && (
                  <button className="btn small" onClick={async () => {
                    const what = b.operationType === "copy" ? "The copies will be moved to the Trash (only copies that haven’t been changed since)." : "The files will be put back where they were, with their original names.";
                    if (await confirmDialog({ title: "Undo this change?", message: what, okLabel: "Undo" })) runUndo(b.batchId);
                  }}><Undo2 size={12} />Undo</button>
                )}</td>
              </tr>
            ))}
            {!batches.length && <tr><td colSpan={5} className="subtle">No renames, moves or copies yet.</td></tr>}
          </tbody>
        </table>
      </div>
      <h2>File operation journal</h2>
      <p className="subtle" style={{ marginTop: -4 }}>Every operation that writes to disk is recorded here before it starts, with verification results. Indexing never writes to your media folders.</p>
      <div className="card">
        <table className="data">
          <thead><tr><th>When</th><th>Operation</th><th>From</th><th>To</th><th>Status</th><th>Verification</th></tr></thead>
          <tbody>
            {ops.map((o) => (
              <tr key={o.id}>
                <td style={{ whiteSpace: "nowrap" }}>{formatMs(o.createdAt)}</td>
                <td>{o.operationType}</td>
                <td style={{ wordBreak: "break-all", fontSize: 11.5 }}>{o.originalPath}</td>
                <td style={{ wordBreak: "break-all", fontSize: 11.5 }}>
                  {o.destinationPath}
                  {o.destinationPath && o.operationStatus === "completed" && <button className="icon-btn" style={{ width: 20, height: 20 }} title="Show in Finder" onClick={() => api.revealPath(o.destinationPath!)}><FolderSearch size={12} /></button>}
                </td>
                <td><span className={`status-pill ${o.operationStatus}`}>{o.operationStatus}</span>{o.errorMessage && <div className="error-text" style={{ fontSize: 11 }}>{o.errorMessage}</div>}</td>
                <td style={{ fontSize: 11.5 }}>
                  {o.result && (
                    <>
                      {o.result.sourceHash === o.result.destinationHash ? <span style={{ color: "var(--pick)" }}>✓ content verified</span> : <span className="error-text">content mismatch</span>}
                      <br />
                      {o.result.timestamps?.warnings?.length ? (
                        <span className="warn"><AlertTriangle size={11} /> {o.result.timestamps.warnings.join("; ")}</span>
                      ) : (
                        <span style={{ color: "var(--pick)" }}>✓ dates preserved</span>
                      )}
                    </>
                  )}
                </td>
              </tr>
            ))}
            {!ops.length && <tr><td colSpan={6} className="subtle">No file operations yet.</td></tr>}
          </tbody>
        </table>
      </div>
    </div>
  );
}
