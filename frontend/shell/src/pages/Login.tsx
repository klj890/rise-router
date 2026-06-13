import { App, Button, Card, Form, Input, Typography, theme } from 'antd'
import { MobileOutlined, SafetyOutlined } from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { useAuthStore } from '../store/auth'
import { useThemeStore } from '../theme/store'

const { Title, Text } = Typography

/**
 * 登录页。M0 占位：手机号 + 验证码（贴合国情主注册通道），不调后端，写入本地 demo token。
 * 真实认证在 M1（OIDC + 短信）接入。
 */
export default function LoginPage() {
  const navigate = useNavigate()
  const { message } = App.useApp()
  const { token } = theme.useToken()
  const login = useAuthStore((s) => s.login)
  const brand = useThemeStore((s) => s.brand)
  const appName = brand.appName ?? 'Rise Router'

  const onFinish = (values: { phone: string }) => {
    login('demo-token-m0', values.phone)
    message.success('已登录（M0 占位）')
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
        <div
          style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 20 }}
        >
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
              企业级 AI API Router 控制台
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
            label="手机号"
            rules={[{ required: true, message: '请输入手机号' }]}
          >
            <Input prefix={<MobileOutlined />} placeholder="手机号" size="large" />
          </Form.Item>
          <Form.Item name="code" label="验证码">
            <Input prefix={<SafetyOutlined />} placeholder="验证码（M0 占位，可留空）" size="large" />
          </Form.Item>
          <Button type="primary" htmlType="submit" block size="large">
            登录
          </Button>
        </Form>
      </Card>
    </div>
  )
}
