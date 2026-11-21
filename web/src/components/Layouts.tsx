import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  closestCorners,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import { SortableContext, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Circle, Ellipsis, ExternalLink, Link as LinkIcon, ListTree, Maximize2, Minimize2, Plus } from "lucide-react";
import { useCallback, useMemo, useRef, useState, type ReactNode } from "react";
import { useNavigate } from "react-router";
import type { WorkItem } from "../api/types";
import { useI18n } from "../i18n";
import { patchForGroup, sortBetween, type Group, type ViewState } from "../lib/view";
import { keyOf } from "../lib/workItems";
import { Avatar } from "./Avatar";
import { Dot } from "./ColorPicker";
import { Popover } from "./Layout";
import { PriorityIcon, StateIcon } from "./Props";
import { AssigneesProp, CycleProp, DateProp, EstimateProp, LabelsProp, ModulesProp, PriorityProp, StateProp, type PropsCtx } from "./WorkItemProps";

export interface LayoutProps {
  groups: Group[];
  ctx: PropsCtx;
  view: ViewState;
  onOpen: (id: string) => void;
  activeId: string | null;
  onPatch: (id: string, body: Partial<WorkItem>) => void;
  quickAdd: (g: Group) => ReactNode;
  /** The group header's "+": create an item with the group's value. */
  onCreateIn?: (g: Group) => void;
}

export function GroupIcon({ g }: { g: Group }) {
  switch (g.kind) {
    case "state":
      return <StateIcon group={g.state!.group} color={g.state!.color} size={16} />;
    case "priority":
      return <PriorityIcon p={g.priority!} size={16} />;
    case "assignee":
      return g.member ? <Avatar name={g.member.name} email={g.member.email} src={g.member.avatar} size={20} /> : <Avatar name="?" size={20} />;
    case "label":
      return <Dot color={g.label?.color ?? "#666666"} size={12} />;
    default:
      return <Circle className="faint" size={16} />;
  }
}

export function groupName(g: Group, t: ReturnType<typeof useI18n>["t"]): string {
  switch (g.kind) {
    case "state":
      return g.state!.name;
    case "priority":
      return t(`prio.${g.priority!}`);
    case "assignee":
      return g.member ? g.member.name || g.member.email : t("noAssignee");
    case "label":
      return g.label ? g.label.name : t("noLabel");
    default:
      return t("groupBy.none");
  }
}

/** Property chips on a row or card, in Plane's order, per the view's display settings. */
export function ItemChips({ item, ctx, view, onPatch }: { item: WorkItem; ctx: PropsCtx; view: ViewState; onPatch: (b: Partial<WorkItem>) => void }) {
  const d = view.display;
  return (
    <span className="item-chips" onClick={(e) => e.stopPropagation()} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
      {d.state ? <StateProp item={item} ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.priority ? <PriorityProp item={item} ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.start_date ? <DateProp item={item} field="start_date" ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.due_date ? <DateProp item={item} field="due_date" ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.assignees ? <AssigneesProp item={item} ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.modules ? <ModulesProp item={item} ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.cycle ? <CycleProp item={item} ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.estimate ? <EstimateProp item={item} ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.sub_items && item.sub_item_count ? <SubCount n={item.sub_item_count} /> : null}
      {d.labels ? <LabelsProp item={item} ctx={ctx} onPatch={onPatch} chip /> : null}
    </span>
  );
}

function SubCount({ n }: { n: number }) {
  const { t } = useI18n();
  return (
    <span className="count-pill" title={t("wi.subItemCount", { n })}>
      <ListTree aria-hidden />
      {n}
    </span>
  );
}

