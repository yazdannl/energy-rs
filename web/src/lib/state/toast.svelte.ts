/** A tiny toast queue: commands report through it, so a failed write is never silent. */

export type ToastKind = 'ok' | 'error' | 'info';

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  detail: string | null;
}

let nextId = 1;

class ToastStore {
  items = $state<Toast[]>([]);
  private timers = new Map<number, ReturnType<typeof setTimeout>>();

  push(kind: ToastKind, message: string, detail?: string): void {
    const id = nextId++;
    this.items = [...this.items, { id, kind, message, detail: detail ?? null }];
    const timer = setTimeout(() => this.dismiss(id), kind === 'error' ? 9_000 : 5_000);
    this.timers.set(id, timer);
  }

  dismiss(id: number): void {
    const timer = this.timers.get(id);
    if (timer !== undefined) clearTimeout(timer);
    this.timers.delete(id);
    this.items = this.items.filter((toast) => toast.id !== id);
  }
}

export const toasts = new ToastStore();
