import { useQueryClient } from "@tanstack/react-query";
import { Archive, Boxes, CalendarPlus, Check, ChevronsRight, CircleCheck, Contrast, CornerLeftUp, Ellipsis, History, Link as LinkIcon, Maximize2, Tag, Triangle, UserRound, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useNavigate } from "react-router";
import { ApiError, get, post, del } from "../api/client";
import type { Activity, Relation, RelationKind, WorkItem } from "../api/types";
import { useI18n } from "../i18n";
import { copyText } from "../lib/copy";
import { errorText } from "../lib/errors";
import { pcan } from "../lib/perm";
import { itemsPath, keyOf, useActivity, useArchiveItem, useCreateItem, useRelations, useUpdateItem, useWorkItem, useWorkItems, wiq } from "../lib/workItems";
import { Avatar } from "./Avatar";
import { Attachments, Comments, ItemReactions, Links, SubscribeButton, mentionPeople, useEditorUpload } from "./Collab";
import { Dialog } from "./Dialog";
import { useItemActions } from "./ItemActions";
import { Popover } from "./Layout";
import { ProjectBar } from "./PageHeader";
import { Editor } from "./Editor";
import { Picker } from "./Picker";
import { StateIcon, fmtDate } from "./Props";
import { ErrorAlert, FullPageSpinner } from "./ui";
import { AssigneesProp, CycleProp, ModulesProp, DateProp, EstimateProp, LabelsProp, PriorityProp, StateProp, type PropsCtx } from "./WorkItemProps";

