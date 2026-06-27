import { api } from './client'

/**
 * 多模态任务监控（控制台 ops 视图）：管理令牌（X-Admin-Token）守卫的跨租户只读列表 + 取消。
 * 注：任务**提交**由 API 消费方用其密钥经 `/v1/tasks`（Bearer 密钥）发起，控制台不代提交。
 */

export type TaskStatus = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled'

export interface Task {
  id: number
  org_id: number
  org_name: string
  /** video.generation / image.generation / audio.speech … */
  type: string
  model_slug: string
  status: TaskStatus
  input: unknown
  extra: unknown
  vendor_task_id: string | null
  /** 计费量纲数量，如 {second:7} / {image:4} / {call:1} */
  usage: Record<string, number> | null
  charged_amount: string | null
  error: string | null
  webhook_url: string | null
  webhook_state: string | null
  poll_count: number
  created_at: string
  started_at: string | null
  finished_at: string | null
}

/** 跨租户任务列表（倒序，管理令牌）。 */
export async function listTasks(limit = 100): Promise<Task[]> {
  const { data } = await api.get<Task[]>('/api/task/admin/tasks', { params: { limit } })
  return data
}

/** 控制台取消任务（含上游取消）。 */
export async function cancelTask(id: number): Promise<Task> {
  const { data } = await api.post<Task>(`/api/task/admin/tasks/${id}/cancel`, {})
  return data
}
