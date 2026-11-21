import { ChevronDown } from "lucide-react";
import { useCallback, useRef, useState, type FormEvent, type ReactNode } from "react";
import { useNavigate, useParams } from "react-router";
import type { MyWorkspace, WorkItem } from "../api/types";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { pcan } from "../lib/perm";
import { useProjectCtx } from "../lib/projectCtx";
import { useProject, useProjects } from "../lib/projects";
import { useShellEvent } from "../lib/shell";
import { useCreateItem } from "../lib/workItems";
import { ProjectIcon } from "../pages/Projects";
import { Dialog } from "./Dialog";
import { AiAssist } from "./AiAssist";
import { Editor, type EditorHandle, type EditorValue } from "./Editor";
import { Picker } from "./Picker";
import { ErrorAlert } from "./ui";
import { AssigneesProp, CycleProp, DateProp, EstimateProp, LabelsProp, ModulesProp, PriorityProp, StateProp, type PropsCtx } from "./WorkItemProps";

/** Plane's create modal: project pill, title, description, a row of property chips, then
 * "create more", Save as draft and Create. */
export function CreateItemDialog({
  slug,
  open,
  onClose,
  ctx,
  onCreated,
  defaults,
  lead,
}: {
  slug: string;
  open: boolean;
  onClose: () => void;
  ctx: PropsCtx;
  onCreated: (w: WorkItem) => void;
  defaults?: Partial<WorkItem>;
  lead?: ReactNode;
}) {
  const { t } = useI18n();
  const create = useCreateItem(slug, ctx.project.id);
  const blank = (): WorkItem => ({
    id: "", project_id: ctx.project.id, sequence: 0, title: "", state_id: ctx.states.find((s) => s.is_default)?.id ?? ctx.states[0]!.id,
    priority: "none", start_date: null, due_date: null, estimate: null, parent_id: null, sort_order: 0, created_by: "", created_at: "",
    updated_at: "", completed_at: null, archived_at: null, assignee_ids: ctx.project.default_assignee_id ? [ctx.project.default_assignee_id] : [], label_ids: [], sub_item_count: 0,
    cycle_id: null, module_ids: [], ...defaults,
  });
  const [draft, setDraft] = useState<WorkItem>(blank);
  const [desc, setDesc] = useState<EditorValue | null>(null);
  const editorRef = useRef<EditorHandle>(null);
  const [more, setMore] = useState(false);
  const [editorKey, setEditorKey] = useState(0);
  // A different project means different states, labels and members: start over.
  const [forProject, setForProject] = useState(ctx.project.id);
  if (forProject !== ctx.project.id) {
    setForProject(ctx.project.id);
    setDraft(blank());
  }
  const onPatch = (b: Partial<WorkItem>) => setDraft((d) => ({ ...d, ...b }));
  const submit = (e: FormEvent) => {
    e.preventDefault();
    // Which button submitted: state set in its onClick would not be visible yet here.
    const asDraft = ((e.nativeEvent as SubmitEvent).submitter as HTMLButtonElement | null)?.dataset.draft === "1";
    create.mutate(
      {
        is_draft: asDraft,
        title: draft.title, state_id: draft.state_id, priority: draft.priority, assignee_ids: draft.assignee_ids, label_ids: draft.label_ids,
        start_date: draft.start_date, due_date: draft.due_date, estimate: draft.estimate, cycle_id: draft.cycle_id ?? null, module_ids: draft.module_ids ?? [],
        description_json: desc?.json ?? null, description_html: desc?.html ?? "",
      },
      {
        onSuccess: (w) => {
          setDraft(blank());
          setDesc(null);
          setEditorKey((k) => k + 1);
          if (!more) {
            onClose();
            if (!w.is_draft) onCreated(w);
          }
        },
      },
    );
  };
  return (
    <Dialog open={open} onClose={onClose} title={t("wi.new")} width={720} lead={lead}>
      <form className="create-item" onSubmit={submit}>
        <ErrorAlert message={create.isError ? errorText(create.error, t) : null} />
        <input className="create-item-title" autoFocus placeholder={t("wi.titlePlaceholder")} aria-label={t("wi.title")} value={draft.title} onChange={(e) => onPatch({ title: e.target.value })} maxLength={255} required />
        <div className="create-item-desc">
          <Editor key={`${ctx.project.id}:${editorKey}`} handle={editorRef} placeholder={t("wi.descriptionPlaceholder")} ariaLabel={t("wi.description")} onChange={setDesc} debounce={150} minHeight={120} />
          <AiAssist slug={slug} title={draft.title} onUse={(html) => editorRef.current?.insert(html)} />
        </div>
        <div className="chip-row">
          <StateProp item={draft} ctx={ctx} onPatch={onPatch} variant="pill" />
          <PriorityProp item={draft} ctx={ctx} onPatch={onPatch} variant="pill" />
          <AssigneesProp item={draft} ctx={ctx} onPatch={onPatch} variant="pill" />
          <LabelsProp item={draft} ctx={ctx} onPatch={onPatch} variant="pill" />
          <DateProp item={draft} field="start_date" ctx={ctx} onPatch={onPatch} variant="pill" />
          <DateProp item={draft} field="due_date" ctx={ctx} onPatch={onPatch} variant="pill" />
          <EstimateProp item={draft} ctx={ctx} onPatch={onPatch} variant="pill" />
          <CycleProp item={draft} ctx={ctx} onPatch={onPatch} variant="pill" />
          <ModulesProp item={draft} ctx={ctx} onPatch={onPatch} variant="pill" />
        </div>
        <div className="create-item-foot">
          <label className="switch-label">
            <input type="checkbox" role="switch" className="switch" checked={more} onChange={(e) => setMore(e.target.checked)} />
            {t("wi.createMore")}
          </label>
          <span className="grow" />
          <button type="button" className="btn" onClick={onClose}>{t("common.discard")}</button>
          <button className="btn" data-draft="1" disabled={create.isPending || !draft.title.trim()}>{t("draft.save")}</button>
          <button className="btn btn-primary" disabled={create.isPending || !draft.title.trim()}>{t("wi.create")}</button>
        </div>
      </form>
    </Dialog>
  );
}

