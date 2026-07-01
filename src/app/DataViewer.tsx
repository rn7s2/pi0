import { useState } from 'react';

import type { TextRecord } from '../shared/schemas';

/** Format a Date as the `YYYY-MM-DDTHH:mm` a datetime-local input expects (local time). */
function toLocalInput(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}`;
}

export function DataViewer() {
  const now = new Date();
  const [start, setStart] = useState(toLocalInput(new Date(now.getTime() - 3_600_000)));
  const [end, setEnd] = useState(toLocalInput(now));
  const [records, setRecords] = useState<TextRecord[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const query = async () => {
    setLoading(true);
    setError(null);
    try {
      const startMs = new Date(start).getTime();
      const endMs = new Date(end).getTime();
      setRecords(await window.pi0.queryText({ startMs, endMs }));
    } catch (e) {
      setError((e as Error).message);
      setRecords(null);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="panel">
      <h2>Recorded text</h2>

      <div className="range">
        <label>
          From
          <input type="datetime-local" value={start} onChange={(e) => setStart(e.target.value)} />
        </label>
        <label>
          To
          <input type="datetime-local" value={end} onChange={(e) => setEnd(e.target.value)} />
        </label>
        <button className="btn" onClick={query} disabled={loading}>
          {loading ? 'Loading…' : 'Query'}
        </button>
      </div>

      {error && <p className="error">{error}</p>}

      {records && (
        <div className="results">
          <p className="muted">{records.length} record(s)</p>
          {records.map((r, i) => (
            <div className="record" key={`${r.ts}-${i}`}>
              <div className="record-meta">
                <span className="record-app">{r.appRaw}</span>
                <span className="record-time">{new Date(r.ts).toLocaleString()}</span>
              </div>
              <pre className="record-text">{r.text}</pre>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