/** A work item, full page or in the peek panel. */
export function WorkItemDetail({ slug, ctx, id, peek, onClose }: { slug: string; ctx: PropsCtx; id: string; peek?: boolean; onClose?: () => void }) {
  const { t } = useI18n();
  const p = ctx.project;
  const q = useWorkItem(slug, p.id, id);
  const update = useUpdateItem(slug, p.id);
  const archive = useArchiveItem(slug, p.id);
  const navigate = useNavigate();
  const [copied, setCopied] = useState(false);
  const upload = useEditorUpload(slug, ctx.project.id);
  const stamp = useRef("");
  const stampKey = useRef("");
  const textChain = useRef<Promise<void>>(Promise.resolve());

  if (q.isLoading) return <FullPageSpinner />;
  if (!q.data) return <div className="empty">{t("wi.notFound")}</div>;
  const item = q.data;
  const conflict = update.error instanceof ApiError && update.error.status === 409;
  // The version the open editor's text is based on; reset when the editor is reloaded.
  const editorKey = `${item.id}:${conflict ? item.updated_at : ""}`;
  if (stampKey.current !== editorKey) {
    stampKey.current = editorKey;
    stamp.current = item.updated_at;
  }
  const key = keyOf(p.identifier, item.sequence);
  const editable = ctx.editable && !item.archived_at;
  const pctx = { ...ctx, editable };
  const onPatch = (body: Partial<WorkItem>) => update.mutate({ id: item.id, body });
  // Text edits carry the version they started from; a 409 means someone else saved first.
  // Saves run one at a time; the server only counts other people's text changes as conflicts.
  const onText = (body: Partial<WorkItem>) => {
    textChain.current = textChain.current.then(async () => {
      try {
        await update.mutateAsync({ id: item.id, body: { ...body, expected_updated_at: stamp.current } });
      } catch {
        /* shown through update.error */
      }
    });
  };

  const fullUrl = `/${slug}/projects/${p.id}/work-items/${item.id}`;
  const parent = item.parent_id;

  const copy = async () => {
    setCopied(await copyText(location.origin + fullUrl));
    setTimeout(() => setCopied(false), 1500);
  };
  const actions = (
    <>
      <SubscribeButton slug={slug} ctx={ctx} item={item} />
      <button className="icon-btn bordered" onClick={copy} aria-label={copied ? t("common.copied") : t("wi.copyLink")} title={copied ? t("common.copied") : t("wi.copyLink")}>
        {copied ? <Check /> : <LinkIcon />}
      </button>
      <DetailMenu
        slug={slug}
        ctx={ctx}
        item={item}
        // Archiving or deleting leaves the item: back to the list it came from.
        onGone={() => (peek && onClose ? onClose() : navigate(`/${slug}/projects/${p.id}${item.archived_at ? "/archives" : ""}`))}
      />
    </>
  );
  const props = <PropsBlock slug={slug} ctx={pctx} item={item} onPatch={onPatch} />;

  return (
    <div className={peek ? "wi wi-peek" : "wi"}>
      {peek ? (
        <div className="wi-peek-head">
          {onClose ? (
            <button className="icon-btn" aria-label={t("common.close")} title={t("common.close")} onClick={onClose}>
              <ChevronsRight />
            </button>
          ) : null}
          <Link className="icon-btn" to={fullUrl} aria-label={t("wi.openFull")} title={t("wi.openFull")}>
            <Maximize2 />
          </Link>
          <span className="grow" />
          {actions}
        </div>
      ) : (
        <ProjectBar ctx={ctx} crumbs={[{ label: key }]}>
          {actions}
        </ProjectBar>
      )}
      <ErrorAlert message={update.isError && !conflict ? errorText(update.error, t) : archive.isError ? errorText(archive.error, t) : null} />
      {item.archived_at ? (
        <div className="wi-archived">
          <Archive aria-hidden />
          <span className="grow">{t("wi.archivedNote")}</span>
          {pcan.viewArchives(p) ? <Link className="btn btn-sm btn-ghost" to={`/${slug}/projects/${p.id}/archives`}>{t("arch.goTo")}</Link> : null}
          {ctx.editable ? <button className="btn btn-sm" onClick={() => archive.mutate({ id: item.id, on: false })}>{t("wi.unarchive")}</button> : null}
        </div>
      ) : null}
      <div className="wi-body">
        <div className="wi-main">
          <div className="wi-crumb">
            {parent ? (
              <>
                <ParentLink slug={slug} ctx={ctx} parentId={parent} />
                <span className="faint">/</span>
              </>
            ) : null}
            <span className="wi-key">{key}</span>
            {item.archived_at ? <span className="badge badge-neutral">{t("wi.archived")}</span> : null}
          </div>
          {conflict ? <div className="alert alert-warning">{t("wi.conflict")}</div> : null}
          {item.is_draft ? <DraftBanner slug={slug} ctx={ctx} item={item} /> : null}
          {item.in_intake ? (
            <div className="alert alert-warning draft-banner">
              <span className="grow">{t("wi.inIntake")}</span>
              <Link className="btn btn-sm" to={`/${slug}/projects/${ctx.project.id}/intake/${item.id}`}>{t("wi.toIntake")}</Link>
            </div>
          ) : null}
          <TitleInput value={item.title} editable={editable} onSave={(title) => onText({ title })} />
          <div className="wi-meta" role="group" aria-label={t("wi.properties")}>
            <StateProp item={item} ctx={pctx} onPatch={onPatch} />
            <PriorityProp item={item} ctx={pctx} onPatch={onPatch} />
            <AssigneesProp item={item} ctx={pctx} onPatch={onPatch} />
            <DateProp item={item} field="start_date" ctx={pctx} onPatch={onPatch} />
            <DateProp item={item} field="due_date" ctx={pctx} onPatch={onPatch} />
          </div>
          <div className="wi-desc">
            <Editor
              key={editorKey}
              initial={item.description_json ?? undefined}
              initialHtml={item.description_html}
              editable={editable}
              placeholder={t("wi.descriptionPlaceholder")}
              ariaLabel={t("wi.description")}
              onChange={(v) => onText({ description_json: v.json, description_html: v.html })}
              people={mentionPeople(ctx)}
              onUpload={upload}
            />
          </div>
          <ItemReactions slug={slug} ctx={ctx} item={item} />
          <SubItems slug={slug} ctx={pctx} item={item} />
          <Relations slug={slug} ctx={pctx} item={item} />
          <Attachments slug={slug} ctx={pctx} item={item} />
          <Links slug={slug} ctx={pctx} item={item} />
          {peek ? <div className="wi-props-inline">{props}</div> : null}
          <ActivityFeed slug={slug} ctx={ctx} item={item} />
          <Comments slug={slug} ctx={ctx} item={item} />
        </div>
        {peek ? null : (
          <aside className="wi-side" aria-label={t("wi.properties")}>
            {props}
          </aside>
        )}
      </div>
    </div>
  );
}

