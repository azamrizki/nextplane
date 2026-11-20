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
import { useMemo, useRef, useState, type ReactNode } from "react";
import type { WorkItem } from "../api/types";
import { useI18n } from "../i18n";
import { patchForGroup, sortBetween, type Group, type ViewState } from "../lib/view";
import { keyOf } from "../lib/workItems";
import { Avatar } from "./Avatar";
import { Dot } from "./ColorPicker";
import { PriorityIcon, StateIcon } from "./Props";
import { AssigneesProp, DateProp, EstimateProp, LabelsProp, PriorityProp, StateProp, type PropsCtx } from "./WorkItemProps";

export interface LayoutProps {
  groups: Group[];
  ctx: PropsCtx;
  view: ViewState;
  onOpen: (id: string) => void;
  activeId: string | null;
  onPatch: (id: string, body: Partial<WorkItem>) => void;
  quickAdd: (g: Group) => ReactNode;
}

export function GroupTitle({ g }: { g: Group }) {
  const { t } = useI18n();
  switch (g.kind) {
    case "state":
      return <><StateIcon group={g.state!.group} color={g.state!.color} /><span>{g.state!.name}</span></>;
    case "priority":
      return <><PriorityIcon p={g.priority!} /><span>{t(`prio.${g.priority!}`)}</span></>;
    case "assignee":
      return g.member ? <><Avatar name={g.member.name} email={g.member.email} src={g.member.avatar} size={18} /><span>{g.member.name || g.member.email}</span></> : <span>{t("noAssignee")}</span>;
    case "label":
      return g.label ? <><Dot color={g.label.color} /><span>{g.label.name}</span></> : <span>{t("noLabel")}</span>;
    default:
      return <span>{t("groupBy.none")}</span>;
  }
}

/** Property chips shown on a row or card, per the view's display settings. */
export function ItemChips({ item, ctx, view, onPatch, showState }: { item: WorkItem; ctx: PropsCtx; view: ViewState; onPatch: (b: Partial<WorkItem>) => void; showState?: boolean }) {
  const d = view.display;
  return (
    <span className="list-props" onClick={(e) => e.stopPropagation()} onPointerDown={(e) => e.stopPropagation()}>
      {showState ? <StateProp item={item} ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.labels ? <LabelsProp item={item} ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.start_date ? <DateProp item={item} field="start_date" ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.due_date ? <DateProp item={item} field="due_date" ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.estimate ? <EstimateProp item={item} ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.priority ? <PriorityProp item={item} ctx={ctx} onPatch={onPatch} chip /> : null}
      {d.assignees ? <AssigneesProp item={item} ctx={ctx} onPatch={onPatch} chip /> : null}
    </span>
  );
}

type Row = { kind: "group"; g: Group } | { kind: "item"; item: WorkItem; g: Group } | { kind: "add"; g: Group };

/** List layout: grouped rows, virtualized so large projects stay fast, with multi-select. */
export function ListLayout({ groups, ctx, view, onOpen, activeId, onPatch, quickAdd, selected, setSelected }: LayoutProps & { selected: Set<string>; setSelected: (s: Set<string>) => void }) {
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
  const scroller = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: (i) => (rows[i]!.kind === "item" ? 40 : 36),
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
                <section className="list-group-head" aria-label={r.g.state?.name ?? r.g.key}>
                  <button className="icon-btn group-toggle" aria-label={r.g.key} aria-expanded={!collapsed.has(r.g.key)} onClick={() => {
                    const c = new Set(collapsed);
                    if (c.has(r.g.key)) c.delete(r.g.key);
                    else c.add(r.g.key);
                    setCollapsed(c);
                  }}>{collapsed.has(r.g.key) ? "▸" : "▾"}</button>
                  <GroupTitle g={r.g} />
                  <span className="faint small">{r.g.items.length}</span>
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
                  {view.display.key ? <span className="mono faint small list-key">{keyOf(ctx.project.identifier, r.item.sequence)}</span> : null}
                  <StateProp item={r.item} ctx={ctx} onPatch={(b) => onPatch(r.item.id, b)} chip />
                  <span className="ellipsis grow list-title">{r.item.title}</span>
                  {view.display.sub_items && r.item.sub_item_count ? <span className="faint small">⑂ {r.item.sub_item_count}</span> : null}
                  <ItemChips item={r.item} ctx={ctx} view={view} onPatch={(b) => onPatch(r.item.id, b)} />
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
export function KanbanLayout({ groups, ctx, view, onOpen, activeId, onPatch, quickAdd }: LayoutProps) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const [dragging, setDragging] = useState<WorkItem | null>(null);
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
  return (
    <DndContext sensors={sensors} collisionDetection={closestCorners} onDragStart={onStart} onDragEnd={onEnd} onDragCancel={() => setDragging(null)}>
      <div className="board">
        {cols.map((g) => (
          <Column key={g.key} g={g} ctx={ctx} view={view} onOpen={onOpen} activeId={activeId} onPatch={onPatch} quickAdd={quickAdd} />
        ))}
      </div>
      <DragOverlay>{dragging ? <Card item={dragging} ctx={ctx} view={view} onPatch={() => {}} overlay /> : null}</DragOverlay>
    </DndContext>
  );
}

function Column({ g, ctx, view, onOpen, activeId, onPatch, quickAdd }: Omit<LayoutProps, "groups"> & { g: Group }) {
  const { setNodeRef, isOver } = useDroppable({ id: `col::${g.key}` });
  const ids = g.items.map((i) => `${g.key}::${i.id}`);
  return (
    <section className={"board-col" + (isOver ? " over" : "")} aria-label={g.state?.name ?? g.member?.name ?? g.label?.name ?? g.priority ?? g.key}>
      <div className="board-col-head">
        <GroupTitle g={g} />
        <span className="faint small">{g.items.length}</span>
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
      <Card item={item} ctx={ctx} view={view} onPatch={onPatch} onClick={() => onOpen(item.id)} active={active} />
    </div>
  );
}

function Card({ item, ctx, view, onPatch, onClick, active, overlay }: { item: WorkItem; ctx: PropsCtx; view: ViewState; onPatch: (b: Partial<WorkItem>) => void; onClick?: () => void; active?: boolean; overlay?: boolean }) {
  return (
    <div className={"card board-card" + (active ? " active" : "") + (overlay ? " overlay" : "")} onClick={onClick} role="button" tabIndex={-1}>
      <div className="board-card-top">
        {view.display.key ? <span className="mono faint small">{keyOf(ctx.project.identifier, item.sequence)}</span> : null}
        {view.display.sub_items && item.sub_item_count ? <span className="faint small">⑂ {item.sub_item_count}</span> : null}
      </div>
      <div className="board-card-title">{item.title}</div>
      <ItemChips item={item} ctx={ctx} view={view} onPatch={onPatch} showState={view.groupBy !== "state"} />
    </div>
  );
}
