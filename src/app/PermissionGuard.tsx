import { useCallback, useEffect, useRef, useState } from 'react';
import { Badge, Button, Card, Modal, Space, Tag, Typography } from '@arco-design/web-react';

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
        <Card size="small" className="guard-perm" key={kind}>
            <div className="guard-perm-head">
                <Space size="small">
                    <Badge status={granted ? 'success' : 'error'} />
                    <Typography.Text bold>{label}</Typography.Text>
                </Space>
                <Tag color={granted ? 'green' : 'red'} size="small">
                    {granted ? 'Granted' : 'Required'}
                </Tag>
            </div>
            <Typography.Text type="secondary" className="guard-perm-why">
                {why}
            </Typography.Text>
            {!granted && (
                <Space size="small" style={{ marginTop: 12 }}>
                    <Button
                        type="primary"
                        size="small"
                        loading={busy === kind}
                        onClick={() => void request(kind)}
                    >
                        Grant access
                    </Button>
                    <Button
                        size="small"
                        onClick={() => void window.pi0.openPermissionSettings(kind)}
                    >
                        Open System Settings
                    </Button>
                </Space>
            )}
        </Card>
    );

    return (
        <Modal
            visible
            title="Welcome to pi0"
            closable={false}
            maskClosable={false}
            escToExit={false}
            style={{ width: 540 }}
            footer={
                <div className="guard-footer">
                    {perms && !perms.screenRecording && (
                        <Button size="small" onClick={() => void window.pi0.relaunchApp()}>
                            Relaunch pi0
                        </Button>
                    )}
                    <span className="guard-footer-spacer" />
                    <Button size="small" status="danger" onClick={() => void window.pi0.quitApp()}>
                        Quit
                    </Button>
                </div>
            }
        >
            <Typography.Paragraph type="secondary" style={{ marginTop: 0 }}>
                pi0 records your keystrokes and screen to build your personal workbench. Grant both
                macOS permissions to continue.
            </Typography.Paragraph>

            {perms ? (
                <Space direction="vertical" size="medium" style={{ width: '100%' }}>
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
                </Space>
            ) : (
                <Typography.Text type="secondary">Checking permissions…</Typography.Text>
            )}
        </Modal>
    );
}
