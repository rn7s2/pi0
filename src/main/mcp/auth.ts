// Bearer-token check for the MCP endpoint. The token is pi0's mandatory access
// floor: loopback binding already blocks remote/browser callers, and this stops
// any *local* program that merely opens a socket to the port. The token itself
// lives (encrypted) in the SQLite store; here we only compare what a request
// presented against the expected value.
import crypto from 'node:crypto';

/**
 * Constant-time bearer check. Returns true iff `header` is exactly
 * `Bearer <token>`. Length differences short-circuit before the timing-safe
 * compare (which requires equal-length buffers) without leaking the token.
 */
export function isAuthorized(header: string | undefined, token: string): boolean {
    if (!header || !header.startsWith('Bearer ') || !token) return false;
    const presented = Buffer.from(header.slice('Bearer '.length));
    const expected = Buffer.from(token);
    if (presented.length !== expected.length) return false;
    return crypto.timingSafeEqual(presented, expected);
}
