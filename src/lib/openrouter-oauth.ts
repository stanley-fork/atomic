/**
 * OpenRouter OAuth PKCE flow utilities.
 *
 * Flow:
 * 1. Generate code_verifier / code_challenge
 * 2. Open popup to https://openrouter.ai/auth with challenge + callback_url
 * 3. User authorizes; OpenRouter redirects popup to callback_url?code=...
 * 4. Callback page posts code back via window.postMessage
 * 5. Exchange code + verifier for API key via POST /api/v1/auth/keys
 *
 * Uses S256 when crypto.subtle is available (secure contexts), falls back to
 * plain method otherwise (e.g. HTTP development servers).
 */

function base64url(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = '';
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

export async function generatePKCE(): Promise<{
  codeVerifier: string;
  codeChallenge: string;
  codeChallengeMethod: 'S256' | 'plain';
}> {
  const bytes = new Uint8Array(48);
  crypto.getRandomValues(bytes);
  const codeVerifier = base64url(bytes.buffer as ArrayBuffer);

  if (crypto.subtle) {
    const hash = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(codeVerifier));
    return { codeVerifier, codeChallenge: base64url(hash), codeChallengeMethod: 'S256' };
  }

  return { codeVerifier, codeChallenge: codeVerifier, codeChallengeMethod: 'plain' };
}

export function getCallbackUrl(): string {
  return `${window.location.origin}/openrouter-callback.html`;
}

export function openOAuthPopup(codeChallenge: string, codeChallengeMethod: 'S256' | 'plain'): Window | null {
  const callbackUrl = getCallbackUrl();
  const params = new URLSearchParams({
    callback_url: callbackUrl,
    code_challenge: codeChallenge,
    code_challenge_method: codeChallengeMethod,
  });

  const url = `https://openrouter.ai/auth?${params.toString()}`;
  const width = 600;
  const height = 700;
  const left = window.screenX + (window.outerWidth - width) / 2;
  const top = window.screenY + (window.outerHeight - height) / 2;

  return window.open(
    url,
    'openrouter-oauth',
    `width=${width},height=${height},left=${left},top=${top},popup=true`
  );
}

export async function exchangeCodeForKey(
  code: string,
  codeVerifier: string,
  codeChallengeMethod: 'S256' | 'plain',
): Promise<string> {
  const resp = await fetch('https://openrouter.ai/api/v1/auth/keys', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      code,
      code_verifier: codeVerifier,
      code_challenge_method: codeChallengeMethod,
    }),
  });

  if (!resp.ok) {
    const err = await resp.json().catch(() => ({ error: `HTTP ${resp.status}` }));
    throw new Error(err.error || `OpenRouter returned ${resp.status}`);
  }

  const data: { key: string } = await resp.json();
  return data.key;
}
