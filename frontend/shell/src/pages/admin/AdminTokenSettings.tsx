import { useState } from 'react'
import { Card, Form, Input, Button, Typography, Alert, Space, message } from 'antd'
import { useAuthStore } from '../../store/auth'

/** 管理令牌设置：RBAC 落地前用 X-Admin-Token 匹配后端 RR_ADMIN_TOKEN。本地持久化。 */
export default function AdminTokenSettings() {
  const adminToken = useAuthStore((s) => s.adminToken)
  const setAdminToken = useAuthStore((s) => s.setAdminToken)
  const [value, setValue] = useState(adminToken ?? '')

  return (
    <Card title="管理令牌（X-Admin-Token）" style={{ maxWidth: 640 }}>
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 16 }}
        message="过渡方案（RBAC 落地前）"
        description="管理台所有 CRUD 端点用此令牌鉴权，需与后端环境变量 RR_ADMIN_TOKEN 一致。仅存于本浏览器 localStorage。"
      />
      <Form layout="vertical">
        <Form.Item label="令牌">
          <Input.Password
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder="粘贴 RR_ADMIN_TOKEN"
            autoComplete="off"
          />
        </Form.Item>
        <Space>
          <Button
            type="primary"
            onClick={() => {
              setAdminToken(value)
              message.success(value.trim() ? '已保存管理令牌' : '已清除管理令牌')
            }}
          >
            保存
          </Button>
          <Button
            onClick={() => {
              setValue('')
              setAdminToken(null)
              message.success('已清除管理令牌')
            }}
          >
            清除
          </Button>
        </Space>
      </Form>
      <Typography.Paragraph type="secondary" style={{ marginTop: 16, marginBottom: 0 }}>
        当前状态：{adminToken ? '已设置' : '未设置'}
      </Typography.Paragraph>
    </Card>
  )
}
