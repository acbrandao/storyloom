// Full-window viewer for photos, videos and audio. Media streams through `media://<id>`
// (served by Rust with Range support), so seeking in long 4K files is instant.
import { useCallback, useEffect, useRef, useState } from "react";
import { Ban, Check, ChevronLeft, ChevronRight, Clock, ExternalLink, Heart, Maximize, Music, SkipBack, SkipForward, X, Info, Scissors } from "lucide-react";
import { ClipEditor } from "./ClipEditor";
import { api, mediaUrl, thumbUrl } from "../api";
import { mediaList } from "../hooks/mediaList";
import { setState, useStore } from "../store";
import type { MediaDetail } from "../types";
import { Stars } from "./ui";
import { curate } from "../actions";
import { formatBytes, formatLocal, formatTimecode } from "../format";

const PLAYABLE = ["mp4", "mov", "m4v", "webm", "3gp", "insv", "lrv"];
const SPEEDS = [0.25, 0.5, 1, 1.5, 2, 4];

export function Viewer() {
  const id = useStore((s) => s.viewerId);
  const revision = useStore((s) => s.revision);
  const [d, setD] = useState<MediaDetail | null>(null);
  const [index, setIndex] = useState(-1);
  const videoRef = useRef<HTMLVideoElement | null>(null);

  useEffect(() => {
    if (id == null) return;
    api.mediaDetail(id).then(setD).catch(() => setD(null));
    setIndex(mediaList.indexOf(id));
  }, [id, revision]);

  const go = useCallback(async (delta: number) => {
    const i = (index >= 0 ? index : mediaList.indexOf(id ?? -1)) + delta;
    const it = await mediaList.getAsync(i);
    if (it) setState({ viewerId: it.id, focusIndex: i, anchorIndex: i, selection: new Set([it.id]) });
  }, [index, id]);

  const close = useCallback(() => setState({ viewerId: null, clipMode: false }), []);

  useEffect(() => {
    if (id == null) return;
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT") return;
      const v = videoRef.current;
      const fps = d?.frameRate || 30;
      const k = e.key.toLowerCase();
      let handled = true;
      if (e.key === "Escape") close();
      else if (e.key === "ArrowRight" && !(v && e.shiftKey)) go(1);
      else if (e.key === "ArrowLeft" && !(v && e.shiftKey)) go(-1);
      else if (v && e.key === "ArrowRight") v.currentTime = Math.min(v.duration, v.currentTime + 5);
      else if (v && e.key === "ArrowLeft") v.currentTime = Math.max(0, v.currentTime - 5);
      else if (v && e.key === " ") v.paused ? v.play() : v.pause();
      else if (v && e.key === ".") { v.pause(); v.currentTime = Math.min(v.duration, v.currentTime + 1 / fps); }
      else if (v && e.key === ",") { v.pause(); v.currentTime = Math.max(0, v.currentTime - 1 / fps); }
      else if (v && e.key === "]") v.playbackRate = SPEEDS[Math.min(SPEEDS.length - 1, SPEEDS.indexOf(v.playbackRate) + 1)] ?? 1;
      else if (v && e.key === "[") v.playbackRate = SPEEDS[Math.max(0, SPEEDS.indexOf(v.playbackRate) - 1)] ?? 1;
      else if (e.key === " " && !v) close();
      else if (/^[0-5]$/.test(e.key) && !e.metaKey) curate([id], { rating: Number(e.key) });
      else if (k === "p") curate([id], { selectionStatus: "selected" });
      else if (k === "x") curate([id], { selectionStatus: "rejected" });
      else if (k === "u") curate([id], { selectionStatus: "none" });
      else if (k === "f" && !e.metaKey) curate([id], { favorite: !d?.favorite });
      else if (k === "r" && !e.metaKey) curate([id], { reviewStatus: d?.reviewStatus === "needs_review" ? "none" : "needs_review" });
      else if (k === "c" && !e.metaKey && d?.mediaType === "video") setState((st) => ({ clipMode: !st.clipMode }));
      else handled = false;
      if (handled) { e.preventDefault(); e.stopPropagation(); }
      // Auto-advance after rating/flag for fast culling.
      if (handled && useStoreAuto() && (/^[0-5]$/.test(e.key) || k === "p" || k === "x")) setTimeout(() => go(1), 120);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [id, d, go, close]);

  if (id == null) return null;
  const ext = d?.extension ?? "";
  return (
    <div className="viewer">
      <div className="viewer-top" data-tauri-drag-region>
        <button className="icon-btn" onClick={close} title="Close (Esc)"><X size={18} /></button>
        <span className="title">{d?.currentFilename}</span>
        <span className="subtle">{d ? (d.userCaptureDatetime ?? d.captureDatetime ? formatLocal(d.userCaptureDatetime ?? d.captureDatetime) : "Capture date unknown") : ""}</span>
        <span className="subtle">{index >= 0 ? `${index + 1} of ${mediaList.total.toLocaleString()}` : ""}</span>
        <span className="spacer" data-tauri-drag-region />
        {d && (
          <>
            <Stars value={d.rating} size={18} onChange={(r) => curate([d.id], { rating: r })} />
            <button className={`btn small ${d.selectionStatus === "selected" ? "on" : ""}`} onClick={() => curate([d.id], { selectionStatus: d.selectionStatus === "selected" ? "none" : "selected" })}><Check size={13} />Keep</button>
            <button className={`btn small ${d.selectionStatus === "rejected" ? "on" : ""}`} onClick={() => curate([d.id], { selectionStatus: d.selectionStatus === "rejected" ? "none" : "rejected" })}><Ban size={13} />Reject</button>
            <button className={`btn small ${d.reviewStatus === "needs_review" ? "on" : ""}`} onClick={() => curate([d.id], { reviewStatus: d.reviewStatus === "needs_review" ? "none" : "needs_review" })}><Clock size={13} /></button>
            <button className={`btn small ${d.favorite ? "on" : ""}`} onClick={() => curate([d.id], { favorite: !d.favorite })}><Heart size={13} fill={d.favorite ? "#ff6b8b" : "none"} /></button>
          </>
        )}
      </div>
      <div className="viewer-stage">
        {d?.isMissing ? (
          <div className="unsupported"><Info size={32} /><div>This file is missing or its drive is not connected.</div></div>
        ) : d?.mediaType === "photo" ? (
          <PhotoView key={d.id} id={d.id} />
        ) : d?.mediaType === "video" ? (
          PLAYABLE.includes(ext) ? (
            <VideoView key={d.id} d={d} videoRef={videoRef} />
          ) : (
            <div className="unsupported">
              <img src={thumbUrl(d.id, d.thumbnailStatus)} style={{ maxWidth: 480, borderRadius: 8 }} alt="" />
              <div>.{ext.toUpperCase()} videos can’t be played inside Media Curator yet.</div>
              <button className="btn" onClick={() => api.openExternal(d.id)}><ExternalLink size={14} />Open in default player</button>
            </div>
          )
        ) : d?.mediaType === "audio" ? (
          <AudioView key={d.id} d={d} />
        ) : null}
        <button className="viewer-nav prev" onClick={() => go(-1)} title="Previous (←)"><ChevronLeft size={26} /></button>
        <button className="viewer-nav next" onClick={() => go(1)} title="Next (→)"><ChevronRight size={26} /></button>
      </div>
      {d && d.mediaType === "photo" && (
        <div className="viewer-bottom">
          <span className="subtle">{d.width && d.height ? `${d.width} × ${d.height}` : ""} · {formatBytes(d.fileSize)}</span>
          {d.exposure && <span className="subtle">{d.exposure}</span>}
          {d.cameraModel && <span className="subtle">{d.cameraModel}</span>}
          <span className="spacer" />
          <span className="subtle">Click to zoom · ←/→ browse · 0–5 rate · P keep · X reject</span>
        </div>
      )}
    </div>
  );
}

function useStoreAuto() {
  return (window as any).__autoAdvance === true;
}

function PhotoView({ id }: { id: number }) {
  const [actual, setActual] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState(false);
  return (
    <div className="viewer-scroll">
      {!loaded && !error && <img src={thumbUrl(id)} style={{ position: "absolute", filter: "blur(2px)", opacity: 0.6 }} alt="" />}
      {error ? (
        <div className="unsupported"><Info size={28} /><div>This image can’t be displayed.</div><button className="btn" onClick={() => api.openExternal(id)}><ExternalLink size={14} />Open in default app</button></div>
      ) : (
        <img src={mediaUrl(id)} className={actual ? "actual" : "fit"} onClick={() => setActual((a) => !a)} onLoad={() => setLoaded(true)} onError={() => setError(true)} draggable={false} alt="" style={{ opacity: loaded ? 1 : 0 }} />
      )}
    </div>
  );
}

function VideoView({ d, videoRef }: { d: MediaDetail; videoRef: React.MutableRefObject<HTMLVideoElement | null> }) {
  const [t, setT] = useState(0);
  const clipMode = useStore((s) => s.clipMode);
  const [speed, setSpeed] = useState(1);
  const fps = d.frameRate || 30;
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    v.playbackRate = speed;
  }, [speed, videoRef]);
  const step = (dir: number) => {
    const v = videoRef.current;
    if (!v) return;
    v.pause();
    v.currentTime = Math.max(0, Math.min(v.duration, v.currentTime + dir / fps));
  };
  return (
    <div style={{ display: "flex", flexDirection: "column", width: "100%", height: "100%" }}>
      <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", minHeight: 0 }}>
        <video
          ref={videoRef}
          src={mediaUrl(d.id)}
          poster={thumbUrl(d.id, d.thumbnailStatus)}
          controls
          autoPlay
          preload="metadata"
          onTimeUpdate={(e) => setT(e.currentTarget.currentTime)}
          onSeeked={(e) => setT(e.currentTarget.currentTime)}
          onRateChange={(e) => setSpeed(e.currentTarget.playbackRate)}
        />
      </div>
      <div className="viewer-bottom">
        <span className="timecode">{formatTimecode(t, fps)} / {formatTimecode(d.duration ?? 0, fps)}</span>
        <button className="icon-btn" title="Previous frame (,)" onClick={() => step(-1)}><SkipBack size={16} /></button>
        <button className="icon-btn" title="Next frame (.)" onClick={() => step(1)}><SkipForward size={16} /></button>
        <div className="seg">
          {SPEEDS.map((s) => <button key={s} className={speed === s ? "on" : ""} onClick={() => setSpeed(s)}>{s}×</button>)}
        </div>
        <span className="spacer" />
        <span className="subtle">{[d.width && `${d.width}×${d.height}`, d.frameRate && `${d.frameRate} fps`, d.codec?.toUpperCase()].filter(Boolean).join(" · ")}</span>
        <button className="icon-btn" title="Full screen" onClick={() => videoRef.current?.requestFullscreen?.()}><Maximize size={16} /></button>
        <button className={`btn small ${clipMode ? "on" : ""}`} title="Mark and export clips (C)" onClick={() => setState({ clipMode: !clipMode })}><Scissors size={13} />Clips{d.derivedSegments ? ` (${d.derivedSegments})` : ""}</button>
        {!clipMode && <span className="subtle">Space play · ,/. frame · [ ] speed · Shift+←/→ ±5 s · C clips</span>}
      </div>
      {clipMode && <ClipEditor d={d} video={videoRef.current} time={t} />}
    </div>
  );
}

function AudioView({ d }: { d: MediaDetail }) {
  const [art, setArt] = useState(true);
  return (
    <div className="audio-stage">
      <div className="audio-art">
        {art ? <img src={thumbUrl(d.id, d.thumbnailStatus)} onError={() => setArt(false)} alt="" /> : <Music size={90} color="#555" />}
      </div>
      <div style={{ textAlign: "center" }}>
        <div style={{ fontSize: 18, fontWeight: 600 }}>{d.title ?? d.currentFilename}</div>
        <div className="subtle">{[d.artist, d.album].filter(Boolean).join(" — ")}</div>
      </div>
      <audio src={mediaUrl(d.id)} controls autoPlay style={{ width: 420 }} />
    </div>
  );
}