function Prop({ label, icon, children }: { label: string; icon: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="wi-prop">
      <span className="wi-prop-label">
        {icon}
        <span className="ellipsis">{label}</span>
      </span>
      <div className="wi-prop-value">{children}</div>
    </div>
  );
}

/** Plane's slim properties panel: details, project structure, then who and when. */
function PropsBlock({ slug, ctx, item, onPatch }: { slug: string; ctx: PropsCtx; item: WorkItem; onPatch: (b: Partial<WorkItem>) => void }) {
  const { t, lang } = useI18n();
  const p = ctx.project;
  const creator = ctx.members.find((m) => m.user_id === item.created_by);
  const when = (iso: string) => new Date(iso).toLocaleString(lang === "id" ? "id-ID" : "en-US", { month: "short", day: "numeric", year: "numeric", hour: "numeric", minute: "2-digit" });
  return (
    <div className="wi-props">
      <div className="wi-props-title">{t("wi.properties")}</div>
      <div className="wi-props-group">
        <Prop label={t("wi.labels")} icon={<Tag />}><LabelsProp item={item} ctx={ctx} onPatch={onPatch} /></Prop>
        {p.estimate_type !== "none" ? <Prop label={t("wi.estimate")} icon={<Triangle />}><EstimateProp item={item} ctx={ctx} onPatch={onPatch} /></Prop> : null}
        <Prop label={t("wi.parent")} icon={<CornerLeftUp />}><ParentProp slug={slug} ctx={ctx} item={item} onPatch={onPatch} /></Prop>
      </div>
      {p.feature_cycles || p.feature_modules ? (
        <div className="wi-props-group">
          {p.feature_cycles ? <Prop label={t("wi.cycle")} icon={<Contrast />}><CycleProp item={item} ctx={ctx} onPatch={onPatch} /></Prop> : null}
          {p.feature_modules ? <Prop label={t("wi.modules")} icon={<Boxes />}><ModulesProp item={item} ctx={ctx} onPatch={onPatch} /></Prop> : null}
        </div>
      ) : null}
      <div className="wi-audit">
        <span><UserRound aria-hidden />{t("wi.createdBy", { name: creator ? creator.name || creator.email : "—" })}</span>
        <span><CalendarPlus aria-hidden />{t("wi.createdOn", { when: when(item.created_at) })}</span>
        <span><History aria-hidden />{t("wi.updatedOn", { when: when(item.updated_at) })}</span>
        {item.completed_at ? <span><CircleCheck aria-hidden />{t("wi.completedOn", { when: when(item.completed_at) })}</span> : null}
      </div>
    </div>
  );
}

function DetailMenu({ slug, ctx, item, onGone }: { slug: string; ctx: PropsCtx; item: WorkItem; onGone: () => void }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const a = useItemActions(slug, ctx, item, { archived: onGone, deleted: onGone });
  if (!a.any) return null;
  return (
    <>
      <Popover
        open={open}
        onClose={close}
        right
        trigger={
          <button className="icon-btn bordered" onClick={() => setOpen(!open)} aria-label={t("wi.actions")} aria-haspopup="menu" aria-expanded={open}>
            <Ellipsis />
          </button>
        }
      >
        <div className="menu" role="menu">
          {a.entries(close)}
        </div>
      </Popover>
      {a.dialogs}
    </>
  );
}

function TitleInput({ value, editable, onSave }: { value: string; editable: boolean; onSave: (v: string) => void }) {
  const { t } = useI18n();
  const [v, setV] = useState(value);
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => setV(value), [value]);
  useEffect(() => {
    const el = ref.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = el.scrollHeight + "px";
    }
  }, [v]);
  const save = () => {
    const clean = v.trim().replace(/\s+/g, " ");
    if (clean && clean !== value) onSave(clean);
    else setV(value);
  };
  return (
    <textarea
      ref={ref}
      className="wi-title"
      aria-label={t("wi.title")}
      value={v}
      rows={1}
      readOnly={!editable}
      maxLength={255}
      onChange={(e) => setV(e.target.value.replace(/\n/g, " "))}
      onBlur={save}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          (e.target as HTMLTextAreaElement).blur();
        }
        if (e.key === "Escape") {
          setV(value);
          (e.target as HTMLTextAreaElement).blur();
        }
      }}
    />
  );
}

