import { lookup } from "node:dns/promises";
import { isIP } from "node:net";

/** Defense in depth; production egress must independently deny browser UDP. */
export const CERTIFIED_BROWSER_TRANSPORT_HARDENING_ARGS = Object.freeze([
  "--disable-quic",
  "--disable-features=WebTransport",
  "--force-webrtc-ip-handling-policy=disable_non_proxied_udp",
] as const);

export async function assertPublicApplicationUrl(rawUrl: string): Promise<URL> {
  const url = new URL(rawUrl);
  if (url.protocol !== "https:") throw new Error("Application links must use HTTPS");
  if (url.username || url.password) throw new Error("Application links cannot contain credentials");
  const host = stripIpv6Brackets(url.hostname.toLowerCase());
  if (host === "localhost" || host.endsWith(".local") || host.endsWith(".internal")) {
    throw new Error("Private network application links are blocked");
  }
  const addresses = isIP(host) ? [{ address: host }] : await lookup(host, { all: true, verbatim: true });
  if (addresses.length === 0 || addresses.some(({ address }) => isPrivateAddress(address))) {
    throw new Error("Private network application links are blocked");
  }
  return url;
}

export function isPrivateAddress(rawAddress: string): boolean {
  const address = stripIpv6Brackets(rawAddress.toLowerCase());
  if (address.includes(".")) {
    const mapped = address.match(/^(?:::ffff:)?(\d+\.\d+\.\d+\.\d+)$/)?.[1];
    if (!mapped) return true;
    const bytes = mapped.split(".").map(Number);
    if (bytes.length !== 4 || bytes.some((value) => !Number.isInteger(value) || value < 0 || value > 255)) return true;
    const [a, b] = bytes;
    return a === 0
      || a === 10
      || a === 127
      || a >= 224
      || (a === 100 && b >= 64 && b <= 127)
      || (a === 169 && b === 254)
      || (a === 172 && b >= 16 && b <= 31)
      || (a === 192 && b === 0)
      || (a === 192 && b === 168)
      || (a === 198 && (b === 18 || b === 19));
  }
  if (isIP(address) !== 6) return true;
  if (address === "::" || address === "::1") return true;
  if (address.startsWith("::ffff:")) return isPrivateAddress(address.slice(7));
  const first = Number.parseInt(address.split(":")[0] || "0", 16);
  return (first & 0xfe00) === 0xfc00
    || (first & 0xffc0) === 0xfe80
    || (first & 0xff00) === 0xff00;
}

function stripIpv6Brackets(value: string): string {
  return value.startsWith("[") && value.endsWith("]") ? value.slice(1, -1) : value;
}
