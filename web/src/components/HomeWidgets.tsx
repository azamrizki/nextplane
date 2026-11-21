import { useMutation, useQueryClient } from "@tanstack/react-query";
import TaskItem from "@tiptap/extension-task-item";
import TaskList from "@tiptap/extension-task-list";
import Placeholder from "@tiptap/extension-placeholder";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { ArrowDown, ArrowUp, Bold, ChevronDown, Ellipsis, ExternalLink, FileText, Italic, Link as LinkIcon, ListChecks, Palette, Pencil, Plus, Search, StickyNote, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Link, useNavigate } from "react-router";
import { del, patch, post } from "../api/client";
import type { MyWorkspace } from "../api/types";
import { useI18n } from "../i18n";
import { copyText } from "../lib/copy";
import { errorText } from "../lib/errors";
import { STICKY_COLORS, hostOf, hq, useQuickLinks, useRecents, useStickies, useWidgets, type QuickLink, type RecentEntity, type Sticky, type StickyColor, type Widget, type WidgetKey } from "../lib/home";
import { toast } from "../lib/toast";
import { keyOf } from "../lib/workItems";
import { ProjectIcon } from "../pages/Projects";
import { ConfirmDialog, Dialog } from "./Dialog";
import { Popover } from "./Layout";
import { PriorityIcon, StateIcon } from "./Props";
import { ErrorAlert, TextField } from "./ui";
import { timeAgo } from "./WorkItemDetail";

const WIDGET_LABEL = { quick_links: "hw.links", recents: "hw.recents", stickies: "hw.stickies" } as const;

/** The home page's widgets in the person's order, the enabled ones only. */
export function HomeWidgets({ ws }: { ws: MyWorkspace }) {
  const { t } = useI18n();
  const widgets = useWidgets(ws.slug);
  if (!widgets.data) return null;
  const on = widgets.data.filter((w) => w.is_enabled);
  if (!on.length) return <div className="card empty small">{t("hw.allOff")}</div>;
  return (
    <div className="home-widgets">
      {on.map((w) =>
        w.key === "quick_links" ? <QuickLinksWidget key={w.key} ws={ws} /> : w.key === "recents" ? <RecentsWidget key={w.key} ws={ws} /> : <StickiesWidget key={w.key} ws={ws} />,
      )}
    </div>
  );
}

/** Show or hide each widget and change their order. */
export function ManageWidgets({ ws, open, onClose }: { ws: MyWorkspace; open: boolean; onClose: () => void }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const widgets = useWidgets(ws.slug);
  // Shown at once, then confirmed (or put back) by the server's answer.
  const [list, setList] = useState<Widget[]>(widgets.data ?? []);
  useEffect(() => setList(widgets.data ?? []), [widgets.data]);
  const save = useMutation({
    mutationFn: (v: { key: WidgetKey; body: Partial<Widget> }) => patch<Widget[]>(`/workspaces/${ws.slug}/home/widgets/${v.key}`, v.body),
    onError: (e) => {
      setList(widgets.data ?? []);
      toast({ kind: "error", title: errorText(e, t) });
    },
    onSuccess: (next) => qc.setQueryData(hq.widgets(ws.slug), next),
  });
  const set = {
    mutate: (v: { key: WidgetKey; body: Partial<Widget> }) => {
      setList((cur) => cur.map((w) => (w.key === v.key ? { ...w, ...v.body } : w)).sort((a, b) => b.sort_order - a.sort_order));
      save.mutate(v);
    },
  };
  // Moving up puts a widget just above its upper neighbour (between that one and the next).
  const move = (i: number, dir: -1 | 1) => {
    const j = i + dir;
    const a = list[i];
    const b = list[j];
    if (!a || !b) return;
    const beyond = list[j + dir];
    const order = beyond ? (b.sort_order + beyond.sort_order) / 2 : b.sort_order + (dir === -1 ? 10_000 : -10_000);
    set.mutate({ key: a.key, body: { sort_order: order } });
  };
  return (
    <Dialog open={open} onClose={onClose} title={t("hw.manage")} width={420}>
      <div className="rows card manage-widgets">
        {list.map((w, i) => (
          <div key={w.key} className="row">
            <span className="grow">{t(WIDGET_LABEL[w.key])}</span>
            <button className="icon-btn icon-btn-sm" disabled={i === 0} onClick={() => move(i, -1)} aria-label={t("hw.moveUp", { name: t(WIDGET_LABEL[w.key]) })}>
              <ArrowUp />
            </button>
            <button className="icon-btn icon-btn-sm" disabled={i === list.length - 1} onClick={() => move(i, 1)} aria-label={t("hw.moveDown", { name: t(WIDGET_LABEL[w.key]) })}>
              <ArrowDown />
            </button>
            <input type="checkbox" role="switch" className="switch" aria-label={t(WIDGET_LABEL[w.key])} checked={w.is_enabled} onChange={(e) => set.mutate({ key: w.key, body: { is_enabled: e.target.checked } })} />
          </div>
        ))}
      </div>
    </Dialog>
  );
}