function ParentLink({ slug, ctx, parentId }: { slug: string; ctx: PropsCtx; parentId: string }) {
  const parent = useWorkItem(slug, ctx.project.id, parentId);
  if (!parent.data) return null;
  const s = ctx.states.find((x) => x.id === parent.data!.state_id);
  return (
    <Link to={`/${slug}/projects/${ctx.project.id}/work-items/${parentId}`} className="wi-parent small">
      {s ? <StateIcon group={s.group} color={s.color} size={12} /> : null}
      <span className="mono faint">{keyOf(ctx.project.identifier, parent.data.sequence)}</span>
      <span className="ellipsis">{parent.data.title}</span>
    </Link>
  );
}

function ParentProp({ slug, ctx, item, onPatch }: { slug: string; ctx: PropsCtx; item: WorkItem; onPatch: (b: Partial<WorkItem>) => void }) {
  const { t } = useI18n();
  const all = useWorkItems(slug, ctx.project.id, "", ctx.editable);
  const current = (all.data ?? []).find((w) => w.id === item.parent_id);
  return (
    <Picker
      label={t("wi.parent")}
      disabled={!ctx.editable}
      allowClear={t("wi.noParent")}
      options={(all.data ?? []).filter((w) => w.id !== item.id).map((w) => ({ value: w.id, label: w.title, hint: keyOf(ctx.project.identifier, w.sequence) }))}
      value={item.parent_id ? [item.parent_id] : []}
      onChange={(v) => onPatch({ parent_id: v[0] ?? null })}
      trigger={<span className={current ? "ellipsis" : "faint"}>{current ? `${keyOf(ctx.project.identifier, current.sequence)} ${current.title}` : t("wi.noParent")}</span>}
    />
  );
}

