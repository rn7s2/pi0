import { useState } from 'react';
import {
    Alert,
    Button,
    Card,
    DatePicker,
    Empty,
    Space,
    Spin,
    Tag,
    Typography,
} from '@arco-design/web-react';
import { IconSearch } from '@arco-design/web-react/icon';
import dayjs, { type Dayjs } from 'dayjs';

import type { TextRecord } from '../shared/schemas';

const { RangePicker } = DatePicker;

export function DataViewer() {
    const now = dayjs();
    const [range, setRange] = useState<Dayjs[]>([now.subtract(1, 'hour'), now]);
    const [records, setRecords] = useState<TextRecord[] | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const query = async () => {
        setLoading(true);
        setError(null);
        try {
            const [start, end] = range;
            setRecords(
                await window.pi0.queryText({ startMs: start.valueOf(), endMs: end.valueOf() }),
            );
        } catch (e) {
            setError((e as Error).message);
            setRecords(null);
        } finally {
            setLoading(false);
        }
    };

    return (
        <div className="panel">
            <Typography.Title heading={5} style={{ marginTop: 0 }}>
                Recorded text
            </Typography.Title>

            <Space wrap size="medium" align="center" style={{ marginBottom: 20 }}>
                <RangePicker
                    showTime
                    value={range}
                    onChange={(_, dates) => setRange(dates as Dayjs[])}
                    format="YYYY-MM-DD HH:mm"
                    style={{ width: 380 }}
                    shortcuts={[
                        { text: 'Last hour', value: () => [dayjs().subtract(1, 'hour'), dayjs()] },
                        { text: 'Today', value: () => [dayjs().startOf('day'), dayjs()] },
                        { text: 'Last 24h', value: () => [dayjs().subtract(1, 'day'), dayjs()] },
                    ]}
                />
                <Button type="primary" icon={<IconSearch />} loading={loading} onClick={query}>
                    Query
                </Button>
            </Space>

            {error && <Alert type="error" content={error} style={{ marginBottom: 16 }} showIcon />}

            <Spin loading={loading} style={{ display: 'block' }}>
                {records && (
                    <>
                        <Typography.Text type="secondary">
                            {records.length} record{records.length === 1 ? '' : 's'}
                        </Typography.Text>
                        {records.length === 0 ? (
                            <Empty description="No records in this range" />
                        ) : (
                            <Space direction="vertical" size="small" style={{ width: '100%' }}>
                                {records.map((r, i) => (
                                    <Card
                                        key={`${r.ts}-${i}`}
                                        size="small"
                                        className="record-card"
                                        title={
                                            <div className="record-meta">
                                                <Tag size="small" color="arcoblue">
                                                    {r.appRaw}
                                                </Tag>
                                                <Typography.Text
                                                    type="secondary"
                                                    className="record-time"
                                                >
                                                    {new Date(r.ts).toLocaleString()}
                                                </Typography.Text>
                                            </div>
                                        }
                                    >
                                        <pre className="record-text">{r.text}</pre>
                                    </Card>
                                ))}
                            </Space>
                        )}
                    </>
                )}
            </Spin>
        </div>
    );
}
