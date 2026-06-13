import { Card, Col, Row, Tag, Typography, Alert, theme } from 'antd'
import { useQuery } from '@tanstack/react-query'
import { api } from '../api/client'

const { Title, Text } = Typography

interface ReadyResp {
  status: string
  db: string
}

/** 单个指标卡：标题 + 等宽数字 + 副标。 */
function MetricCard({
  title,
  value,
  suffix,
  accent,
}: {
  title: string
  value: string
  suffix?: string
  accent?: boolean
}) {
  const { token } = theme.useToken()
  return (
    <Card style={{ border: `1px solid ${token.colorBorderSecondary}` }}>
      <Text type="secondary" style={{ fontSize: 13 }}>
        {title}
      </Text>
      <div style={{ marginTop: 8, display: 'flex', alignItems: 'baseline', gap: 6 }}>
        <span
          className="rr-num"
          style={{
            fontSize: 28,
            fontWeight: 600,
            color: accent ? token.colorPrimary : token.colorText,
          }}
        >
          {value}
        </span>
        {suffix && (
          <Text type="secondary" style={{ fontSize: 13 }}>
            {suffix}
          </Text>
        )}
      </div>
    </Card>
  )
}

/** 概览页。M0：实时探测后端 /readyz，验证前后端联通；其余为后续里程碑占位。 */
export default function DashboardPage() {
  const ready = useQuery<ReadyResp>({
    queryKey: ['readyz'],
    queryFn: async () => (await api.get<ReadyResp>('/readyz')).data,
    refetchInterval: 10000,
    retry: false,
  })

  const backendUp = ready.isSuccess
  const dbState = ready.data?.db ?? (ready.isError ? 'unreachable' : '...')

  return (
    <div>
      <Title level={4} style={{ marginTop: 0 }}>
        概览
      </Title>
      <Text type="secondary">M0 脚手架：前端 Shell 已联通后端健康检查。</Text>

      {ready.isError && (
        <Alert
          type="warning"
          showIcon
          message="后端 /readyz 不可达或处于 degraded（数据库未连接）"
          style={{ margin: '16px 0' }}
        />
      )}

      <Row gutter={[16, 16]} style={{ marginTop: 16 }}>
        <Col xs={24} sm={12} md={6}>
          <MetricCard title="今日调用" value="1,284,920" accent />
        </Col>
        <Col xs={24} sm={12} md={6}>
          <MetricCard title="账户余额" value="8,420.00" suffix="元" />
        </Col>
        <Col xs={24} sm={12} md={6}>
          <Card>
            <Text type="secondary" style={{ fontSize: 13 }}>
              后端服务
            </Text>
            <div style={{ marginTop: 12 }}>
              <Tag color={backendUp ? 'success' : 'error'}>{backendUp ? '在线' : '离线 / 降级'}</Tag>
            </div>
          </Card>
        </Col>
        <Col xs={24} sm={12} md={6}>
          <Card>
            <Text type="secondary" style={{ fontSize: 13 }}>
              数据库
            </Text>
            <div style={{ marginTop: 12 }}>
              <Tag color={dbState === 'up' ? 'success' : 'warning'}>{dbState}</Tag>
            </div>
          </Card>
        </Col>
      </Row>
    </div>
  )
}
