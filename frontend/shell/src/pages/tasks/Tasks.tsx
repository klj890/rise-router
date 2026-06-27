import { useMemo, useState } from 'react'
import { Button, Empty, Form, Input, Select, InputNumber, Radio, message } from 'antd'
import { PlusOutlined } from '@ant-design/icons'
import { PageHeader, SectionCard, StatusPill, FilterTabs, KpiCard, FormDrawer, DrawerSection } from '../../components/ui'
import type { PillTone } from '../../components/ui'

type TaskStatus = 'running' | 'queued' | 'succeeded' | 'failed'
const STATUS: Record<TaskStatus, { label: string; tone: PillTone }> = {
  running: { label: '运行中', tone: 'primary' },
  queued: { label: '排队中', tone: 'warning' },
  succeeded: { label: '成功', tone: 'success' },
  failed: { label: '失败', tone: 'danger' },
}
const TYPE_LABEL: Record<string, string> = { video: '视频', image: '图像', audio: '语音', text: '文本' }

interface Task {
  id: string
  type: string
  model: string
  by: string
  status: TaskStatus
  elapsed: string
  artifacts: string
  cost: string
}

const TASKS: Task[] = [
  { id: 'task_7f3a91', type: 'video', model: 'kling-v2', by: 'Acme 智能科技', status: 'running', elapsed: '02:14', artifacts: '0 / 1', cost: '¥1.20' },
  { id: 'task_7f3a8c', type: 'image', model: 'flux-1.1-pro', by: '云帆数据', status: 'succeeded', elapsed: '00:08', artifacts: '4', cost: '¥0.32' },
  { id: 'task_7f3a72', type: 'audio', model: 'cosyvoice-2', by: 'Acme 智能科技', status: 'succeeded', elapsed: '00:03', artifacts: '1', cost: '¥0.05' },
  { id: 'task_7f3a55', type: 'video', model: 'kling-v2', by: '星河科技', status: 'queued', elapsed: '—', artifacts: '0', cost: '—' },
  { id: 'task_7f3a41', type: 'image', model: 'sdxl', by: '云帆数据', status: 'failed', elapsed: '00:01', artifacts: '0', cost: '—' },
]