function WidgetHead({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="widget-head">
      <h3>{title}</h3>
      <span className="grow" />
      {children}
    </div>
  );
}

// ---- Quick links ------------------------------------------------------------------------------

function QuickLinksWidget({ ws }: { ws: MyWorkspace }) {
  const { t } = useI18n();
  const links = useQuickLinks(ws.slug);
  const [editing, setEditing] = useState<QuickLink | "new" | null>(null);
  const [all, setAll] = useState(false);
  const list = links.data ?? [];
  return (
    <section className="widget" aria-label={t("hw.links")}>
      <WidgetHead title={t("hw.links")}>
        <button className="btn btn-sm btn-ghost" onClick={() => setEditing("new")}>
          <Plus />
          {t("hw.addLink")}
        </button>
      </WidgetHead>
      {links.isSuccess && !list.length ? <p className="muted small widget-empty">{t("hw.linksEmpty")}</p> : null}
      <div className={"link-grid" + (all ? "" : " capped")}>
        {list.map((l) => (
          <LinkCard key={l.id} ws={ws} link={l} onEdit={() => setEditing(l)} />
        ))}
      </div>
      {list.length > 6 ? (
        <button className="btn btn-sm btn-ghost widget-more" onClick={() => setAll((a) => !a)}>
          {all ? t("hw.showLess") : t("hw.showAll")}
          <ChevronDown className={all ? "rot180" : ""} />
        </button>
      ) : null}
      {editing ? <LinkDialog ws={ws} link={editing === "new" ? undefined : editing} onClose={() => setEditing(null)} /> : null}
    </section>
  );
}

function LinkCard({ ws, link, onEdit }: { ws: MyWorkspace; link: QuickLink; onEdit: () => void }) {
  const { t, lang } = useI18n();
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const remove = async () => {
    close();
    try {
      await del(`/workspaces/${ws.slug}/quick-links/${link.id}`);
      qc.invalidateQueries({ queryKey: hq.links(ws.slug) });
    } catch (e) {
      toast({ kind: "error", title: errorText(e, t) });
    }
  };
  return (
    <div className="link-card">
      <a href={link.url} target="_blank" rel="noopener noreferrer" className="link-card-main">
        <span className="link-card-icon" aria-hidden>
          <LinkIcon />
        </span>
        <span className="link-card-text">
          <span className="ellipsis link-card-title">{link.title || link.url}</span>
          <span className="faint small ellipsis">
            {hostOf(link.url)} · {timeAgo(link.created_at, t, lang)}
          </span>
        </span>
      </a>
      <Popover
        open={open}
        onClose={close}
        right
        trigger={
          <button className="icon-btn icon-btn-sm link-card-menu" onClick={() => setOpen(!open)} aria-label={t("hw.linkActions", { name: link.title || hostOf(link.url) })} aria-haspopup="menu" aria-expanded={open}>
            <Ellipsis />
          </button>
        }
      >
        <div className="menu" role="menu">
          <button className="menu-item" role="menuitem" onClick={() => (close(), onEdit())}>
            <Pencil />
            {t("common.edit")}
          </button>
          <a className="menu-item" role="menuitem" href={link.url} target="_blank" rel="noopener noreferrer" onClick={close}>
            <ExternalLink />
            {t("hw.openNewTab")}
          </a>
          <button className="menu-item" role="menuitem" onClick={() => (close(), void copyText(link.url))}>
            <LinkIcon />
            {t("nav.copyLink")}
          </button>
          <button className="menu-item danger" role="menuitem" onClick={remove}>
            <Trash2 />
            {t("common.delete")}
          </button>
        </div>
      </Popover>
    </div>
  );
}

