import { useMemo } from 'react'
import { Card, Col, Row, Tag, Typography, Alert, theme } from 'antd'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { api } from '../api/client'

// 后端 /readyz 的 db 状态 → i18n key（未知值回落显示原始串）。
const DB_STATUS_KEY: Record<string, string> = {
  up: 'up',
  not_connected: 'notConnected',
  unreachable: 'unreachable',
  '...': 'loading',
}

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
  const { t, i18n } = useTranslation()
  const ready = useQuery<ReadyResp>({
    queryKey: ['readyz'],
    queryFn: async () => (await api.get<ReadyResp>('/readyz')).data,
    refetchInterval: 10000,
    retry: false,
  })

  const backendUp = ready.isSuccess
  const dbState = ready.data?.db ?? (ready.isError ? 'unreachable' : '...')
  const dbLabel = DB_STATUS_KEY[dbState] ? t(`common:dbStatus.${DB_STATUS_KEY[dbState]}`) : dbState
  // 整数指标用 numberFmt；货币金额用 balanceFmt 保留两位小数；按 locale 缓存格式化器。
  const numberFmt = useMemo(() => new Intl.NumberFormat(i18n.language), [i18n.language])
  const balanceFmt = useMemo(
    () => new Intl.NumberFormat(i18n.language, { minimumFractionDigits: 2, maximumFractionDigits: 2 }),
    [i18n.language],
  )

  return (
    <div>
      <Title level={4} style={{ marginTop: 0 }}>
        {t('common:dashboard.title')}
      </Title>
      <Text type="secondary">{t('common:dashboard.scaffoldHint')}</Text>

      {ready.isError && (
        <Alert
          type="warning"
          showIcon
          message={t('common:dashboard.readyzError')}
          style={{ margin: '16px 0' }}
        />
      )}

      <Row gutter={[16, 16]} style={{ marginTop: 16 }}>
        <Col xs={24} sm={12} md={6}>
          <MetricCard title={t('common:dashboard.todayCalls')} value={numberFmt.format(1284920)} accent />
        </Col>
        <Col xs={24} sm={12} md={6}>
          <MetricCard
            title={t('common:dashboard.balance')}
            value={balanceFmt.format(8420)}
            suffix={t('common:dashboard.unitYuan')}
          />
        </Col>
        <Col xs={24} sm={12} md={6}>
          <Card>
            <Text type="secondary" style={{ fontSize: 13 }}>
              {t('common:dashboard.backend')}
            </Text>
            <div style={{ marginTop: 12 }}>
              <Tag color={backendUp ? 'success' : 'error'}>
                {backendUp ? t('common:status.online') : t('common:status.offline')}
              </Tag>
            </div>
          </Card>
        </Col>
        <Col xs={24} sm={12} md={6}>
          <Card>
            <Text type="secondary" style={{ fontSize: 13 }}>
              {t('common:dashboard.database')}
            </Text>
            <div style={{ marginTop: 12 }}>
              <Tag color={dbState === 'up' ? 'success' : 'warning'}>{dbLabel}</Tag>
            </div>
          </Card>
        </Col>
      </Row>
    </div>
  )
}