function SubItems({ slug, ctx, item }: { slug: string; ctx: PropsCtx; item: WorkItem }) {
  const { t } = useI18n();
  const subs = useWorkItems(slug, ctx.project.id, `parent=${item.id}`);
  const create = useCreateItem(slug, ctx.project.id);
  const qc = useQueryClient();
  const [adding, setAdding] = useState(false);
  const [title, setTitle] = useState("");
  const list = subs.data ?? [];
  if (!list.length && !ctx.editable) return null;
  return (
    <section className="wi-section">
      <h3>{t("wi.subItems")} {list.length ? <span className="faint small">{list.filter((s) => s.completed_at).length}/{list.length}</span> : null}</h3>
      <div className="rows card">
        {list.map((s) => (
          <ItemRow key={s.id} slug={slug} ctx={ctx} item={s} />
        ))}
        {adding ? (
          <form
            className="row"
            onSubmit={(e) => {
              e.preventDefault();
              if (!title.trim()) return;
              const tt = title;
              setTitle("");
              create.mutate({ title: tt, parent_id: item.id }, {
                onSuccess: () => qc.invalidateQueries({ queryKey: wiq.one(slug, ctx.project.id, item.id) }),
                onError: () => setTitle((cur) => cur || tt),
              });
            }}
          >
            <input className="inline-input" autoFocus value={title} placeholder={t("wi.quickAddPlaceholder")} onChange={(e) => setTitle(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setAdding(false)} onBlur={() => !title && setAdding(false)} aria-label={t("wi.title")} />
          </form>
        ) : ctx.editable ? (
          <button className="row row-btn faint" onClick={() => setAdding(true)}>{t("wi.addSubItem")}</button>
        ) : null}
      </div>
    </section>
  );
}

export function ItemRow({ slug, ctx, item }: { slug: string; ctx: PropsCtx; item: { id: string; sequence: number; title: string; state_id: string } }) {
  const s = ctx.states.find((x) => x.id === item.state_id);
  return (
    <Link to={`/${slug}/projects/${ctx.project.id}/work-items/${item.id}`} className="row row-link">
      {s ? <StateIcon group={s.group} color={s.color} /> : null}
      <span className="mono faint small">{keyOf(ctx.project.identifier, item.sequence)}</span>
      <span className="ellipsis grow">{item.title}</span>
    </Link>
  );
}

const REL_KINDS: RelationKind[] = ["blocks", "blocked_by", "relates_to", "duplicate_of"];

function Relations({ slug, ctx, item }: { slug: string; ctx: PropsCtx; item: WorkItem }) {
  const { t } = useI18n();
  const rel = useRelations(slug, ctx.project.id, item.id);
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refresh = () => {
    qc.invalidateQueries({ queryKey: wiq.relations(slug, ctx.project.id, item.id) });
    qc.invalidateQueries({ queryKey: wiq.activity(slug, ctx.project.id, item.id) });
  };
  const list = rel.data ?? [];
  if (!list.length && !ctx.editable) return null;
  return (
    <section className="wi-section">
      <h3 className="row-flex">
        {t("wi.relations")}
        {ctx.editable ? <button className="btn btn-sm btn-ghost" onClick={() => setOpen(true)}>+ {t("wi.addRelation")}</button> : null}
      </h3>
      <ErrorAlert message={error} />
      {list.length ? (
        <div className="rows card">
          {list.map((r: Relation) => (
            <div className="row" key={r.id}>
              <span className="faint small" style={{ width: 100 }}>{t(`rel.${r.kind}`)}</span>
              <Link className="grow ellipsis row-link-inline" to={`/${slug}/projects/${r.item.project_id}/work-items/${r.item.id}`}>
                <span className="mono faint small">{keyOf(r.item.identifier, r.item.sequence)}</span> {r.item.title}
              </Link>
              {ctx.editable ? (
                <button className="icon-btn" aria-label={t("common.remove")} onClick={() => del<void>(`${itemsPath(slug, ctx.project.id, item.id)}/relations/${r.id}`).then(refresh, (e) => setError(errorText(e, t)))}><X /></button>
              ) : null}
            </div>
          ))}
        </div>
      ) : null}
      <AddRelation open={open} onClose={() => setOpen(false)} slug={slug} ctx={ctx} item={item} onDone={refresh} />
    </section>
  );
}

function AddRelation({ open, onClose, slug, ctx, item, onDone }: { open: boolean; onClose: () => void; slug: string; ctx: PropsCtx; item: WorkItem; onDone: () => void }) {
  const { t } = useI18n();
  const [kind, setKind] = useState<RelationKind>("relates_to");
  const [q, setQ] = useState("");
  const [results, setResults] = useState<{ id: string; label: string }[]>([]);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!open) return;
    const h = setTimeout(async () => {
      const found: { id: string; label: string }[] = [];
      const m = q.trim().match(/^([A-Za-z][A-Za-z0-9]*)-(\d+)$/);
      if (m) {
        try {
          const r = await get<{ project_id: string; id: string }>(`/workspaces/${slug}/work-items/by-key/${q.trim()}`);
          const w = await get<WorkItem>(itemsPath(slug, r.project_id, r.id));
          found.push({ id: w.id, label: `${q.trim().toUpperCase()} ${w.title}` });
        } catch {
          /* not found */
        }
      }
      const list = await get<WorkItem[]>(`${itemsPath(slug, ctx.project.id)}?limit=20${q.trim() ? `&q=${encodeURIComponent(q.trim())}` : ""}`).catch(() => []);
      for (const w of list) if (w.id !== item.id && !found.some((f) => f.id === w.id)) found.push({ id: w.id, label: `${keyOf(ctx.project.identifier, w.sequence)} ${w.title}` });
      setResults(found);
    }, 200);
    return () => clearTimeout(h);
  }, [q, open, slug, ctx.project, item.id]);
  const add = async (other: string) => {
    try {
      await post(`${itemsPath(slug, ctx.project.id, item.id)}/relations`, { kind, work_item_id: other });
      onDone();
      setQ("");
      onClose();
    } catch (e) {
      setError(errorText(e, t));
    }
  };
  return (
    <Dialog open={open} onClose={onClose} title={t("wi.addRelation")} width={520}>
      <div className="form">
        <ErrorAlert message={error} />
        <div className="seg">
          {REL_KINDS.map((k) => (
            <button key={k} type="button" className={kind === k ? "on" : ""} onClick={() => setKind(k)}>{t(`rel.${k}`)}</button>
          ))}
        </div>
        <input className="input" autoFocus placeholder={t("wi.searchItem")} value={q} onChange={(e) => setQ(e.target.value)} aria-label={t("wi.searchItem")} />
        <div className="rows card" style={{ maxHeight: 280, overflow: "auto" }}>
          {results.map((r) => (
            <button key={r.id} className="row row-btn" onClick={() => add(r.id)}>{r.label}</button>
          ))}
          {!results.length ? <div className="row faint">{t("common.noResults")}</div> : null}
        </div>
      </div>
    </Dialog>
  );
}