function LinkDialog({ ws, link, onClose }: { ws: MyWorkspace; link?: QuickLink; onClose: () => void }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const [url, setUrl] = useState(link?.url ?? "");
  const [title, setTitle] = useState(link?.title ?? "");
  const save = useMutation({
    mutationFn: () => (link ? patch(`/workspaces/${ws.slug}/quick-links/${link.id}`, { url, title }) : post(`/workspaces/${ws.slug}/quick-links`, { url, title })),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: hq.links(ws.slug) });
      onClose();
    },
  });
  return (
    <Dialog
      open
      onClose={onClose}
      title={link ? t("hw.editLink") : t("hw.addLink")}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button className="btn btn-primary" disabled={!url.trim() || save.isPending} onClick={() => save.mutate()}>
            {link ? t("common.save") : t("hw.addLink")}
          </button>
        </>
      }
    >
      <ErrorAlert message={save.isError ? errorText(save.error, t) : null} />
      <TextField label={t("hw.url")} placeholder="https://" value={url} onChange={(e) => setUrl(e.target.value)} autoFocus />
      <TextField label={t("hw.linkTitle")} value={title} onChange={(e) => setTitle(e.target.value)} maxLength={255} />
    </Dialog>
  );
}

// ---- Recents ----------------------------------------------------------------------------------

function RecentsWidget({ ws }: { ws: MyWorkspace }) {
  const { t, lang } = useI18n();
  const [entity, setEntity] = useState<RecentEntity | "">("");
  const recents = useRecents(ws.slug, entity);
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const label = { "": t("hw.all"), work_item: t("hw.workItems"), page: t("hw.pages"), project: t("hw.projects") };
  const list = recents.data ?? [];
  return (
    <section className="widget" aria-label={t("hw.recents")}>
      <WidgetHead title={t("hw.recents")}>
        <Popover
          open={open}
          onClose={close}
          right
          trigger={
            <button className="btn btn-sm filter-pill-btn" onClick={() => setOpen(!open)} aria-haspopup="menu" aria-expanded={open} aria-label={t("hw.recentsFilter", { name: label[entity] })}>
              {label[entity]}
              <ChevronDown />
            </button>
          }
        >
          <div className="menu" role="menu">
            {(["", "work_item", "page", "project"] as const).map((e) => (
              <button key={e} className={"menu-item" + (e === entity ? " active" : "")} role="menuitem" onClick={() => (close(), setEntity(e))}>
                {label[e]}
              </button>
            ))}
          </div>
        </Popover>
      </WidgetHead>
      {recents.isSuccess && !list.length ? <p className="muted small widget-empty">{entity ? t("hw.recentsEmptyKind", { name: label[entity].toLowerCase() }) : t("hw.recentsEmpty")}</p> : null}
      <div className="recents">
        {list.map((r) => {
          const when = <span className="faint small recent-when">{timeAgo(r.visited_at, t, lang)}</span>;
          if (r.entity === "work_item") {
            return (
              <Link key={`w${r.id}`} to={`/${ws.slug}/projects/${r.project_id}/work-items/${r.id}`} className="recent-row">
                {r.state ? <StateIcon group={r.state.group} color={r.state.color} /> : null}
                <span className="mono faint small recent-key">{keyOf(r.identifier, r.sequence)}</span>
                <span className="grow ellipsis">{r.title}</span>
                <PriorityIcon p={r.priority} />
                {when}
              </Link>
            );
          }
          if (r.entity === "page") {
            return (
              <Link key={`p${r.id}`} to={`/${ws.slug}/projects/${r.project_id}/pages/${r.id}`} className="recent-row">
                <FileText className="recent-icon" aria-hidden />
                <span className="grow ellipsis">{r.title || t("pg.untitled")}</span>
                <span className="faint small ellipsis recent-project">{r.project_name}</span>
                {when}
              </Link>
            );
          }
          return (
            <Link key={`j${r.id}`} to={`/${ws.slug}/projects/${r.id}`} className="recent-row">
              <ProjectIcon p={{ name: r.name, icon: r.icon, cover_color: r.cover_color }} size={16} />
              <span className="grow ellipsis">{r.name}</span>
              {when}
            </Link>
          );
        })}
      </div>
    </section>
  );
}

