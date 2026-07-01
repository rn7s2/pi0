import { useEffect, useState } from 'react';

import type { PermissionStatus } from '../shared/schemas';

export function PermissionsPanel() {
    const [perms, setPerms] = useState<PermissionStatus | null>(null);

    const check = () => {
        void window.pi0.permissionsStatus().then(setPerms);
    };

    useEffect(() => {
        check();
    }, []);

    const row = (label: string, ok: boolean, hint: string) => (
        <div className="perm-row">
            <span className={`dot ${ok ? 'ok' : 'bad'}`} />
            <span className="perm-label">{label}</span>
            <span className="perm-state">{ok ? 'granted' : 'not granted'}</span>
            {!ok && <span className="perm-hint">{hint}</span>}
        </div>
    );

    return (
        <div className="perms">
            <h3>macOS permissions</h3>
            {perms ? (
                <>
                    {row(
                        'Input Monitoring',
                        perms.inputMonitoring,
                        'System Settings → Privacy & Security → Input Monitoring → enable pi0',
                    )}
                    {row(
                        'Screen Recording',
                        perms.screenRecording,
                        'System Settings → Privacy & Security → Screen Recording → enable pi0, then relaunch',
                    )}
                </>
            ) : (
                <p className="muted">Checking…</p>
            )}
            <button className="btn small" onClick={check}>
                Recheck
            </button>
        </div>
    );
}
