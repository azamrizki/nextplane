// Comments, reactions, attachments, links and the subscribe button on a work item.
import { Popover } from "./Layout";
import { Link as LinkIcon, Bell, BellOff, Ellipsis, Eye, EyeOff, FileImage, FileText, Globe, Pencil, SmilePlus, Trash2, X } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useRef, useState, type ReactNode } from "react";
import { ApiError, del, get, patch, post } from "../api/client";
import type { WorkItem } from "../api/types";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { pcan } from "../lib/perm";
import { useMe } from "../lib/session";
import { itemsPath } from "../lib/workItems";
import { projectPath } from "../lib/projects";
import { Avatar } from "./Avatar";
import { Editor, type EditorHandle, type MentionPerson } from "./Editor";
import { ErrorAlert } from "./ui";
import { Dialog } from "./Dialog";
import type { PropsCtx } from "./WorkItemProps";

export interface Comment {
  id: string;
  author_id: string;
  author_name: string;
  /** Shown on the project's public board. */
  is_public: boolean;
  body_json: unknown;
  body_html: string;
  created_at: string;
  edited_at: string | null;
  hidden_at: string | null;
  reactions: Reaction[];
}
export interface Reaction {
  emoji: string;
  user_ids: string[];
}
export interface FileMeta {
  id: string;
  file_name: string;
  mime: string;
  size: number;
  uploaded_by: string;
  created_at: string;
}
interface Link {
  id: string;
  url: string;
  title: string;
}

const EMOJI = ["👍", "🎉", "❤️", "🚀", "👀", "😄", "😕", "👎"];

/** Uploads one file as multipart; returns the parsed JSON. */
export async function uploadFile<T>(path: string, file: File): Promise<T> {
  const fd = new FormData();
  fd.append("file", file);
  const res = await fetch(`/api/v1${path}`, { method: "POST", body: fd, credentials: "same-origin" });
  const data = await res.json().catch(() => undefined);
  if (!res.ok) throw new ApiError(res.status, data?.error?.code ?? "upload", data?.error?.message ?? res.statusText);
  return data as T;
}

export function useEditorUpload(slug: string, pid: string) {
  return (f: File) => uploadFile<{ url: string }>(`${projectPath(slug, pid)}/uploads`, f).then((r) => r.url);
}

export function mentionPeople(ctx: PropsCtx): MentionPerson[] {
  return ctx.members.map((m) => ({ id: m.user_id, label: m.name || m.email }));
}

function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export function Reactions({ list, onToggle, canReact }: { list: Reaction[]; onToggle: (e: string) => void; canReact: boolean }) {
  const { t } = useI18n();
  const me = useMe().data!;
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  return (
    <div className="reactions">
      {list.map((r) => (
        <button key={r.emoji} type="button" className={"reaction" + (r.user_ids.includes(me.id) ? " mine" : "")} disabled={!canReact} onClick={() => onToggle(r.emoji)} aria-pressed={r.user_ids.includes(me.id)}>
          {r.emoji} <span>{r.user_ids.length}</span>
        </button>
      ))}
      {canReact ? (
        <Popover
          open={open}
          onClose={close}
          trigger={
            <button type="button" className="reaction add" aria-label={t("react.add")} aria-expanded={open} onClick={() => setOpen((o) => !o)}>
              <SmilePlus />
            </button>
          }
        >
          <span className="emoji-pop" role="menu">
            {EMOJI.map((e) => (
              <button key={e} type="button" role="menuitem" className="emoji" aria-label={e} onClick={() => { onToggle(e); setOpen(false); }}>{e}</button>
            ))}
          </span>
        </Popover>
      ) : null}
    </div>
  );
}

export function ItemReactions({ slug, ctx, item }: { slug: string; ctx: PropsCtx; item: WorkItem }) {
  const url = `${itemsPath(slug, ctx.project.id, item.id)}/reactions`;
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["reactions", item.id], queryFn: () => get<Reaction[]>(url) });
  const toggle = useMutation({ mutationFn: (emoji: string) => post<Reaction[]>(url, { emoji }), onSuccess: (r) => qc.setQueryData(["reactions", item.id], r) });
  return <Reactions list={q.data ?? []} onToggle={(e) => toggle.mutate(e)} canReact={!ctx.project.archived_at} />;
}

