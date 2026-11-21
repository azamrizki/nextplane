import { Archive, ArchiveRestore, Trash2 } from "lucide-react";
import { useState, type ReactNode } from "react";
import type { WorkItem } from "../api/types";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { pcan } from "../lib/perm";
import { useMe } from "../lib/session";
import { toast } from "../lib/toast";
import { keyOf, useArchiveItem, useDeleteItem } from "../lib/workItems";
import { ConfirmDialog } from "./Dialog";
import type { PropsCtx } from "./WorkItemProps";

/** Only work items in a completed or cancelled state can be archived (as in Plane). */
export function isClosed(ctx: PropsCtx, item: WorkItem): boolean {
  const g = ctx.states.find((s) => s.id === item.state_id)?.group;
  return g === "completed" || g === "cancelled";
}

/** Archive or restore, and delete, for one work item: the menu entries and their confirm
 * dialogs. Put `entries` in a menu and `dialogs` next to it (outside the popover). */
export function useItemActions(
  slug: string,
  ctx: PropsCtx,
  item: WorkItem,
  on?: { archived?: () => void; deleted?: () => void },
): { entries: (close: () => void) => ReactNode; dialogs: ReactNode; any: boolean } {
  const { t } = useI18n();
  const p = ctx.project;
  const me = useMe().data;
  const archive = useArchiveItem(slug, p.id);
  const remove = useDeleteItem(slug, p.id);
  const [ask, setAsk] = useState<"archive" | "delete" | null>(null);
  const key = keyOf(p.identifier, item.sequence);
  const base = `/${slug}/projects/${p.id}`;
  const canArchive = pcan.editWorkItem(p) && !p.archived_at;
  const canDelete = pcan.deleteWorkItem(p, item.created_by === me?.id) && !p.archived_at;
  const closed = isClosed(ctx, item);
  const archived = !!item.archived_at;

  const restore = () =>
    archive.mutate(
      { id: item.id, on: false },
      {
        onSuccess: () => toast({ kind: "success", title: t("arch.restored", { key }), body: t("arch.restoredBody"), link: { label: t("arch.toItems"), to: base } }),
        onError: (e) => toast({ kind: "error", title: errorText(e, t) }),
      },
    );

  const entries = (close: () => void) => (
    <>
      {canArchive && archived ? (
        <button className="menu-item" role="menuitem" onClick={() => (close(), restore())}>
          <ArchiveRestore />
          {t("wi.unarchive")}
        </button>
      ) : null}
      {canArchive && !archived ? (
        <button
          className="menu-item menu-item-2line"
          role="menuitem"
          aria-disabled={!closed}
          disabled={!closed}
          title={closed ? undefined : t("arch.onlyClosed")}
          onClick={() => (close(), setAsk("archive"))}
        >
          <Archive />
          <span className="menu-item-text">
            {t("wi.archive")}
            {!closed ? <span className="menu-item-sub">{t("arch.onlyClosed")}</span> : null}
          </span>
        </button>
      ) : null}
      {canDelete ? (
        <button className="menu-item danger" role="menuitem" onClick={() => (close(), setAsk("delete"))}>
          <Trash2 />
          {t("common.delete")}
        </button>
      ) : null}
    </>
  );

  const dialogs = (
    <>
      <ConfirmDialog
        open={ask === "archive"}
        onClose={() => setAsk(null)}
        title={t("arch.confirmTitle", { key })}
        body={t("arch.confirmBody")}
        confirmLabel={t("wi.archive")}
        busy={archive.isPending}
        onConfirm={() =>
          archive.mutate(
            { id: item.id, on: true },
            {
              onSuccess: () => {
                setAsk(null);
                toast({ kind: "success", title: t("arch.archived", { key }), link: { label: t("arch.view"), to: `${base}/archives` } });
                on?.archived?.();
              },
              onError: (e) => (setAsk(null), toast({ kind: "error", title: errorText(e, t) })),
            },
          )
        }
      />
      <ConfirmDialog
        open={ask === "delete"}
        onClose={() => setAsk(null)}
        title={t("wi.delete")}
        body={t("wi.deleteBody", { key })}
        confirmLabel={t("common.delete")}
        danger
        busy={remove.isPending}
        onConfirm={() =>
          remove.mutate(item.id, {
            onSuccess: () => {
              setAsk(null);
              toast({ kind: "success", title: t("arch.deleted", { key }) });
              on?.deleted?.();
            },
            onError: (e) => (setAsk(null), toast({ kind: "error", title: errorText(e, t) })),
          })
        }
      />
    </>
  );
  return { entries, dialogs, any: canArchive || canDelete };
}
