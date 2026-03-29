import { useEffect, useState } from "react";
import { useSearchParams, useNavigate } from "react-router-dom";
import { setToken, getInstanceStatus } from "../api";

type ProvisioningStage = "initializing" | "provisioning" | "starting" | "ready";

const STAGE_LABELS: Record<ProvisioningStage, string> = {
  initializing: "Setting up your account...",
  provisioning: "Creating your instance...",
  starting: "Starting your server...",
  ready: "Your instance is ready!",
};

export default function Success() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const [stage, setStage] = useState<ProvisioningStage>("initializing");
  const [error, setError] = useState("");

  useEffect(() => {
    const token = searchParams.get("token");
    if (token) {
      setToken(token);
    }

    // Poll for instance status
    let cancelled = false;
    const poll = async () => {
      setStage("provisioning");

      while (!cancelled) {
        try {
          const status = await getInstanceStatus();

          if (status.status === "running") {
            setStage("ready");
            await new Promise((r) => setTimeout(r, 1500));
            if (!cancelled) navigate("/dashboard");
            return;
          }

          if (status.status === "failed") {
            setError("Provisioning failed. Please contact support.");
            return;
          }

          if (status.status === "provisioning") {
            setStage("starting");
          }
        } catch {
          // Instance may not exist yet, keep polling
        }

        await new Promise((r) => setTimeout(r, 2000));
      }
    };

    // Small delay before first poll to let webhook process
    const timeout = setTimeout(poll, 3000);
    return () => {
      cancelled = true;
      clearTimeout(timeout);
    };
  }, [searchParams, navigate]);

  return (
    <div className="min-h-screen bg-bg-primary flex items-center justify-center">
      <div className="max-w-md mx-auto px-6 text-center">
        {error ? (
          <>
            <div className="w-16 h-16 mx-auto mb-6 rounded-full bg-red-50 flex items-center justify-center">
              <svg className="w-8 h-8 text-red-500" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z" />
              </svg>
            </div>
            <h1 className="font-display text-2xl tracking-tight mb-3">
              Something went wrong
            </h1>
            <p className="text-text-secondary">{error}</p>
          </>
        ) : (
          <>
            {/* Animated spinner */}
            <div className="w-16 h-16 mx-auto mb-6 relative">
              <div className="absolute inset-0 rounded-full border-2 border-border-light" />
              <div className="absolute inset-0 rounded-full border-2 border-transparent border-t-accent animate-spin" />
              {stage === "ready" && (
                <div className="absolute inset-0 flex items-center justify-center">
                  <svg className="w-8 h-8 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12.75l6 6 9-13.5" />
                  </svg>
                </div>
              )}
            </div>

            <h1 className="font-display text-2xl tracking-tight mb-3">
              {STAGE_LABELS[stage]}
            </h1>

            <p className="text-sm text-text-muted">
              {stage === "ready"
                ? "Redirecting to your dashboard..."
                : "This usually takes about 30 seconds."}
            </p>

            {/* Progress dots */}
            <div className="flex items-center justify-center gap-2 mt-8">
              {(["initializing", "provisioning", "starting", "ready"] as ProvisioningStage[]).map(
                (s, i) => (
                  <div
                    key={s}
                    className={`w-2 h-2 rounded-full transition-all duration-300 ${
                      i <=
                      ["initializing", "provisioning", "starting", "ready"].indexOf(stage)
                        ? "bg-accent"
                        : "bg-border"
                    }`}
                  />
                )
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
