import { Button, Dropdown, Tooltip, type MenuProps } from 'antd'
import { GlobalOutlined } from '@ant-design/icons'
import { useTranslation } from 'react-i18next'
import { useLocaleStore } from '../i18n/store'
import { SUPPORTED_LOCALES, LOCALE_LABELS, type Locale } from '../i18n/config'

/** Header 语言切换：i18next + AntD + dayjs 由 store 统一驱动。 */
export default function LocaleSwitcher() {
  const { t } = useTranslation()
  const locale = useLocaleStore((s) => s.locale)
  const setLocale = useLocaleStore((s) => s.setLocale)

  const menu: MenuProps = {
    selectable: true,
    selectedKeys: [locale],
    onClick: ({ key }) => setLocale(key as Locale),
    items: SUPPORTED_LOCALES.map((l) => ({ key: l, label: LOCALE_LABELS[l] })),
  }

  return (
    <Tooltip title={t('common:locale.switch')}>
      <Dropdown menu={menu} trigger={['click']} placement="bottomRight">
        <Button type="text" icon={<GlobalOutlined />} />
      </Dropdown>
    </Tooltip>
  )
}