export function Comments({ slug, ctx, item }: { slug: string; ctx: PropsCtx; item: WorkItem }) {
  const { t, lang } = useI18n();
  const me = useMe().data!;
  const qc = useQueryClient();
  const base = `${itemsPath(slug, ctx.project.id, item.id)}/comments`;
  const key = ["comments", item.id];
  const list = useQuery({ queryKey: key, queryFn: () => get<Comment[]>(base) });
  const box = useRef<EditorHandle>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = () => {
    qc.invalidateQueries({ queryKey: key });
    qc.invalidateQueries({ queryKey: ["subscription", item.id] });
  };
  const send = useMutation({
    mutationFn: () => {
      const v = box.current?.value();
      return post<Comment>(base, { body_json: v?.json, body_html: v?.html ?? "" });
    },
    onSuccess: () => {
      box.current?.clear();
      setError(null);
      refresh();
    },
    onError: (e) => setError(errorText(e, t)),
  });
  const act = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
      setError(null);
      refresh();
    } catch (e) {
      setError(errorText(e, t));
    }
  };
  const canComment = pcan.comment(ctx.project) && !ctx.project.archived_at;
  const upload = useEditorUpload(slug, ctx.project.id);
  const people = mentionPeople(ctx);
  return (
    <section className="wi-section" aria-label={t("comments.title")}>
      <h3>{t("comments.title")}</h3>
      <ErrorAlert message={error} />
      <ol className="comments">
        {(list.data ?? []).map((c) => {
          const m = ctx.members.find((x) => x.user_id === c.author_id);
          const mine = c.author_id === me.id;
          return (
            <li key={c.id} className="comment">
              <Avatar name={m?.name ?? ""} email={m?.email} src={m?.avatar} size={24} />
              <div className="comment-body">
                <div className="comment-head">
                  <b>{m ? m.name || m.email : c.author_name || "—"}</b>
                  {c.is_public ? <span className="badge badge-neutral" title={t("cm.makePrivate")}>{t("cm.public")}</span> : null}
                  <span className="faint small">{new Date(c.created_at).toLocaleString(lang === "id" ? "id-ID" : "en-US", { dateStyle: "medium", timeStyle: "short" })}</span>
                  {c.edited_at ? <span className="faint small">({t("comments.edited")})</span> : null}
                  <span className="grow" />
                  <CommentMenu
                    items={[
                      ...(mine && canComment && !c.hidden_at ? [{ label: t("comments.edit"), icon: <Pencil />, run: () => setEditing(c.id) }] : []),
                      ...((ctx.project.is_published || c.is_public) && (mine ? canComment : ctx.project.my_role === "admin") && !ctx.project.archived_at
                        ? [{ label: c.is_public ? t("cm.makePrivate") : t("cm.makePublic"), icon: <Globe />, run: () => act(() => post(`${base}/${c.id}/public`, { public: !c.is_public })) }]
                        : []),
                      ...(ctx.project.my_role === "admin" && !ctx.project.archived_at
                        ? [{ label: c.hidden_at ? t("comments.unhide") : t("comments.hide"), icon: c.hidden_at ? <Eye /> : <EyeOff />, run: () => act(() => post(`${base}/${c.id}/${c.hidden_at ? "unhide" : "hide"}`)) }]
                        : []),
                      ...(mine && canComment ? [{ label: t("common.delete"), icon: <Trash2 />, danger: true, run: () => act(() => del(`${base}/${c.id}`)) }] : []),
                    ]}
                  />
                </div>
                {c.hidden_at ? (
                  <p className="faint small" style={{ margin: 0 }}>{t("comments.hidden")}</p>
                ) : editing === c.id ? (
                  <EditComment c={c} people={people} upload={upload} onCancel={() => setEditing(null)} onSave={(v) => act(async () => { await patch(`${base}/${c.id}`, { body_json: v.json, body_html: v.html }); setEditing(null); })} />
                ) : (
                  <div className="prose comment-html" dangerouslySetInnerHTML={{ __html: c.body_html }} />
                )}
                <Reactions list={c.reactions} canReact={!ctx.project.archived_at} onToggle={(emoji) => act(() => post(`${base}/${c.id}/reactions`, { emoji }))} />
              </div>
            </li>
          );
        })}
      </ol>
      {canComment ? (
        <div className="comment-box card">
          <Editor handle={box} placeholder={t("comments.placeholder")} ariaLabel={t("comments.title")} minHeight={70} people={people} onUpload={upload} onSubmit={() => send.mutate()} />
          <div className="comment-actions">
            <span className="faint small">⌘/Ctrl + Enter</span>
            <button className="btn btn-sm btn-primary" disabled={send.isPending} onClick={() => !box.current?.isEmpty() && send.mutate()}>{t("comments.send")}</button>
          </div>
        </div>
      ) : null}
    </section>
  );
}

