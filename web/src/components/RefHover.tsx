// A live card for work item keys (WEB-12) in a page: title and state, fetched when shown.
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState, type RefObject } from "react";
import { createPortal } from "react-dom";
import { Link, useNavigate } from "react-router";
import { get } from "../api/client";
import { useI18n } from "../i18n";

interface RefInfo {
  project_id: string;
  id: string;
  key: string;
  title: string;
  state: { name: string; color: string; group: string } | null;
  archived: boolean;
}

const useRef_ = (slug: string, key: string) =>
  useQuery({
    queryKey: ["work-item-ref", slug, key],
    queryFn: () => get<RefInfo>(`/workspaces/${slug}/work-items/by-key/${encodeURIComponent(key)}`),
    enabled: !!key,
    retry: false,
    staleTime: 5000,
  });

export function RefHover({ slug, container, editable }: { slug: string; container: RefObject<HTMLElement | null>; editable: boolean }) {
  const { t } = useI18n();
  const nav = useNavigate();
  const [shown, setShown] = useState<{ key: string; x: number; y: number } | null>(null);
  const hide = useRef<ReturnType<typeof setTimeout> | null>(null);
  const q = useRef_(slug, shown?.key ?? "");
  const keep = () => {
    if (hide.current) clearTimeout(hide.current);
    hide.current = null;
  };
  const later = () => {
    keep();
    hide.current = setTimeout(() => setShown(null), 250);
  };

  useEffect(() => {
    const el = container.current;
    if (!el) return;
    const refAt = (e: Event) => (e.target as HTMLElement | null)?.closest?.(".wi-ref") as HTMLElement | null;
    const over = (e: MouseEvent) => {
      const r = refAt(e);
      if (!r) return;
      keep();
      const box = r.getBoundingClientRect();
      setShown({ key: r.dataset.key ?? r.textContent ?? "", x: box.left, y: box.bottom + 4 });
    };
    const out = (e: MouseEvent) => {
      if (refAt(e)) later();
    };
    const click = async (e: MouseEvent) => {
      const r = refAt(e);
      if (!r || (editable && !(e.ctrlKey || e.metaKey))) return;
      e.preventDefault();
      try {
        const info = await get<RefInfo>(`/workspaces/${slug}/work-items/by-key/${encodeURIComponent(r.dataset.key ?? "")}`);
        nav(`/${slug}/projects/${info.project_id}/work-items/${info.id}`);
      } catch {
        /* the card says it is missing */
      }
    };
    el.addEventListener("mouseover", over);
    el.addEventListener("mouseout", out);
    el.addEventListener("click", click);
    return () => {
      el.removeEventListener("mouseover", over);
      el.removeEventListener("mouseout", out);
      el.removeEventListener("click", click);
    };
  }, [container, editable, slug, nav]);
  useEffect(() => keep, []);

  if (!shown) return null;
  const info = q.data;
  return createPortal(
    <div className="ref-card card" role="tooltip" style={{ left: shown.x, top: shown.y }} onMouseEnter={keep} onMouseLeave={later}>
      {q.isLoading ? <span className="faint small">{shown.key}</span> : null}
      {q.isError ? <span className="faint small">{t("pg.refMissing")}</span> : null}
      {info ? (
        <>
          <div className="row-flex">
            <span className="faint small">{info.key}</span>
            {info.state ? (
              <span className="small ref-state">
                <span className="ref-dot" style={{ background: info.state.color }} />
                {info.state.name}
              </span>
            ) : null}
          </div>
          <div className={"ref-title" + (info.state?.group === "completed" || info.state?.group === "cancelled" ? " done" : "")}>{info.title}</div>
          <div className="row-flex">
            {editable ? <span className="faint small">{t("pg.refHint")}</span> : <span />}
            <Link className="btn btn-sm" to={`/${slug}/projects/${info.project_id}/work-items/${info.id}`}>{t("pg.refOpen")}</Link>
          </div>
        </>
      ) : null}
    </div>,
    document.body,
  );
}
