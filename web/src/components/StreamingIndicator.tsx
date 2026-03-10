export function StreamingIndicator() {
  return (
    <div className="flex items-center gap-1 py-1">
      <span className="dot-1 w-1.5 h-1.5 rounded-full bg-accent-primary" />
      <span className="dot-2 w-1.5 h-1.5 rounded-full bg-accent-primary" />
      <span className="dot-3 w-1.5 h-1.5 rounded-full bg-accent-primary" />
    </div>
  );
}
