import { useEffect, useState } from "react";

/** A short message in the corner after an action, optionally with a link ("View archives"). */
export interface Toast {
  id: number;
  kind: "success" | "error" | "info";
  title: string;
  body?: string;
  link?: { label: string; to: string };
}

type Listener = (list: Toast[]) => void;
let list: Toast[] = [];
let next = 1;
const listeners = new Set<Listener>();
const emit = () => listeners.forEach((l) => l(list));

export function toast(t: Omit<Toast, "id">, ms = 5000): number {
  const id = next++;
  list = [...list.slice(-3), { ...t, id }];
  emit();
  setTimeout(() => dismiss(id), ms);
  return id;
}

export function dismiss(id: number) {
  list = list.filter((t) => t.id !== id);
  emit();
}

export function useToasts(): Toast[] {
  const [v, setV] = useState(list);
  useEffect(() => {
    listeners.add(setV);
    return () => {
      listeners.delete(setV);
    };
  }, []);
  return v;
}
