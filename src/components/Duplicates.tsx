import { useEffect, useState } from "react";
import { Ban, Copy, FolderSearch } from "lucide-react";
import { api, thumbUrl } from "../api";
import { useStore } from "../store";
import type { DuplicateGroup } from "../types";
import { formatBytes, formatLocal } from "../format";
import { curate } from "../actions";
import { Stars } from "./ui";

/** Exact duplicates (identical content). Nothing is deleted: the user decides. */
export function DuplicatesView() {
  const revision = useStore((s) => s.revision);
  const [groups, setGroups] = useState<DuplicateGroup[] | null>(null);
  useEffect(() => {
    api.duplicates().then(setGroups).catch(() => setGroups([]));
  }, [revision]);
  if (!groups) return <div className="page" />;
  if (!groups.length)
    return (
      <div className="empty-state">
        <Copy size={46} />
        <h2>No exact duplicates found</h2>
        <p>Files with identical content are detected in the background after each scan. Similar-looking photos (bursts, edits) are not treated as duplicates.</p>
      </div>
    );
  const wasted = groups.reduce((a, g) => a + g.fileSize * (g.items.length - 1), 0);
  return (
    <div className="page">
      <p className="subtle" style={{ marginTop: 0 }}>
        {groups.length.toLocaleString()} groups of byte-identical files · {formatBytes(wasted)} in extra copies. StoryLoom never deletes files — mark the copies you don’t want as Rejected, or remove them yourself in Finder.
      </p>
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        {groups.map((g) => (
          <div className="dup-group" key={g.fileHash}>
            <div className="subtle">{g.items.length} copies · {formatBytes(g.fileSize)} each</div>
            <div className="dup-items">
              {g.items.map((it) => (
                <div className="dup-item" key={it.id}>
                  <img src={thumbUrl(it.id)} alt="" />
                  <div className="info">
                    <b style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{it.filename}</b>
                    <span className="path" title={it.path}><bdi>{it.path}</bdi></span>
                    <span className="subtle">{formatLocal(it.sortDatetime)}</span>
                    <div className="row">
                      <Stars value={it.rating} size={11} />
                      {it.selectionStatus === "rejected" && <span className="status-pill failed">rejected</span>}
                    </div>
                    <div className="row">
                      <button className="btn small" onClick={() => api.revealInFinder(it.id)}><FolderSearch size={12} />Show</button>
                      <button className="btn small" onClick={() => curate([it.id], { selectionStatus: it.selectionStatus === "rejected" ? "none" : "rejected" })}><Ban size={12} />{it.selectionStatus === "rejected" ? "Unreject" : "Reject"}</button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
