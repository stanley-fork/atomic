const STATUS_CONFIG: Record<
  string,
  { label: string; color: string; dotColor: string; pulse: boolean }
> = {
  running: {
    label: "Running",
    color: "text-green-700 bg-green-50 border-green-200",
    dotColor: "bg-green-500",
    pulse: true,
  },
  stopped: {
    label: "Stopped",
    color: "text-text-muted bg-bg-secondary border-border-light",
    dotColor: "bg-text-muted",
    pulse: false,
  },
  provisioning: {
    label: "Provisioning",
    color: "text-accent bg-accent-subtle border-accent/10",
    dotColor: "bg-accent",
    pulse: true,
  },
  pending: {
    label: "Pending",
    color: "text-amber-700 bg-amber-50 border-amber-200",
    dotColor: "bg-amber-500",
    pulse: true,
  },
  destroying: {
    label: "Shutting down",
    color: "text-red-700 bg-red-50 border-red-200",
    dotColor: "bg-red-500",
    pulse: true,
  },
  destroyed: {
    label: "Destroyed",
    color: "text-text-muted bg-bg-secondary border-border-light",
    dotColor: "bg-text-muted",
    pulse: false,
  },
  failed: {
    label: "Failed",
    color: "text-red-700 bg-red-50 border-red-200",
    dotColor: "bg-red-500",
    pulse: false,
  },
};

export default function InstanceStatusBadge({ status }: { status: string }) {
  const config = STATUS_CONFIG[status] || STATUS_CONFIG.pending;

  return (
    <span
      className={`inline-flex items-center gap-2 px-3 py-1.5 text-xs font-medium uppercase tracking-wide rounded-full border ${config.color}`}
    >
      <span
        className={`w-1.5 h-1.5 rounded-full ${config.dotColor} ${config.pulse ? "animate-pulse" : ""}`}
      />
      {config.label}
    </span>
  );
}
