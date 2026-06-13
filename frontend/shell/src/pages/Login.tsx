import { Button, Card, Form, Input, Typography, message } from 'antd'
import { MobileOutlined, SafetyOutlined } from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { useAuthStore } from '../store/auth'

const { Title, Paragraph } = Typography

/**
 * 登录页。M0 为占位实现：手机号 + 验证码表单（贴合国情主注册通道的设计），
 * 但不调用后端，直接写入一个本地 demo token。真实认证在 M1（OIDC + 短信）接入。
 */
export default function LoginPage() {
  const navigate = useNavigate()
  const login = useAuthStore((s) => s.login)

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
        background: '#0f1f3a',
      }}
    >
      <Card style={{ width: 380 }}>
        <Title level={3} style={{ textAlign: 'center', marginBottom: 4 }}>
          Rise Router
        </Title>
        <Paragraph type="secondary" style={{ textAlign: 'center' }}>
          企业级 AI API Router 控制台
        </Paragraph>
        <Form layout="vertical" onFinish={onFinish} initialValues={{ phone: '13800000000' }}>
          <Form.Item
            name="phone"
            label="手机号"
            rules={[{ required: true, message: '请输入手机号' }]}
          >
            <Input prefix={<MobileOutlined />} placeholder="手机号" />
          </Form.Item>
          <Form.Item name="code" label="验证码">
            <Input prefix={<SafetyOutlined />} placeholder="验证码（M0 占位，可留空）" />
          </Form.Item>
          <Button type="primary" htmlType="submit" block>
            登录
          </Button>
        </Form>
      </Card>
    </div>
  )
}