function EditComment({ c, people, upload, onSave, onCancel }: { c: Comment; people: MentionPerson[]; upload: (f: File) => Promise<string>; onSave: (v: { json: unknown; html: string }) => void; onCancel: () => void }) {
  const { t } = useI18n();
  const h = useRef<EditorHandle>(null);
  return (
    <div className="card">
      <Editor handle={h} initial={c.body_json ?? undefined} initialHtml={c.body_html} autoFocus people={people} onUpload={upload} minHeight={60} onSubmit={() => { const v = h.current?.value(); if (v) onSave(v); }} />
      <div className="comment-actions">
        <button className="btn btn-sm" onClick={onCancel}>{t("common.cancel")}</button>
        <button className="btn btn-sm btn-primary" onClick={() => { const v = h.current?.value(); if (v) onSave(v); }}>{t("comments.save")}</button>
      </div>
    </div>
  );
}

export function Attachments({ slug, ctx, item }: { slug: string; ctx: PropsCtx; item: WorkItem }) {
  const { t } = useI18n();
  const me = useMe().data!;
  const qc = useQueryClient();
  const base = `${itemsPath(slug, ctx.project.id, item.id)}/attachments`;
  const key = ["attachments", item.id];
  const list = useQuery({ queryKey: key, queryFn: () => get<FileMeta[]>(base) });
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [drag, setDrag] = useState(false);
  const [preview, setPreview] = useState<FileMeta | null>(null);
  const canAdd = ctx.editable;
  const send = async (files: FileList | File[]) => {
    setBusy(true);
    setError(null);
    for (const f of Array.from(files)) {
      try {
        await uploadFile(base, f);
      } catch (e) {
        setError(errorText(e, t));
      }
    }
    setBusy(false);
    qc.invalidateQueries({ queryKey: key });
  };
  const files = list.data ?? [];
  if (!files.length && !canAdd) return null;
  return (
    <section
      className={"wi-section" + (drag ? " dropping" : "")}
      aria-label={t("files.title")}
      onDragOver={(e) => { if (canAdd && e.dataTransfer.types.includes("Files")) { e.preventDefault(); setDrag(true); } }}
      onDragLeave={() => setDrag(false)}
      onDrop={(e) => { if (!canAdd) return; e.preventDefault(); setDrag(false); send(e.dataTransfer.files); }}
    >
      <h3 className="row-flex">
        {t("files.title")}
        {canAdd ? <button className="btn btn-sm btn-ghost" onClick={() => input.current?.click()} disabled={busy}>{busy ? t("files.uploading") : `+ ${t("files.upload")}`}</button> : null}
      </h3>
      <input ref={input} type="file" multiple hidden onChange={(e) => e.target.files && send(e.target.files)} aria-label={t("files.upload")} />
      <ErrorAlert message={error} />
      {files.length ? (
        <div className="files">
          {files.map((f) => (
            <div key={f.id} className="file card">
              {f.mime.startsWith("image/") ? (
                <button className="thumb-btn" onClick={() => setPreview(f)} aria-label={`${t("files.preview")}: ${f.file_name}`}>
                  <img src={`/api/v1/files/${f.id}`} alt="" className="file-thumb" />
                </button>
              ) : (
                <span className="file-icon">{f.mime.startsWith("image/") ? <FileImage /> : <FileText />}</span>
              )}
              <a className="ellipsis grow" href={`/api/v1/files/${f.id}`} target="_blank" rel="noreferrer">{f.file_name}</a>
              <span className="faint small">{fmtSize(f.size)}</span>
              {pcan.deleteWorkItem(ctx.project, f.uploaded_by === me.id) && ctx.editable ? (
                <button className="icon-btn" aria-label={t("common.delete")} onClick={async () => { try { await del(`${base}/${f.id}`); qc.invalidateQueries({ queryKey: key }); } catch (e) { setError(errorText(e, t)); } }}><X /></button>
              ) : null}
            </div>
          ))}
        </div>
      ) : canAdd ? (
        <div className="dropzone faint small">{t("files.drop")} <button className="link-btn" onClick={() => input.current?.click()}>{t("files.upload").toLowerCase()}</button></div>
      ) : null}
      <Dialog open={!!preview} onClose={() => setPreview(null)} title={preview?.file_name ?? ""} width={900}>
        {preview ? (
          <div className="lightbox">
            <img src={`/api/v1/files/${preview.id}`} alt={preview.file_name} />
            <a className="btn btn-sm" href={`/api/v1/files/${preview.id}`} download={preview.file_name}>{t("files.download")} · {fmtSize(preview.size)}</a>
          </div>
        ) : null}
      </Dialog>
    </section>
  );
}