/** "New work item" from the sidebar or N then I: the same modal with a project picker,
 * starting in the current project (or the first one you can create in). */
export function GlobalCreate({ ws }: { ws: MyWorkspace }) {
  const { t } = useI18n();
  const navigate = useNavigate();
  const { projectId } = useParams();
  const projects = useProjects(ws.slug);
  const [open, setOpen] = useState(false);
  const [pid, setPid] = useState("");
  const allowed = (projects.data ?? []).filter((p) => p.is_member && !p.archived_at && pcan.createWorkItem(p));
  useShellEvent(
    "create-item",
    useCallback(() => {
      const start = allowed.find((p) => p.id === projectId) ?? allowed[0];
      if (!start) {
        navigate(`/${ws.slug}/projects`);
        return;
      }
      setPid(start.id);
      setOpen(true);
    }, [allowed, projectId, navigate, ws.slug]),
  );
  const project = useProject(ws.slug, pid);
  const ctx = useProjectCtx(ws.slug, open && pid ? project.data : undefined);
  if (!open || !ctx) return null;
  const lead = (
    <Picker
      label={t("wi.project")}
      className="prop-btn prop-chip create-item-project"
      value={[pid]}
      onChange={(v) => v[0] && setPid(v[0])}
      options={allowed.map((p) => ({ value: p.id, label: p.name, hint: p.identifier, icon: <ProjectIcon p={p} size={16} /> }))}
      trigger={
        <>
          <ProjectIcon p={ctx.project} size={16} />
          <span>{ctx.project.name}</span>
          <ChevronDown aria-hidden />
        </>
      }
    />
  );
  return (
    <CreateItemDialog
      slug={ws.slug}
      open={open}
      onClose={() => setOpen(false)}
      ctx={ctx}
      lead={lead}
      onCreated={(w) => navigate(`/${ws.slug}/projects/${w.project_id}?peek=${w.id}`)}
    />
  );
}
