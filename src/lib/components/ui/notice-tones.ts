import type { IconName } from "./Icon.svelte";
import type { NotificationTone } from "../../stores/notifications";

// Shared by toasts and the notification history so a tone looks the same in both.
export const noticeTones: Record<NotificationTone, { icon: IconName; text: string; line: string }> = {
  info: { icon: "info", text: "text-legio-info", line: "inset-ring-legio-info/25" },
  success: { icon: "success", text: "text-legio-success", line: "inset-ring-legio-success/25" },
  warning: { icon: "warning", text: "text-legio-warning", line: "inset-ring-legio-warning/25" },
  error: { icon: "danger", text: "text-legio-error", line: "inset-ring-legio-error/25" },
};
