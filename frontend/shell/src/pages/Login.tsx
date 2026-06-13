import { App, Button, Card, Form, Input, Typography, theme } from 'antd'
import { MobileOutlined, SafetyOutlined } from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '../store/auth'
import { useThemeStore } from '../theme/store'

const { Title, Text } = Typography

/**
 * 登录页。M0 占位：手机号 + 验证码（贴合国情主注册通道），不调后端，写入本地 demo token。
 * 真实认证在 M1（OIDC + 短信）接入。
 */
export default function LoginPage() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { message } = App.useApp()
  const { token } = theme.useToken()
  const login = useAuthStore((s) => s.login)
  const brand = useThemeStore((s) => s.brand)
  const appName = brand.appName ?? t('auth:login.title')

  const onFinish = (values: { phone: string }) => {
    login('demo-token-m0', values.phone)
    message.success(t('auth:login.success'))
    navigate('/dashboard', { replace: true })
  }

  return (
    <div
      style={{
        minHeight: '100vh',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: token.colorBgLayout,
      }}
    >
      <Card
        style={{ width: 380, border: `1px solid ${token.colorBorderSecondary}` }}
        styles={{ body: { padding: 32 } }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 20 }}>
          <span
            style={{
              width: 28,
              height: 28,
              borderRadius: 8,
              background: token.colorPrimary,
              display: 'inline-block',
            }}
          />
          <div style={{ lineHeight: 1.2 }}>
            <Title level={4} style={{ margin: 0 }}>
              {appName}
            </Title>
            <Text type="secondary" style={{ fontSize: 12 }}>
              {t('auth:login.subtitle')}
            </Text>
          </div>
        </div>
        <Form
          layout="vertical"
          onFinish={onFinish}
          requiredMark={false}
          initialValues={{ phone: '13800000000' }}
        >
          <Form.Item
            name="phone"
            label={t('auth:login.phone')}
            rules={[{ required: true, message: t('auth:login.phoneRequired') }]}
          >
            <Input prefix={<MobileOutlined />} placeholder={t('auth:login.phonePlaceholder')} size="large" />
          </Form.Item>
          <Form.Item name="code" label={t('auth:login.code')}>
            <Input prefix={<SafetyOutlined />} placeholder={t('auth:login.codePlaceholder')} size="large" />
          </Form.Item>
          <Button type="primary" htmlType="submit" block size="large">
            {t('auth:login.submit')}
          </Button>
        </Form>
      </Card>
    </div>
  )
}
