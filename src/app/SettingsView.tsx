import { useEffect, useState } from 'react';
import {
    Button,
    Divider,
    Form,
    Input,
    InputNumber,
    Message,
    Spin,
    Switch,
    Typography,
} from '@arco-design/web-react';
import { IconSave } from '@arco-design/web-react/icon';

import type { Settings } from '../shared/schemas';
import { PermissionsPanel } from './PermissionsPanel';

const FormItem = Form.Item;

interface SettingsForm {
    intervalSec: number;
    captureOnHotkey: boolean;
}

export function SettingsView() {
    const [form] = Form.useForm<SettingsForm>();
    const [settings, setSettings] = useState<Settings | null>(null);
    const [saving, setSaving] = useState(false);

    useEffect(() => {
        void window.pi0.getSettings().then((s) => {
            setSettings(s);
            form.setFieldsValue({
                intervalSec: Math.round(s.intervalMs / 1000),
                captureOnHotkey: s.captureOnHotkey,
            });
        });
    }, [form]);

    const save = async () => {
        if (!settings) return;
        const values = await form.validate();
        setSaving(true);
        try {
            const next = await window.pi0.saveSettings({
                intervalMs: Math.min(
                    3_600_000,
                    Math.max(1000, Math.round(values.intervalSec) * 1000),
                ),
                hotkey: settings.hotkey,
                captureOnHotkey: values.captureOnHotkey,
            });
            setSettings(next);
            Message.success('Settings saved');
        } finally {
            setSaving(false);
        }
    };

    if (!settings) {
        return <Spin loading style={{ display: 'block', marginTop: 40 }} />;
    }

    return (
        <div className="panel">
            <Typography.Title heading={5} style={{ marginTop: 0 }}>
                Capture settings
            </Typography.Title>

            <Form
                form={form}
                layout="vertical"
                style={{ maxWidth: 440 }}
                initialValues={{
                    intervalSec: Math.round(settings.intervalMs / 1000),
                    captureOnHotkey: settings.captureOnHotkey,
                }}
            >
                <FormItem
                    label="Screenshot interval"
                    field="intervalSec"
                    rules={[{ required: true, type: 'number', min: 1, max: 3600 }]}
                >
                    <InputNumber
                        min={1}
                        max={3600}
                        step={1}
                        suffix="seconds"
                        style={{ width: 200 }}
                    />
                </FormItem>

                <FormItem
                    label="Capture on hotkey"
                    field="captureOnHotkey"
                    triggerPropName="checked"
                    extra={`Triggers a screenshot when you press ${settings.hotkey.join(' + ')}`}
                >
                    <Switch />
                </FormItem>

                <FormItem label="Data folder">
                    <Input readOnly value={settings.dataDir} />
                </FormItem>

                <FormItem>
                    <Button type="primary" icon={<IconSave />} loading={saving} onClick={save}>
                        Save
                    </Button>
                </FormItem>
            </Form>

            <Divider />

            <PermissionsPanel />
        </div>
    );
}
