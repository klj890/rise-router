export interface FilterTabItem {
  key: string
  label: string
  count?: number
}

interface FilterTabsProps {
  items: FilterTabItem[]
  value: string
  onChange: (key: string) => void
}

/** 带计数的分段筛选（全部/启用/降级…）。选中走 primary-weak 底，与设计稿一致。 */
export default function FilterTabs({ items, value, onChange }: FilterTabsProps) {
  return (
    <div style={{ display: 'inline-flex', gap: 6, flexWrap: 'wrap' }}>
      {items.map((it) => {
        const active = it.key === value
        return (
          <button
            key={it.key}
            type="button"
            onClick={() => onChange(it.key)}
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 6,
              height: 32,
              padding: '0 13px',
              borderRadius: 8,
              border: `1px solid ${active ? 'var(--rr-primary-border)' : 'var(--rr-border)'}`,
              background: active ? 'var(--rr-primary-weak)' : 'transparent',
              color: active ? 'var(--rr-primary)' : 'var(--rr-text-2)',
              fontSize: 13,
              fontWeight: active ? 600 : 500,
              cursor: 'pointer',
              transition: 'all .15s ease',
            }}
          >
            {it.label}
            {it.count != null && (
              <span
                className="rr-num"
                style={{
                  fontSize: 11.5,
                  padding: '0 6px',
                  borderRadius: 6,
                  background: active ? 'var(--rr-primary-border)' : 'var(--rr-surface-2)',
                  color: active ? 'var(--rr-primary)' : 'var(--rr-text-3)',
                }}
              >
                {it.count}
              </span>
            )}
          </button>
        )
      })}
    </div>
  )
}
