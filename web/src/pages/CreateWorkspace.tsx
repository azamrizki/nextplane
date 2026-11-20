import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router";
import { get, post } from "../api/client";
import type { MyWorkspace } from "../api/types";
import { ErrorAlert, Logo, TextField } from "../components/ui";
import { useT } from "../i18n";
import { errorText } from "../lib/errors";
import { slugify } from "../lib/perm";
import { wq, useWorkspaces } from "../lib/queries";

export function CreateWorkspacePage() {
  const t = useT();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const list = useWorkspaces();
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [slugEdited, setSlugEdited] = useState(false);
  const effectiveSlug = slugEdited ? slug : slugify(name);
  const [debounced, setDebounced] = useState(effectiveSlug);
  useEffect(() => {
    const h = setTimeout(() => setDebounced(effectiveSlug), 250);
    return () => clearTimeout(h);
  }, [effectiveSlug]);
  const check = useQuery({
    queryKey: ["slug-check", debounced],
    queryFn: () => get<{ available: boolean; reason?: string }>(`/workspaces/slug-check?slug=${encodeURIComponent(debounced)}`),
    enabled: debounced.length >= 3,
  });

  const create = useMutation({
    mutationFn: () => post<MyWorkspace>("/workspaces", { name, slug: effectiveSlug }),
    onSuccess: async (ws) => {
      await qc.invalidateQueries({ queryKey: wq.list });
      navigate(`/${ws.slug}`, { replace: true });
    },
  });
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    create.mutate();
  };
  const hasAny = (list.data?.length ?? 0) > 0;

  return (
    <div className="auth-page">
      <div className="auth-card">
        <div className="auth-head">
          <Logo />
          <h1>{hasAny ? t("ws.create.title") : t("ws.none.title")}</h1>
          <p className="muted small" style={{ margin: 0 }}>
            {hasAny ? t("ws.create.subtitle") : t("ws.none.body")}
          </p>
        </div>
        <form className="form" onSubmit={onSubmit}>
          <ErrorAlert message={create.isError ? errorText(create.error, t) : null} />
          <TextField label={t("ws.name")} value={name} onChange={(e) => setName(e.target.value)} required maxLength={80} autoFocus />
          <TextField
            label={t("ws.slug")}
            value={effectiveSlug}
            onChange={(e) => {
              setSlugEdited(true);
              setSlug(e.target.value.toLowerCase());
            }}
            required
            minLength={3}
            maxLength={48}
            hint={
              check.data && !check.data.available ? (
                <span style={{ color: "var(--danger)" }}>{check.data.reason ?? t("ws.slugTaken")}</span>
              ) : (
                <>
                  {location.host}/<b>{effectiveSlug || "…"}</b> · {t("ws.slugHint")}
                </>
              )
            }
          />
          <button className="btn btn-primary btn-block" disabled={create.isPending || !name.trim() || check.data?.available === false}>
            {t("ws.create.submit")}
          </button>
          {hasAny ? (
            <Link to="/" className="small">
              ← {t("common.back")}
            </Link>
          ) : null}
        </form>
      </div>
    </div>
  );
}