// ---- Stickies ---------------------------------------------------------------------------------

/** "Add sticky", refused while your newest sticky is still empty (as in Plane). */
export function useAddSticky(slug: string, newest: Sticky | undefined) {
  const { t } = useI18n();
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      if (newest && isEmpty(newest)) {
        toast({ kind: "info", title: t("st.fillFirst") });
        return null;
      }
      return post<Sticky>(`/workspaces/${slug}/stickies`, {});
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: hq.allStickies(slug) }),
    onError: (e) => toast({ kind: "error", title: errorText(e, t) }),
  });
}

const isEmpty = (s: Sticky) => !s.description_html.replace(/<[^>]*>/g, "").trim() && !s.description_html.includes('data-type="taskItem"');

function StickiesWidget({ ws }: { ws: MyWorkspace }) {
  const { t } = useI18n();
  const [q, setQ] = useState("");
  const [debounced, setDebounced] = useState("");
  useEffect(() => {
    const h = setTimeout(() => setDebounced(q), 500);
    return () => clearTimeout(h);
  }, [q]);
  const stickies = useStickies(ws.slug, debounced, 12);
  const add = useAddSticky(ws.slug, stickies.data?.[0]);
  return (
    <section className="widget" aria-label={t("hw.stickies")}>
      <WidgetHead title={t("hw.stickies")}>
        <label className="widget-search">
          <Search aria-hidden />
          <input className="input input-sm" placeholder={t("st.search")} aria-label={t("st.search")} value={q} onChange={(e) => setQ(e.target.value)} />
        </label>
        <button className="btn btn-sm btn-ghost" onClick={() => add.mutate()} disabled={add.isPending}>
          <Plus />
          {t("st.add")}
        </button>
      </WidgetHead>
      {stickies.isSuccess && !stickies.data.length ? <p className="muted small widget-empty">{debounced ? t("st.noMatch") : t("st.empty")}</p> : null}
      <StickyGrid ws={ws} list={stickies.data ?? []} capped />
      {stickies.data && stickies.data.length ? (
        <Link to={`/${ws.slug}/stickies`} className="btn btn-sm btn-ghost widget-more">
          {t("hw.showAll")}
        </Link>
      ) : null}
    </section>
  );
}

export function StickyGrid({ ws, list, capped }: { ws: MyWorkspace; list: Sticky[]; capped?: boolean }) {
  return (
    <div className={"sticky-grid" + (capped ? " capped" : "")}>
      {list.map((s) => (
        <StickyCard key={s.id} ws={ws} sticky={s} />
      ))}
    </div>
  );
}

