import { useEffect, useRef, useState } from 'react'
import { App, Button, Form, Input, Space } from 'antd'
import { MobileOutlined, SafetyOutlined, LockOutlined, WechatOutlined } from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { api } from '../api/client'
import { useAuthStore } from '../store/auth'
import { useThemeStore } from '../theme/store'

const PHONE_RE = /^1[3-9]\d{9}$/

type Tab = 'code' | 'password' | 'register'

interface SendCodeResp {
  sent: boolean
  dev_code: string
}
interface LoginResp {
  token: string
  user: { phone: string; nickname?: string | null }
  registered: boolean
}

const STATS = [
  { v: '40+', l: '上游渠道适配' },
  { v: '99.95%', l: '路由可用性' },
  { v: '5 要素', l: '定价完全解耦' },
]

/**
 * 登录页：分屏（左品牌渐变 + 右表单）。手机号 + 短信验证码为国情主通道。
 * 发码 → 倒计时 → 验码 → 后端签发 JWT（首次登录自动建 org-of-one）。
 * 密码登录 / 注册为 UI 占位（后端暂仅支持验证码通道）。
 */
export default function LoginPage() {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { message } = App.useApp()
  const login = useAuthStore((s) => s.login)
  const brand = useThemeStore((s) => s.brand)
  const appName = brand.appName ?? t('auth:login.title')

  const [tab, setTab] = useState<Tab>('code')
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

  const tabs: { key: Tab; label: string }[] = [
    { key: 'code', label: '验证码登录' },
    { key: 'password', label: '密码登录' },
    { key: 'register', label: '注册' },
  ]
  const codeMode = tab !== 'password'

  return (
    <div style={{ minHeight: '100vh', display: 'flex', background: 'var(--rr-bg-layout)' }}>
      {/* 左品牌渐变 */}
      <div
        style={{
          flex: '1 1 0',
          minWidth: 0,
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'space-between',
          padding: '56px 60px',
          color: '#fff',
          background: 'linear-gradient(155deg, #3f37d4, #5b50e8, #7c4ddb)',
        }}
        className="rr-login-hero"
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <span
            style={{
              width: 40,
              height: 40,
              borderRadius: 11,
              background: 'rgba(255,255,255,.18)',
              display: 'inline-flex',
              alignItems: 'center',
              justifyContent: 'center',
              fontWeight: 700,
              fontSize: 19,
            }}
          >
            R
          </span>
          <div style={{ lineHeight: 1.2 }}>
            <div style={{ fontWeight: 700, fontSize: 17 }}>{appName}</div>
            <div style={{ fontSize: 11, letterSpacing: '.08em', opacity: 0.8 }}>CONTROL PLANE</div>
          </div>
        </div>

        <div>
          <h1 style={{ fontSize: 34, fontWeight: 700, lineHeight: 1.25, margin: 0, letterSpacing: '-0.02em' }}>
            企业级 AI API Router
            <br />
            统一接入 · 透明定价
          </h1>
          <p style={{ marginTop: 18, fontSize: 15, opacity: 0.85, maxWidth: 440, lineHeight: 1.7 }}>
            多上游渠道路由、五要素解耦定价、财务对账与 CRM 一体化 —— 让管理员一眼看清「某客户调某模型到底多少钱」。
          </p>
          <div style={{ display: 'flex', gap: 40, marginTop: 40 }}>
            {STATS.map((s) => (
              <div key={s.l}>
                <div className="rr-num" style={{ fontSize: 28, fontWeight: 700 }}>
                  {s.v}
                </div>
                <div style={{ fontSize: 12.5, opacity: 0.8, marginTop: 4 }}>{s.l}</div>
              </div>
            ))}
          </div>
        </div>

        <div style={{ fontSize: 12, opacity: 0.7 }}>© 2026 Rise Router · 沪ICP备 0000000 号</div>
      </div>

      {/* 右表单 */}
      <div
        style={{
          flex: '0 0 480px',
          maxWidth: '100%',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          padding: 40,
        }}
      >
        <div style={{ width: '100%', maxWidth: 360 }}>
          <h2 style={{ fontSize: 23, fontWeight: 700, margin: 0, color: 'var(--rr-text)' }}>
            {tab === 'register' ? '创建账号' : '欢迎回来'}
          </h2>
          <p style={{ color: 'var(--rr-text-2)', marginTop: 6, fontSize: 13.5 }}>
            {t('auth:login.subtitle')}
          </p>

          {/* 三态切换 */}
          <div
            style={{
              display: 'flex',
              gap: 4,
              margin: '22px 0 20px',
              padding: 4,
              borderRadius: 10,
              background: 'var(--rr-surface-2)',
              border: '1px solid var(--rr-border)',
            }}
          >
            {tabs.map((tb) => {
              const active = tab === tb.key
              return (
                <button
                  key={tb.key}
                  type="button"
                  onClick={() => setTab(tb.key)}
                  style={{
                    flex: 1,
                    height: 32,
                    border: 'none',
                    borderRadius: 7,
                    cursor: 'pointer',
                    fontSize: 13,
                    fontWeight: active ? 600 : 500,
                    color: active ? 'var(--rr-primary)' : 'var(--rr-text-2)',
                    background: active ? 'var(--rr-surface)' : 'transparent',
                    boxShadow: active ? 'var(--rr-shadow)' : 'none',
                  }}
                >
                  {tb.label}
                </button>
              )
            })}
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

            {tab === 'register' && (
              <Form.Item
                name="org_name"
                label="组织名称"
                rules={[{ required: true, message: '请填写组织名称' }]}
              >
                <Input placeholder="企业 / 团队名称（将创建为你的组织）" size="large" maxLength={128} />
              </Form.Item>
            )}

            {codeMode ? (
              <Form.Item label={t('auth:login.code')} required>
                <Space.Compact style={{ width: '100%' }}>
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
                    {countdown > 0 ? t('auth:login.resendIn', { s: countdown }) : t('auth:login.sendCode')}
                  </Button>
                </Space.Compact>
              </Form.Item>
            ) : (
              <Form.Item name="password" label="密码" rules={[{ required: true, message: '请输入密码' }]}>
                <Input.Password prefix={<LockOutlined />} placeholder="请输入登录密码" size="large" />
              </Form.Item>
            )}

            {tab === 'password' ? (
              <Button
                type="primary"
                block
                size="large"
                onClick={() => message.info('密码登录通道即将开放，请先使用验证码登录')}
              >
                登录
              </Button>
            ) : (
              <Button type="primary" htmlType="submit" block size="large" loading={submitting}>
                {tab === 'register' ? '注册并登录' : t('auth:login.submit')}
              </Button>
            )}
          </Form>

          {/* 微信登录入口 */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 12, margin: '22px 0 16px' }}>
            <span style={{ flex: 1, height: 1, background: 'var(--rr-border)' }} />
            <span style={{ fontSize: 12, color: 'var(--rr-text-3)' }}>其他方式</span>
            <span style={{ flex: 1, height: 1, background: 'var(--rr-border)' }} />
          </div>
          <Button
            block
            size="large"
            icon={<WechatOutlined style={{ color: '#09bb07' }} />}
            onClick={() => message.info('微信登录即将开放')}
          >
            微信登录
          </Button>
        </div>
      </div>
    </div>
  )
}
