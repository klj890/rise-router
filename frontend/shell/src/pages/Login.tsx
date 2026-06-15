import { useEffect, useRef, useState } from 'react'
import { App, Button, Card, Form, Input, Space, Typography, theme } from 'antd'
import { MobileOutlined, SafetyOutlined } from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { api } from '../api/client'
import { useAuthStore } from '../store/auth'
import { useThemeStore } from '../theme/store'

const { Title, Text } = Typography

const PHONE_RE = /^1[3-9]\d{9}$/

interface SendCodeResp {
  sent: boolean
  dev_code: string
}
interface LoginResp {
  token: string
  user: { phone: string; nickname?: string | null }
  registered: boolean
}

/**
 * 登录页：手机号 + 短信验证码注册/登录（国情主通道）。
 * 发码 → 倒计时 → 验码 → 后端签发 JWT 会话令牌（首次登录自动建 org-of-one）。
 * mock 短信网关把验证码经 dev_code 回显，前端用 message 提示便于演示。
 */
export default function LoginPage() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { message } = App.useApp()
  const { token } = theme.useToken()
  const login = useAuthStore((s) => s.login)
  const brand = useThemeStore((s) => s.brand)
  const appName = brand.appName ?? t('auth:login.title')

  const [form] = Form.useForm()
  const [sending, setSending] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [countdown, setCountdown] = useState(0)
  const timer = useRef<ReturnType<typeof setInterval> | null>(null)

  const clearTimer = () => {
    if (timer.current) {
      clearInterval(timer.current)
      timer.current = null
    }
  }
  useEffect(() => clearTimer, [])

  const startCountdown = () => {
    setCountdown(60)
    clearTimer()
    timer.current = setInterval(() => {
      setCountdown((c) => {
        if (c <= 1) {
          clearTimer()
          return 0
        }
        return c - 1
      })
    }, 1000)
  }

  const sendCode = async () => {
    const phone = (form.getFieldValue('phone') as string | undefined)?.trim()
    if (!phone || !PHONE_RE.test(phone)) {
      message.error(t('auth:login.phoneInvalid'))
      return
    }
    setSending(true)
    try {
      const { data } = await api.post<SendCodeResp>('/api/identity/auth/send-code', { phone })
      message.success(t('auth:login.codeSent'))
      // mock 网关：把验证码提示出来便于演示（真实接入后端不再回显）
      if (data.dev_code) {
        message.info(t('auth:login.devCode', { code: data.dev_code }), 8)
        form.setFieldValue('code', data.dev_code)
      }
      startCountdown()
    } catch (e) {
      message.error((e as { localizedMessage?: string }).localizedMessage ?? '发送失败')
    } finally {
      setSending(false)
    }
  }

  const onFinish = async (values: { phone: string; code: string }) => {
    setSubmitting(true)
    try {
      const { data } = await api.post<LoginResp>('/api/identity/auth/login', {
        phone: values.phone.trim(),
        code: values.code.trim(),
      })
      login(data.token, data.user.nickname || data.user.phone)
      message.success(data.registered ? t('auth:login.registered') : t('auth:login.success'))
      navigate('/dashboard', { replace: true })
    } catch (e) {
      message.error((e as { localizedMessage?: string }).localizedMessage ?? '登录失败')
    } finally {
      setSubmitting(false)
    }
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
        <Form form={form} layout="vertical" onFinish={onFinish} requiredMark={false}>
          <Form.Item
            name="phone"
            label={t('auth:login.phone')}
            rules={[
              { required: true, message: t('auth:login.phoneRequired') },
              { pattern: PHONE_RE, message: t('auth:login.phoneInvalid') },
            ]}
          >
            <Input
              prefix={<MobileOutlined />}
              placeholder={t('auth:login.phonePlaceholder')}
              size="large"
              autoComplete="tel"
            />
          </Form.Item>
          <Form.Item label={t('auth:login.code')} required>
            <Space.Compact style={{ width: '100%' }}>
              {/* name 必须落在 Input 这个真正受控的子组件上：直接包 Space.Compact 会把
                  value/onChange 注入到布局容器而非 Input，导致 code 字段不入表单 store。 */}
              <Form.Item
                name="code"
                noStyle
                rules={[{ required: true, message: t('auth:login.codeRequired') }]}
              >
                <Input
                  prefix={<SafetyOutlined />}
                  placeholder={t('auth:login.codePlaceholder')}
                  size="large"
                  autoComplete="one-time-code"
                />
              </Form.Item>
              <Button size="large" onClick={sendCode} loading={sending} disabled={countdown > 0}>
                {countdown > 0
                  ? t('auth:login.resendIn', { s: countdown })
                  : t('auth:login.sendCode')}
              </Button>
            </Space.Compact>
          </Form.Item>
          <Button type="primary" htmlType="submit" block size="large" loading={submitting}>
            {t('auth:login.submit')}
          </Button>
        </Form>
      </Card>
    </div>
  )
}
