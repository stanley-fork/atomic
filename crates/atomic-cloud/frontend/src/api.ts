const API_BASE = "";

function getToken(): string | null {
  return localStorage.getItem("atomic_management_token");
}

export function setToken(token: string) {
  localStorage.setItem("atomic_management_token", token);
}

export function clearToken() {
  localStorage.removeItem("atomic_management_token");
}

export function hasToken(): boolean {
  return !!getToken();
}

async function apiFetch(path: string, options: RequestInit = {}) {
  const token = getToken();
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(options.headers as Record<string, string>),
  };
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }
  const resp = await fetch(`${API_BASE}${path}`, { ...options, headers });
  if (!resp.ok) {
    const body = await resp.json().catch(() => ({ error: resp.statusText }));
    throw new Error(body.error || resp.statusText);
  }
  return resp.json();
}

export async function checkSubdomain(
  subdomain: string
): Promise<{ available: boolean; reason?: string }> {
  return apiFetch(
    `/api/checkout/check-subdomain?subdomain=${encodeURIComponent(subdomain)}`
  );
}

export async function createCheckout(
  email: string,
  subdomain: string
): Promise<{ checkout_url: string }> {
  return apiFetch("/api/checkout", {
    method: "POST",
    body: JSON.stringify({ email, subdomain }),
  });
}

export interface InstanceStatus {
  id: string;
  subdomain: string;
  status: string;
  fly_state: string | null;
  subdomain_url: string;
  mcp_url: string;
  created_at: string;
}

export async function exchangeSession(
  sessionId: string
): Promise<{ management_token?: string; status: string }> {
  const resp = await fetch(
    `${API_BASE}/api/checkout/session?session_id=${encodeURIComponent(sessionId)}`
  );
  if (resp.status === 202) {
    return resp.json(); // Pending — webhook not processed yet
  }
  if (!resp.ok) {
    const body = await resp.json().catch(() => ({ error: resp.statusText }));
    throw new Error(body.error || resp.statusText);
  }
  return resp.json();
}

export async function getInstanceStatus(): Promise<InstanceStatus> {
  return apiFetch("/api/instance/status");
}

export async function startInstance(): Promise<{ status: string }> {
  return apiFetch("/api/instance/start", { method: "POST" });
}

export async function stopInstance(): Promise<{ status: string }> {
  return apiFetch("/api/instance/stop", { method: "POST" });
}

export async function restartInstance(): Promise<{ status: string }> {
  return apiFetch("/api/instance/restart", { method: "POST" });
}

export async function getBillingPortalUrl(): Promise<{ portal_url: string }> {
  return apiFetch("/api/instance/portal", { method: "POST" });
}

export async function sendMagicLink(
  email: string
): Promise<{ status: string; message: string }> {
  return apiFetch("/api/auth/send", {
    method: "POST",
    body: JSON.stringify({ email }),
  });
}

export async function verifyMagicLink(
  token: string
): Promise<{ management_token: string; instance_id: string; status: string }> {
  return apiFetch(`/api/auth/verify?token=${encodeURIComponent(token)}`);
}
