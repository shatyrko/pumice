export function bytes(n, digits) {
  if (n == null || isNaN(n)) return '–';
  const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
  let v = Math.max(0, n);
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  const d = digits ?? (i === 0 ? 0 : v >= 100 ? 0 : v >= 10 ? 1 : 2);
  return `${v.toFixed(d)} ${units[i]}`;
}

export function rate(n) {
  return `${bytes(n)}/s`;
}

export function ago(secs) {
  if (!secs) return '–';
  const d = Math.floor((Date.now() / 1000 - secs) / 86400);
  if (d < 1) return 'today';
  if (d === 1) return 'yesterday';
  if (d < 30) return `${d} days ago`;
  if (d < 365) return `${Math.floor(d / 30)} mo ago`;
  const y = Math.floor(d / 365);
  return `${y} yr ago`;
}

export function dateTime(secs) {
  const d = new Date(secs * 1000);
  return d.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

export function dur(secs) {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d) return `${d}d ${h}h`;
  if (h) return `${h}h ${m}m`;
  return `${m}m`;
}

export function shortPath(p, home) {
  if (home && p.startsWith(home)) return '~' + p.slice(home.length);
  return p;
}

export function baseName(p) {
  const i = p.lastIndexOf('/');
  return i >= 0 ? p.slice(i + 1) : p;
}

export function dirName(p) {
  const i = p.lastIndexOf('/');
  return i > 0 ? p.slice(0, i) : '/';
}

export function mb(v) {
  if (v == null) return '–';
  return v >= 1024 ? `${(v / 1024).toFixed(1)} GB` : `${v.toFixed(0)} MB`;
}
export function mhz(v) {
  if (v == null) return '–';
  return v >= 1000 ? `${(v / 1000).toFixed(2)} GHz` : `${v.toFixed(0)} MHz`;
}
export function watts(v) {
  if (v == null) return '–';
  return v < 10 ? `${v.toFixed(1)} W` : `${v.toFixed(0)} W`;
}
export function pct(v, d = 0) {
  return v == null ? '–' : `${v.toFixed(d)}%`;
}
export function temp(c, unit, withUnit = true) {
  if (c == null) return '–';
  const v = unit === 'f' ? (c * 9) / 5 + 32 : c;
  return `${v.toFixed(0)}°${withUnit ? (unit === 'f' ? 'F' : 'C') : ''}`;
}
export function hours(h) {
  if (h == null) return '–';
  return dur(h * 3600);
}
/** 'ok' | 'warn' | 'crit' against two limits. */
export function level(v, warn, crit) {
  if (v == null) return 'ok';
  if (crit != null && v >= crit) return 'crit';
  if (warn != null && v >= warn) return 'warn';
  return 'ok';
}
