import type { ReactNode } from 'react'
import { Drawer, Button, Space } from 'antd'

interface FormDrawerProps {
  open: boolean
  title: ReactNode
  /** 标题下副标 */
  subtitle?: ReactNode
  onClose: () => void
  onOk?: () => void
  okText?: string
  okLoading?: boolean
  okDisabled?: boolean
  width?: number
  children: ReactNode
  /** 自定义底部（覆盖默认取消/确定） */
  footer?: ReactNode
  /** 是否显示底部操作栏，默认 true */
  showFooter?: boolean
}

/**
 * 右侧抽屉骨架：统一标题/副标 + 底部操作栏。设计稿所有「新建/详情」走抽屉而非弹窗。
 * 配合 DrawerSection 实现编号分区(① ② ③)。
 */
export default function FormDrawer({
  open,
  title,
  subtitle,
  onClose,
  onOk,
  okText = '保存',
  okLoading,
  okDisabled,
  width = 560,
  children,
  footer,
  showFooter = true,
}: FormDrawerProps) {
  return (
    <Drawer
      open={open}
      onClose={onClose}
      width={width}
      destroyOnClose
      title={
        <div>
          <div style={{ fontSize: 17, fontWeight: 700, color: 'var(--rr-text)' }}>{title}</div>
          {subtitle && <div style={{ fontSize: 12.5, color: 'var(--rr-text-2)', marginTop: 2 }}>{subtitle}</div>}
        </div>
      }
      footer={
        showFooter ? (
          footer ?? (
            <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
              <Space>
                <Button onClick={onClose}>取消</Button>
                <Button type="primary" loading={okLoading} disabled={okDisabled} onClick={onOk}>
                  {okText}
                </Button>
              </Space>
            </div>
          )
        ) : null
      }
    >
      {children}
    </Drawer>
  )
}

interface DrawerSectionProps {
  /** 分区序号（① ② ③） */
  index?: number
  title: ReactNode
  hint?: ReactNode
  children: ReactNode
  last?: boolean
}

/** 抽屉内编号分区：圆形序号 + 标题 + 内容。 */
export function DrawerSection({ index, title, hint, children, last }: DrawerSectionProps) {
  return (
    <div style={{ marginBottom: last ? 0 : 26 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 9, marginBottom: 12 }}>
        {index != null && (
          <span
            className="rr-num"
            style={{
              width: 20,
              height: 20,
              borderRadius: '50%',
              background: 'var(--rr-primary-weak)',
              color: 'var(--rr-primary)',
              fontSize: 12,
              fontWeight: 700,
              display: 'inline-flex',
              alignItems: 'center',
              justifyContent: 'center',
              flexShrink: 0,
            }}
          >
            {index}
          </span>
        )}
        <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--rr-text)' }}>{title}</span>
        {hint && <span style={{ fontSize: 12, color: 'var(--rr-text-3)' }}>{hint}</span>}
      </div>
      {children}
    </div>
  )
}