export function Links({ slug, ctx, item }: { slug: string; ctx: PropsCtx; item: WorkItem }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const base = `${itemsPath(slug, ctx.project.id, item.id)}/links`;
  const key = ["links", item.id];
  const list = useQuery({ queryKey: key, queryFn: () => get<Link[]>(base) });
  const [adding, setAdding] = useState(false);
  const [url, setUrl] = useState("");
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);
  const links = list.data ?? [];
  if (!links.length && !ctx.editable) return null;
  return (
    <section className="wi-section" aria-label={t("links.title")}>
      <h3 className="row-flex">
        {t("links.title")}
        {ctx.editable ? <button className="btn btn-sm btn-ghost" onClick={() => setAdding(true)}>+ {t("links.add")}</button> : null}
      </h3>
      <ErrorAlert message={error} />
      {adding ? (
        <form className="link-form" onSubmit={async (e) => {
          e.preventDefault();
          try {
            qc.setQueryData(key, await post<Link[]>(base, { url, title }));
            setUrl(""); setTitle(""); setAdding(false); setError(null);
          } catch (err) { setError(errorText(err, t)); }
        }}>
          <input className="input" placeholder="https://" aria-label={t("links.url")} value={url} onChange={(e) => setUrl(e.target.value)} autoFocus required />
          <input className="input" placeholder={t("links.linkTitle")} aria-label={t("links.linkTitle")} value={title} onChange={(e) => setTitle(e.target.value)} />
          <button className="btn btn-sm btn-primary">{t("common.add")}</button>
          <button type="button" className="btn btn-sm" onClick={() => setAdding(false)}>{t("common.cancel")}</button>
        </form>
      ) : null}
      {links.length ? (
        <div className="rows card">
          {links.map((l) => (
            <div key={l.id} className="row">
              <LinkIcon aria-hidden className="link-icon" />
              <a className="ellipsis grow" href={l.url} target="_blank" rel="noopener noreferrer nofollow">{l.title || l.url}</a>
              {ctx.editable ? <button className="icon-btn" aria-label={t("common.remove")} onClick={async () => { await del(`${base}/${l.id}`); qc.invalidateQueries({ queryKey: key }); }}><X /></button> : null}
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}

export function SubscribeButton({ slug, ctx, item }: { slug: string; ctx: PropsCtx; item: WorkItem }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const url = `${itemsPath(slug, ctx.project.id, item.id)}/subscription`;
  const key = ["subscription", item.id];
  const q = useQuery({ queryKey: key, queryFn: () => get<{ subscribed: boolean }>(url) });
  const on = !!q.data?.subscribed;
  const toggle = useMutation({
    mutationFn: () => (on ? del<{ subscribed: boolean }>(url) : post<{ subscribed: boolean }>(url)),
    onSuccess: (r) => qc.setQueryData(key, r),
  });
  return (
    <button className="btn btn-sm btn-ghost" title={t("sub.hint")} onClick={() => toggle.mutate()} disabled={toggle.isPending || q.isLoading} aria-pressed={on}>
      {on ? <BellOff /> : <Bell />}
      {on ? t("sub.unsubscribe") : t("sub.subscribe")}
    </button>
  );
}

/** "…" on a comment: edit, show on the public board, hide, delete (whichever apply). */
function CommentMenu({ items }: { items: { label: string; icon: ReactNode; run: () => void; danger?: boolean }[] }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  if (!items.length) return null;
  return (
    <Popover
      open={open}
      onClose={close}
      right
      trigger={
        <button type="button" className="icon-btn icon-btn-sm" aria-label={t("comments.actions")} title={t("comments.actions")} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>
          <Ellipsis />
        </button>
      }
    >
      <div className="menu" role="menu">
        {items.map((it) => (
          <button key={it.label} type="button" role="menuitem" className={"menu-item" + (it.danger ? " danger" : "")} onClick={() => (close(), it.run())}>
            {it.icon}
            {it.label}
          </button>
        ))}
      </div>
    </Popover>
  );
}