export function timeAgo(iso: string, t: (k: "time.now" | "time.min" | "time.hour" | "time.day", v?: Record<string, number>) => string, lang: string): string {
  const s = (Date.now() - new Date(iso).getTime()) / 1000;
  if (s < 60) return t("time.now");
  if (s < 3600) return t("time.min", { n: Math.floor(s / 60) });
  if (s < 86400) return t("time.hour", { n: Math.floor(s / 3600) });
  if (s < 7 * 86400) return t("time.day", { n: Math.floor(s / 86400) });
  return new Date(iso).toLocaleDateString(lang === "id" ? "id-ID" : "en-US");
}

function ActivityFeed({ slug, ctx, item }: { slug: string; ctx: PropsCtx; item: WorkItem }) {
  const { t, lang } = useI18n();
  const act = useActivity(slug, ctx.project.id, item.id);
  const name = (id: string) => {
    const m = ctx.members.find((x) => x.user_id.replace(/-/g, "") === id.replace(/-/g, ""));
    return m ? m.name || m.email : "—";
  };
  const value = (a: Activity, v: string | null): string => {
    if (!v) return "";
    if (a.field === "state") return ctx.states.find((s) => s.id.replace(/-/g, "") === v)?.name ?? "—";
    if (a.field === "priority") return t(`prio.${v as "none"}`);
    if (a.field === "assignee") return name(v);
    if (a.field === "label") return ctx.labels.find((l) => l.id.replace(/-/g, "") === v)?.name ?? "—";
    if (a.field === "module") return ctx.modules?.find((m) => m.id.replace(/-/g, "") === v)?.name ?? "—";
    if (a.field === "cycle") return ctx.cycles?.find((c) => c.id.replace(/-/g, "") === v)?.name ?? "—";
    if (a.field === "due_date" || a.field === "start_date") return fmtDate(v, lang);
    if (a.field === "relation") return t(`rel.${v.split(":")[0] as RelationKind}`);
    if (a.field === "parent") return "";
    return v;
  };
  const line = (a: Activity): string => {
    const who = name(a.actor_id);
    const field = a.field ? t(`field.${a.field}` as "field.title") : "";
    if (a.verb === "created") return t("act.created", { who });
    if (a.verb === "archived") return t("act.archived", { who });
    if (a.verb === "restored") return t("act.restored", { who });
    if (a.verb === "added") return t("act.added", { who, field, value: value(a, a.new_value) });
    if (a.verb === "removed") return t("act.removed", { who, field, value: value(a, a.old_value) });
    const v = value(a, a.new_value);
    return v ? t("act.updatedTo", { who, field, value: v }) : t("act.updated", { who, field });
  };
  return (
    <section className="wi-section">
      <h3>{t("wi.activity")}</h3>
      <ol className="activity">
        {(act.data ?? []).map((a) => {
          const m = ctx.members.find((x) => x.user_id === a.actor_id);
          return (
            <li key={a.id}>
              <Avatar name={m?.name ?? ""} email={m?.email} src={m?.avatar} size={18} />
              <span className="grow">{line(a)}</span>
              <span className="faint small" title={new Date(a.created_at).toLocaleString()}>{timeAgo(a.created_at, t, lang)}</span>
            </li>
          );
        })}
      </ol>
    </section>
  );
}

function DraftBanner({ slug, ctx, item }: { slug: string; ctx: PropsCtx; item: WorkItem }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const publish = async () => {
    await post(itemsPath(slug, ctx.project.id, item.id) + "/publish");
    qc.invalidateQueries({ queryKey: wiq.one(slug, ctx.project.id, item.id) });
    qc.invalidateQueries({ queryKey: wiq.allLists(slug, ctx.project.id) });
    qc.invalidateQueries({ queryKey: ["drafts", slug] });
  };
  return (
    <div className="alert alert-warning draft-banner">
      <span className="badge">{t("draft.badge")}</span>
      <span className="grow">{t("draft.hint")}</span>
      <button className="btn btn-sm btn-primary" onClick={publish}>{t("draft.publish")}</button>
    </div>
  );
}
