import { App, Button, Card, ColorPicker, Input, Segmented, Slider, Space, Typography } from 'antd'
import { useThemeStore } from '../../theme/store'
import { useResolvedMode } from '../../theme/useResolvedMode'
import { ACCENT_LIST, ACCENTS } from '../../theme/presets'
import { RADIUS_BASE } from '../../theme/tokens'
import type { ThemeMode } from '../../theme/types'

const { Title, Text } = Typography

/** 外观设置：暗/浅切换、强调色预设、per-租户白标覆盖（实时生效 + 持久化）。 */
export default function AppearancePage() {
  const { message } = App.useApp()
  const resolved = useResolvedMode()
  const mode = useThemeStore((s) => s.mode)
  const accentId = useThemeStore((s) => s.accentId)
  const brand = useThemeStore((s) => s.brand)
  const setMode = useThemeStore((s) => s.setMode)
  const setAccent = useThemeStore((s) => s.setAccent)
  const setBrand = useThemeStore((s) => s.setBrand)
  const resetBrand = useThemeStore((s) => s.resetBrand)

  return (
    <div style={{ maxWidth: 720 }}>
      <Title level={4} style={{ marginTop: 0 }}>
        外观设置
      </Title>
      <Text type="secondary">
        主题偏好保存在本地浏览器；私有化部署可由后端 /api/branding 统一下发白标。
      </Text>

      <Card title="主题模式" style={{ marginTop: 16 }}>
        <Segmented
          value={mode}
          onChange={(v) => setMode(v as ThemeMode)}
          options={[
            { label: '暗色', value: 'dark' },
            { label: '浅色', value: 'light' },
            { label: '跟随系统', value: 'system' },
          ]}
        />
      </Card>

      <Card title="强调色预设" style={{ marginTop: 16 }}>
        <Space wrap>
          {ACCENT_LIST.map((a) => (
            <Button
              key={a.id}
              type={accentId === a.id ? 'primary' : 'default'}
              onClick={() => setAccent(a.id)}
              icon={
                <span
                  style={{
                    width: 12,
                    height: 12,
                    borderRadius: 3,
                    background: a[resolved],
                    display: 'inline-block',
                  }}
                />
              }
            >
              {a.name}
            </Button>
          ))}
        </Space>
      </Card>

      <Card title="白标覆盖（per-租户）" style={{ marginTop: 16 }}>
        <Space direction="vertical" style={{ width: '100%' }} size="middle">
          <div>
            <Text>应用名</Text>
            <Input
              style={{ marginTop: 6 }}
              placeholder="Rise Router"
              value={brand.appName ?? ''}
              onChange={(e) => setBrand({ appName: e.target.value || undefined })}
            />
          </div>
          <div>
            <Text>主色覆盖</Text>
            <div style={{ marginTop: 6 }}>
              <ColorPicker
                value={brand.colorPrimary ?? ACCENTS[accentId][resolved]}
                onChange={(c) => setBrand({ colorPrimary: c.toHexString() })}
                showText
              />
            </div>
          </div>
          <div>
            <Text>圆角：{brand.borderRadius ?? RADIUS_BASE}px</Text>
            <Slider
              min={0}
              max={16}
              value={brand.borderRadius ?? RADIUS_BASE}
              onChange={(v) => setBrand({ borderRadius: v })}
            />
          </div>
          <div>
            <Text>Logo URL</Text>
            <Input
              style={{ marginTop: 6 }}
              placeholder="https://.../logo.svg（留空用色块占位）"
              value={brand.logoUrl ?? ''}
              onChange={(e) => setBrand({ logoUrl: e.target.value || undefined })}
            />
          </div>
          <Button
            onClick={() => {
              resetBrand()
              message.success('已重置白标覆盖')
            }}
          >
            重置白标
          </Button>
        </Space>
      </Card>
    </div>
  )
}
