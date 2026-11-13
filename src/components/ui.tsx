// Shared UI primitives: stars, modal dialogs (prompt/confirm), toasts, context menus.
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { AlertCircle, CheckCircle2, Info, Star } from "lucide-react";

export function Stars({ value, onChange, size = 13 }: { value: number; onChange?: (v: number) => void; size?: number }) {
  return (
    <span className={`stars ${onChange ? "interactive" : ""}`} aria-label={`${value} of 5 stars`}>
      {[1, 2, 3, 4, 5].map((i) => (
        <Star
          key={i}
          size={size}
          className={i <= value ? "on" : ""}
          onClick={onChange ? (e) => { e.stopPropagation(); onChange(value === i ? 0 : i); } : undefined}
        />
      ))}
    </span>
  );
}

export function Modal({ title, children, footer, onClose, wide }: { title: ReactNode; children: ReactNode; footer?: ReactNode; onClose: () => void; wide?: boolean }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={`modal ${wide ? "wide" : ""}`} role="dialog" aria-modal>
        <div className="modal-head">{title}</div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </div>
  );
}

/** Mount a transient React tree (dialogs) and resolve when it closes. */
function mountTransient<T>(render: (done: (v: T) => void) => ReactNode): Promise<T> {
  return new Promise((resolve) => {
    const el = document.createElement("div");
    document.body.appendChild(el);
    const root = createRoot(el);
    const done = (v: T) => {
      root.unmount();
      el.remove();
      resolve(v);
    };
    root.render(<>{render(done)}</>);
  });
}

function PromptDialog({ title, label, initial, placeholder, hint, okLabel, done }: { title: string; label?: string; initial?: string; placeholder?: string; hint?: string; okLabel?: string; done: (v: string | null) => void }) {
  const [v, setV] = useState(initial ?? "");
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  return (
    <Modal
      title={title}
      onClose={() => done(null)}
      footer={
        <>
          <button className="btn" onClick={() => done(null)}>Cancel</button>
          <button className="btn primary" disabled={!v.trim()} onClick={() => done(v.trim())}>{okLabel ?? "OK"}</button>
        </>
      }
    >
      <label className="field">
        {label && <span>{label}</span>}
        <input
          ref={ref}
          className="input"
          value={v}
          placeholder={placeholder}
          onChange={(e) => setV(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && v.trim()) done(v.trim());
          }}
        />
        {hint && <span className="hint">{hint}</span>}
      </label>
    </Modal>
  );
}

export function promptText(opts: { title: string; label?: string; initial?: string; placeholder?: string; hint?: string; okLabel?: string }) {
  return mountTransient<string | null>((done) => <PromptDialog {...opts} done={done} />);
}

export function confirmDialog(opts: { title: string; message: ReactNode; okLabel?: string; danger?: boolean }) {
  return mountTransient<boolean>((done) => (
    <Modal
      title={opts.title}
      onClose={() => done(false)}
      footer={
        <>
          <button className="btn" onClick={() => done(false)}>Cancel</button>
          <button className={`btn ${opts.danger ? "danger" : "primary"}`} autoFocus onClick={() => done(true)}>{opts.okLabel ?? "OK"}</button>
        </>
      }
    >
      <div>{opts.message}</div>
    </Modal>
  ));
}

// ---------------- toasts ----------------
type ToastAction = { label: string; onClick: () => void };
type ToastT = { id: number; kind: "info" | "success" | "error"; text: string; action?: ToastAction };
let toastListeners: ((t: ToastT[]) => void)[] = [];
let toasts: ToastT[] = [];
let toastId = 0;

export function toast(text: string, kind: ToastT["kind"] = "info", action?: ToastAction) {
  const t = { id: ++toastId, kind, text, action };
  toasts = [...toasts, t];
  toastListeners.forEach((l) => l(toasts));
  setTimeout(() => {
    toasts = toasts.filter((x) => x.id !== t.id);
    toastListeners.forEach((l) => l(toasts));
  }, kind === "error" || action ? 10000 : 3500);
}

export function Toasts() {
  const [list, setList] = useState<ToastT[]>([]);
  useEffect(() => {
    toastListeners.push(setList);
    return () => {
      toastListeners = toastListeners.filter((l) => l !== setList);
    };
  }, []);
  return (
    <div className="toasts">
      {list.map((t) => (
        <div key={t.id} className={`toast ${t.kind}`}>
          {t.kind === "error" ? <AlertCircle size={16} /> : t.kind === "success" ? <CheckCircle2 size={16} /> : <Info size={16} />}
          <span>{t.text}</span>
          {t.action && <button className="btn small" style={{ marginLeft: 8 }} onClick={t.action.onClick}>{t.action.label}</button>}
        </div>
      ))}
    </div>
  );
}

// ---------------- context menu ----------------
export type MenuEntry =
  | { label: string; icon?: ReactNode; shortcut?: string; danger?: boolean; onClick: () => void; disabled?: boolean }
  | { separator: true }
  | { title: string };

let menuSetter: ((m: { x: number; y: number; items: MenuEntry[] } | null) => void) | null = null;

export function openMenu(e: { clientX: number; clientY: number; preventDefault?: () => void }, items: MenuEntry[]) {
  e.preventDefault?.();
  menuSetter?.({ x: e.clientX, y: e.clientY, items });
}

export function ContextMenuHost() {
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x: 0, y: 0 });
  useEffect(() => {
    menuSetter = setMenu;
    return () => {
      menuSetter = null;
    };
  }, []);
  useLayoutEffect(() => {
    if (!menu || !ref.current) return;
    const r = ref.current.getBoundingClientRect();
    setPos({ x: Math.min(menu.x, window.innerWidth - r.width - 8), y: Math.min(menu.y, window.innerHeight - r.height - 8) });
  }, [menu]);
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("mousedown", close);
    window.addEventListener("blur", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);
  if (!menu) return null;
  return (
    <div className="menu" ref={ref} style={{ left: pos.x, top: pos.y }} onMouseDown={(e) => e.stopPropagation()}>
      {menu.items.map((it, i) =>
        "separator" in it ? (
          <div key={i} className="menu-sep" />
        ) : "title" in it ? (
          <div key={i} className="menu-sub-title">{it.title}</div>
        ) : (
          <div
            key={i}
            className={`menu-item ${it.danger ? "danger" : ""}`}
            style={it.disabled ? { opacity: 0.4, pointerEvents: "none" } : undefined}
            onClick={() => {
              setMenu(null);
              it.onClick();
            }}
          >
            {it.icon}
            <span>{it.label}</span>
            {it.shortcut && <span className="shortcut">{it.shortcut}</span>}
          </div>
        ),
      )}
    </div>
  );
}
