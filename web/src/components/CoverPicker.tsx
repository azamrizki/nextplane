import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ImagePlus, Search, Trash2, Upload } from "lucide-react";
import { useRef, useState } from "react";
import { get, patch } from "../api/client";
import type { InstanceInfoFull, Project } from "../api/types";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { pq, projectPath } from "../lib/projects";
import { useInstance } from "../lib/session";
import { toast } from "../lib/toast";
import { useEditorUpload } from "./Collab";
import { Dialog } from "./Dialog";

interface Photo {
  id: string;
  thumb: string;
  regular: string;
  description: string;
  author: string;
  author_url: string;
  download_location: string;
}

/** Unsplash asks for links back with the app's name (its API guidelines). */
export const unsplashRef = (url: string) => `${url}${url.includes("?") ? "&" : "?"}utm_source=nextplane&utm_medium=referral`;

/** "Photo by … on Unsplash", for wherever an Unsplash cover shows. */
export function CoverCredit({ credit }: { credit: Project["cover_credit"] }) {
  const { t } = useI18n();
  if (!credit) return null;
  return (
    <span className="cover-credit">
      {t("cover.by")}{" "}
      <a href={unsplashRef(credit.url)} target="_blank" rel="noopener noreferrer">
        {credit.name}
      </a>{" "}
      {t("cover.on")}{" "}
      <a href={unsplashRef("https://unsplash.com")} target="_blank" rel="noopener noreferrer">
        Unsplash
      </a>
    </span>
  );
}

/** Project settings › cover: an uploaded image or an Unsplash photo (when set up), over the
 * cover colour. */
export function CoverPicker({ slug, p, disabled }: { slug: string; p: Project; disabled?: boolean }) {
  const { t } = useI18n();
  const qc = useQueryClient();
  const instance = useInstance();
  const unsplash = !!(instance.data as InstanceInfoFull | undefined)?.unsplash_enabled;
  const upload = useEditorUpload(slug, p.id);
  const file = useRef<HTMLInputElement>(null);
  const [picking, setPicking] = useState(false);
  const save = useMutation({
    mutationFn: (body: Record<string, unknown>) => patch<Project>(projectPath(slug, p.id), body),
    onSuccess: (np) => {
      qc.setQueryData(pq.one(slug, p.id), np);
      qc.invalidateQueries({ queryKey: pq.all(slug) });
    },
    onError: (e) => toast({ kind: "error", title: errorText(e, t) }),
  });
  return (
    <div className="cover-picker">
      <div className="cover-preview" style={{ ["--cover" as string]: p.cover_color ?? "#8b8d98", backgroundImage: p.cover_image ? `url("${p.cover_image}")` : undefined }}>
        <CoverCredit credit={p.cover_image ? p.cover_credit : null} />
      </div>
      {!disabled ? (
        <div className="row-flex">
          {unsplash ? (
            <button type="button" className="btn btn-sm" onClick={() => setPicking(true)}>
              <ImagePlus />
              {t("cover.unsplash")}
            </button>
          ) : null}
          <button type="button" className="btn btn-sm" onClick={() => file.current?.click()} disabled={save.isPending}>
            <Upload />
            {t("cover.upload")}
          </button>
          <input
            ref={file}
            type="file"
            accept="image/png,image/jpeg,image/webp"
            hidden
            aria-label={t("cover.upload")}
            onChange={async (e) => {
              const f = e.target.files?.[0];
              e.target.value = "";
              if (!f) return;
              if (f.size > 5 * 1024 * 1024) return toast({ kind: "error", title: t("cover.tooBig") });
              try {
                save.mutate({ cover_image: await upload(f) });
              } catch (err) {
                toast({ kind: "error", title: errorText(err, t) });
              }
            }}
          />
          {p.cover_image ? (
            <button type="button" className="btn btn-sm btn-ghost" onClick={() => save.mutate({ cover_image: null })}>
              <Trash2 />
              {t("cover.remove")}
            </button>
          ) : null}
        </div>
      ) : null}
      {picking ? (
        <UnsplashDialog
          slug={slug}
          onClose={() => setPicking(false)}
          onPick={(ph) => {
            setPicking(false);
            save.mutate({ cover_image: ph.regular, cover_credit: { name: ph.author, url: ph.author_url }, unsplash_download: ph.download_location });
          }}
        />
      ) : null}
    </div>
  );
}

function UnsplashDialog({ slug, onClose, onPick }: { slug: string; onClose: () => void; onPick: (p: Photo) => void }) {
  const { t } = useI18n();
  const [q, setQ] = useState("");
  const [query, setQuery] = useState("");
  const photos = useQuery({
    queryKey: ["unsplash", slug, query],
    queryFn: () => get<Photo[]>(`/workspaces/${slug}/unsplash${query ? `?query=${encodeURIComponent(query)}` : ""}`),
  });
  return (
    <Dialog open onClose={onClose} title={t("cover.unsplash")} width={640}>
      <form className="cover-search" onSubmit={(e) => (e.preventDefault(), setQuery(q.trim()))}>
        <Search aria-hidden />
        <input className="input" autoFocus placeholder={t("cover.search")} aria-label={t("cover.search")} value={q} onChange={(e) => setQ(e.target.value)} />
        <button className="btn">{t("cover.searchGo")}</button>
      </form>
      {photos.isError ? <p className="muted small">{errorText(photos.error, t)}</p> : null}
      {photos.isSuccess && !photos.data.length ? <p className="muted small">{t("cover.none")}</p> : null}
      <div className="cover-grid">
        {(photos.data ?? []).map((ph) => (
          <figure key={ph.id} className="cover-thumb">
            <button type="button" onClick={() => onPick(ph)} aria-label={ph.description || t("cover.photoBy", { name: ph.author })}>
              <img src={ph.thumb} alt="" loading="lazy" />
            </button>
            <figcaption className="faint small ellipsis">
              <a href={unsplashRef(ph.author_url)} target="_blank" rel="noopener noreferrer">
                {ph.author}
              </a>
            </figcaption>
          </figure>
        ))}
      </div>
    </Dialog>
  );
}
