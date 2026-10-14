export interface QuotaWindow { remaining: number | null; minutes: number | null; resetsAt: number | null }
export interface Bucket { id: string; name: string; primary: QuotaWindow | null; secondary: QuotaWindow | null }
export interface Snapshot { buckets: Bucket[]; selected: string; updatedAt: number | null; error: string | null; refreshing: boolean }
export function periodLabel(minutes: number | null, fallback: string): string {
  if (!minutes || minutes <= 0) return fallback;
  if (minutes === 10080) return '每周额度';
  if (minutes % 1440 === 0) return `${minutes / 1440} 天额度`;
  if (minutes % 60 === 0) return `${minutes / 60} 小时额度`;
  return `${minutes} 分钟额度`;
}
export function resetLabel(timestamp: number | null, now = Date.now()): string {
  if (timestamp == null || !Number.isFinite(timestamp)) return '重置时间未知';
  const seconds = Math.ceil(timestamp - now / 1000);
  if (seconds <= 0) return '等待额度更新';
  const mins = Math.ceil(seconds / 60);
  if (mins < 60) return `${mins} 分钟后重置`;
  const hours = Math.floor(mins / 60), rest = mins % 60;
  if (hours < 24) return `${hours} 小时${rest ? ` ${rest} 分` : ''}后重置`;
  return `${Math.floor(hours / 24)} 天 ${hours % 24} 小时后重置`;
}
export function percentText(value: number | null): string {
  return value == null || !Number.isFinite(value) ? '—' : `${Math.round(Math.max(0, Math.min(100, value)))}`;
}
