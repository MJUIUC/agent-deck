import type { Thread } from "@/types";
import { formatThreadTime } from "@/hooks/useTimeFormat";
import styles from "./ThreadItem.module.css";

interface ThreadItemProps {
  thread: Thread;
  isActive: boolean;
  onClick: (threadId: string) => void;
}

export function ThreadItem({ thread, isActive, onClick }: ThreadItemProps) {
  const persona = thread.persona;
  const emoji = persona?.emoji ?? "🤖";
  const timeStr = formatThreadTime(thread.updated_at);
  const preview = thread.last_message_preview ?? "";

  return (
    <div
      className={[styles.item, isActive ? styles.itemActive : ""].join(" ")}
      onClick={() => onClick(thread.id)}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick(thread.id);
        }
      }}
    >
      {/* Active bar rendered via ::before in CSS module */}
      <div className={styles.avatar}>{emoji}</div>

      <div className={styles.info}>
        <div className={styles.row}>
          <span className={styles.title}>{thread.title}</span>
          <span className={styles.time}>{timeStr}</span>
        </div>
        {preview && <div className={styles.preview}>{preview}</div>}
      </div>
    </div>
  );
}