function StickyCard({ ws, sticky }: { ws: MyWorkspace; sticky: Sticky }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const [color, setColor] = useState<StickyColor>(sticky.color);
  const [palette, setPalette] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const closePalette = useCallback(() => setPalette(false), []);
  const url = `/workspaces/${ws.slug}/stickies/${sticky.id}`;
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const save = useCallback(
    async (body: Record<string, unknown>) => {
      try {
        await patch(url, body);
      } catch (e) {
        toast({ kind: "error", title: errorText(e, t) });
      }
    },
    [url, t],
  );
  const editor = useEditor({
    extensions: [
      StarterKit.configure({ heading: false, codeBlock: false, blockquote: false, horizontalRule: false, code: false, strike: false }),
      TaskList,
      TaskItem.configure({ nested: true }),
      Placeholder.configure({ placeholder: t("st.placeholder") }),
    ],
    content: (sticky.description_json as object | null) ?? sticky.description_html,
    editorProps: { attributes: { class: "prose sticky-prose", "aria-label": t("st.note"), role: "textbox", "aria-multiline": "true" } },
    // Saved half a second after typing stops (as in Plane).
    onUpdate: ({ editor }) => {
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => void save({ description_json: editor.getJSON(), description_html: editor.getHTML() }), 500);
    },
  });
  useEffect(
    () => () => {
      if (timer.current && editor) {
        clearTimeout(timer.current);
        void save({ description_json: editor.getJSON(), description_html: editor.getHTML() });
      }
    },
    [editor, save],
  );
  const btn = (label: string, icon: ReactNode, on: boolean, run: () => void) => (
    <button type="button" className={"icon-btn icon-btn-sm" + (on ? " on" : "")} aria-label={label} aria-pressed={on} title={label} onMouseDown={(e) => e.preventDefault()} onClick={run}>
      {icon}
    </button>
  );
  return (
    <div className={`sticky sticky-${color}`}>
      <EditorContent editor={editor} />
      <div className="sticky-tools">
        {editor ? (
          <>
            {btn(t("st.bold"), <Bold />, editor.isActive("bold"), () => editor.chain().focus().toggleBold().run())}
            {btn(t("st.italic"), <Italic />, editor.isActive("italic"), () => editor.chain().focus().toggleItalic().run())}
            {btn(t("st.tasks"), <ListChecks />, editor.isActive("taskList"), () => editor.chain().focus().toggleTaskList().run())}
          </>
        ) : null}
        <Popover
          open={palette}
          onClose={closePalette}
          trigger={
            <button className="icon-btn icon-btn-sm" onClick={() => setPalette(!palette)} aria-label={t("st.color")} title={t("st.color")} aria-haspopup="menu" aria-expanded={palette}>
              <Palette />
            </button>
          }
        >
          <div className="menu sticky-swatches" role="menu">
            {STICKY_COLORS.map((c) => (
              <button
                key={c}
                role="menuitemradio"
                aria-checked={c === color}
                aria-label={t(`st.c.${c}`)}
                title={t(`st.c.${c}`)}
                className={`sticky-swatch sticky-${c}` + (c === color ? " on" : "")}
                onClick={() => {
                  setColor(c);
                  closePalette();
                  void save({ color: c });
                }}
              />
            ))}
          </div>
        </Popover>
        <span className="grow" />
        <button className="icon-btn icon-btn-sm" onClick={() => setDeleting(true)} aria-label={t("st.delete")} title={t("st.delete")}>
          <Trash2 />
        </button>
      </div>
      <ConfirmDialog
        open={deleting}
        onClose={() => setDeleting(false)}
        title={t("st.delete")}
        body={t("st.deleteBody")}
        confirmLabel={t("common.delete")}
        danger
        onConfirm={async () => {
          setDeleting(false);
          if (timer.current) clearTimeout(timer.current);
          timer.current = null;
          try {
            await del(url);
            qc.invalidateQueries({ queryKey: hq.allStickies(ws.slug) });
          } catch (e) {
            toast({ kind: "error", title: errorText(e, t) });
          }
        }}
      />
    </div>
  );
}

export function StickyIcon() {
  return <StickyNote />;
}

export function useGoStickies() {
  const navigate = useNavigate();
  return (slug: string) => navigate(`/${slug}/stickies`);
}
