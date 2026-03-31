import { useEffect, useState, useCallback } from "react";
import {
  getInstanceStatus,
  startInstance,
  stopInstance,
  restartInstance,
  getBillingPortalUrl,
  type InstanceStatus,
} from "../api";
import InstanceStatusBadge from "../components/InstanceStatus";
import EndpointInfo from "../components/EndpointInfo";

export default function Dashboard() {
  const [instance, setInstance] = useState<InstanceStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState("");
  const [error, setError] = useState("");

  const fetchStatus = useCallback(async () => {
    try {
      const status = await getInstanceStatus();
      setInstance(status);
      setError("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load status");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchStatus();
    const interval = setInterval(fetchStatus, 10000);
    return () => clearInterval(interval);
  }, [fetchStatus]);

  async function handleAction(
    action: "start" | "stop" | "restart",
    fn: () => Promise<unknown>
  ) {
    setActionLoading(action);
    try {
      await fn();
      // Poll quickly for status update
      setTimeout(fetchStatus, 1000);
      setTimeout(fetchStatus, 3000);
    } catch (err) {
      setError(err instanceof Error ? err.message : `Failed to ${action}`);
    } finally {
      setActionLoading("");
    }
  }

  async function handleBilling() {
    try {
      const { portal_url } = await getBillingPortalUrl();
      window.location.href = portal_url;
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to open billing portal"
      );
    }
  }

  if (loading) {
    return (
      <div className="min-h-screen bg-bg-primary flex items-center justify-center">
        <div className="w-8 h-8 relative">
          <div className="absolute inset-0 rounded-full border-2 border-border-light" />
          <div className="absolute inset-0 rounded-full border-2 border-transparent border-t-accent animate-spin" />
        </div>
      </div>
    );
  }

  if (!instance) {
    return (
      <div className="min-h-screen bg-bg-primary flex items-center justify-center">
        <p className="text-text-muted">No instance found.</p>
      </div>
    );
  }

  const isRunning = instance.status === "running";
  const isStopped = instance.status === "stopped";
  const isLocked = isRunning && instance.fly_state === "started";

  return (
    <div className="min-h-screen bg-bg-primary">
      {/* Nav */}
      <nav className="fixed top-0 left-0 right-0 z-50 bg-bg-primary/80 backdrop-blur-md border-b border-border-light">
        <div className="max-w-6xl mx-auto px-6 h-16 flex items-center justify-between">
          <a href="/" className="font-display text-xl tracking-tight">
            atomic
          </a>
          <button
            onClick={handleBilling}
            className="text-sm text-text-secondary hover:text-text-primary transition-colors"
          >
            Manage billing
          </button>
        </div>
      </nav>

      <div className="max-w-2xl mx-auto px-6 pt-28 pb-16">
        <div className="mb-8">
          <h1 className="font-display text-3xl tracking-tight mb-2">
            Your Instance
          </h1>
          <p className="text-text-secondary">
            {instance.subdomain_url.replace("https://", "")}
          </p>
        </div>

        {error && (
          <div className="mb-6 p-3 rounded-lg bg-red-50 border border-red-200 text-sm text-red-600">
            {error}
          </div>
        )}

        {/* Status Card */}
        <div className="bg-bg-white rounded-xl border border-border-light p-6 mb-6">
          <div className="flex items-center justify-between mb-6">
            <div className="flex items-center gap-3">
              <span className="text-sm font-medium text-text-muted uppercase tracking-wide">
                Status
              </span>
              <InstanceStatusBadge status={instance.status} />
            </div>
          </div>

          {/* Controls */}
          <div className="flex items-center gap-3">
            {isStopped && (
              <button
                onClick={() => handleAction("start", startInstance)}
                disabled={!!actionLoading}
                className="inline-flex items-center gap-2 px-5 py-2.5 text-sm font-medium text-white bg-accent hover:bg-accent-dark rounded-xl transition-all hover:shadow-lg hover:shadow-accent/20 disabled:opacity-50"
              >
                {actionLoading === "start" ? (
                  "Starting..."
                ) : (
                  <>
                    <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M5.25 5.653c0-.856.917-1.398 1.667-.986l11.54 6.348a1.125 1.125 0 010 1.971l-11.54 6.347a1.125 1.125 0 01-1.667-.985V5.653z" />
                    </svg>
                    Start
                  </>
                )}
              </button>
            )}

            {isRunning && (
              <>
                <button
                  onClick={() => handleAction("restart", restartInstance)}
                  disabled={!!actionLoading}
                  className="inline-flex items-center gap-2 px-5 py-2.5 text-sm font-medium text-text-primary bg-bg-white border border-border rounded-xl hover:border-accent/30 hover:bg-accent-subtle/50 transition-all disabled:opacity-50"
                >
                  {actionLoading === "restart" ? (
                    "Restarting..."
                  ) : (
                    <>
                      <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                        <path strokeLinecap="round" strokeLinejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182" />
                      </svg>
                      Restart
                    </>
                  )}
                </button>
                <button
                  onClick={() => handleAction("stop", stopInstance)}
                  disabled={!!actionLoading}
                  className="inline-flex items-center gap-2 px-5 py-2.5 text-sm font-medium text-red-600 bg-bg-white border border-red-200 rounded-xl hover:bg-red-50 transition-all disabled:opacity-50"
                >
                  {actionLoading === "stop" ? (
                    "Stopping..."
                  ) : (
                    <>
                      <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                        <path strokeLinecap="round" strokeLinejoin="round" d="M5.25 7.5A2.25 2.25 0 017.5 5.25h9a2.25 2.25 0 012.25 2.25v9a2.25 2.25 0 01-2.25 2.25h-9a2.25 2.25 0 01-2.25-2.25v-9z" />
                      </svg>
                      Stop
                    </>
                  )}
                </button>
              </>
            )}
          </div>

          {/* Locked state hint */}
          {isLocked && (
            <div className="mt-4 p-3 rounded-lg bg-accent-subtle border border-accent/10 text-sm text-accent-dark">
              If your instance was recently restarted, visit{" "}
              <a
                href={instance.subdomain_url}
                target="_blank"
                rel="noopener noreferrer"
                className="font-medium underline"
              >
                {instance.subdomain_url}
              </a>{" "}
              to unlock it with your passphrase.
            </div>
          )}
        </div>

        {/* Endpoints */}
        <div className="bg-bg-white rounded-xl border border-border-light p-6 mb-6">
          <h2 className="text-sm font-medium text-text-muted uppercase tracking-wide mb-4">
            Endpoints
          </h2>
          <div className="space-y-3">
            <EndpointInfo label="Instance URL" url={instance.subdomain_url} />
            <EndpointInfo label="MCP Endpoint" url={instance.mcp_url} />
          </div>
        </div>

        {/* Quick Actions */}
        <div className="bg-bg-white rounded-xl border border-border-light p-6">
          <h2 className="text-sm font-medium text-text-muted uppercase tracking-wide mb-4">
            Account
          </h2>
          <div className="space-y-2">
            <button
              onClick={handleBilling}
              className="w-full text-left px-4 py-3 rounded-lg text-sm text-text-secondary hover:bg-bg-secondary transition-colors flex items-center justify-between group"
            >
              <span>Manage billing & subscription</span>
              <svg className="w-4 h-4 text-text-muted group-hover:text-text-secondary transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M13.5 6H5.25A2.25 2.25 0 003 8.25v10.5A2.25 2.25 0 005.25 21h10.5A2.25 2.25 0 0018 18.75V10.5m-10.5 6L21 3m0 0h-5.25M21 3v5.25" />
              </svg>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