export default function Tasks() {
  const [typeFilter, setTypeFilter] = useState('all')
  const [open, setOpen] = useState(false)
  const [submitType, setSubmitType] = useState('image')
  const [form] = Form.useForm()

  const counts = useMemo(() => {
    const c: Record<string, number> = { running: 0, queued: 0, succeeded: 0, failed: 0 }
    TASKS.forEach((t) => (c[t.status] += 1))
    return c
  }, [])

  const filtered = typeFilter === 'all' ? TASKS : TASKS.filter((t) => t.type === typeFilter)

  return (
    <div>
      <PageHeader
        title="多模态任务"
        subtitle="统一 /v1/tasks 提交、查询与取消 —— 文本 / 图像 / 语音 / 视频共用状态机，产物落对象存储，按量纲计费。"
        extra={
          <Button type="primary" icon={<PlusOutlined />} onClick={() => setOpen(true)}>
            提交任务
          </Button>
        }
      />

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 16, marginBottom: 16 }}>
        <KpiCard label="运行中" value={counts.running} accent />
        <KpiCard label="排队中" value={counts.queued} />
        <KpiCard label="今日完成" value="1,602" />
        <KpiCard label="失败" value={counts.failed} />
      </div>

      <SectionCard flush>
        <div style={{ padding: '14px 18px', borderBottom: '1px solid var(--rr-border)' }}>
          <FilterTabs
            items={[
              { key: 'all', label: '全部' },
              { key: 'text', label: '文本' },
              { key: 'image', label: '图像' },
              { key: 'audio', label: '语音' },
              { key: 'video', label: '视频' },
            ]}
            value={typeFilter}
            onChange={setTypeFilter}
          />
        </div>
        {filtered.length === 0 ? (
          <Empty style={{ padding: 48 }} description="无匹配任务" />
        ) : (
          <table className="rr-table">
            <thead>
              <tr>
                <th style={{ textAlign: 'left' }}>任务 ID</th>
                <th style={{ textAlign: 'left' }}>类型</th>
                <th style={{ textAlign: 'left' }}>模型</th>
                <th style={{ textAlign: 'left' }}>提交方</th>
                <th>状态</th>
                <th style={{ textAlign: 'right' }}>耗时</th>
                <th style={{ textAlign: 'right' }}>产物</th>
                <th style={{ textAlign: 'right' }}>计费</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((t) => (
                <tr key={t.id}>
                  <td className="rr-num" style={{ fontWeight: 600 }}>{t.id}</td>
                  <td><span className="rr-chip">{TYPE_LABEL[t.type] ?? t.type}</span></td>
                  <td className="rr-num" style={{ color: 'var(--rr-text-2)' }}>{t.model}</td>
                  <td style={{ color: 'var(--rr-text-2)' }}>{t.by}</td>
                  <td>
                    <StatusPill tone={STATUS[t.status].tone} dot>{STATUS[t.status].label}</StatusPill>
                  </td>
                  <td className="rr-num" style={{ textAlign: 'right' }}>{t.elapsed}</td>
                  <td className="rr-num" style={{ textAlign: 'right' }}>{t.artifacts}</td>
                  <td className="rr-num" style={{ textAlign: 'right' }}>{t.cost}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </SectionCard>

      <FormDrawer
        open={open}
        title="提交任务"
        subtitle="统一任务 API：选择类型 → 模型 → 参数 → 可选 webhook 回调"
        onClose={() => setOpen(false)}
        okText="提交"
        onOk={() => {
          message.success('任务已提交（演示）')
          setOpen(false)
        }}
      >
        <Form form={form} layout="vertical">
          <DrawerSection index={1} title="任务类型">
            <Radio.Group
              value={submitType}
              onChange={(e) => setSubmitType(e.target.value)}
              optionType="button"
              options={[
                { value: 'image', label: '图像生成' },
                { value: 'video', label: '视频生成' },
                { value: 'audio', label: '语音合成' },
              ]}
            />
          </DrawerSection>
          <DrawerSection index={2} title="模型与输入">
            <Form.Item label="模型" name="model">
              <Select
                placeholder="选择模型"
                options={
                  submitType === 'video'
                    ? [{ value: 'kling-v2', label: 'kling-v2' }]
                    : submitType === 'audio'
                      ? [{ value: 'cosyvoice-2', label: 'cosyvoice-2' }]
                      : [{ value: 'flux-1.1-pro', label: 'flux-1.1-pro' }, { value: 'sdxl', label: 'sdxl' }]
                }
              />
            </Form.Item>
            <Form.Item label="Prompt" name="prompt">
              <Input.TextArea rows={3} placeholder="描述你想生成的内容" />
            </Form.Item>
          </DrawerSection>
          <DrawerSection index={3} title="类型参数" last>
            {submitType === 'image' && (
              <div style={{ display: 'flex', gap: 12 }}>
                <Form.Item label="尺寸" name="size" style={{ flex: 1 }}>
                  <Select defaultValue="1024x1024" options={[{ value: '1024x1024', label: '1024×1024' }, { value: '1792x1024', label: '1792×1024' }]} />
                </Form.Item>
                <Form.Item label="数量" name="n" style={{ width: 120 }}>
                  <InputNumber min={1} max={4} defaultValue={1} style={{ width: '100%' }} />
                </Form.Item>
              </div>
            )}
            {submitType === 'video' && (
              <div style={{ display: 'flex', gap: 12 }}>
                <Form.Item label="时长(秒)" name="duration" style={{ flex: 1 }}>
                  <InputNumber min={1} max={10} defaultValue={5} style={{ width: '100%' }} />
                </Form.Item>
                <Form.Item label="分辨率" name="resolution" style={{ flex: 1 }}>
                  <Select defaultValue="720p" options={[{ value: '720p', label: '720p' }, { value: '1080p', label: '1080p' }]} />
                </Form.Item>
              </div>
            )}
            {submitType === 'audio' && (
              <div style={{ display: 'flex', gap: 12 }}>
                <Form.Item label="音色" name="voice" style={{ flex: 1 }}>
                  <Select defaultValue="warm" options={[{ value: 'warm', label: '温暖' }, { value: 'news', label: '播报' }]} />
                </Form.Item>
                <Form.Item label="语速" name="speed" style={{ width: 120 }}>
                  <InputNumber min={0.5} max={2} step={0.1} defaultValue={1} style={{ width: '100%' }} />
                </Form.Item>
              </div>
            )}
            <Form.Item label="回调 Webhook" name="webhook">
              <Input placeholder="https://...（可选，任务完成回调）" />
            </Form.Item>
          </DrawerSection>
        </Form>
      </FormDrawer>
    </div>
  )
}