/** "…" on a row or card: open, copy link. */
function ItemMenu({ item, ctx, onOpen }: { item: WorkItem; ctx: PropsCtx; onOpen: (id: string) => void }) {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const close = useCallback(() => setOpen(false), []);
  const url = `${location.pathname.split("/projects/")[0]}/projects/${ctx.project.id}/work-items/${item.id}`;
  return (
    <span className="item-menu" onClick={(e) => e.stopPropagation()} onPointerDown={(e) => e.stopPropagation()}>
      <Popover
        open={open}
        onClose={close}
        right
        trigger={
          <button className="icon-btn icon-btn-sm" onClick={() => setOpen(!open)} aria-label={t("wi.actions")} aria-haspopup="menu" aria-expanded={open} tabIndex={-1}>
            <Ellipsis />
          </button>
        }
      >
        <div className="menu" role="menu">
          <button className="menu-item" role="menuitem" onClick={() => (close(), onOpen(item.id))}>
            <Maximize2 />
            {t("wi.peek")}
          </button>
          <button className="menu-item" role="menuitem" onClick={() => (close(), navigate(url))}>
            <ExternalLink />
            {t("wi.openFull")}
          </button>
          <button className="menu-item" role="menuitem" onClick={() => (close(), void navigator.clipboard?.writeText(location.origin + url))}>
            <LinkIcon />
            {t("nav.copyLink")}
          </button>
        </div>
      </Popover>
    </span>
  );
}

type Row = { kind: "group"; g: Group } | { kind: "item"; item: WorkItem; g: Group } | { kind: "add"; g: Group };

