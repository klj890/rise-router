import { Card, Col, Row, Tag, Typography, Alert } from 'antd'
import { useQuery } from '@tanstack/react-query'
import { api } from '../api/client'

const { Title, Paragraph } = Typography

interface ReadyResp {
  status: string
  db: string
}

/** 概览页。M0：实时探测后端 /readyz，验证前后端联通；其余为后续里程碑占位卡片。 */
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
      <Title level={4}>概览</Title>
      <Paragraph type="secondary">M0 脚手架：前端 Shell 已联通后端健康检查。</Paragraph>

      {ready.isError && (
        <Alert
          type="warning"
          showIcon
          message="后端 /readyz 不可达或处于 degraded（数据库未连接）"
          style={{ marginBottom: 16 }}
        />
      )}

      <Row gutter={16}>
        <Col xs={24} sm={12} md={8}>
          <Card title="后端服务">
            <Tag color={backendUp ? 'green' : 'red'}>{backendUp ? '在线' : '离线/降级'}</Tag>
          </Card>
        </Col>
        <Col xs={24} sm={12} md={8}>
          <Card title="数据库">
            <Tag color={dbState === 'up' ? 'green' : 'orange'}>{dbState}</Tag>
          </Card>
        </Col>
        <Col xs={24} sm={12} md={8}>
          <Card title="里程碑">
            <Tag color="blue">M0 脚手架</Tag>
          </Card>
        </Col>
      </Row>
    </div>
  )
}
