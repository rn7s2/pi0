import { useCallback, useEffect, useRef, useState } from 'react';

import type { PermissionKind, PermissionStatus } from '../shared/schemas';

/**
 * Boot-time guard modal: pi0 must hold both macOS grants before the app is
 * usable. Polls TCC status, lets the user trigger the system prompt or jump to
 * System Settings, and calls `onGranted` as soon as everything is in place.
 */
export function PermissionGuard({ onGranted }: { onGranted: () => void }) {
  const [perms, setPerms] = useState<PermissionStatus | null>(null);
  const [busy, setBusy] = useState<PermissionKind | null>(null);
  const grantedRef = useRef(onGranted);
  grantedRef.current = onGranted;

  const refresh = useCallback(async () => {
    const next = await window.pi0.permissionsStatus();
    setPerms(next);
    if (next.inputMonitoring && next.screenRecording) {
      grantedRef.current();
    }
    return next;
  }, []);

  // Re-check while the guard is up so granting in System Settings auto-advances.
  useEffect(() => {
    void refresh();
    const id = window.setInterval(() => void refresh(), 1500);
    return () => window.clearInterval(id);
  }, [refresh]);

  const request = async (kind: PermissionKind) => {
    setBusy(kind);
    try {
      setPerms(await window.pi0.requestPermission(kind));
      await refresh();
    } finally {
      setBusy(null);
    }
  };

  const row = (kind: PermissionKind, label: string, why: string, granted: boolean) => (
    <div className="guard-perm">
      <span className={`dot ${granted ? 'ok' : 'bad'}`} />
      <div className="guard-perm-body">
        <div className="guard-perm-head">
          <span className="guard-perm-label">{label}</span>
          <span className={`guard-perm-state ${granted ? 'ok' : ''}`}>
            {granted ? 'Granted ✓' : 'Required'}
          </span>
        </div>
        <p className="guard-perm-why">{why}</p>
        {!granted && (
          <div className="guard-perm-actions">
            <button className="btn small" disabled={busy === kind} onClick={() => void request(kind)}>
              {busy === kind ? 'Requesting…' : 'Grant access'}
            </button>
            <button
              className="btn small ghost"
              onClick={() => void window.pi0.openPermissionSettings(kind)}
            >
              Open System Settings
            </button>
          </div>
        )}
      </div>
    </div>
  );

  return (
    <div className="guard-backdrop">
      <div className="guard-modal" role="dialog" aria-modal="true">
        <h1 className="guard-title">Welcome to pi0</h1>
        <p className="guard-sub">
          pi0 records your keystrokes and screen to build your personal workbench. Grant both macOS
          permissions to continue.
        </p>

        {perms ? (
          <div className="guard-perms">
            {row(
              'inputMonitoring',
              'Input Monitoring',
              'Lets pi0 log keystrokes so it can organise the text you type.',
              perms.inputMonitoring,
            )}
            {row(
              'screenRecording',
              'Screen Recording',
              'Lets pi0 take periodic screenshots. Enabling this may require a relaunch.',
              perms.screenRecording,
            )}
          </div>
        ) : (
          <p className="muted">Checking permissions…</p>
        )}

        <div className="guard-footer">
          {perms && !perms.screenRecording && (
            <button className="btn small ghost" onClick={() => void window.pi0.relaunchApp()}>
              Relaunch pi0
            </button>
          )}
          <span className="guard-footer-spacer" />
          <button className="btn small ghost" onClick={() => void window.pi0.quitApp()}>
            Quit
          </button>
        </div>
      </div>
    </div>
  );
}