/** List layout: grouped rows, virtualized so large projects stay fast, with multi-select. */
export function ListLayout({ groups, ctx, view, onOpen, activeId, onPatch, quickAdd, onCreateIn, selected, setSelected }: LayoutProps & { selected: Set<string>; setSelected: (s: Set<string>) => void }) {
  const { t } = useI18n();
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const rows = useMemo(() => {
    const out: Row[] = [];
    const single = groups.length === 1 && groups[0]!.kind === "none";
    for (const g of groups) {
      if (!single) out.push({ kind: "group", g });
      if (collapsed.has(g.key)) continue;
      for (const item of g.items) out.push({ kind: "item", item, g });
      if (ctx.editable) out.push({ kind: "add", g });
    }
    return out;
  }, [groups, collapsed, ctx.editable]);
  // Keys share one width so titles line up (Plane: ~7px per character).
  const keyWidth = useMemo(() => {
    let max = 1;
    for (const g of groups) for (const i of g.items) max = Math.max(max, i.sequence);
    return 7 * (ctx.project.identifier.length + 1 + String(max).length) + 4;
  }, [groups, ctx.project.identifier]);
  const scroller = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => 44,
    overscan: 12,
    getItemKey: (i) => {
      const r = rows[i]!;
      return r.kind === "item" ? `${r.g.key}:${r.item.id}` : `${r.kind}:${r.g.key}`;
    },
  });
  const toggle = (id: string) => {
    const s = new Set(selected);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    setSelected(s);
  };
  const selecting = selected.size > 0;
  return (
    <div className="list" ref={scroller}>
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {v.getVirtualItems().map((vi) => {
          const r = rows[vi.index]!;
          return (
            <div key={vi.key} data-index={vi.index} ref={v.measureElement} style={{ position: "absolute", top: 0, left: 0, right: 0, transform: `translateY(${vi.start}px)` }}>
              {r.kind === "group" ? (
                <section className="list-group-head" aria-label={groupName(r.g, t)}>
                  <button
                    className="group-toggle"
                    aria-label={r.g.key}
                    aria-expanded={!collapsed.has(r.g.key)}
                    onClick={() => {
                      const c = new Set(collapsed);
                      if (c.has(r.g.key)) c.delete(r.g.key);
                      else c.add(r.g.key);
                      setCollapsed(c);
                    }}
                  >
                    <GroupIcon g={r.g} />
                    <span className="group-name">{groupName(r.g, t)}</span>
                    <span className="group-count">{r.g.items.length}</span>
                  </button>
                  {ctx.editable && onCreateIn ? (
                    <button className="icon-btn icon-btn-sm group-add" onClick={() => onCreateIn(r.g)} aria-label={t("wi.newIn", { name: groupName(r.g, t) })} title={t("wi.new")}>
                      <Plus />
                    </button>
                  ) : null}
                </section>
              ) : r.kind === "add" ? (
                quickAdd(r.g)
              ) : (
                <div
                  className={"list-row" + (activeId === r.item.id ? " active" : "") + (selected.has(r.item.id) ? " selected" : "")}
                  onClick={() => (selecting ? toggle(r.item.id) : onOpen(r.item.id))}
                  role="button"
                  tabIndex={0}
                  data-group={r.g.state?.name ?? r.g.key}
                  onKeyDown={(e) => {
                    if (e.target !== e.currentTarget) return;
                    if (e.key === "Enter") onOpen(r.item.id);
                    if (e.key === "x") toggle(r.item.id);
                  }}
                >
                  <input type="checkbox" className={"row-check" + (selecting ? " show" : "")} aria-label={t("bulk.select", { key: keyOf(ctx.project.identifier, r.item.sequence) })} checked={selected.has(r.item.id)} onClick={(e) => e.stopPropagation()} onChange={() => toggle(r.item.id)} />
                  {view.display.key ? <span className="list-key" style={{ minWidth: keyWidth }}>{keyOf(ctx.project.identifier, r.item.sequence)}</span> : null}
                  <span className="list-title ellipsis" title={r.item.title}>{r.item.title}</span>
                  <ItemChips item={r.item} ctx={ctx} view={view} onPatch={(b) => onPatch(r.item.id, b)} />
                  <ItemMenu item={r.item} ctx={ctx} onOpen={onOpen} />
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Kanban layout: one column per group; drag cards to change the group or the order. */
export function KanbanLayout({ groups, ctx, view, onOpen, activeId, onPatch, quickAdd, onCreateIn }: LayoutProps) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const [dragging, setDragging] = useState<WorkItem | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const cols = groups.filter((g) => g.kind !== "none" || groups.length === 1);
  const find = (id: string) => {
    const [gk, iid] = id.split("::");
    const g = cols.find((c) => c.key === gk);
    return { g, item: g?.items.find((i) => i.id === iid) };
  };
  const onStart = (e: DragStartEvent) => setDragging(find(String(e.active.id)).item ?? null);
  const onEnd = (e: DragEndEvent) => {
    setDragging(null);
    if (!e.over || !ctx.editable) return;
    const from = find(String(e.active.id));
    if (!from.item || !from.g) return;
    const overId = String(e.over.id);
    const target = overId.startsWith("col::") ? cols.find((c) => `col::${c.key}` === overId) : find(overId).g;
    if (!target) return;
    const list = target.items.filter((i) => i.id !== from.item!.id);
    let index = list.length;
    if (!overId.startsWith("col::")) {
      const overItem = find(overId).item;
      const at = overItem ? list.findIndex((i) => i.id === overItem.id) : -1;
      if (at >= 0) index = at + (from.g.key === target.key && target.items.indexOf(from.item) < target.items.indexOf(overItem!) ? 1 : 0);
    }
    const body: Partial<WorkItem> = { ...(patchForGroup(target, from.item) ?? {}) };
    if (view.orderBy === "manual") {
      const order = sortBetween(list[index - 1]?.sort_order, list[index]?.sort_order);
      if (order !== from.item.sort_order) body.sort_order = order;
    }
    if (Object.keys(body).length) onPatch(from.item.id, body);
  };
  const toggleCol = (k: string) => {
    const c = new Set(collapsed);
    if (c.has(k)) c.delete(k);
    else c.add(k);
    setCollapsed(c);
  };
  return (
    <DndContext sensors={sensors} collisionDetection={closestCorners} onDragStart={onStart} onDragEnd={onEnd} onDragCancel={() => setDragging(null)}>
      <div className="board">
        {cols.map((g) =>
          collapsed.has(g.key) ? (
            <CollapsedColumn key={g.key} g={g} onExpand={() => toggleCol(g.key)} />
          ) : (
            <Column key={g.key} g={g} ctx={ctx} view={view} onOpen={onOpen} activeId={activeId} onPatch={onPatch} quickAdd={quickAdd} onCreateIn={onCreateIn} onCollapse={() => toggleCol(g.key)} />
          ),
        )}
      </div>
      <DragOverlay>{dragging ? <Card item={dragging} ctx={ctx} view={view} onPatch={() => {}} onOpen={() => {}} overlay /> : null}</DragOverlay>
    </DndContext>
  );
}

function CollapsedColumn({ g, onExpand }: { g: Group; onExpand: () => void }) {
  const { t } = useI18n();
  const name = groupName(g, t);
  return (
    <button className="board-col-collapsed" onClick={onExpand} title={`${name} · ${g.items.length}`} aria-label={t("view.expandColumn", { name })}>
      <GroupIcon g={g} />
      <span className="board-col-vname">{name}</span>
      <span className="group-count">{g.items.length}</span>
    </button>
  );
}

function Column({ g, ctx, view, onOpen, activeId, onPatch, quickAdd, onCreateIn, onCollapse }: Omit<LayoutProps, "groups"> & { g: Group; onCollapse: () => void }) {
  const { t } = useI18n();
  const { setNodeRef, isOver } = useDroppable({ id: `col::${g.key}` });
  const ids = g.items.map((i) => `${g.key}::${i.id}`);
  const name = groupName(g, t);
  return (
    <section className={"board-col" + (isOver ? " over" : "")} aria-label={g.state?.name ?? g.member?.name ?? g.label?.name ?? g.priority ?? g.key}>
      <div className="board-col-head">
        <span className="board-col-icon">
          <GroupIcon g={g} />
        </span>
        <span className="group-name ellipsis">{name}</span>
        <span className="group-count">{g.items.length}</span>
        <span className="grow" />
        <button className="icon-btn icon-btn-xs" onClick={onCollapse} aria-label={t("view.collapseColumn", { name })} title={t("view.collapseColumn", { name })}>
          <Minimize2 />
        </button>
        {ctx.editable && onCreateIn ? (
          <button className="icon-btn icon-btn-xs" onClick={() => onCreateIn(g)} aria-label={t("wi.newIn", { name })} title={t("wi.new")}>
            <Plus />
          </button>
        ) : null}
      </div>
      <div className="board-col-body" ref={setNodeRef}>
        <SortableContext items={ids} strategy={verticalListSortingStrategy}>
          {g.items.map((item) => (
            <SortableCard key={item.id} id={`${g.key}::${item.id}`} item={item} ctx={ctx} view={view} onOpen={onOpen} active={activeId === item.id} onPatch={(b) => onPatch(item.id, b)} />
          ))}
        </SortableContext>
        {ctx.editable ? quickAdd(g) : null}
      </div>
    </section>
  );
}

function SortableCard({ id, item, ctx, view, onOpen, active, onPatch }: { id: string; item: WorkItem; ctx: PropsCtx; view: ViewState; onOpen: (id: string) => void; active: boolean; onPatch: (b: Partial<WorkItem>) => void }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id, disabled: !ctx.editable });
  return (
    <div ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition, opacity: isDragging ? 0.4 : 1 }} {...attributes} {...listeners}>
      <Card item={item} ctx={ctx} view={view} onPatch={onPatch} onOpen={onOpen} active={active} />
    </div>
  );
}

function Card({ item, ctx, view, onPatch, onOpen, active, overlay }: { item: WorkItem; ctx: PropsCtx; view: ViewState; onPatch: (b: Partial<WorkItem>) => void; onOpen: (id: string) => void; active?: boolean; overlay?: boolean }) {
  return (
    <div className={"board-card" + (active ? " active" : "") + (overlay ? " overlay" : "")} onClick={() => onOpen(item.id)} role="button" tabIndex={-1}>
      <div className="board-card-top">
        {view.display.key ? <span className="board-card-key">{keyOf(ctx.project.identifier, item.sequence)}</span> : null}
        <ItemMenu item={item} ctx={ctx} onOpen={onOpen} />
      </div>
      <div className="board-card-title">{item.title}</div>
      <ItemChips item={item} ctx={ctx} view={view} onPatch={onPatch} />
    </div>
  );
}
