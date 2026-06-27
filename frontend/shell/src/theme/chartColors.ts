// 图表系列色板：靛蓝主导（贴合设计稿主色），其余为区分度高的辅助色，足够多系列时循环。
export const CHART_PALETTE = ['#7C75F5', '#34C5D6', '#3CCB7F', '#E0A235', '#F06A5D', '#A78BFA']

/** 取第 i 个系列色（循环）。 */
export const seriesColor = (i: number) => CHART_PALETTE[i % CHART_PALETTE.length]
