import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { del, get, patch, post } from "../api/client";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { pcan } from "../lib/perm";
import { projectPath } from "../lib/projects";
import { useMe } from "../lib/session";
import { DEFAULT_VIEW, EMPTY_FILTERS, type ViewState } from "../lib/view";
import { Dialog } from "./Dialog";
import { Popover } from "./Layout";
import { ErrorAlert, TextField } from "./ui";
import type { PropsCtx } from "./WorkItemProps";
import { useWs } from "../pages/WorkspaceHome";

interface SavedView {
  id: string;
  owner_id: string;
  name: string;
  is_shared: boolean;
  config: Partial<ViewState>;
}

/** The "Views" menu: apply a saved view, save the current one, rename, share, delete. */
export function SavedViews({ ctx, view, setView }: { ctx: PropsCtx; view: ViewState; setView: (p: Partial<ViewState>) => void }) {
  const { t } = useI18n();
  const me = useMe().data!;
  const qc = useQueryClient();
  const ws = useWs();
  const url = `${projectPath(ws.slug, ctx.project.id)}/views`;
  const key = ["views", ctx.project.id];
  const list = useQuery({ queryKey: key, queryFn: () => get<SavedView[]>(url) });
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState<SavedView | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const config = (): Partial<ViewState> => ({ ...view, filters: { ...view.filters, q: "" } });
  const apply = (v: SavedView) => {
    setView({ ...DEFAULT_VIEW, ...v.config, filters: { ...EMPTY_FILTERS, ...v.config.filters } , display: { ...DEFAULT_VIEW.display, ...v.config.display } });
    setActive(v);
    setOpen(false);
  };
  const canEdit = (v: SavedView) => v.owner_id === me.id || ctx.project.my_role === "admin";
  const act = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
      setError(null);
      qc.invalidateQueries({ queryKey: key });
    } catch (e) {
      setError(errorText(e, t));
    }
  };
  const views = list.data ?? [];
  return (
    <>
      <Popover
        open={open}
        onClose={() => setOpen(false)}
        trigger={
          <button className="btn btn-sm" onClick={() => setOpen((o) => !o)} aria-expanded={open} aria-haspopup="menu">
            {active ? active.name : t("views.title")} ▾
          </button>
        }
      >
        <div className="menu" role="menu" style={{ width: 300 }}>
          {views.length === 0 ? <div className="menu-label">{t("views.none")}</div> : null}
          {views.map((v) => (
            <div key={v.id} className="view-row">
              <button className="menu-item grow" role="menuitem" onClick={() => apply(v)}>
                <span className="grow ellipsis">{v.name}</span>
                <span className="faint small">{v.is_shared ? t("views.team") : t("views.mine")}</span>
              </button>
              {canEdit(v) ? (
                <button className="icon-btn" aria-label={`${t("common.delete")}: ${v.name}`} onClick={() => { act(() => del(`${url}/${v.id}`)); if (active?.id === v.id) setActive(null); }}>✕</button>
              ) : null}
            </div>
          ))}
          {pcan.createWorkItem(ctx.project) ? (
            <>
              <div className="menu-sep" />
              {active && canEdit(active) ? (
                <button className="menu-item" role="menuitem" onClick={() => { setOpen(false); act(() => patch(`${url}/${active.id}`, { config: config() })); }}>
                  {t("views.update", { name: active.name })}
                </button>
              ) : null}
              <button className="menu-item" role="menuitem" onClick={() => { setOpen(false); setSaving(true); }}>{t("views.saveAs")}</button>
            </>
          ) : null}
          {error ? <div className="menu-label" style={{ color: "var(--danger)" }}>{error}</div> : null}
        </div>
      </Popover>
      <SaveDialog open={saving} onClose={() => setSaving(false)} onSave={async (name, shared) => {
        const v = await post<SavedView>(url, { name, is_shared: shared, config: config() });
        qc.invalidateQueries({ queryKey: key });
        setActive(v);
      }} />
    </>
  );
}

function SaveDialog({ open, onClose, onSave }: { open: boolean; onClose: () => void; onSave: (name: string, shared: boolean) => Promise<void> }) {
  const { t } = useI18n();
  const [name, setName] = useState("");
  const [shared, setShared] = useState(false);
  const save = useMutation({ mutationFn: () => onSave(name, shared), onSuccess: () => { setName(""); onClose(); } });
  return (
    <Dialog open={open} onClose={onClose} title={t("views.save")}>
      <form className="form" onSubmit={(e) => { e.preventDefault(); save.mutate(); }}>
        <ErrorAlert message={save.isError ? errorText(save.error, t) : null} />
        <TextField label={t("views.name")} value={name} onChange={(e) => setName(e.target.value)} required maxLength={80} autoFocus />
        <label className="checkbox">
          <input type="checkbox" checked={shared} onChange={(e) => setShared(e.target.checked)} />
          <span>{t("views.shared")}<br /><span className="faint small">{t("views.sharedHint")}</span></span>
        </label>
        <div><button className="btn btn-primary" disabled={save.isPending || !name.trim()}>{t("views.save")}</button></div>
      </form>
    </Dialog>
  );
}
