import { useState } from 'react'
import { Card, Form, Input, Button, Descriptions, Tag, Table, Typography, Alert, Space } from 'antd'
import { useMutation } from '@tanstack/react-query'
import { pricePreview, type PricePreview as Preview } from '../../api/admin'

/** 价格预览（所见即所得）：复用后端 /api/pricing/preview，与计费热路径同一 resolve_price。 */
export default function PricePreviewPage() {
  const [model, setModel] = useState('')
  const [group, setGroup] = useState('')

  const mutation = useMutation({
    mutationFn: () => pricePreview(model, group),
    onError: () => {},
  })

  const result: Preview | undefined = mutation.data

  return (
    <div style={{ maxWidth: 880 }}>
      <Typography.Title level={4}>价格预览</Typography.Title>
      <Typography.Paragraph type="secondary">
        输入模型 slug 与分组 slug（留空=默认价），得到该组合的最终单价 + 命中折扣明细。
        与网关计费走同一解析函数，所见即所得。
      </Typography.Paragraph>

      <Card style={{ marginBottom: 16 }}>
        <Form layout="inline" onFinish={() => mutation.mutate()}>
          <Form.Item label="模型 slug" required>
            <Input value={model} onChange={(e) => setModel(e.target.value)} placeholder="gpt-4o" />
          </Form.Item>
          <Form.Item label="分组 slug">
            <Input value={group} onChange={(e) => setGroup(e.target.value)} placeholder="留空=默认价" />
          </Form.Item>
          <Form.Item>
            <Button type="primary" htmlType="submit" loading={mutation.isPending} disabled={!model.trim()}>
              预览
            </Button>
          </Form.Item>
        </Form>
      </Card>

      {mutation.isError && (
        <Alert
          type="error"
          showIcon
          style={{ marginBottom: 16 }}
          message="预览失败"
          description={(mutation.error as { localizedMessage?: string })?.localizedMessage ?? '请检查模型/分组 slug 是否存在、是否已配价。'}
        />
      )}

      {result && (
        <Card>
          <Descriptions column={2} size="small" bordered style={{ marginBottom: 16 }}>
            <Descriptions.Item label="模型">{result.model_slug}</Descriptions.Item>
            <Descriptions.Item label="分组">{result.group_slug ?? '（默认价）'}</Descriptions.Item>
            <Descriptions.Item label="计费量纲">{result.billing_unit}</Descriptions.Item>
            <Descriptions.Item label="币种">{result.currency}</Descriptions.Item>
            <Descriptions.Item label="价格版本">v{result.price_version}</Descriptions.Item>
            <Descriptions.Item label="折扣系数">{result.discount_factor}</Descriptions.Item>
          </Descriptions>

          <Space size="large" align="start" style={{ width: '100%' }} wrap>
            <div>
              <Typography.Text strong>折前单价</Typography.Text>
              <pre style={{ margin: '8px 0' }}>{JSON.stringify(result.base_unit_prices, null, 2)}</pre>
            </div>
            <div>
              <Typography.Text strong>折后单价</Typography.Text>
              <pre style={{ margin: '8px 0' }}>{JSON.stringify(result.final_unit_prices, null, 2)}</pre>
            </div>
          </Space>

          <Typography.Text strong>命中折扣</Typography.Text>
          <Table
            style={{ marginTop: 8 }}
            size="small"
            rowKey="id"
            pagination={false}
            dataSource={result.applied_discounts}
            columns={[
              { title: '名称', dataIndex: 'name' },
              { title: '类型', dataIndex: 'kind' },
              { title: '值', dataIndex: 'value' },
              {
                title: '已并入单价',
                dataIndex: 'applied',
                render: (v: boolean) =>
                  v ? <Tag color="green">是</Tag> : <Tag>否（结算期作用账单）</Tag>,
              },
            ]}
          />
        </Card>
      )}
    </div>
  )
}
