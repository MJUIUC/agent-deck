import styles from "./MessageBubble.module.css";

export function StreamingIndicator() {
  return (
    <div className={styles.streamingIndicator}>
      <span className={`${styles.dot} dot-1`} />
      <span className={`${styles.dot} dot-2`} />
      <span className={`${styles.dot} dot-3`} />
    </div>
  );
}
