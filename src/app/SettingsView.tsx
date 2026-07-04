import { useEffect, useState } from 'react';
import {
    Button,
    Divider,
    Form,
    Input,
    InputNumber,
    Message,
    Spin,
    Typography,
} from '@arco-design/web-react';
import { IconSave } from '@arco-design/web-react/icon';

import type { Settings } from '../shared/schemas';
import { PermissionsPanel } from './PermissionsPanel';

const FormItem = Form.Item;

interface SettingsForm {
    intervalSec: number;
    mcpPort: number;
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
                mcpPort: s.mcpPort,
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
                mcpPort: values.mcpPort,
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
                    mcpPort: settings.mcpPort,
                }}
            >
                <FormItem
                    label="Screenshot interval"
                    field="intervalSec"
                    rules={[{ required: true, type: 'number', min: 1, max: 3600 }]}
                    extra="Screenshots are OCR'd into text on-device and the image is deleted right after — only the recognized text is kept."
                >
                    <InputNumber
                        min={1}
                        max={3600}
                        step={1}
                        suffix="seconds"
                        style={{ width: 200 }}
                    />
                </FormItem>

                <FormItem label="Data folder">
                    <Input readOnly value={settings.dataDir} />
                </FormItem>

                <Divider />

                <Typography.Title heading={5}>MCP server</Typography.Title>

                <FormItem
                    label="Port"
                    field="mcpPort"
                    rules={[{ required: true, type: 'number', min: 1024, max: 65535 }]}
                    extra={`Agents connect via Streamable HTTP at http://127.0.0.1:${settings.mcpPort}/mcp`}
                >
                    <InputNumber min={1024} max={65535} step={1} style={{ width: 200 }} />
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
