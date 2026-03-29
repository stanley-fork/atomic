import { useState, useEffect } from "react";
import { checkSubdomain, createCheckout } from "../api";

export default function Landing() {
  const [email, setEmail] = useState("");
  const [subdomain, setSubdomain] = useState("");
  const [subdomainStatus, setSubdomainStatus] = useState<{
    available: boolean;
    reason?: string;
  } | null>(null);
  const [checking, setChecking] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  // Debounced subdomain availability check
  useEffect(() => {
    if (subdomain.length < 3) {
      setSubdomainStatus(null);
      return;
    }

    setChecking(true);
    const timeout = setTimeout(async () => {
      try {
        const result = await checkSubdomain(subdomain);
        setSubdomainStatus(result);
      } catch {
        setSubdomainStatus(null);
      } finally {
        setChecking(false);
      }
    }, 400);

    return () => clearTimeout(timeout);
  }, [subdomain]);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setSubmitting(true);

    try {
      const { checkout_url } = await createCheckout(email, subdomain);
      window.location.href = checkout_url;
    } catch (err) {
      setError(err instanceof Error ? err.message : "Something went wrong");
      setSubmitting(false);
    }
  }

  const canSubmit =
    email && subdomain.length >= 3 && subdomainStatus?.available && !submitting;

  return (
    <div className="min-h-screen bg-bg-primary">
      {/* Nav */}
      <nav className="fixed top-0 left-0 right-0 z-50 bg-bg-primary/80 backdrop-blur-md border-b border-border-light">
        <div className="max-w-6xl mx-auto px-6 h-16 flex items-center justify-between">
          <a href="https://atomic.so" className="font-display text-xl tracking-tight">
            atomic
          </a>
          <span className="text-sm text-text-muted">Managed Hosting</span>
        </div>
      </nav>

      {/* Hero */}
      <section className="relative overflow-hidden">
        {/* Subtle grid background */}
        <div
          className="absolute inset-0 opacity-[0.03]"
          style={{
            backgroundImage:
              "radial-gradient(circle, #1a1a1a 1px, transparent 1px)",
            backgroundSize: "32px 32px",
          }}
        />

        <div className="max-w-6xl mx-auto px-6 pt-32 pb-16 md:pt-40 md:pb-24">
          <div className="max-w-2xl mx-auto text-center">
            <div className="inline-flex items-center gap-2 px-3 py-1.5 mb-8 text-xs font-medium tracking-wide uppercase text-accent bg-accent-subtle rounded-full border border-accent/10">
              <span className="w-1.5 h-1.5 rounded-full bg-accent animate-pulse" />
              Managed Hosting
            </div>

            <h1 className="font-display text-5xl md:text-6xl leading-[1.05] tracking-tight mb-6">
              Your knowledge base,
              <br />
              <span className="italic text-accent">hosted for you</span>
            </h1>

            <p className="text-lg text-text-secondary leading-relaxed mb-12 max-w-lg mx-auto">
              Get a personal Atomic instance with an MCP endpoint in minutes.
              We handle the servers, SSL, and backups. You own your data.
            </p>
          </div>

          {/* Signup Form */}
          <div className="max-w-md mx-auto">
            <form onSubmit={handleSubmit}>
              <div className="bg-bg-white rounded-xl border border-border-light p-6 space-y-4 shadow-sm">
                {/* Email */}
                <div>
                  <label className="block text-sm font-medium mb-1.5">
                    Email
                  </label>
                  <input
                    type="email"
                    required
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    placeholder="you@example.com"
                    className="w-full px-3.5 py-2.5 rounded-lg border border-border bg-bg-primary text-text-primary placeholder:text-text-muted text-sm focus:outline-none focus:border-accent/50 focus:ring-2 focus:ring-accent/10 transition-all"
                  />
                </div>

                {/* Subdomain */}
                <div>
                  <label className="block text-sm font-medium mb-1.5">
                    Subdomain
                  </label>
                  <div className="flex items-center gap-0">
                    <input
                      type="text"
                      required
                      value={subdomain}
                      onChange={(e) =>
                        setSubdomain(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ""))
                      }
                      placeholder="your-name"
                      className="flex-1 px-3.5 py-2.5 rounded-l-lg border border-r-0 border-border bg-bg-primary text-text-primary placeholder:text-text-muted text-sm focus:outline-none focus:border-accent/50 focus:ring-2 focus:ring-accent/10 transition-all"
                    />
                    <span className="px-3.5 py-2.5 rounded-r-lg border border-border bg-bg-secondary text-text-muted text-sm">
                      .atomic.so
                    </span>
                  </div>
                  {/* Availability indicator */}
                  {subdomain.length >= 3 && (
                    <div className="mt-1.5 text-xs">
                      {checking ? (
                        <span className="text-text-muted">Checking...</span>
                      ) : subdomainStatus?.available ? (
                        <span className="text-green-600">Available</span>
                      ) : (
                        <span className="text-red-500">
                          {subdomainStatus?.reason || "Unavailable"}
                        </span>
                      )}
                    </div>
                  )}
                </div>

                {error && (
                  <p className="text-sm text-red-500">{error}</p>
                )}

                <button
                  type="submit"
                  disabled={!canSubmit}
                  className="w-full inline-flex items-center justify-center gap-2 px-7 py-3 text-base font-medium text-white bg-accent hover:bg-accent-dark rounded-xl transition-all hover:shadow-lg hover:shadow-accent/20 disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:shadow-none"
                >
                  {submitting ? "Redirecting..." : "Get started — $8/mo"}
                </button>
              </div>
            </form>

            {/* Trust signals */}
            <div className="mt-6 flex items-center justify-center gap-6 text-xs text-text-muted">
              <span className="flex items-center gap-1.5">
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M16.5 10.5V6.75a4.5 4.5 0 10-9 0v3.75m-.75 11.25h10.5a2.25 2.25 0 002.25-2.25v-6.75a2.25 2.25 0 00-2.25-2.25H6.75a2.25 2.25 0 00-2.25 2.25v6.75a2.25 2.25 0 002.25 2.25z" />
                </svg>
                Encrypted at rest
              </span>
              <span className="flex items-center gap-1.5">
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M14.25 9.75L16.5 12l-2.25 2.25m-4.5 0L7.5 12l2.25-2.25M6 20.25h12A2.25 2.25 0 0020.25 18V6A2.25 2.25 0 0018 3.75H6A2.25 2.25 0 003.75 6v12A2.25 2.25 0 006 20.25z" />
                </svg>
                Open source
              </span>
              <span className="flex items-center gap-1.5">
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3" />
                </svg>
                Download anytime
              </span>
            </div>
          </div>
        </div>
      </section>

      {/* Features */}
      <section className="bg-bg-secondary py-20 md:py-28">
        <div className="max-w-6xl mx-auto px-6">
          <div className="max-w-2xl mx-auto text-center mb-12">
            <h2 className="font-display text-3xl md:text-4xl tracking-tight mb-4">
              What you get
            </h2>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-6 max-w-4xl mx-auto">
            <FeatureCard
              title="Your own instance"
              description="Isolated VM running Atomic with your own subdomain, SSL, and persistent storage."
              icon={
                <path strokeLinecap="round" strokeLinejoin="round" d="M5.25 14.25h13.5m-13.5 0a3 3 0 01-3-3m3 3a3 3 0 100 6h13.5a3 3 0 100-6m-16.5-3a3 3 0 013-3h13.5a3 3 0 013 3m-19.5 0a4.5 4.5 0 01.9-2.7L5.737 5.1a3.375 3.375 0 012.7-1.35h7.126c1.062 0 2.062.5 2.7 1.35l2.587 3.45a4.5 4.5 0 01.9 2.7" />
              }
            />
            <FeatureCard
              title="MCP endpoint"
              description="A stable, authenticated MCP server for your AI agent workflows. Connect Claude, Cursor, or any MCP client."
              icon={
                <path strokeLinecap="round" strokeLinejoin="round" d="M13.19 8.688a4.5 4.5 0 011.242 7.244l-4.5 4.5a4.5 4.5 0 01-6.364-6.364l1.757-1.757m13.35-.622l1.757-1.757a4.5 4.5 0 00-6.364-6.364l-4.5 4.5a4.5 4.5 0 001.242 7.244" />
              }
            />
            <FeatureCard
              title="You own your data"
              description="Download your full database anytime. Encrypted at rest. Open source code you can audit."
              icon={
                <path strokeLinecap="round" strokeLinejoin="round" d="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z" />
              }
            />
          </div>
        </div>
      </section>

      {/* Footer */}
      <footer className="border-t border-border-light py-8">
        <div className="max-w-6xl mx-auto px-6 flex items-center justify-between text-sm text-text-muted">
          <span>Atomic</span>
          <div className="flex items-center gap-6">
            <a href="https://atomic.so" className="hover:text-text-primary transition-colors">
              Home
            </a>
            <a href="https://atomic.so/getting-started" className="hover:text-text-primary transition-colors">
              Docs
            </a>
            <a href="https://github.com/kenforthewin/atomic" className="hover:text-text-primary transition-colors">
              GitHub
            </a>
          </div>
        </div>
      </footer>
    </div>
  );
}

function FeatureCard({
  title,
  description,
  icon,
}: {
  title: string;
  description: string;
  icon: React.ReactNode;
}) {
  return (
    <div className="p-6 bg-bg-white rounded-xl border border-border-light hover:border-accent/20 transition-all hover:shadow-md">
      <div className="w-10 h-10 rounded-lg bg-accent-subtle flex items-center justify-center mb-4">
        <svg
          className="w-5 h-5 text-accent"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={1.5}
        >
          {icon}
        </svg>
      </div>
      <h3 className="font-medium text-lg mb-2">{title}</h3>
      <p className="text-sm text-text-secondary leading-relaxed">
        {description}
      </p>
    </div>
  );
}
