import { Plus, StickyNote } from "lucide-react";
import { useEffect, useState } from "react";
import { StickyGrid, useAddSticky } from "../components/HomeWidgets";
import { PageHeader } from "../components/PageHeader";
import { ExpandableSearch } from "../components/ViewBar";
import { useI18n } from "../i18n";
import { useStickies } from "../lib/home";
import { useWs } from "./WorkspaceHome";

const PAGE = 30;

/** `/:slug/stickies`: all your stickies, 30 at a time, newest on top (as in Plane). */
export function StickiesPage() {
  const { t } = useI18n();
  const ws = useWs();
  const [q, setQ] = useState("");
  const [debounced, setDebounced] = useState("");
  const [limit, setLimit] = useState(PAGE);
  useEffect(() => {
    const h = setTimeout(() => (setDebounced(q), setLimit(PAGE)), 500);
    return () => clearTimeout(h);
  }, [q]);
  const list = useStickies(ws.slug, debounced, limit);
  const add = useAddSticky(ws.slug, list.data?.[0]);
  const shown = list.data ?? [];
  return (
    <div className="page page-wide">
      <PageHeader
        crumbs={[{ icon: <StickyNote />, label: t("st.title") }]}
        actions={
          <>
            <ExpandableSearch value={q} onChange={setQ} label={t("st.search")} />
            <button className="btn btn-primary" onClick={() => add.mutate()} disabled={add.isPending}>
              <Plus />
              {t("st.add")}
            </button>
          </>
        }
      />
      {list.isSuccess && !shown.length ? (
        <div className="archive-empty">
          <span className="archive-empty-icon" aria-hidden>
            <StickyNote />
          </span>
          <h2>{debounced ? t("st.noMatch") : t("st.empty")}</h2>
          <button className="btn btn-primary" onClick={() => add.mutate()}>
            {t("st.add")}
          </button>
        </div>
      ) : null}
      <StickyGrid ws={ws} list={shown} />
      {shown.length >= limit ? (
        <button className="btn widget-more" onClick={() => setLimit((l) => l + PAGE)}>
          {t("st.loadMore")}
        </button>
      ) : null}
    </div>
  );
}
