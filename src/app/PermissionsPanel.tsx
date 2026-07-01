import { useEffect, useState } from 'react';
import { Badge, Button, Space, Typography } from '@arco-design/web-react';
import { IconRefresh } from '@arco-design/web-react/icon';

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
        <div className="perm-row" key={label}>
            <Badge status={ok ? 'success' : 'error'} />
            <Typography.Text className="perm-label">{label}</Typography.Text>
            <Typography.Text type="secondary">{ok ? 'granted' : 'not granted'}</Typography.Text>
            {!ok && (
                <Typography.Text type="secondary" className="perm-hint">
                    {hint}
                </Typography.Text>
            )}
        </div>
    );

    return (
        <div className="perms">
            <Typography.Title heading={6}>macOS permissions</Typography.Title>
            {perms ? (
                <Space direction="vertical" size={6} style={{ width: '100%' }}>
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
                </Space>
            ) : (
                <Typography.Text type="secondary">Checking…</Typography.Text>
            )}
            <Button size="small" icon={<IconRefresh />} onClick={check} style={{ marginTop: 12 }}>
                Recheck
            </Button>
        </div>
    );
}
