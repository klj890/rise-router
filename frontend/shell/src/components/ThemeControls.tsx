import { Button, Dropdown, Tooltip, type MenuProps } from 'antd'
import { MoonOutlined, SunOutlined, BgColorsOutlined, SettingOutlined } from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { useThemeStore } from '../theme/store'
import { useResolvedMode } from '../theme/useResolvedMode'
import { ACCENT_LIST } from '../theme/presets'
import type { AccentId } from '../theme/types'

/** Header 右侧主题控件：暗/浅切换 + 强调色预设 + 跳转外观设置。 */
export default function ThemeControls() {
  const navigate = useNavigate()
  const resolved = useResolvedMode()
  const accentId = useThemeStore((s) => s.accentId)
  const setMode = useThemeStore((s) => s.setMode)
  const setAccent = useThemeStore((s) => s.setAccent)

  const accentMenu: MenuProps = {
    selectable: true,
    selectedKeys: [accentId],
    onClick: ({ key }) => setAccent(key as AccentId),
    items: ACCENT_LIST.map((a) => ({
      key: a.id,
      label: (
        <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span
            style={{
              width: 12,
              height: 12,
              borderRadius: 3,
              background: a[resolved],
              display: 'inline-block',
            }}
          />
          {a.name}
        </span>
      ),
    })),
  }

  return (
    <span style={{ display: 'inline-flex', gap: 2, alignItems: 'center' }}>
      <Tooltip title={resolved === 'dark' ? '切换浅色' : '切换暗色'}>
        <Button
          type="text"
          icon={resolved === 'dark' ? <SunOutlined /> : <MoonOutlined />}
          onClick={() => setMode(resolved === 'dark' ? 'light' : 'dark')}
        />
      </Tooltip>
      <Tooltip title="强调色">
        <Dropdown menu={accentMenu} trigger={['click']} placement="bottomRight">
          <Button type="text" icon={<BgColorsOutlined />} />
        </Dropdown>
      </Tooltip>
      <Tooltip title="外观设置">
        <Button
          type="text"
          icon={<SettingOutlined />}
          onClick={() => navigate('/settings/appearance')}
        />
      </Tooltip>
    </span>
  )
}
